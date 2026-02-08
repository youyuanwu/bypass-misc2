//! I/O channel management for SPDK
//!
//! An [`IoChannel`] is a per-thread handle for submitting I/O to a device.
//! Channels are bound to the OS thread that created them and must not be
//! moved to other threads.
//!
//! # Thread Safety
//!
//! `IoChannel` is `!Send` and `!Sync` - it must stay on the OS thread
//! that created it. This is enforced at compile time via `PhantomData<*mut ()>`.
//!
//! # Obtaining Channels
//!
//! Channels are obtained from device-specific methods, not from `SpdkThread`:
//! - `BdevDesc::get_io_channel()` - for block devices
//! - `Blobstore::alloc_io_channel()` - for blobstore
//!
//! SPDK internally reference-counts channels. Calling `get_io_channel()` twice
//! on the same thread returns the same channel (with incremented refcount).

use std::marker::PhantomData;
use std::ptr::NonNull;

use spdk_io_sys::*;

use crate::thread::CurrentThread;

/// Per-thread I/O channel for submitting I/O operations.
///
/// Must be created and used on the same OS thread. The channel is automatically
/// released when dropped via `spdk_put_io_channel()`.
///
/// # Thread Safety
///
/// `!Send + !Sync` - must stay on the creating OS thread.
///
/// # Example
///
/// ```no_run
/// use spdk_io::{SpdkEnv, SpdkThread};
///
/// // Channels are obtained from device descriptors:
/// // let channel = bdev_desc.get_io_channel()?;
/// // desc.read(&channel, &mut buf, offset, len).await?;
/// ```
pub struct IoChannel {
    ptr: NonNull<spdk_io_channel>,
    /// Prevent Send/Sync - channel is bound to creating thread
    _marker: PhantomData<*mut ()>,
}

impl IoChannel {
    /// Create an IoChannel from a NonNull pointer.
    ///
    /// # Safety
    ///
    /// The caller must ensure:
    /// - `ptr` is a valid pointer returned by `spdk_bdev_get_io_channel()`,
    ///   `spdk_bs_alloc_io_channel()`, or similar SPDK function
    /// - The channel is being used on the same thread that created it
    /// - The channel has not been released via `spdk_put_io_channel()`
    #[inline]
    pub(crate) fn from_ptr(ptr: NonNull<spdk_io_channel>) -> Self {
        Self {
            ptr,
            _marker: PhantomData,
        }
    }

    /// Get the SPDK thread this channel is bound to.
    ///
    /// Returns a borrowed reference to the thread that owns this channel.
    pub fn thread(&self) -> Option<CurrentThread> {
        let ptr = unsafe { spdk_io_channel_get_thread(self.ptr.as_ptr()) };
        if ptr.is_null() {
            None
        } else {
            // Safety: spdk_io_channel_get_thread returns the thread that owns
            // this channel, which must be the current thread (since IoChannel
            // is !Send). We're just borrowing a reference.
            Some(CurrentThread::from_ptr(ptr))
        }
    }

    /// Get the raw pointer to the underlying `spdk_io_channel`.
    ///
    /// # Safety
    ///
    /// The caller must ensure the pointer is not used after the channel is dropped.
    #[inline]
    pub fn as_ptr(&self) -> *mut spdk_io_channel {
        self.ptr.as_ptr()
    }
}

impl Drop for IoChannel {
    fn drop(&mut self) {
        // Safety: We own this channel reference and it hasn't been released yet.
        // spdk_put_io_channel() decrements the refcount and may destroy the
        // channel if this was the last reference.
        unsafe {
            spdk_put_io_channel(self.ptr.as_ptr());
        }
    }
}

// IoChannel is !Send and !Sync due to PhantomData<*mut ()>
