//! NVMe I/O queue pair.
//!
//! Queue pairs are used to submit I/O commands to namespaces.

use std::marker::PhantomData;
use std::ptr::NonNull;

use spdk_io_sys::*;

/// NVMe I/O queue pair.
///
/// Used to submit I/O commands to a namespace. Each thread should
/// have its own qpair for lock-free operation.
///
/// # Thread Safety
///
/// `!Send + !Sync` - qpair must stay on the allocating thread.
///
/// # Example
///
/// ```no_run
/// use spdk_io::nvme::{NvmeController, TransportId};
///
/// # fn example() -> spdk_io::Result<()> {
/// let trid = TransportId::pcie("0000:00:04.0")?;
/// let ctrlr = NvmeController::connect(&trid, None)?;
/// let qpair = ctrlr.alloc_io_qpair(None)?;
///
/// // Use qpair for I/O operations
/// let completions = qpair.process_completions(0);
/// # Ok(())
/// # }
/// ```
pub struct NvmeQpair {
    pub(crate) ptr: NonNull<spdk_nvme_qpair>,
    _marker: PhantomData<*mut ()>, // !Send + !Sync
}

impl NvmeQpair {
    /// Create from raw pointer (internal use).
    pub(crate) fn from_ptr(ptr: NonNull<spdk_nvme_qpair>) -> Self {
        Self {
            ptr,
            _marker: PhantomData,
        }
    }

    /// Process I/O completions.
    ///
    /// Call this to check for completed I/O operations. Returns the
    /// number of completions processed.
    ///
    /// # Arguments
    ///
    /// * `max_completions` - Max completions to process (0 = unlimited)
    ///
    /// # Returns
    ///
    /// Number of completions processed, or negative error code.
    pub fn process_completions(&self, max_completions: u32) -> i32 {
        unsafe { spdk_nvme_qpair_process_completions(self.ptr.as_ptr(), max_completions) }
    }

    /// Get raw pointer (for internal use).
    pub(crate) fn as_ptr(&self) -> *mut spdk_nvme_qpair {
        self.ptr.as_ptr()
    }
}

impl Drop for NvmeQpair {
    fn drop(&mut self) {
        unsafe {
            spdk_nvme_ctrlr_free_io_qpair(self.ptr.as_ptr());
        }
    }
}
