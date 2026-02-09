//! NVMe namespace.
//!
//! Represents a namespace on an NVMe controller.

use std::ffi::c_void;
use std::marker::PhantomData;
use std::ptr::NonNull;

use spdk_io_sys::*;

use crate::complete::{CompletionSender, completion};
use crate::dma::DmaBuf;
use crate::error::{Error, Result};

use super::controller::NvmeController;
use super::qpair::NvmeQpair;

/// NVMe namespace handle.
///
/// Represents a namespace on a controller. Obtained via
/// [`NvmeController::namespace()`].
///
/// # Lifetime
///
/// The namespace is borrowed from the controller and becomes invalid
/// when the controller is dropped.
///
/// # Example
///
/// ```no_run
/// use spdk_io::nvme::{NvmeController, TransportId};
/// use spdk_io::DmaBuf;
///
/// # async fn example() -> spdk_io::Result<()> {
/// let trid = TransportId::pcie("0000:00:04.0")?;
/// let ctrlr = NvmeController::connect(&trid, None)?;
///
/// let ns = ctrlr.namespace(1).expect("Namespace 1 not found");
/// println!("Sector size: {}", ns.sector_size());
/// println!("Num sectors: {}", ns.num_sectors());
///
/// let qpair = ctrlr.alloc_io_qpair(None)?;
/// let mut buf = DmaBuf::alloc(ns.sector_size() as usize, 4096)?;
///
/// ns.read(&qpair, &mut buf, 0, 1).await?;
/// # Ok(())
/// # }
/// ```
pub struct NvmeNamespace<'a> {
    ptr: NonNull<spdk_nvme_ns>,
    _ctrlr: PhantomData<&'a NvmeController>,
}

impl<'a> NvmeNamespace<'a> {
    /// Create from raw pointer (internal use).
    pub(crate) fn from_ptr(ptr: NonNull<spdk_nvme_ns>) -> Self {
        Self {
            ptr,
            _ctrlr: PhantomData,
        }
    }

    /// Get namespace ID.
    pub fn id(&self) -> u32 {
        unsafe { spdk_nvme_ns_get_id(self.ptr.as_ptr()) }
    }

    /// Get sector size in bytes.
    pub fn sector_size(&self) -> u32 {
        unsafe { spdk_nvme_ns_get_sector_size(self.ptr.as_ptr()) }
    }

    /// Get total number of sectors.
    pub fn num_sectors(&self) -> u64 {
        unsafe { spdk_nvme_ns_get_num_sectors(self.ptr.as_ptr()) }
    }

    /// Get total size in bytes.
    pub fn size(&self) -> u64 {
        self.num_sectors() * self.sector_size() as u64
    }

    /// Check if namespace is active.
    pub fn is_active(&self) -> bool {
        unsafe { spdk_nvme_ns_is_active(self.ptr.as_ptr()) }
    }

    /// Submit a read command.
    ///
    /// # Arguments
    ///
    /// * `qpair` - Queue pair for submission
    /// * `buf` - DMA buffer to read into (must be large enough for num_blocks * sector_size)
    /// * `lba` - Starting logical block address
    /// * `num_blocks` - Number of blocks to read
    ///
    /// # Example
    ///
    /// ```no_run
    /// # async fn example(ns: &spdk_io::nvme::NvmeNamespace<'_>, qpair: &spdk_io::nvme::NvmeQpair) -> spdk_io::Result<()> {
    /// use spdk_io::DmaBuf;
    ///
    /// let mut buf = DmaBuf::alloc(4096, 4096)?;
    /// ns.read(qpair, &mut buf, 0, 1).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn read(
        &self,
        qpair: &NvmeQpair,
        buf: &mut DmaBuf,
        lba: u64,
        num_blocks: u32,
    ) -> Result<()> {
        let (tx, rx) = completion();

        let rc = unsafe {
            spdk_nvme_ns_cmd_read(
                self.ptr.as_ptr(),
                qpair.as_ptr(),
                buf.as_mut_ptr() as *mut c_void,
                lba,
                num_blocks,
                Some(nvme_io_complete),
                tx.into_raw(),
                0, // io_flags
            )
        };

        if rc != 0 {
            return Err(Error::from_errno(-rc));
        }

        rx.await
    }

    /// Submit a write command.
    ///
    /// # Arguments
    ///
    /// * `qpair` - Queue pair for submission
    /// * `buf` - DMA buffer with data to write
    /// * `lba` - Starting logical block address
    /// * `num_blocks` - Number of blocks to write
    ///
    /// # Example
    ///
    /// ```no_run
    /// # async fn example(ns: &spdk_io::nvme::NvmeNamespace<'_>, qpair: &spdk_io::nvme::NvmeQpair) -> spdk_io::Result<()> {
    /// use spdk_io::DmaBuf;
    ///
    /// let mut buf = DmaBuf::alloc(4096, 4096)?;
    /// buf.as_mut_slice().fill(0xAB);
    /// ns.write(qpair, &buf, 0, 1).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn write(
        &self,
        qpair: &NvmeQpair,
        buf: &DmaBuf,
        lba: u64,
        num_blocks: u32,
    ) -> Result<()> {
        let (tx, rx) = completion();

        let rc = unsafe {
            spdk_nvme_ns_cmd_write(
                self.ptr.as_ptr(),
                qpair.as_ptr(),
                buf.as_ptr() as *mut c_void,
                lba,
                num_blocks,
                Some(nvme_io_complete),
                tx.into_raw(),
                0, // io_flags
            )
        };

        if rc != 0 {
            return Err(Error::from_errno(-rc));
        }

        rx.await
    }

    /// Get raw pointer (for internal use).
    #[allow(dead_code)]
    pub(crate) fn as_ptr(&self) -> *mut spdk_nvme_ns {
        self.ptr.as_ptr()
    }
}

/// C callback for NVMe I/O completion.
unsafe extern "C" fn nvme_io_complete(ctx: *mut c_void, cpl: *const spdk_nvme_cpl) {
    let tx = unsafe { CompletionSender::<()>::from_raw(ctx) };

    let cpl = unsafe { &*cpl };

    // Check if the command completed successfully
    // Access status_raw through the anonymous union
    // SCT (Status Code Type) is in bits 9:11, SC (Status Code) is in bits 1:8
    // A command is successful if both SCT and SC are 0
    let status_raw = unsafe { cpl.__bindgen_anon_1.status_raw };
    let sct = (status_raw >> 9) & 0x7;
    let sc = (status_raw >> 1) & 0xff;

    if sct == 0 && sc == 0 {
        tx.success(());
    } else {
        tx.error(Error::NvmeError {
            sct: sct as u8,
            sc: sc as u8,
        });
    }
}
