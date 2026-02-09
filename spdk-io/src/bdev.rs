//! Block Device (Bdev) API
//!
//! This module provides wrappers for SPDK's block device layer.
//!
//! # Creating Bdevs
//!
//! Bdevs are typically created via JSON configuration at SPDK init time
//! using [`SpdkApp`](crate::SpdkApp). For example:
//!
//! ```json
//! {
//!   "subsystems": [{
//!     "subsystem": "bdev",
//!     "config": [{
//!       "method": "bdev_null_create",
//!       "params": {"name": "Null0", "num_blocks": 262144, "block_size": 512}
//!     }]
//!   }]
//! }
//! ```
//!
//! # Example
//!
//! ```no_run
//! use spdk_io::{SpdkApp, Bdev};
//!
//! SpdkApp::builder()
//!     .name("my_app")
//!     .config_file("./config.json")
//!     .run(|| {
//!         // Bdev was created via JSON config
//!         let bdev = Bdev::get_by_name("Null0").unwrap();
//!         println!("Block size: {}", bdev.block_size());
//!         
//!         // Open for I/O
//!         let desc = bdev.open(true).unwrap();
//!         let channel = desc.get_io_channel().unwrap();
//!         
//!         SpdkApp::stop();
//!     })
//!     .unwrap();
//! ```

use std::ffi::{CStr, CString, c_void};
use std::marker::PhantomData;
use std::ptr::NonNull;

use spdk_io_sys::*;

use crate::channel::IoChannel;
use crate::complete::{CompletionSender, completion};
use crate::dma::DmaBuf;
use crate::error::{Error, Result};

/// Block device handle.
///
/// Obtained via [`Bdev::get_by_name()`] after bdevs are created via JSON config.
/// The bdev itself is managed by SPDK's bdev layer - this is just a handle.
///
/// # Thread Safety
///
/// `!Send + !Sync` - conservative default. While `spdk_bdev` itself may be
/// thread-safe, we keep this restriction until we verify all usage patterns.
pub struct Bdev {
    ptr: NonNull<spdk_bdev>,
    _marker: PhantomData<*mut ()>, // !Send + !Sync
}

impl Bdev {
    /// Look up a bdev by name.
    ///
    /// Returns `None` if no bdev with that name exists.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use spdk_io::Bdev;
    ///
    /// if let Some(bdev) = Bdev::get_by_name("Null0") {
    ///     println!("Found bdev: {}", bdev.name());
    /// }
    /// ```
    pub fn get_by_name(name: &str) -> Option<Self> {
        let name_cstr = CString::new(name).ok()?;
        let ptr = unsafe { spdk_bdev_get_by_name(name_cstr.as_ptr()) };
        NonNull::new(ptr).map(|ptr| Self {
            ptr,
            _marker: PhantomData,
        })
    }

    /// Get the bdev name.
    pub fn name(&self) -> &str {
        unsafe {
            let name_ptr = spdk_bdev_get_name(self.ptr.as_ptr());
            CStr::from_ptr(name_ptr)
                .to_str()
                .unwrap_or("<invalid utf8>")
        }
    }

    /// Get the block size in bytes.
    pub fn block_size(&self) -> u32 {
        unsafe { spdk_bdev_get_block_size(self.ptr.as_ptr()) }
    }

    /// Get the number of blocks.
    pub fn num_blocks(&self) -> u64 {
        unsafe { spdk_bdev_get_num_blocks(self.ptr.as_ptr()) }
    }

    /// Get the total size in bytes.
    pub fn size_bytes(&self) -> u64 {
        self.block_size() as u64 * self.num_blocks()
    }

    /// Open this bdev for I/O operations.
    ///
    /// # Arguments
    ///
    /// * `write` - If true, open for read/write access. If false, read-only.
    ///
    /// # Errors
    ///
    /// Returns an error if the bdev cannot be opened (e.g., already claimed
    /// exclusively by another module).
    pub fn open(&self, write: bool) -> Result<BdevDesc> {
        let name_cstr = CString::new(self.name())?;
        let mut desc: *mut spdk_bdev_desc = std::ptr::null_mut();

        let rc = unsafe {
            spdk_bdev_open_ext(
                name_cstr.as_ptr(),
                write,
                Some(bdev_event_callback),
                std::ptr::null_mut(), // No event context for now
                &mut desc,
            )
        };

        if rc != 0 {
            return Err(Error::Os(rc));
        }

        NonNull::new(desc)
            .map(|ptr| BdevDesc {
                ptr,
                _marker: PhantomData,
            })
            .ok_or(Error::InvalidArgument("null descriptor".into()))
    }

    /// Get the raw pointer.
    ///
    /// # Safety
    ///
    /// The returned pointer is valid as long as the bdev exists in SPDK.
    pub fn as_ptr(&self) -> *mut spdk_bdev {
        self.ptr.as_ptr()
    }

    /// Create a Bdev from a raw pointer.
    ///
    /// # Safety
    ///
    /// The pointer must be valid and point to a registered bdev.
    pub unsafe fn from_ptr(ptr: *mut spdk_bdev) -> Option<Self> {
        NonNull::new(ptr).map(|ptr| Self {
            ptr,
            _marker: PhantomData,
        })
    }
}

// Bdev is Copy since it's just a handle (pointer) to SPDK-managed data
impl Copy for Bdev {}

impl Clone for Bdev {
    fn clone(&self) -> Self {
        *self
    }
}

/// Open descriptor to a bdev (like a file descriptor).
///
/// Use [`get_io_channel()`](BdevDesc::get_io_channel) to obtain a thread-local
/// channel for I/O operations.
///
/// # Thread Safety
///
/// `!Send + !Sync` - the descriptor must be closed on the same thread it was
/// opened on.
///
/// # Drop
///
/// Automatically closes the descriptor when dropped.
pub struct BdevDesc {
    ptr: NonNull<spdk_bdev_desc>,
    _marker: PhantomData<*mut ()>, // !Send + !Sync
}

impl BdevDesc {
    /// Get an I/O channel for this descriptor on the current thread.
    ///
    /// I/O channels are per-thread and must be used on the thread that
    /// created them.
    pub fn get_io_channel(&self) -> Result<IoChannel> {
        let ch = unsafe { spdk_bdev_get_io_channel(self.ptr.as_ptr()) };
        NonNull::new(ch)
            .map(IoChannel::from_ptr)
            .ok_or(Error::ChannelAlloc)
    }

    /// Get the underlying bdev.
    pub fn bdev(&self) -> Bdev {
        let bdev_ptr = unsafe { spdk_bdev_desc_get_bdev(self.ptr.as_ptr()) };
        Bdev {
            ptr: NonNull::new(bdev_ptr).expect("descriptor has null bdev"),
            _marker: PhantomData,
        }
    }

    /// Get the raw pointer.
    pub fn as_ptr(&self) -> *mut spdk_bdev_desc {
        self.ptr.as_ptr()
    }

    /// Read data from the bdev.
    ///
    /// Reads `buf.len()` bytes from the specified byte offset into the buffer.
    ///
    /// # Arguments
    ///
    /// * `channel` - I/O channel obtained from [`get_io_channel()`](Self::get_io_channel)
    /// * `buf` - DMA buffer to read into (must be allocated via [`DmaBuf::alloc()`])
    /// * `offset` - Byte offset to start reading from
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The read submission fails (e.g., invalid offset/length)
    /// - The I/O operation fails
    ///
    /// # Example
    ///
    /// ```no_run
    /// use spdk_io::{Bdev, DmaBuf};
    ///
    /// # async fn example() -> spdk_io::Result<()> {
    /// let bdev = Bdev::get_by_name("Null0").unwrap();
    /// let desc = bdev.open(false)?;
    /// let channel = desc.get_io_channel()?;
    ///
    /// let mut buf = DmaBuf::alloc(4096, 512)?;
    /// desc.read(&channel, &mut buf, 0).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn read(&self, channel: &IoChannel, buf: &mut DmaBuf, offset: u64) -> Result<()> {
        let (tx, rx) = completion::<()>();

        let rc = unsafe {
            spdk_bdev_read(
                self.ptr.as_ptr(),
                channel.as_ptr(),
                buf.as_mut_ptr() as *mut c_void,
                offset,
                buf.len() as u64,
                Some(bdev_io_completion_cb),
                tx.into_raw(),
            )
        };

        if rc != 0 {
            return Err(Error::Os(rc));
        }

        rx.await
    }

    /// Write data to the bdev.
    ///
    /// Writes `buf.len()` bytes from the buffer to the specified byte offset.
    ///
    /// # Arguments
    ///
    /// * `channel` - I/O channel obtained from [`get_io_channel()`](Self::get_io_channel)
    /// * `buf` - DMA buffer containing data to write
    /// * `offset` - Byte offset to start writing at
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The bdev was opened read-only
    /// - The write submission fails (e.g., invalid offset/length)
    /// - The I/O operation fails
    ///
    /// # Example
    ///
    /// ```no_run
    /// use spdk_io::{Bdev, DmaBuf};
    ///
    /// # async fn example() -> spdk_io::Result<()> {
    /// let bdev = Bdev::get_by_name("Null0").unwrap();
    /// let desc = bdev.open(true)?;  // Open for write
    /// let channel = desc.get_io_channel()?;
    ///
    /// let mut buf = DmaBuf::alloc_zeroed(4096, 512)?;
    /// buf.as_mut_slice()[..5].copy_from_slice(b"hello");
    /// desc.write(&channel, &buf, 0).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn write(&self, channel: &IoChannel, buf: &DmaBuf, offset: u64) -> Result<()> {
        let (tx, rx) = completion::<()>();

        let rc = unsafe {
            spdk_bdev_write(
                self.ptr.as_ptr(),
                channel.as_ptr(),
                buf.as_ptr() as *mut c_void,
                offset,
                buf.len() as u64,
                Some(bdev_io_completion_cb),
                tx.into_raw(),
            )
        };

        if rc != 0 {
            return Err(Error::Os(rc));
        }

        rx.await
    }
}

impl Drop for BdevDesc {
    fn drop(&mut self) {
        // Must be called on the same thread as open
        unsafe { spdk_bdev_close(self.ptr.as_ptr()) };
    }
}

/// Bdev I/O completion callback.
///
/// Called by SPDK when a read/write operation completes.
/// Frees the bdev_io and signals the completion.
unsafe extern "C" fn bdev_io_completion_cb(
    bdev_io: *mut spdk_bdev_io,
    success: bool,
    cb_arg: *mut c_void,
) {
    // SAFETY: bdev_io is valid and must be freed after use
    unsafe { spdk_bdev_free_io(bdev_io) };

    // SAFETY: cb_arg was created by CompletionSender::into_raw()
    let tx = unsafe { CompletionSender::<()>::from_raw(cb_arg) };
    if success {
        tx.success(());
    } else {
        tx.error(Error::IoError);
    }
}

/// Bdev event callback (currently a no-op).
///
/// This callback receives notifications about bdev events like removal.
/// For now we just log and ignore.
#[allow(non_upper_case_globals)]
extern "C" fn bdev_event_callback(
    event_type: spdk_bdev_event_type,
    bdev: *mut spdk_bdev,
    _event_ctx: *mut c_void,
) {
    // Get bdev name for logging
    let name = if !bdev.is_null() {
        unsafe {
            CStr::from_ptr(spdk_bdev_get_name(bdev))
                .to_str()
                .unwrap_or("<unknown>")
        }
    } else {
        "<null>"
    };

    match event_type {
        spdk_bdev_event_type_SPDK_BDEV_EVENT_REMOVE => {
            // Bdev is being removed
            // In a real application, we'd notify the user to close descriptors
            eprintln!("bdev event: {} is being removed", name);
        }
        spdk_bdev_event_type_SPDK_BDEV_EVENT_RESIZE => {
            eprintln!("bdev event: {} was resized", name);
        }
        spdk_bdev_event_type_SPDK_BDEV_EVENT_MEDIA_MANAGEMENT => {
            eprintln!("bdev event: {} media management", name);
        }
        _ => {
            eprintln!("bdev event: {} unknown event {}", name, event_type);
        }
    }
}
