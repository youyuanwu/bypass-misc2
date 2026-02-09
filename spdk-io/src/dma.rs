//! DMA-capable buffer allocation.
//!
//! SPDK requires I/O buffers to be allocated from DMA-capable memory regions
//! (pinned, physically contiguous, and accessible by NVMe devices).
//!
//! # Example
//!
//! ```no_run
//! use spdk_io::DmaBuf;
//!
//! // Allocate a 4KB buffer aligned to 4KB (typical block size)
//! let mut buf = DmaBuf::alloc(4096, 4096).expect("allocation failed");
//!
//! // Write some data
//! buf.as_mut_slice()[..5].copy_from_slice(b"hello");
//!
//! // Use with bdev read/write operations
//! // ...
//! ```

use std::ptr::NonNull;

use spdk_io_sys::{spdk_dma_free, spdk_dma_malloc, spdk_dma_zmalloc};

use crate::error::{Error, Result};

/// A DMA-capable memory buffer for SPDK I/O operations.
///
/// Buffers are allocated via `spdk_dma_malloc()` which returns pinned,
/// physically contiguous memory suitable for DMA transfers to/from NVMe
/// devices.
///
/// # Thread Safety
///
/// `DmaBuf` is `Send` but not `Sync`. It can be moved between threads,
/// but cannot be shared across threads simultaneously (no interior mutability).
///
/// # Memory Layout
///
/// The buffer is always aligned to at least cache line size (64 bytes),
/// and can be further aligned as requested. For NVMe operations, alignment
/// to the block size (typically 512 or 4096) is recommended.
pub struct DmaBuf {
    ptr: NonNull<u8>,
    len: usize,
}

// DmaBuf is Send - can be moved between threads
// The underlying memory is just bytes, no thread-local state
unsafe impl Send for DmaBuf {}

// Not Sync - no interior mutability, so sharing requires external sync
// (which is fine since we provide &mut self for modifications)

impl DmaBuf {
    /// Allocate a DMA-capable buffer.
    ///
    /// # Arguments
    ///
    /// * `size` - Size in bytes to allocate
    /// * `align` - Alignment requirement (must be power of 2, or 0 for default).
    ///   The buffer will be aligned to at least cache line size (64 bytes).
    ///
    /// # Errors
    ///
    /// Returns [`Error::DmaAlloc`] if allocation fails (e.g., out of hugepage memory).
    ///
    /// # Example
    ///
    /// ```no_run
    /// use spdk_io::DmaBuf;
    ///
    /// // Allocate 4KB buffer aligned to 512 bytes (typical block size)
    /// let buf = DmaBuf::alloc(4096, 512)?;
    /// # Ok::<(), spdk_io::Error>(())
    /// ```
    pub fn alloc(size: usize, align: usize) -> Result<Self> {
        if size == 0 {
            return Err(Error::InvalidArgument("size must be > 0".to_string()));
        }

        let ptr = unsafe { spdk_dma_malloc(size, align, std::ptr::null_mut()) };

        NonNull::new(ptr as *mut u8)
            .map(|ptr| Self { ptr, len: size })
            .ok_or(Error::DmaAlloc(size))
    }

    /// Allocate a zeroed DMA-capable buffer.
    ///
    /// Same as [`alloc`](Self::alloc) but the memory is zeroed.
    ///
    /// # Arguments
    ///
    /// * `size` - Size in bytes to allocate
    /// * `align` - Alignment requirement (must be power of 2, or 0 for default)
    ///
    /// # Errors
    ///
    /// Returns [`Error::DmaAlloc`] if allocation fails.
    pub fn alloc_zeroed(size: usize, align: usize) -> Result<Self> {
        if size == 0 {
            return Err(Error::InvalidArgument("size must be > 0".to_string()));
        }

        let ptr = unsafe { spdk_dma_zmalloc(size, align, std::ptr::null_mut()) };

        NonNull::new(ptr as *mut u8)
            .map(|ptr| Self { ptr, len: size })
            .ok_or(Error::DmaAlloc(size))
    }

    /// Get the buffer length in bytes.
    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Check if the buffer is empty (zero length).
    ///
    /// Note: Zero-length buffers cannot be created via `alloc()`.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Get mutable raw pointer to the buffer.
    #[inline]
    pub fn as_mut_ptr(&mut self) -> *mut u8 {
        self.ptr.as_ptr()
    }

    /// Get raw pointer to the buffer.
    #[inline]
    pub fn as_ptr(&self) -> *const u8 {
        self.ptr.as_ptr()
    }

    /// Get an immutable slice view of the buffer.
    #[inline]
    pub fn as_slice(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.ptr.as_ptr(), self.len) }
    }

    /// Get a mutable slice view of the buffer.
    #[inline]
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.ptr.as_ptr(), self.len) }
    }
}

impl Drop for DmaBuf {
    fn drop(&mut self) {
        unsafe {
            spdk_dma_free(self.ptr.as_ptr() as *mut std::ffi::c_void);
        }
    }
}

impl std::fmt::Debug for DmaBuf {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DmaBuf")
            .field("ptr", &self.ptr)
            .field("len", &self.len)
            .finish()
    }
}

impl AsRef<[u8]> for DmaBuf {
    fn as_ref(&self) -> &[u8] {
        self.as_slice()
    }
}

impl AsMut<[u8]> for DmaBuf {
    fn as_mut(&mut self) -> &mut [u8] {
        self.as_mut_slice()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zero_size_fails() {
        assert!(DmaBuf::alloc(0, 0).is_err());
        assert!(DmaBuf::alloc_zeroed(0, 0).is_err());
    }
}
