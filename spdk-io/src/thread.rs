//! SPDK thread management
//!
//! An [`SpdkThread`] is SPDK's lightweight scheduling context (similar to a green thread).
//! It is NOT an OS thread - it runs on whatever OS thread calls [`SpdkThread::poll()`].
//!
//! Each OS thread that performs SPDK I/O needs an `SpdkThread` attached to it.
//! The thread provides:
//! - Message passing between SPDK threads
//! - Poller scheduling
//! - I/O channel allocation
//!
//! # Example
//!
//! ```no_run
//! use spdk_io::{SpdkEnv, SpdkThread};
//!
//! fn main() {
//!     let _env = SpdkEnv::builder()
//!         .name("app")
//!         .no_pci(true)
//!         .no_huge(true)
//!         .mem_size_mb(64)
//!         .build()
//!         .expect("Failed to init SPDK");
//!
//!     // Create and attach thread to current OS thread
//!     let thread = SpdkThread::new("worker").expect("Failed to create thread");
//!
//!     // Poll in a loop (typically in an async task)
//!     loop {
//!         let work_done = thread.poll();
//!         if work_done == 0 {
//!             // Yield to other tasks...
//!             break; // For example only
//!         }
//!     }
//! }
//! ```

use std::ffi::{CString, c_void};
use std::marker::PhantomData;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use spdk_io_sys::*;

use crate::complete::{CompletionReceiver, completion};
use crate::error::{Error, Result};

/// Global flag to track if thread library is initialized
static THREAD_LIB_INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Default message mempool size (same as SPDK's SPDK_DEFAULT_MSG_MEMPOOL_SIZE)
pub const DEFAULT_MSG_MEMPOOL_SIZE: usize = 262144 - 1;

/// Smaller mempool size for testing (1023 entries)
pub const SMALL_MSG_MEMPOOL_SIZE: usize = 1023;

/// Initialize the SPDK thread library with custom mempool size.
///
/// This is called automatically when creating the first [`SpdkThread`].
pub(crate) fn thread_lib_init_ext(msg_mempool_size: usize) -> Result<()> {
    if THREAD_LIB_INITIALIZED.swap(true, Ordering::SeqCst) {
        return Ok(()); // Already initialized
    }

    let rc = unsafe { spdk_thread_lib_init_ext(None, None, 0, msg_mempool_size) };
    if rc != 0 {
        THREAD_LIB_INITIALIZED.store(false, Ordering::SeqCst);
        return Err(Error::EnvInit(format!(
            "spdk_thread_lib_init_ext failed with error code {}",
            rc
        )));
    }

    Ok(())
}

/// Initialize the SPDK thread library with default settings.
pub(crate) fn thread_lib_init() -> Result<()> {
    if THREAD_LIB_INITIALIZED.swap(true, Ordering::SeqCst) {
        return Ok(()); // Already initialized
    }

    // Use the simple init function - it uses default mempool settings
    let rc = unsafe { spdk_thread_lib_init(None, 0) };
    if rc != 0 {
        THREAD_LIB_INITIALIZED.store(false, Ordering::SeqCst);
        return Err(Error::EnvInit(format!(
            "spdk_thread_lib_init failed with error code {}",
            rc
        )));
    }

    Ok(())
}

/// Mark the thread library as initialized without calling init.
///
/// Used when the thread library was initialized externally (e.g., by `spdk_app_start()`).
pub(crate) fn assume_thread_lib_initialized() {
    THREAD_LIB_INITIALIZED.store(true, Ordering::SeqCst);
}

/// Finalize the SPDK thread library.
pub(crate) fn thread_lib_fini() {
    if THREAD_LIB_INITIALIZED.swap(false, Ordering::SeqCst) {
        unsafe {
            spdk_thread_lib_fini();
        }
    }
}

/// SPDK thread context.
///
/// This is a lightweight scheduling context, not an OS thread. It must be
/// polled on the OS thread that created it.
///
/// # Thread Safety
///
/// `SpdkThread` is `!Send` and `!Sync` - it must stay on the OS thread
/// that created it. This is enforced at compile time.
pub struct SpdkThread {
    ptr: NonNull<spdk_thread>,
    /// Prevent Send/Sync - thread must stay on creating OS thread
    _marker: PhantomData<*mut ()>,
}

impl SpdkThread {
    /// Attach an SPDK thread context to the CURRENT OS thread.
    ///
    /// This does NOT create a new OS thread - you're already on one.
    /// An SPDK thread is a lightweight scheduling context (like a green thread),
    /// not a real OS thread. It provides:
    /// - Message passing between SPDK threads
    /// - Poller scheduling  
    /// - I/O channel allocation
    ///
    /// The thread library is initialized automatically if needed.
    ///
    /// # Arguments
    ///
    /// * `name` - Thread name (for debugging/logging)
    ///
    /// # Errors
    ///
    /// Returns an error if thread creation fails.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use spdk_io::SpdkThread;
    ///
    /// // On your existing OS thread, attach an SPDK context
    /// let thread = SpdkThread::current("worker").unwrap();
    ///
    /// // Now poll it in your event loop
    /// loop {
    ///     thread.poll();
    ///     // ... do other work ...
    ///     # break;
    /// }
    /// ```
    pub fn current(name: &str) -> Result<Self> {
        // Initialize thread library if not already done
        thread_lib_init()?;
        Self::attach(name)
    }

    /// Attach an SPDK thread context to the current OS thread.
    ///
    /// Unlike [`current()`](Self::current), this does NOT initialize the
    /// thread library. Use this when the library was already initialized
    /// (e.g., by `SpdkApp` or explicit [`thread_lib_init()`] call).
    ///
    /// # Errors
    ///
    /// Returns an error if thread creation fails.
    pub fn attach(name: &str) -> Result<Self> {
        let name_cstr = CString::new(name)?;

        let ptr = unsafe { spdk_thread_create(name_cstr.as_ptr(), std::ptr::null()) };

        let ptr = NonNull::new(ptr)
            .ok_or_else(|| Error::EnvInit("spdk_thread_create returned NULL".to_string()))?;

        // Set as current thread for this OS thread
        unsafe {
            spdk_set_thread(ptr.as_ptr());
        }

        Ok(Self {
            ptr,
            _marker: PhantomData,
        })
    }

    /// Alias for [`current`](Self::current) - creates an SPDK thread on current OS thread.
    #[inline]
    pub fn new(name: &str) -> Result<Self> {
        Self::current(name)
    }

    /// Attach an SPDK thread context to the current OS thread with custom mempool size.
    ///
    /// Use [`SMALL_MSG_MEMPOOL_SIZE`] for testing without hugepages.
    /// This uses `spdk_thread_lib_init_ext` which allows custom mempool sizes.
    ///
    /// # Arguments
    ///
    /// * `name` - Thread name (for debugging/logging)
    /// * `msg_mempool_size` - Size of message mempool (only used if thread lib not yet initialized)
    ///
    /// # Errors
    ///
    /// Returns an error if thread creation fails.
    pub fn current_with_mempool_size(name: &str, msg_mempool_size: usize) -> Result<Self> {
        // Initialize thread library if needed (with custom mempool size)
        thread_lib_init_ext(msg_mempool_size)?;

        let name_cstr = CString::new(name)?;

        let ptr = unsafe { spdk_thread_create(name_cstr.as_ptr(), std::ptr::null()) };

        let ptr = NonNull::new(ptr)
            .ok_or_else(|| Error::EnvInit("spdk_thread_create returned NULL".to_string()))?;

        // Set as current thread for this OS thread
        unsafe {
            spdk_set_thread(ptr.as_ptr());
        }

        Ok(Self {
            ptr,
            _marker: PhantomData,
        })
    }

    /// Alias for [`current_with_mempool_size`](Self::current_with_mempool_size).
    #[inline]
    pub fn new_with_mempool_size(name: &str, msg_mempool_size: usize) -> Result<Self> {
        Self::current_with_mempool_size(name, msg_mempool_size)
    }

    /// Get the SPDK thread currently attached to this OS thread.
    ///
    /// Returns `None` if no thread is attached.
    pub fn get_current() -> Option<CurrentThread> {
        let ptr = unsafe { spdk_get_thread() };
        NonNull::new(ptr).map(|ptr| CurrentThread {
            ptr,
            _marker: PhantomData,
        })
    }

    /// Get the app thread (first thread created).
    ///
    /// Returns `None` if no threads have been created.
    pub fn app_thread() -> Option<CurrentThread> {
        let ptr = unsafe { spdk_thread_get_app_thread() };
        NonNull::new(ptr).map(|ptr| CurrentThread {
            ptr,
            _marker: PhantomData,
        })
    }

    /// Poll the thread to process messages and run pollers.
    ///
    /// Returns the number of events processed. If 0, consider yielding
    /// to other tasks before polling again.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use spdk_io::SpdkThread;
    /// # fn example(thread: &SpdkThread) {
    /// loop {
    ///     let work = thread.poll();
    ///     if work == 0 {
    ///         // Yield to async runtime
    ///         std::thread::yield_now();
    ///     }
    /// }
    /// # }
    /// ```
    pub fn poll(&self) -> i32 {
        unsafe { spdk_thread_poll(self.ptr.as_ptr(), 0, 0) }
    }

    /// Poll with a maximum number of messages to process.
    ///
    /// # Arguments
    ///
    /// * `max_msgs` - Maximum messages to process (0 = unlimited)
    pub fn poll_max(&self, max_msgs: u32) -> i32 {
        unsafe { spdk_thread_poll(self.ptr.as_ptr(), max_msgs, 0) }
    }

    /// Check if the thread has active pollers.
    pub fn has_active_pollers(&self) -> bool {
        unsafe { spdk_thread_has_active_pollers(self.ptr.as_ptr()) != 0 }
    }

    /// Check if the thread has any pollers (active or timed).
    pub fn has_pollers(&self) -> bool {
        unsafe { spdk_thread_has_pollers(self.ptr.as_ptr()) }
    }

    /// Check if the thread is idle (no work pending).
    pub fn is_idle(&self) -> bool {
        unsafe { spdk_thread_is_idle(self.ptr.as_ptr()) }
    }

    /// Check if the thread is running (not exited).
    pub fn is_running(&self) -> bool {
        unsafe { spdk_thread_is_running(self.ptr.as_ptr()) }
    }

    /// Get the thread name.
    pub fn name(&self) -> &str {
        unsafe {
            let ptr = spdk_thread_get_name(self.ptr.as_ptr());
            if ptr.is_null() {
                ""
            } else {
                std::ffi::CStr::from_ptr(ptr).to_str().unwrap_or("")
            }
        }
    }

    /// Get the thread ID.
    pub fn id(&self) -> u64 {
        unsafe { spdk_thread_get_id(self.ptr.as_ptr()) }
    }

    /// Get the total number of SPDK threads.
    pub fn count() -> u32 {
        unsafe { spdk_thread_get_count() }
    }

    /// Get the raw pointer to the underlying `spdk_thread`.
    ///
    /// # Safety
    ///
    /// The caller must ensure the pointer is not used after the thread is dropped.
    pub fn as_ptr(&self) -> *mut spdk_thread {
        self.ptr.as_ptr()
    }

    /// Spawn a new OS thread with an SPDK thread context.
    ///
    /// This creates a new OS thread and attaches a new SPDK thread to it.
    /// The closure receives a reference to the `SpdkThread` which can be
    /// used for polling and I/O operations.
    ///
    /// # Arguments
    ///
    /// * `name` - Name for both the OS thread and SPDK thread
    /// * `f` - Closure to run on the new thread
    ///
    /// # Returns
    ///
    /// A [`JoinHandle`] that can be used to wait for the thread to complete.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use spdk_io::{SpdkThread, spdk_poller};
    ///
    /// let handle = SpdkThread::spawn("worker", |thread| {
    ///     // Run a polling loop
    ///     for _ in 0..100 {
    ///         thread.poll();
    ///         std::thread::yield_now();
    ///     }
    ///     42 // Return value
    /// });
    ///
    /// let result = handle.join().unwrap();
    /// assert_eq!(result, 42);
    /// ```
    pub fn spawn<F, T>(name: &str, f: F) -> JoinHandle<T>
    where
        F: FnOnce(&SpdkThread) -> T + Send + 'static,
        T: Send + 'static,
    {
        let name_owned = name.to_string();

        let handle = thread::Builder::new()
            .name(name_owned.clone())
            .spawn(move || {
                // Create SPDK thread on this new OS thread
                // Use attach() - assumes thread lib is already initialized
                // (either by SpdkApp or explicit thread_lib_init call)
                let spdk_thread = SpdkThread::attach(&name_owned).expect(
                    "Failed to create SPDK thread in spawned thread. \
                             Is the thread library initialized (e.g., via SpdkApp)?",
                );

                // Run the user's function (spdk_thread is dropped after f returns)
                f(&spdk_thread)
            })
            .expect("Failed to spawn OS thread");

        JoinHandle { handle }
    }

    /// Get a thread-safe handle for cross-thread message passing.
    ///
    /// The returned [`ThreadHandle`] can be cloned and sent to other threads.
    /// Use it to dispatch work to this thread via [`ThreadHandle::send()`].
    pub fn handle(&self) -> ThreadHandle {
        ThreadHandle {
            ptr: self.ptr.as_ptr(),
        }
    }
}

impl Drop for SpdkThread {
    fn drop(&mut self) {
        unsafe {
            // Request thread exit
            spdk_thread_exit(self.ptr.as_ptr());

            // Poll until exited
            while !spdk_thread_is_exited(self.ptr.as_ptr()) {
                spdk_thread_poll(self.ptr.as_ptr(), 0, 0);
            }

            // Clear current thread
            spdk_set_thread(std::ptr::null_mut());

            // Destroy the thread
            spdk_thread_destroy(self.ptr.as_ptr());
        }

        // If this was the last thread, finalize the library
        if Self::count() == 0 {
            thread_lib_fini();
        }
    }
}

/// A borrowed reference to the current SPDK thread.
///
/// This is returned by [`SpdkThread::get_current()`] and does not own the thread.
/// It cannot be used to destroy the thread.
pub struct CurrentThread {
    ptr: NonNull<spdk_thread>,
    _marker: PhantomData<*mut ()>,
}

impl CurrentThread {
    /// Create a CurrentThread from a raw pointer.
    ///
    /// # Safety
    ///
    /// The pointer must be a valid `spdk_thread` pointer and must remain
    /// valid for the lifetime of the returned `CurrentThread`.
    #[inline]
    pub(crate) fn from_ptr(ptr: *mut spdk_thread) -> Self {
        Self {
            ptr: NonNull::new(ptr).expect("CurrentThread::from_ptr called with null"),
            _marker: PhantomData,
        }
    }

    /// Poll the thread.
    pub fn poll(&self) -> i32 {
        unsafe { spdk_thread_poll(self.ptr.as_ptr(), 0, 0) }
    }

    /// Get the thread name.
    pub fn name(&self) -> &str {
        unsafe {
            let ptr = spdk_thread_get_name(self.ptr.as_ptr());
            if ptr.is_null() {
                ""
            } else {
                std::ffi::CStr::from_ptr(ptr).to_str().unwrap_or("")
            }
        }
    }

    /// Get the thread ID.
    pub fn id(&self) -> u64 {
        unsafe { spdk_thread_get_id(self.ptr.as_ptr()) }
    }

    /// Get the raw pointer.
    pub fn as_ptr(&self) -> *mut spdk_thread {
        self.ptr.as_ptr()
    }
}

/// Handle to a spawned SPDK thread.
///
/// Returned by [`SpdkThread::spawn()`]. Use [`join()`](Self::join) to wait
/// for the thread to complete and get its result.
///
/// # Example
///
/// ```no_run
/// use spdk_io::SpdkThread;
///
/// let handle = SpdkThread::spawn("worker", |thread| {
///     for _ in 0..10 {
///         thread.poll();
///     }
///     "done"
/// });
///
/// let result = handle.join().unwrap();
/// assert_eq!(result, "done");
/// ```
pub struct JoinHandle<T> {
    handle: thread::JoinHandle<T>,
}

impl<T> JoinHandle<T> {
    /// Wait for the thread to finish and return its result.
    ///
    /// # Errors
    ///
    /// Returns `Err` if the thread panicked.
    pub fn join(self) -> Result<T> {
        self.handle.join().map_err(|_| Error::ThreadPanic)
    }

    /// Check if the thread has finished.
    pub fn is_finished(&self) -> bool {
        self.handle.is_finished()
    }

    /// Get the underlying OS thread handle.
    pub fn thread(&self) -> &thread::Thread {
        self.handle.thread()
    }
}

/// Thread-safe handle for sending messages to an SPDK thread.
///
/// Unlike [`SpdkThread`] (which is `!Send + !Sync`), this handle can be
/// cloned and sent across OS threads. Use it to dispatch closures to
/// execute on the target SPDK thread.
///
/// Internally uses `spdk_thread_send_msg()` which is thread-safe.
#[derive(Clone)]
pub struct ThreadHandle {
    ptr: *mut spdk_thread,
}

// SAFETY: spdk_thread_send_msg() is thread-safe
unsafe impl Send for ThreadHandle {}
unsafe impl Sync for ThreadHandle {}

impl ThreadHandle {
    /// Send a closure to execute on the target thread.
    ///
    /// Returns immediately. The closure will run when the target thread
    /// is next polled.
    pub fn send<F>(&self, f: F)
    where
        F: FnOnce() + Send + 'static,
    {
        // Box the closure and convert to raw pointer
        let boxed: Box<Box<dyn FnOnce() + Send>> = Box::new(Box::new(f));
        let ctx = Box::into_raw(boxed) as *mut c_void;

        unsafe {
            spdk_thread_send_msg(self.ptr, Some(msg_callback), ctx);
        }
    }

    /// Send a closure and await the result.
    ///
    /// This sends the closure to execute on the target thread and returns
    /// a future that resolves when the closure completes.
    ///
    /// # Panics
    ///
    /// Panics if the closure panics on the target thread.
    pub fn call<F, T>(&self, f: F) -> CompletionReceiver<T>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        let (tx, rx) = completion::<T>();

        self.send(move || {
            let result = f();
            tx.success(result);
        });

        rx
    }

    /// Get the target thread's ID.
    pub fn id(&self) -> u64 {
        unsafe { spdk_thread_get_id(self.ptr) }
    }

    /// Get the raw pointer to the target thread.
    pub fn as_ptr(&self) -> *mut spdk_thread {
        self.ptr
    }
}

/// Callback for spdk_thread_send_msg
unsafe extern "C" fn msg_callback(ctx: *mut c_void) {
    // Reconstruct the boxed closure
    let boxed: Box<Box<dyn FnOnce() + Send>> = unsafe { Box::from_raw(ctx as *mut _) };
    // Call the closure
    boxed();
}
