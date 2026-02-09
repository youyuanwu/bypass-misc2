//! NVMf target management.

use std::ffi::{CString, c_void};
use std::marker::PhantomData;
use std::mem::MaybeUninit;
use std::ptr::NonNull;

use spdk_io_sys::*;

use crate::complete::{CompletionSender, completion};
use crate::error::{Error, Result};

use super::opts::NvmfTargetOpts;
use super::subsystem::NvmfSubsystem;
use super::transport::NvmfTransport;

/// NVMf target instance.
///
/// Creates and manages subsystems, transports, and listeners.
/// This is the main entry point for setting up an NVMe-oF target
/// for in-process testing.
///
/// # Thread Safety
///
/// `!Send + !Sync` - target operations must stay on one thread.
pub struct NvmfTarget {
    ptr: NonNull<spdk_nvmf_tgt>,
    _marker: PhantomData<*mut ()>, // !Send + !Sync
}

impl NvmfTarget {
    /// Create a new NVMf target.
    ///
    /// # Arguments
    ///
    /// * `name` - Target name (used for RPC identification)
    pub fn create(name: &str) -> Result<Self> {
        Self::create_with_opts(NvmfTargetOpts {
            name: Some(name.to_string()),
            ..Default::default()
        })
    }

    /// Create a new NVMf target with options.
    pub fn create_with_opts(opts: NvmfTargetOpts) -> Result<Self> {
        let mut native_opts: spdk_nvmf_target_opts = unsafe { MaybeUninit::zeroed().assume_init() };

        native_opts.size = std::mem::size_of::<spdk_nvmf_target_opts>();

        // Copy name
        if let Some(ref name) = opts.name {
            let name_bytes = name.as_bytes();
            let max_len = native_opts.name.len() - 1;
            let copy_len = name_bytes.len().min(max_len);
            for (i, &byte) in name_bytes[..copy_len].iter().enumerate() {
                native_opts.name[i] = byte as i8;
            }
        }

        if let Some(max_subsystems) = opts.max_subsystems {
            native_opts.max_subsystems = max_subsystems;
        }

        let ptr = unsafe { spdk_nvmf_tgt_create(&mut native_opts) };

        NonNull::new(ptr)
            .map(|ptr| Self {
                ptr,
                _marker: PhantomData,
            })
            .ok_or_else(|| Error::InvalidArgument("Failed to create NVMf target".into()))
    }

    /// Add a transport to the target.
    ///
    /// The transport must be created via [`NvmfTransport::tcp()`] or similar first.
    pub async fn add_transport(&self, transport: NvmfTransport) -> Result<()> {
        let (tx, rx) = completion();

        unsafe {
            spdk_nvmf_tgt_add_transport(
                self.ptr.as_ptr(),
                transport.into_ptr(),
                Some(add_transport_done),
                tx.into_raw(),
            );
        }

        rx.await
    }

    /// Start listening on a transport address.
    ///
    /// This must be called before adding the listener to a subsystem.
    /// The target will start accepting connections on the specified address.
    ///
    /// # Arguments
    ///
    /// * `trid` - Transport ID specifying the listen address
    pub fn listen(&self, trid: &crate::nvme::TransportId) -> Result<()> {
        // Initialize listen options
        let mut opts: spdk_nvmf_listen_opts = unsafe { MaybeUninit::zeroed().assume_init() };
        unsafe {
            spdk_nvmf_listen_opts_init(&mut opts, std::mem::size_of::<spdk_nvmf_listen_opts>());
        }

        let rc = unsafe { spdk_nvmf_tgt_listen_ext(self.ptr.as_ptr(), trid.as_ptr(), &mut opts) };

        if rc != 0 {
            return Err(Error::from_errno(-rc));
        }

        Ok(())
    }

    /// Create a subsystem.
    ///
    /// # Arguments
    ///
    /// * `nqn` - NVMe Qualified Name for the subsystem  
    /// * `opts` - Subsystem options
    pub fn create_subsystem(
        &self,
        nqn: &str,
        opts: super::opts::NvmfSubsystemOpts,
    ) -> Result<NvmfSubsystem> {
        let nqn_cstr = CString::new(nqn)?;

        let subsystem = unsafe {
            spdk_nvmf_subsystem_create(
                self.ptr.as_ptr(),
                nqn_cstr.as_ptr(),
                spdk_nvmf_subtype_SPDK_NVMF_SUBTYPE_NVME,
                0, // num_ns - will add later
            )
        };

        let subsystem = NonNull::new(subsystem).ok_or_else(|| {
            Error::InvalidArgument(format!("Failed to create subsystem: {}", nqn))
        })?;

        let subsys = NvmfSubsystem::from_ptr(subsystem);

        // Apply options
        if opts.allow_any_host {
            subsys.set_allow_any_host(true);
        }

        if let Some(ref sn) = opts.serial_number {
            subsys.set_serial_number(sn)?;
        }

        if let Some(ref mn) = opts.model_number {
            subsys.set_model_number(mn)?;
        }

        Ok(subsys)
    }

    /// Find a subsystem by NQN.
    pub fn find_subsystem(&self, nqn: &str) -> Option<NvmfSubsystem> {
        let nqn_cstr = CString::new(nqn).ok()?;

        let ptr = unsafe { spdk_nvmf_tgt_find_subsystem(self.ptr.as_ptr(), nqn_cstr.as_ptr()) };

        NonNull::new(ptr).map(NvmfSubsystem::from_ptr)
    }

    /// Get the target name.
    pub fn name(&self) -> &str {
        unsafe {
            let name_ptr = spdk_nvmf_tgt_get_name(self.ptr.as_ptr());
            if name_ptr.is_null() {
                ""
            } else {
                std::ffi::CStr::from_ptr(name_ptr)
                    .to_str()
                    .unwrap_or("<invalid utf8>")
            }
        }
    }

    /// Get raw pointer (for internal use).
    #[allow(dead_code)]
    pub(crate) fn as_ptr(&self) -> *mut spdk_nvmf_tgt {
        self.ptr.as_ptr()
    }
}

impl Drop for NvmfTarget {
    fn drop(&mut self) {
        // spdk_nvmf_tgt_destroy is async, but we need to block here
        // In proper usage, the target should be destroyed before SpdkApp stops
        unsafe {
            spdk_nvmf_tgt_destroy(self.ptr.as_ptr(), Some(destroy_done), std::ptr::null_mut());
        }
    }
}

/// Callback for add_transport completion.
unsafe extern "C" fn add_transport_done(ctx: *mut c_void, status: i32) {
    let tx = unsafe { CompletionSender::<()>::from_raw(ctx) };

    if status == 0 {
        tx.success(());
    } else {
        tx.error(Error::from_errno(-status));
    }
}

/// Callback for target destroy (no-op, just to satisfy the API).
unsafe extern "C" fn destroy_done(_ctx: *mut c_void, _status: i32) {
    // Nothing to do - target is being destroyed in Drop
}
