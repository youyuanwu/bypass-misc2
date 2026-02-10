//! Event dispatching to specific reactor lcores.
//!
//! [`SpdkEvent`] enables dispatching work to a specific reactor (lcore).
//! This is the SPDK-native way to do multi-core I/O - each reactor runs
//! on a dedicated CPU core and processes events from its queue.
//!
//! # When to use
//!
//! - **[`ThreadHandle::send()`](crate::ThreadHandle)**: Send to a specific SPDK thread (by thread ID)
//! - **[`SpdkEvent::call_on()`]**: Send to a specific CPU core (by lcore ID)
//!
//! Use `SpdkEvent` when you want core affinity for I/O operations.
//!
//! # Example
//!
//! ```ignore
//! use spdk_io::{SpdkApp, SpdkEvent, Cores};
//!
//! SpdkApp::builder()
//!     .reactor_mask("0x3")  // Cores 0 and 1
//!     .run(|| {
//!         // Dispatch work to lcore 1
//!         SpdkEvent::call_on(1, || {
//!             println!("Running on lcore 1!");
//!         }).unwrap();
//!
//!         // Current lcore
//!         println!("Current lcore: {}", Cores::current());
//!
//!         SpdkApp::stop();
//!     })
//!     .unwrap();
//! ```

use std::ffi::c_void;

use spdk_io_sys::*;

use crate::complete::{CompletionReceiver, completion};
use crate::error::{Error, Result};

// =============================================================================
// Cores - CPU core utilities
// =============================================================================

/// CPU core utilities for reactor management.
///
/// These functions query the reactor lcores configured via `reactor_mask`
/// in [`SpdkAppBuilder`](crate::SpdkAppBuilder).
pub struct Cores;

impl Cores {
    /// Get the current lcore ID.
    ///
    /// Returns the logical core ID of the calling thread.
    pub fn current() -> u32 {
        unsafe { spdk_env_get_current_core() }
    }

    /// Get the first reactor lcore.
    ///
    /// This is the main reactor core (typically the one running the main callback).
    pub fn first() -> u32 {
        unsafe { spdk_env_get_first_core() }
    }

    /// Get the last reactor lcore.
    pub fn last() -> u32 {
        unsafe { spdk_env_get_last_core() }
    }

    /// Get the total number of reactor cores.
    pub fn count() -> u32 {
        unsafe { spdk_env_get_core_count() }
    }

    /// Iterate over all reactor lcores.
    ///
    /// # Example
    ///
    /// ```ignore
    /// for lcore in Cores::iter() {
    ///     println!("Reactor on lcore {}", lcore);
    /// }
    /// ```
    pub fn iter() -> CoreIterator {
        CoreIterator {
            current: u32::MAX, // Signal to start from first
        }
    }
}

/// Iterator over reactor lcores.
///
/// Created by [`Cores::iter()`].
pub struct CoreIterator {
    current: u32,
}

impl Iterator for CoreIterator {
    type Item = u32;

    fn next(&mut self) -> Option<u32> {
        if self.current == u32::MAX {
            // First iteration
            self.current = Cores::first();
            Some(self.current)
        } else {
            let next = unsafe { spdk_env_get_next_core(self.current) };
            if next == u32::MAX {
                None
            } else {
                self.current = next;
                Some(self.current)
            }
        }
    }
}

// =============================================================================
// SpdkEvent - Event dispatching to lcores
// =============================================================================

/// Event for dispatching work to a specific reactor (lcore).
///
/// Unlike [`ThreadHandle::send()`](crate::ThreadHandle) which targets a specific
/// SPDK thread, `SpdkEvent` dispatches to an lcore's reactor. The reactor may run
/// multiple SPDK threads, but events execute on the reactor's main thread.
///
/// # Example
///
/// ```ignore
/// use spdk_io::SpdkEvent;
///
/// // Dispatch work to lcore 1
/// SpdkEvent::call_on(1, || {
///     let ctrlr = NvmeController::connect(&trid, None).unwrap();
///     let qpair = ctrlr.alloc_io_qpair(None).unwrap();
///     // I/O on lcore 1...
/// })?;
/// ```
pub struct SpdkEvent {
    ptr: *mut spdk_event,
    /// Closure pointer - needed for cleanup if dropped without calling
    closure_ptr: *mut c_void,
}

// SpdkEvent can be sent to other threads (it's just a pointer to queue an event)
// but it's not Sync (can't be shared simultaneously)
unsafe impl Send for SpdkEvent {}

impl SpdkEvent {
    /// Allocate an event to run on a specific lcore.
    ///
    /// The closure will execute on the target lcore's reactor thread.
    /// The event is NOT dispatched until [`call()`](Self::call) is invoked.
    ///
    /// # Arguments
    ///
    /// * `lcore` - Target logical core ID (must be in reactor_mask)
    /// * `f` - Closure to execute on the target lcore
    ///
    /// # Errors
    ///
    /// Returns error if event allocation fails (invalid lcore, out of memory).
    pub fn new<F>(lcore: u32, f: F) -> Result<Self>
    where
        F: FnOnce() + Send + 'static,
    {
        // Double-box because Box<dyn FnOnce()> is a fat pointer
        let boxed: Box<Box<dyn FnOnce() + Send>> = Box::new(Box::new(f));
        let arg1 = Box::into_raw(boxed) as *mut c_void;

        let ptr = unsafe {
            spdk_event_allocate(lcore, Some(event_trampoline), arg1, std::ptr::null_mut())
        };

        if ptr.is_null() {
            // Clean up the boxed closure
            unsafe {
                drop(Box::from_raw(arg1 as *mut Box<dyn FnOnce() + Send>));
            }
            return Err(Error::InvalidArgument(format!(
                "Failed to allocate event for lcore {}. Is it in reactor_mask?",
                lcore
            )));
        }

        Ok(Self {
            ptr,
            closure_ptr: arg1,
        })
    }

    /// Dispatch the event to the target lcore.
    ///
    /// The event is placed on the target reactor's event queue and will
    /// be processed during the reactor's next poll cycle.
    ///
    /// This consumes the event (can only be called once).
    pub fn call(mut self) {
        // Clear closure_ptr - the trampoline will free it after execution
        self.closure_ptr = std::ptr::null_mut();
        unsafe {
            spdk_event_call(self.ptr);
        }
        // Don't run Drop - SPDK owns the event now
        std::mem::forget(self);
    }

    /// Allocate and dispatch in one step (convenience method).
    ///
    /// # Arguments
    ///
    /// * `lcore` - Target logical core ID
    /// * `f` - Closure to execute on the target lcore
    ///
    /// # Errors
    ///
    /// Returns error if event allocation fails.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Dispatch work to lcore 1
    /// SpdkEvent::call_on(1, || {
    ///     println!("Running on lcore 1!");
    /// })?;
    /// ```
    pub fn call_on<F>(lcore: u32, f: F) -> Result<()>
    where
        F: FnOnce() + Send + 'static,
    {
        Self::new(lcore, f)?.call();
        Ok(())
    }

    /// Dispatch a closure and await its completion.
    ///
    /// Sends the closure to the target lcore and returns a receiver that
    /// completes when the closure finishes.
    ///
    /// # Arguments
    ///
    /// * `lcore` - Target logical core ID
    /// * `f` - Closure to execute on the target lcore
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Run computation on lcore 1 and get result
    /// let result = SpdkEvent::call_on_async(1, || {
    ///     expensive_computation()
    /// }).await;
    /// ```
    pub fn call_on_async<F, T>(lcore: u32, f: F) -> Result<CompletionReceiver<T>>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        let (tx, rx) = completion::<T>();

        Self::call_on(lcore, move || {
            let result = f();
            tx.success(result);
        })?;

        Ok(rx)
    }
}

impl Drop for SpdkEvent {
    fn drop(&mut self) {
        // If closure_ptr is non-null, we're being dropped without call() being invoked.
        // We must free the boxed closure to avoid a memory leak.
        if !self.closure_ptr.is_null() {
            // SAFETY: closure_ptr was created by Box::into_raw in new()
            unsafe {
                drop(Box::from_raw(
                    self.closure_ptr as *mut Box<dyn FnOnce() + Send>,
                ));
            }

            // Note: The SPDK event itself (self.ptr) is leaked.
            // SPDK doesn't provide spdk_event_free(), so there's no way to reclaim it.
            // This is a design limitation of SPDK's event API.
            #[cfg(debug_assertions)]
            eprintln!(
                "Warning: SpdkEvent dropped without calling call(). \
                 Closure freed, but SPDK event is leaked (no dealloc API)."
            );
        }
    }
}

/// C callback for spdk_event_allocate.
///
/// This is the trampoline that reconstructs the boxed closure from arg1.
unsafe extern "C" fn event_trampoline(arg1: *mut c_void, _arg2: *mut c_void) {
    // SAFETY: arg1 is a pointer to Box<dyn FnOnce() + Send> created in SpdkEvent::new()
    let boxed: Box<Box<dyn FnOnce() + Send>> = unsafe { Box::from_raw(arg1 as *mut _) };
    // Call the closure
    boxed();
}
