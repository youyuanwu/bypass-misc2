//! NVMf subsystem management.

use std::ffi::{CStr, CString, c_void};
use std::marker::PhantomData;
use std::mem::MaybeUninit;
use std::ptr::NonNull;

use spdk_io_sys::*;

use crate::complete::{CompletionSender, completion};
use crate::error::{Error, Result};
use crate::nvme::TransportId;

/// NVMf subsystem.
///
/// Represents a namespace container that can be exported to initiators.
/// Create via [`NvmfTarget::create_subsystem()`].
pub struct NvmfSubsystem {
    ptr: NonNull<spdk_nvmf_subsystem>,
    _marker: PhantomData<*mut ()>,
}

impl NvmfSubsystem {
    /// Create from raw pointer (internal use).
    pub(crate) fn from_ptr(ptr: NonNull<spdk_nvmf_subsystem>) -> Self {
        Self {
            ptr,
            _marker: PhantomData,
        }
    }

    /// Get the subsystem NQN.
    pub fn nqn(&self) -> &str {
        unsafe {
            let nqn_ptr = spdk_nvmf_subsystem_get_nqn(self.ptr.as_ptr());
            if nqn_ptr.is_null() {
                ""
            } else {
                CStr::from_ptr(nqn_ptr).to_str().unwrap_or("<invalid utf8>")
            }
        }
    }

    /// Set the serial number.
    pub fn set_serial_number(&self, sn: &str) -> Result<()> {
        let sn_cstr = CString::new(sn)?;
        let rc = unsafe { spdk_nvmf_subsystem_set_sn(self.ptr.as_ptr(), sn_cstr.as_ptr()) };
        if rc == 0 {
            Ok(())
        } else {
            Err(Error::InvalidArgument("Failed to set serial number".into()))
        }
    }

    /// Set the model number.
    pub fn set_model_number(&self, mn: &str) -> Result<()> {
        let mn_cstr = CString::new(mn)?;
        let rc = unsafe { spdk_nvmf_subsystem_set_mn(self.ptr.as_ptr(), mn_cstr.as_ptr()) };
        if rc == 0 {
            Ok(())
        } else {
            Err(Error::InvalidArgument("Failed to set model number".into()))
        }
    }

    /// Allow any host to connect.
    pub fn set_allow_any_host(&self, allow: bool) {
        unsafe {
            spdk_nvmf_subsystem_set_allow_any_host(self.ptr.as_ptr(), allow);
        }
    }

    /// Add a bdev as a namespace.
    ///
    /// Returns the namespace ID.
    pub fn add_namespace(&self, bdev_name: &str) -> Result<u32> {
        let bdev_cstr = CString::new(bdev_name)?;

        // Initialize namespace options with defaults
        let mut opts: spdk_nvmf_ns_opts = unsafe { MaybeUninit::zeroed().assume_init() };
        unsafe {
            spdk_nvmf_ns_opts_get_defaults(&mut opts, std::mem::size_of::<spdk_nvmf_ns_opts>());
        }

        let nsid = unsafe {
            spdk_nvmf_subsystem_add_ns_ext(
                self.ptr.as_ptr(),
                bdev_cstr.as_ptr(),
                &opts,
                std::mem::size_of::<spdk_nvmf_ns_opts>(),
                std::ptr::null(),
            )
        };

        if nsid == 0 {
            Err(Error::InvalidArgument(format!(
                "Failed to add namespace: {}",
                bdev_name
            )))
        } else {
            Ok(nsid)
        }
    }

    /// Add a listener address.
    ///
    /// The subsystem will accept connections on this address after starting.
    pub async fn add_listener(&self, trid: &TransportId) -> Result<()> {
        let (tx, rx) = completion();

        unsafe {
            spdk_nvmf_subsystem_add_listener(
                self.ptr.as_ptr(),
                trid.as_ptr() as *mut _,
                Some(listener_done),
                tx.into_raw(),
            );
        }

        rx.await
    }

    /// Start the subsystem (begin accepting connections).
    pub async fn start(&self) -> Result<()> {
        let (tx, rx) = completion();

        let rc = unsafe {
            spdk_nvmf_subsystem_start(
                self.ptr.as_ptr(),
                Some(subsystem_state_change_done),
                tx.into_raw(),
            )
        };

        if rc != 0 {
            return Err(Error::from_errno(-rc));
        }

        rx.await
    }

    /// Stop the subsystem.
    pub async fn stop(&self) -> Result<()> {
        let (tx, rx) = completion();

        let rc = unsafe {
            spdk_nvmf_subsystem_stop(
                self.ptr.as_ptr(),
                Some(subsystem_state_change_done),
                tx.into_raw(),
            )
        };

        if rc != 0 {
            return Err(Error::from_errno(-rc));
        }

        rx.await
    }

    /// Get raw pointer (for internal use).
    #[allow(dead_code)]
    pub(crate) fn as_ptr(&self) -> *mut spdk_nvmf_subsystem {
        self.ptr.as_ptr()
    }
}

// Note: No Drop impl - subsystem is owned by target

/// Callback for listener add completion.
unsafe extern "C" fn listener_done(ctx: *mut c_void, status: i32) {
    let tx = unsafe { CompletionSender::<()>::from_raw(ctx) };

    if status == 0 {
        tx.success(());
    } else {
        tx.error(Error::from_errno(-status));
    }
}

/// Callback for subsystem state change (start/stop).
unsafe extern "C" fn subsystem_state_change_done(
    _subsystem: *mut spdk_nvmf_subsystem,
    ctx: *mut c_void,
    status: i32,
) {
    let tx = unsafe { CompletionSender::<()>::from_raw(ctx) };

    if status == 0 {
        tx.success(());
    } else {
        tx.error(Error::from_errno(-status));
    }
}
