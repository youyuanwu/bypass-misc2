//! NVMf transport management.

use std::ffi::CString;
use std::marker::PhantomData;
use std::ptr::NonNull;

use spdk_io_sys::*;

use crate::error::{Error, Result};

use super::opts::NvmfTransportOpts;

/// Size of spdk_nvmf_transport_opts from SPDK headers.
/// SPDK_STATIC_ASSERT(sizeof(struct spdk_nvmf_transport_opts) == 82, "Incorrect size");
const TRANSPORT_OPTS_SIZE: usize = 82;

/// NVMf transport (TCP, RDMA, etc.)
///
/// Transports handle the network communication for NVMe-oF.
/// Create using [`tcp()`](Self::tcp) or [`rdma()`](Self::rdma),
/// then add to a target via [`NvmfTarget::add_transport()`].
pub struct NvmfTransport {
    ptr: NonNull<spdk_nvmf_transport>,
    _marker: PhantomData<*mut ()>,
}

impl NvmfTransport {
    /// Create a TCP transport with default options.
    pub fn tcp(opts: Option<&NvmfTransportOpts>) -> Result<Self> {
        Self::create("TCP", opts)
    }

    /// Create an RDMA transport with default options.
    pub fn rdma(opts: Option<&NvmfTransportOpts>) -> Result<Self> {
        Self::create("RDMA", opts)
    }

    /// Create a transport by name.
    fn create(transport_name: &str, _opts: Option<&NvmfTransportOpts>) -> Result<Self> {
        let name_cstr = CString::new(transport_name)?;

        // Allocate raw bytes for the opaque transport_opts struct
        // (we can't use the Rust struct directly due to repr(packed) + repr(align) conflict)
        let mut opts_bytes = [0u8; TRANSPORT_OPTS_SIZE];
        let opts_ptr = opts_bytes.as_mut_ptr() as *mut spdk_nvmf_transport_opts;

        // Initialize with defaults
        let ok = unsafe {
            spdk_nvmf_transport_opts_init(name_cstr.as_ptr(), opts_ptr, TRANSPORT_OPTS_SIZE)
        };

        if !ok {
            return Err(Error::InvalidArgument(format!(
                "Transport type not found: {}",
                transport_name
            )));
        }

        // TODO: Apply user opts if provided
        // This would require knowing the field offsets in the opaque struct

        // Create transport (using synchronous API for simplicity)
        let transport = unsafe { spdk_nvmf_transport_create(name_cstr.as_ptr(), opts_ptr) };

        NonNull::new(transport)
            .map(|ptr| Self {
                ptr,
                _marker: PhantomData,
            })
            .ok_or_else(|| {
                Error::InvalidArgument(format!("Failed to create {} transport", transport_name))
            })
    }

    /// Consume self and return the raw pointer.
    /// Used when adding to target (target takes ownership).
    pub(crate) fn into_ptr(self) -> *mut spdk_nvmf_transport {
        let ptr = self.ptr.as_ptr();
        // Transport ownership is transferred to target via add_transport.
        // We use forget to prevent any future Drop from being called.
        #[allow(clippy::forget_non_drop)]
        std::mem::forget(self);
        ptr
    }
}

// Note: No Drop impl - transport ownership is transferred to target via add_transport
