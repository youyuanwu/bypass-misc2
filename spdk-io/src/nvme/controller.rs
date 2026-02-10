//! NVMe controller.
//!
//! Controller management and connection.

use std::ffi::c_void;
use std::mem::MaybeUninit;
use std::ptr::NonNull;

use spdk_io_sys::*;

use crate::error::{Error, Result};
use crate::thread::SpdkThread;

use super::namespace::NvmeNamespace;
use super::opts::{NvmeCtrlrOpts, NvmeQpairOpts};
use super::qpair::NvmeQpair;
use super::transport::TransportId;

/// NVMe controller handle.
///
/// Represents a connected NVMe controller. Obtained via [`connect()`](Self::connect).
///
/// # Thread Safety
///
/// `Send + Sync` - The controller can be shared across threads via `Arc<NvmeController>`.
/// Thread-safe operations use `&self` and are protected by SPDK's internal mutex:
/// - [`alloc_io_qpair()`](Self::alloc_io_qpair)
/// - [`namespace()`](Self::namespace)
/// - [`num_namespaces()`](Self::num_namespaces)
///
/// Non-thread-safe operations require `&mut self`, enforcing single-threaded access:
/// - [`process_admin_completions()`](Self::process_admin_completions)
///
/// Each [`NvmeQpair`] remains `!Send` and must stay on the thread that allocated it.
///
/// # Multi-Core I/O Pattern
///
/// ```ignore
/// use std::sync::Arc;
///
/// // Core 0: Connect and share via Arc
/// let ctrlr = Arc::new(NvmeController::connect(&trid, None)?);
/// let qpair0 = ctrlr.alloc_io_qpair(None)?;  // &self, thread-safe
///
/// let ctrlr_clone = ctrlr.clone();
/// SpdkEvent::call_on(1, move || {
///     let qpair1 = ctrlr_clone.alloc_io_qpair(None).unwrap();  // &self, thread-safe
///     // qpair1 stays on core 1
/// })?;
/// ```
///
/// # Example
///
/// ```no_run
/// use spdk_io::nvme::{NvmeController, TransportId};
///
/// # fn example() -> spdk_io::Result<()> {
/// let trid = TransportId::pcie("0000:00:04.0")?;
/// let ctrlr = NvmeController::connect(&trid, None)?;
///
/// println!("Controller has {} namespaces", ctrlr.num_namespaces());
///
/// if let Some(ns) = ctrlr.namespace(1) {
///     println!("NS1: {} sectors, {} bytes/sector",
///              ns.num_sectors(), ns.sector_size());
/// }
/// # Ok(())
/// # }
/// ```
pub struct NvmeController {
    ptr: NonNull<spdk_nvme_ctrlr>,
}

// SAFETY: spdk_nvme_ctrlr has internal mutex (ctrlr_lock) protecting most operations.
// Thread-safe operations (alloc_io_qpair, namespace, etc.) use &self.
// Non-thread-safe operations (process_admin_completions) require &mut self,
// which Rust's borrow checker enforces as single-threaded access.
unsafe impl Send for NvmeController {}
unsafe impl Sync for NvmeController {}

impl NvmeController {
    /// Connect to an NVMe controller.
    ///
    /// This is the synchronous connect API.
    ///
    /// # Arguments
    ///
    /// * `trid` - Transport identifier (PCIe, TCP, RDMA)
    /// * `opts` - Optional controller options (queue depth, etc.)
    ///
    /// # Errors
    ///
    /// Returns error if connection fails (device not found, permission denied, etc.)
    ///
    /// # Example
    ///
    /// ```no_run
    /// use spdk_io::nvme::{NvmeController, TransportId};
    ///
    /// # fn example() -> spdk_io::Result<()> {
    /// let trid = TransportId::pcie("0000:00:04.0")?;
    /// let ctrlr = NvmeController::connect(&trid, None)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn connect(trid: &TransportId, opts: Option<&NvmeCtrlrOpts>) -> Result<Self> {
        let opts_ptr = match opts {
            Some(opts) => {
                let mut native_opts: spdk_nvme_ctrlr_opts =
                    unsafe { MaybeUninit::zeroed().assume_init() };

                // Initialize with defaults
                unsafe {
                    spdk_nvme_ctrlr_get_default_ctrlr_opts(
                        &mut native_opts,
                        std::mem::size_of::<spdk_nvme_ctrlr_opts>(),
                    );
                }

                // Apply user options
                if let Some(num_io_queues) = opts.num_io_queues {
                    native_opts.num_io_queues = num_io_queues;
                }
                if let Some(io_queue_size) = opts.io_queue_size {
                    native_opts.io_queue_size = io_queue_size;
                }
                if let Some(admin_queue_size) = opts.admin_queue_size {
                    native_opts.admin_queue_size = admin_queue_size;
                }
                if let Some(keep_alive_timeout_ms) = opts.keep_alive_timeout_ms {
                    native_opts.keep_alive_timeout_ms = keep_alive_timeout_ms;
                }

                &native_opts as *const _
            }
            None => std::ptr::null(),
        };

        let ctrlr = unsafe { spdk_nvme_connect(trid.as_ptr(), opts_ptr, 0) };

        NonNull::new(ctrlr)
            .map(|ptr| Self { ptr })
            .ok_or(Error::ControllerNotFound)
    }

    /// Connect to an NVMe controller asynchronously.
    ///
    /// This version polls the SPDK thread between connection attempts,
    /// allowing in-process NVMf targets to process incoming connections.
    ///
    /// # Arguments
    ///
    /// * `trid` - Transport identifier (PCIe, TCP, RDMA)
    /// * `opts` - Optional controller options
    pub fn connect_async(trid: &TransportId, opts: Option<&NvmeCtrlrOpts>) -> Result<Self> {
        use std::cell::RefCell;

        // Thread-local to capture the attached controller (since connect_async doesn't pass cb_ctx)
        thread_local! {
            static ATTACHED_CTRLR: RefCell<Option<*mut spdk_nvme_ctrlr>> = const { RefCell::new(None) };
        }

        unsafe extern "C" fn attach_cb(
            _cb_ctx: *mut c_void,
            _trid: *const spdk_nvme_transport_id,
            ctrlr: *mut spdk_nvme_ctrlr,
            _opts: *const spdk_nvme_ctrlr_opts,
        ) {
            ATTACHED_CTRLR.with(|cell| {
                *cell.borrow_mut() = Some(ctrlr);
            });
        }

        // Clear any previous result
        ATTACHED_CTRLR.with(|cell| *cell.borrow_mut() = None);

        let opts_ptr = match opts {
            Some(opts) => {
                let mut native_opts: spdk_nvme_ctrlr_opts =
                    unsafe { MaybeUninit::zeroed().assume_init() };
                unsafe {
                    spdk_nvme_ctrlr_get_default_ctrlr_opts(
                        &mut native_opts,
                        std::mem::size_of::<spdk_nvme_ctrlr_opts>(),
                    );
                }
                if let Some(num_io_queues) = opts.num_io_queues {
                    native_opts.num_io_queues = num_io_queues;
                }
                if let Some(io_queue_size) = opts.io_queue_size {
                    native_opts.io_queue_size = io_queue_size;
                }
                if let Some(admin_queue_size) = opts.admin_queue_size {
                    native_opts.admin_queue_size = admin_queue_size;
                }
                if let Some(keep_alive_timeout_ms) = opts.keep_alive_timeout_ms {
                    native_opts.keep_alive_timeout_ms = keep_alive_timeout_ms;
                }
                &native_opts as *const _
            }
            None => std::ptr::null(),
        };

        // Start async connect
        let probe_ctx =
            unsafe { spdk_nvme_connect_async(trid.as_ptr(), opts_ptr, Some(attach_cb)) };

        if probe_ctx.is_null() {
            return Err(Error::ControllerNotFound);
        }

        // Get current SPDK thread for polling
        let thread = SpdkThread::get_current().ok_or(Error::Os(22))?; // EINVAL

        // Poll until connection completes
        const EAGAIN: i32 = 11; // EAGAIN on Linux
        loop {
            let rc = unsafe { spdk_nvme_probe_poll_async(probe_ctx) };
            if rc == 0 {
                // Done - check if we got a controller
                break;
            } else if rc == -EAGAIN {
                // Still pending - poll the thread and continue
                thread.poll();
            } else {
                // Error
                return Err(Error::from_errno(-rc));
            }
        }

        ATTACHED_CTRLR
            .with(|cell| cell.borrow_mut().take())
            .and_then(NonNull::new)
            .map(|ptr| Self { ptr })
            .ok_or(Error::ControllerNotFound)
    }

    /// Create an NvmeController from a raw pointer.
    ///
    /// # Safety
    ///
    /// The pointer must be a valid, non-null `spdk_nvme_ctrlr` pointer
    /// obtained from `spdk_nvme_connect` or similar. The caller transfers
    /// ownership to this struct.
    pub unsafe fn from_raw(ptr: *mut spdk_nvme_ctrlr) -> Self {
        Self {
            ptr: unsafe { NonNull::new_unchecked(ptr) },
        }
    }

    /// Get the number of namespaces.
    ///
    /// Note: Some namespace IDs may be inactive.
    pub fn num_namespaces(&self) -> u32 {
        unsafe { spdk_nvme_ctrlr_get_num_ns(self.ptr.as_ptr()) }
    }

    /// Get a namespace by ID (1-indexed).
    ///
    /// Returns `None` if the namespace ID is invalid or inactive.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # fn example(ctrlr: &spdk_io::nvme::NvmeController) {
    /// if let Some(ns) = ctrlr.namespace(1) {
    ///     println!("NS1 sector size: {}", ns.sector_size());
    /// }
    /// # }
    /// ```
    pub fn namespace(&self, ns_id: u32) -> Option<NvmeNamespace<'_>> {
        if ns_id == 0 || ns_id > self.num_namespaces() {
            return None;
        }

        let ns_ptr = unsafe { spdk_nvme_ctrlr_get_ns(self.ptr.as_ptr(), ns_id) };

        NonNull::new(ns_ptr).map(|ptr| {
            let ns = NvmeNamespace::from_ptr(ptr);
            if ns.is_active() { Some(ns) } else { None }
        })?
    }

    /// Allocate an I/O queue pair for submitting commands.
    ///
    /// Each thread should have its own qpair for lock-free I/O.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # fn example(ctrlr: &spdk_io::nvme::NvmeController) -> spdk_io::Result<()> {
    /// let qpair = ctrlr.alloc_io_qpair(None)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn alloc_io_qpair(&self, opts: Option<&NvmeQpairOpts>) -> Result<NvmeQpair> {
        let opts_ptr = match opts {
            Some(opts) => {
                let mut native_opts: spdk_nvme_io_qpair_opts =
                    unsafe { MaybeUninit::zeroed().assume_init() };

                // Initialize with defaults
                unsafe {
                    spdk_nvme_ctrlr_get_default_io_qpair_opts(
                        self.ptr.as_ptr(),
                        &mut native_opts,
                        std::mem::size_of::<spdk_nvme_io_qpair_opts>(),
                    );
                }

                if let Some(io_queue_size) = opts.io_queue_size {
                    native_opts.io_queue_size = io_queue_size;
                }
                if let Some(io_queue_requests) = opts.io_queue_requests {
                    native_opts.io_queue_requests = io_queue_requests;
                }

                &native_opts as *const _
            }
            None => std::ptr::null(),
        };

        let qpair = unsafe { spdk_nvme_ctrlr_alloc_io_qpair(self.ptr.as_ptr(), opts_ptr, 0) };

        NonNull::new(qpair)
            .map(NvmeQpair::from_ptr)
            .ok_or(Error::QpairAlloc)
    }

    /// Process admin command completions.
    ///
    /// Call periodically to process admin command responses and keep-alive.
    ///
    /// # Thread Safety
    ///
    /// Requires `&mut self` because this operation is NOT thread-safe.
    /// Only one thread may call this at a time.
    pub fn process_admin_completions(&mut self) -> i32 {
        unsafe { spdk_nvme_ctrlr_process_admin_completions(self.ptr.as_ptr()) }
    }

    /// Get raw pointer (for internal use).
    #[allow(dead_code)]
    pub(crate) fn as_ptr(&self) -> *mut spdk_nvme_ctrlr {
        self.ptr.as_ptr()
    }
}

impl Drop for NvmeController {
    fn drop(&mut self) {
        // Detach from controller
        unsafe {
            spdk_nvme_detach(self.ptr.as_ptr());
        }
    }
}
