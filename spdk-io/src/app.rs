//! SPDK Application Framework
//!
//! This module provides [`SpdkApp`] and [`SpdkAppBuilder`] for building complete
//! SPDK applications using the event framework (`spdk_app_start()`).
//!
//! Unlike [`SpdkEnv`](crate::SpdkEnv) which only initializes the DPDK environment,
//! `SpdkApp` provides full subsystem initialization including:
//!
//! - Environment initialization (DPDK/hugepages)
//! - All SPDK subsystems (bdev, nvmf, etc.)  
//! - JSON configuration loading for bdevs
//! - Reactor/poller infrastructure
//! - Signal handling and graceful shutdown
//!
//! # Example
//!
//! ```no_run
//! use spdk_io::{SpdkApp, Result};
//!
//! fn main() -> Result<()> {
//!     SpdkApp::builder()
//!         .name("my_app")
//!         .config_file("./config.json")
//!         .run(|| {
//!             println!("SPDK is running!");
//!             // Bdevs from config.json are now available
//!             SpdkApp::stop();
//!         })
//! }
//! ```

use std::ffi::{CString, c_void};
use std::future::Future;

use spdk_io_sys::*;

use crate::complete::block_on;
use crate::env::LogLevel;
use crate::error::{Error, Result};

/// Wrapper to pass a closure through C void pointer.
/// Box<dyn FnOnce()> is a fat pointer, so we wrap it in a struct.
struct CallbackWrapper {
    func: Box<dyn FnOnce()>,
}

/// SPDK Application Framework handle.
///
/// This type doesn't hold state - it provides static methods for
/// controlling the application lifecycle.
pub struct SpdkApp {
    _private: (),
}

impl SpdkApp {
    /// Create a new application builder.
    pub fn builder() -> SpdkAppBuilder {
        SpdkAppBuilder::new()
    }

    /// Signal SPDK to shut down gracefully.
    ///
    /// This kicks off the shutdown process and returns immediately.
    /// Once shutdown is complete, `run()` or `block_on()` will return.
    ///
    /// # Arguments
    /// * `rc` - Return code (0 for success)
    pub fn stop_with_code(rc: i32) {
        unsafe { spdk_app_stop(rc) };
    }

    /// Signal SPDK to shut down gracefully with success code (0).
    pub fn stop() {
        Self::stop_with_code(0);
    }

    /// Start the shutdown process.
    ///
    /// This is typically called by signal handlers, but can be called
    /// directly for applications managing their own shutdown.
    pub fn start_shutdown() {
        unsafe { spdk_app_start_shutdown() };
    }

    /// Get the shared memory ID for this application.
    ///
    /// Returns -1 if shared memory is not enabled.
    pub fn shm_id() -> i32 {
        unsafe { spdk_app_get_shm_id() }
    }
}

/// Builder for configuring and running an SPDK application.
pub struct SpdkAppBuilder {
    name: Option<String>,
    config_file: Option<String>,
    json_data: Option<Vec<u8>>,
    json_config_ignore_errors: bool,
    reactor_mask: Option<String>,
    rpc_addr: Option<String>,
    main_core: Option<i32>,
    mem_size_mb: Option<i32>,
    shm_id: Option<i32>,
    no_pci: bool,
    no_huge: bool,
    log_level: Option<LogLevel>,
}

impl SpdkAppBuilder {
    /// Create a new builder with default options.
    pub fn new() -> Self {
        Self {
            name: None,
            config_file: None,
            json_data: None,
            json_config_ignore_errors: false,
            reactor_mask: None,
            rpc_addr: None,
            main_core: None,
            mem_size_mb: None,
            shm_id: None,
            no_pci: false,
            no_huge: false,
            log_level: None,
        }
    }

    /// Set the application name.
    ///
    /// Used for hugepage file names, logs, and identification.
    pub fn name(mut self, name: &str) -> Self {
        self.name = Some(name.to_string());
        self
    }

    /// Set the path to JSON configuration file.
    ///
    /// The config file can define bdevs, nvmf targets, and other subsystem
    /// configuration. Example:
    ///
    /// ```json
    /// {
    ///   "subsystems": [{
    ///     "subsystem": "bdev",
    ///     "config": [{
    ///       "method": "bdev_null_create",
    ///       "params": {"name": "Null0", "num_blocks": 262144, "block_size": 512}
    ///     }]
    ///   }]
    /// }
    /// ```
    pub fn config_file(mut self, path: &str) -> Self {
        self.config_file = Some(path.to_string());
        self
    }

    /// Set JSON configuration data directly (instead of a file).
    ///
    /// This is an alternative to [`config_file`](Self::config_file). The JSON
    /// string is passed directly to SPDK without writing to a file.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use spdk_io::SpdkApp;
    ///
    /// let config = r#"{
    ///     "subsystems": [{
    ///         "subsystem": "bdev",
    ///         "config": [{
    ///             "method": "bdev_null_create",
    ///             "params": {"name": "Null0", "num_blocks": 1024, "block_size": 512}
    ///         }]
    ///     }]
    /// }"#;
    ///
    /// SpdkApp::builder()
    ///     .name("my_app")
    ///     .json_data(config)
    ///     .run(|| { /* ... */ SpdkApp::stop(); })
    ///     .unwrap();
    /// ```
    pub fn json_data(mut self, json: &str) -> Self {
        self.json_data = Some(json.as_bytes().to_vec());
        self
    }

    /// Ignore errors in JSON configuration.
    ///
    /// If set to true, SPDK will continue initialization even if some
    /// JSON config entries fail to apply.
    pub fn json_config_ignore_errors(mut self, ignore: bool) -> Self {
        self.json_config_ignore_errors = ignore;
        self
    }

    /// Set the CPU core mask for SPDK reactors.
    ///
    /// Format: hex mask like "0x3" (cores 0,1) or list like "0-3,5"
    pub fn reactor_mask(mut self, mask: &str) -> Self {
        self.reactor_mask = Some(mask.to_string());
        self
    }

    /// Set the JSON-RPC server address.
    ///
    /// Can be a Unix domain socket path (e.g., "/var/tmp/spdk.sock")
    /// or IP address with port (e.g., "127.0.0.1:5260").
    pub fn rpc_addr(mut self, addr: &str) -> Self {
        self.rpc_addr = Some(addr.to_string());
        self
    }

    /// Set the main (first) reactor core.
    pub fn main_core(mut self, core: i32) -> Self {
        self.main_core = Some(core);
        self
    }

    /// Set the amount of hugepage memory to reserve in MB.
    pub fn mem_size_mb(mut self, mb: i32) -> Self {
        self.mem_size_mb = Some(mb);
        self
    }

    /// Set the shared memory ID for multi-process mode.
    ///
    /// Use -1 to disable shared memory (single process).
    pub fn shm_id(mut self, id: i32) -> Self {
        self.shm_id = Some(id);
        self
    }

    /// Disable PCI device scanning.
    ///
    /// Useful for testing with virtual bdevs only.
    pub fn no_pci(mut self, no_pci: bool) -> Self {
        self.no_pci = no_pci;
        self
    }

    /// Disable hugepage allocation (use regular memory).
    ///
    /// Useful for testing without configuring hugepages.
    /// Note: Performance will be reduced compared to hugepages.
    pub fn no_huge(mut self, no_huge: bool) -> Self {
        self.no_huge = no_huge;
        self
    }

    /// Set the log level for SPDK messages.
    pub fn log_level(mut self, level: LogLevel) -> Self {
        self.log_level = Some(level);
        self
    }

    /// Run the SPDK application with a synchronous callback.
    ///
    /// The callback runs on the main SPDK reactor thread after all
    /// subsystems are initialized and JSON config is loaded.
    ///
    /// Call [`SpdkApp::stop()`] from within the callback to signal shutdown.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use spdk_io::SpdkApp;
    ///
    /// SpdkApp::builder()
    ///     .name("my_app")
    ///     .config_file("./config.json")
    ///     .run(|| {
    ///         println!("SPDK is running!");
    ///         SpdkApp::stop();
    ///     })
    ///     .expect("SPDK app failed");
    /// ```
    pub fn run<F>(self, f: F) -> Result<()>
    where
        F: FnOnce() + 'static,
    {
        // Convert strings to CStrings (must outlive the call)
        let name_cstr = self.name.as_deref().map(CString::new).transpose()?;
        let config_file_cstr = self.config_file.as_deref().map(CString::new).transpose()?;
        let reactor_mask_cstr = self.reactor_mask.as_deref().map(CString::new).transpose()?;
        let rpc_addr_cstr = self.rpc_addr.as_deref().map(CString::new).transpose()?;

        // Wrap the closure in a struct so we can pass a thin pointer through C
        let wrapper = Box::new(CallbackWrapper { func: Box::new(f) });
        let callback_ptr = Box::into_raw(wrapper) as *mut c_void;

        let rc = unsafe {
            // Initialize opts with defaults
            let mut opts: spdk_app_opts = std::mem::zeroed();
            spdk_app_opts_init(&mut opts, std::mem::size_of::<spdk_app_opts>());

            // Apply configuration
            if let Some(ref name) = name_cstr {
                opts.name = name.as_ptr();
            }
            if let Some(ref config_file) = config_file_cstr {
                opts.json_config_file = config_file.as_ptr();
            }
            if let Some(ref json) = self.json_data {
                opts.json_data = json.as_ptr() as *mut c_void;
                opts.json_data_size = json.len();
            }
            opts.json_config_ignore_errors = self.json_config_ignore_errors;
            if let Some(ref reactor_mask) = reactor_mask_cstr {
                opts.reactor_mask = reactor_mask.as_ptr();
            }
            if let Some(ref rpc_addr) = rpc_addr_cstr {
                opts.rpc_addr = rpc_addr.as_ptr();
            }
            if let Some(main_core) = self.main_core {
                opts.main_core = main_core;
            }
            if let Some(mem_size) = self.mem_size_mb {
                opts.mem_size = mem_size;
            }
            if let Some(shm_id) = self.shm_id {
                opts.shm_id = shm_id;
            }
            opts.no_pci = self.no_pci;
            opts.no_huge = self.no_huge;

            if let Some(level) = self.log_level {
                opts.print_level = level as i32;
            }

            // Start the application
            spdk_app_start(&mut opts, Some(start_callback), callback_ptr)
        };

        // Finalize SPDK
        unsafe { spdk_app_fini() };

        if rc != 0 {
            Err(Error::EnvInit(format!(
                "spdk_app_start failed with error code {}",
                rc
            )))
        } else {
            Ok(())
        }
    }

    /// Run the SPDK application with an async future.
    ///
    /// This is a convenience wrapper that runs a future to completion
    /// using [`block_on`](crate::block_on). For more complex scenarios
    /// (multiple concurrent tasks), use [`run`](Self::run) with a local
    /// executor and [`spdk_poller`](crate::spdk_poller).
    ///
    /// The future runs on the main SPDK reactor thread after all
    /// subsystems are initialized and JSON config is loaded.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use spdk_io::{Bdev, DmaBuf, SpdkApp};
    ///
    /// async fn app_main() {
    ///     let bdev = Bdev::get_by_name("Null0").unwrap();
    ///     let desc = bdev.open(true).unwrap();
    ///     let channel = desc.get_io_channel().unwrap();
    ///
    ///     let mut buf = DmaBuf::alloc(512, 512).unwrap();
    ///     desc.read(&channel, &mut buf, 0).await.unwrap();
    ///
    ///     println!("Read completed!");
    /// }
    ///
    /// SpdkApp::builder()
    ///     .name("my_app")
    ///     .json_data(r#"{"subsystems": [...]}"#)
    ///     .run_async(app_main)
    ///     .expect("SPDK app failed");
    /// ```
    pub fn run_async<F, Fut>(self, f: F) -> Result<()>
    where
        F: FnOnce() -> Fut + 'static,
        Fut: Future<Output = ()> + 'static,
    {
        self.run(move || {
            block_on(f());
            SpdkApp::stop();
        })
    }
}

impl Default for SpdkAppBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// C callback that invokes the Rust closure
extern "C" fn start_callback(ctx: *mut c_void) {
    // Reconstruct the wrapper and call the closure
    let wrapper: Box<CallbackWrapper> = unsafe { Box::from_raw(ctx as *mut CallbackWrapper) };
    (wrapper.func)();
}
