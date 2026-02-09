# spdk-io Design Document

## Overview

`spdk-io` is a Rust library providing safe, ergonomic, async-first bindings to SPDK (Storage Performance Development Kit). The library enables Rust applications to leverage SPDK's high-performance user-space storage stack with native async/await syntax.

## Implementation Status

### Completed

| Component | Status | Notes |
|-----------|--------|-------|
| **spdk-io-sys crate** | ✅ | FFI bindings via bindgen |
| - pkg-config integration | ✅ | Static linking with `--whole-archive` |
| - bindgen generation | ✅ | Rust 2024 compatible, `wrap_unsafe_ops(true)` |
| - System deps handling | ✅ | Filters archive names, probes OpenSSL/ISA-L/uuid |
| **spdk-io-build crate** | ✅ | Build helper for pkg-config parsing |
| - `PkgConfigParser` | ✅ | Parses pkg-config with whole-archive region tracking |
| - Static detection | ✅ | Auto-detects `.a` availability, excludes system roots |
| - `force_whole_archive` | ✅ | Force whole-archive for specific libs (subsystem constructors) |
| **spdk-io crate** | ✅ | Core async I/O API complete |
| - `SpdkEnv` | ✅ | Environment guard with RAII cleanup |
| - `SpdkEnvBuilder` | ✅ | Full configuration: name, core_mask, mem_size, shm_id, no_pci, no_huge, main_core |
| - `SpdkApp` | ✅ | Full application framework via `spdk_app_start()` |
| - `SpdkAppBuilder` | ✅ | Builder for app: name, config_file, json_data, reactor_mask, rpc_addr, mem_size_mb, no_pci, no_huge, `run()`, `run_async()` |
| - `Bdev` | ✅ | Block device handle with lookup by name |
| - `BdevDesc` | ✅ | Open bdev descriptor with async `read()` and `write()` |
| - `DmaBuf` | ✅ | DMA-capable buffer allocation via `spdk_dma_malloc()` |
| - `Completion` | ✅ | Callback-to-future utilities for async I/O |
| - `block_on` | ✅ | Block on futures while polling SPDK thread |
| - `spdk_poller` | ✅ | Async task for executor integration |
| - `SpdkThread` | ✅ | Thread context with polling, `!Send + !Sync` |
| - `SpdkThread::spawn()` | ✅ | Spawn OS thread with SPDK context |
| - `JoinHandle` | ✅ | Handle for spawned thread with join() |
| - `CurrentThread` | ✅ | Borrowed reference to attached thread |
| - `ThreadHandle` | ✅ | Thread-safe handle for cross-thread messaging via `spdk_thread_send_msg()` |
| - `IoChannel` | ✅ | Per-thread I/O channel wrapper, `!Send + !Sync` |
| - `Error` types | ✅ | Comprehensive error enum with thiserror |
| - Integration tests | ✅ | vdev mode (no hugepages required) |
| **nvme module** | ✅ | Direct NVMe driver access |
| - `TransportId` | ✅ | PCIe/TCP/RDMA connection identifiers |
| - `NvmeController` | ✅ | Connect, namespace, alloc_io_qpair |
| - `NvmeNamespace` | ✅ | Async read/write |
| - `NvmeQpair` | ✅ | Per-thread I/O queue |
| **nvmf module** | ✅ | In-process NVMe-oF target (see warning below) |
| - `NvmfTarget` | ✅ | Create, add_transport, create_subsystem |
| - `NvmfTransport` | ✅ | TCP/RDMA transport creation |
| - `NvmfSubsystem` | ✅ | add_namespace, add_listener, start/stop |
| **NVMf subprocess testing** | ✅ | Preferred approach for testing (see `tests/nvmf_test.rs`) |
| **CI/CD** | ✅ | GitHub Actions with SPDK deb package |

### In Progress

| Component | Status | Notes |
|-----------|--------|-------|
| (none) | | |

### Planned

| Component | Notes |
|-----------|-------|
| `Blobstore` / `Blob` | Blobstore API |

### Build & Linking

The crate uses **static linking** with `--whole-archive` for SPDK/DPDK libraries:

```
┌─────────────────────────────────────────────────────────────┐
│  spdk-io-sys build.rs                                       │
├─────────────────────────────────────────────────────────────┤
│  1. pkg-config probes SPDK libs (statik=true)               │
│  2. Separates SPDK/DPDK libs from system libs               │
│  3. Emits --whole-archive for SPDK/DPDK (include all syms)  │
│  4. Links system libs normally (ssl, crypto, numa, etc.)    │
│  5. bindgen generates Rust bindings from wrapper.h          │
└─────────────────────────────────────────────────────────────┘
```

**Why static linking?** SPDK uses callback tables and static initializers that would be
dropped by the linker with `--as-needed`. Using `--whole-archive` ensures all symbols
are included in the final binary.

## Crate Structure

```
spdk-io/
├── spdk-io-build/        # Build helper crate
│   └── src/lib.rs        # PkgConfigParser with force_whole_archive
│
├── spdk-io-sys/          # Low-level FFI bindings
│   ├── build.rs          # Bindgen + linking with force_whole_archive for subsystems
│   ├── src/
│   │   └── lib.rs        # Generated bindings + manual additions
│   └── wrapper.h         # SPDK headers to bind
│
└── spdk-io/              # High-level async Rust API
    ├── src/
    │   ├── lib.rs
    │   ├── app.rs        # SpdkApp/SpdkAppBuilder (spdk_app_start framework)
    │   ├── bdev.rs       # Bdev/BdevDesc block device API with async read/write
    │   ├── channel.rs    # I/O channel management
    │   ├── complete.rs   # Callback-to-future utilities, block_on helper
    │   ├── dma.rs        # DMA buffer management
    │   ├── env.rs        # SpdkEnv/SpdkEnvBuilder (low-level env init)
    │   ├── error.rs      # Error types
    │   ├── poller.rs     # SPDK poller task for executor integration
    │   ├── thread.rs     # SPDK thread management
    │   ├── nvme/         # NVMe driver API
    │   │   ├── mod.rs
    │   │   ├── controller.rs
    │   │   ├── namespace.rs
    │   │   ├── qpair.rs
    │   │   └── transport.rs
    │   └── nvmf/         # NVMf target API
    │       ├── mod.rs
    │       ├── target.rs
    │       ├── subsystem.rs
    │       ├── transport.rs
    │       └── opts.rs
    ├── tests/
    │   ├── app_test.rs   # SpdkApp simple test
    │   ├── bdev_test.rs  # Bdev/BdevDesc with null bdev
    │   ├── env_init.rs   # SpdkEnv initialization test
    │   ├── mempool_test.rs
    │   ├── nvmf_test.rs  # NVMf subprocess integration test
    │   └── thread_test.rs
    └── Cargo.toml
```

### spdk-io-sys

Low-level FFI bindings crate:

- **Generated via bindgen** from SPDK headers
- **Links to SPDK** static libraries via pkg-config or explicit paths
- **Exports raw types**: `spdk_bdev`, `spdk_blob`, `spdk_io_channel`, etc.
- **Exports raw functions**: `spdk_bdev_read()`, `spdk_blob_io_write()`, etc.
- **Minimal safe wrappers**: Only for ergonomics (e.g., `Default` impls)

### spdk-io

High-level async Rust crate:

- **Safe wrappers** around `spdk-io-sys` types
- **Async/await API** for all I/O operations
- **Runtime-agnostic** - works with any local executor (Tokio, async-std, smol, etc.)
- **RAII resource management** (Drop implementations)
- **Error handling** via `Result<T, SpdkError>`
- **Uses `futures` ecosystem** - `futures-util`, `futures-channel` for portability

## Runtime Architecture

### Design Goals

1. **Runtime-agnostic** - works with any single-threaded async executor
2. **User controls the runtime** - start SPDK thread, run your preferred local executor
3. **Async/await for I/O operations** - no manual callback management
4. **Thread-local I/O channels** - lock-free I/O submission
5. **Cooperative scheduling** - yield between SPDK polling and app logic
6. **Uses standard futures traits** - `Future`, `Stream`, `Sink` from `futures` crate

### Threading Model

**Note:** An `spdk_thread` is NOT an OS thread. It's a lightweight scheduling context 
(similar to a green thread or goroutine). It runs on whatever OS thread calls 
`spdk_thread_poll()` on it. Think of it as a task queue + poller state.

```
┌─────────────────────────────────────────────────────────────┐
│                     OS Thread N                              │
│  ┌─────────────────────────────────────────────────────────┐│
│  │         Any Local Async Executor (user's choice)        ││
│  │       (Tokio LocalSet, smol, async-executor, etc.)      ││
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────────┐  ││
│  │  │ App Future  │  │ App Future  │  │ SPDK Poller     │  ││
│  │  │   (task)    │  │   (task)    │  │   (task)        │  ││
│  │  └──────┬──────┘  └──────┬──────┘  └────────┬────────┘  ││
│  │         │                │                   │           ││
│  │         ▼                ▼                   ▼           ││
│  │  ┌───────────────────────────────────────────────────┐  ││
│  │  │              I/O Channel (thread-local)            │  ││
│  │  └───────────────────────────────────────────────────┘  ││
│  └─────────────────────────────────────────────────────────┘│
│                              │                               │
│                              ▼                               │
│  ┌─────────────────────────────────────────────────────────┐│
│  │                    SPDK Thread Context                   ││
│  │     (spdk_thread struct - message queue, pollers)        ││
│  └─────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────┘
```

Each OS thread that uses SPDK has:
- An **SPDK thread context** (`spdk_thread`) - a scheduling/message queue, not a real thread
- **User's choice of local executor** - any `!Send` future executor works
- A **poller task** that calls `spdk_thread_poll()` and yields to the executor

### Async Integration Pattern

SPDK uses callback-based async. We convert to Rust futures using `futures-channel`:

```rust
use futures_channel::oneshot;
use futures_util::future::FutureExt;

pub async fn bdev_read(
    desc: &BdevDesc,
    channel: &IoChannel,
    buf: &mut DmaBuf,
    offset: u64,
    len: u64,
) -> Result<(), SpdkError> {
    // Create oneshot channel for completion (runtime-agnostic)
    let (tx, rx) = oneshot::channel();
    
    // Submit I/O with callback that sends on channel
    unsafe {
        spdk_bdev_read(
            desc.as_ptr(),
            channel.as_ptr(),
            buf.as_mut_ptr(),
            offset,
            len,
            Some(completion_callback),
            tx.into_raw(),
        );
    }
    
    // Await completion (yields to executor, SPDK poller runs)
    rx.await.map_err(|_| SpdkError::Cancelled)?
}

extern "C" fn completion_callback(
    bdev_io: *mut spdk_bdev_io,
    success: bool,
    ctx: *mut c_void,
) {
    let tx = unsafe { Sender::from_raw(ctx) };
    let result = if success { Ok(()) } else { Err(SpdkError::IoError) };
    let _ = tx.send(result);
    unsafe { spdk_bdev_free_io(bdev_io) };
}
```

### SPDK Poller Integration

The SPDK poller runs as an async task that:
1. Calls `spdk_thread_poll()` to process SPDK work
2. Yields to allow other tasks to run
3. Repeats

```rust
use futures_util::future::yield_now;

/// Poller task that drives SPDK's internal event loop
/// Works with any async executor
pub async fn spdk_poller_task(thread: &SpdkThread) {
    loop {
        // Poll SPDK - this processes completions and runs pollers
        let work_done = thread.poll();
        
        if work_done == 0 {
            // No work done, yield to other tasks (runtime-agnostic)
            yield_now().await;
        }
        // If work was done, immediately poll again (busy loop)
    }
}
```

### Runtime Initialization

The user controls the runtime. `spdk-io` provides the SPDK thread and poller:

```rust
use spdk_io::{SpdkEnv, SpdkThread, poller_task};

fn main() {
    // Initialize SPDK environment (hugepages, PCI, etc.)
    let _env = SpdkEnv::builder()
        .name("my_app")
        .mem_size_mb(2048)
        .build()
        .expect("Failed to init SPDK");
    
    // For testing without hugepages (vdev mode):
    // let _env = SpdkEnv::builder()
    //     .name("test")
    //     .no_pci(true)
    //     .no_huge(true)
    //     .mem_size_mb(64)
    //     .build()
    //     .expect("Failed to init SPDK");
    
    // Attach SPDK thread context to current OS thread (no new thread created)
    let spdk_thread = SpdkThread::current("worker-0").expect("Failed to attach SPDK thread");
    
    // User chooses their runtime - here's Tokio example:
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    
    let local = tokio::task::LocalSet::new();
    local.block_on(&rt, async {
        // Spawn the SPDK poller as a background task
        tokio::task::spawn_local(poller_task(&spdk_thread));
        
        // Pass thread handle to app (explicit, no hidden state)
        run_app(&spdk_thread).await
    });
}

async fn run_app(thread: &SpdkThread) -> Result<(), SpdkError> {
    // Get bdev by name (sync lookup)
    let bdev = Bdev::get_by_name("Nvme0n1")
        .ok_or(SpdkError::DeviceNotFound("Nvme0n1".into()))?;
    
    // Open with read-write access (sync)
    let desc = bdev.open(true)?;
    
    // Get I/O channel from descriptor (bound to current thread)
    let channel = desc.get_io_channel()?;
    
    // Allocate DMA buffer
    let mut buf = DmaBuf::alloc(4096, 4096)?;
    
    // Async read (future work)
    desc.read(&channel, &mut buf, 0, 4096).await?;
    
    Ok(())
}
```

### Alternative Runtime Examples

**With `smol`:**
```rust
use smol::LocalExecutor;

fn main() {
    let _env = SpdkEnv::builder().name("app").build().unwrap();
    let spdk_thread = SpdkThread::current("worker").unwrap();
    
    let ex = LocalExecutor::new();
    futures_lite::future::block_on(ex.run(async {
        ex.spawn(poller_task(&spdk_thread)).detach();
        run_app().await
    }));
}
```

**With `async-executor`:**
```rust
use async_executor::LocalExecutor;

fn main() {
    let _env = SpdkEnv::builder().name("app").build().unwrap();
    let spdk_thread = SpdkThread::current("worker").unwrap();
    
    let ex = LocalExecutor::new();
    futures_lite::future::block_on(ex.run(async {
        ex.spawn(poller_task(&spdk_thread)).detach();
        run_app().await
    }));
}
```

## Core Types

### Environment & Initialization

```rust
/// SPDK environment guard - initialized ONCE per process
/// 
/// Like DPDK's EAL, the SPDK environment can only be initialized once.
/// This struct acts as a guard: when dropped, SPDK is cleaned up but
/// CANNOT be re-initialized (DPDK limitation).
/// 
/// Typically held in main() for the lifetime of the program.
pub struct SpdkEnv {
    _private: (), // prevent external construction
}

impl SpdkEnv {
    /// Initialize SPDK environment
    /// 
    /// # Errors
    /// Returns error if:
    /// - Already initialized (can only succeed ONCE per process)
    /// - Hugepage allocation fails
    /// - PCI access fails
    pub fn init() -> Result<SpdkEnv>;
    
    /// Builder pattern for configuration
    pub fn builder() -> SpdkEnvBuilder;
}

impl Drop for SpdkEnv {
    fn drop(&mut self) {
        // Cleans up SPDK/DPDK resources
        // WARNING: After drop, SPDK cannot be re-initialized in this process
        unsafe { spdk_env_fini(); }
    }
}

/// Builder for configuring SPDK environment
pub struct SpdkEnvBuilder {
    name: Option<String>,
    core_mask: Option<String>,
    mem_size_mb: Option<i32>,
    shm_id: Option<i32>,
    no_pci: bool,
    no_huge: bool,
    hugepage_single_segments: bool,
    main_core: Option<i32>,
}

impl SpdkEnvBuilder {
    pub fn name(self, name: &str) -> Self;
    pub fn core_mask(self, mask: &str) -> Self;
    pub fn mem_size_mb(self, mb: i32) -> Self;
    pub fn shm_id(self, id: i32) -> Self;
    pub fn no_pci(self, no_pci: bool) -> Self;
    pub fn no_huge(self, no_huge: bool) -> Self;  // vdev mode - no hugepages
    pub fn hugepage_single_segments(self, single: bool) -> Self;
    pub fn main_core(self, core: i32) -> Self;
    pub fn build(self) -> Result<SpdkEnv>;
}
```

> **Note:** `SpdkEnv` only initializes the DPDK environment. For bdev subsystem and
> JSON configuration support, use [`SpdkApp`](#application-framework-spdkapp) instead.
```

#### Privilege Requirements

SPDK/DPDK typically requires elevated privileges for:
- **Hugepage access** - allocating/mapping hugepages
- **PCI device access** - binding to VFIO/UIO drivers  
- **Memory locking** - `mlockall()` to prevent DMA buffers from swapping

**Options for running:**

| Method | Command | Notes |
|--------|---------|-------|
| Root | `sudo ./app` | Simplest for development |
| Capabilities | `sudo setcap cap_ipc_lock,cap_sys_rawio+ep ./app` | Per-binary grant |
| Systemd | `AmbientCapabilities=CAP_IPC_LOCK CAP_SYS_RAWIO` | Production services |
| Pre-setup | Run `spdk/scripts/setup.sh` first | Prepares hugepages & drivers |

```bash
# One-time system setup (run as root)
sudo /path/to/spdk/scripts/setup.sh

# Then app may run with just capabilities (depends on config)
./my_app
```

### Application Framework (SpdkApp)

While `SpdkEnv` provides low-level environment initialization via `spdk_env_init()`, most SPDK 
applications should use the **SPDK Application Framework** which handles:

1. Environment initialization (DPDK/hugepages)
2. All subsystem initialization (bdev, nvmf, etc.)
3. JSON configuration loading (bdevs, etc.)
4. Reactor/poller infrastructure
5. Signal handling and graceful shutdown

**Comparison:**

| Feature | `SpdkEnv` (low-level) | `SpdkApp` (framework) |
|---------|----------------------|----------------------|
| Init via | `spdk_env_init()` | `spdk_app_start()` |
| Subsystems | Manual init required | Auto-initialized |
| JSON config | Not supported | Native support |
| Bdev creation | Need internal headers | Via JSON config |
| Main loop | User-managed | Framework-managed |
| Use case | Embedding, custom apps | Typical SPDK apps |

```rust
/// SPDK Application Framework - full subsystem initialization
/// 
/// Uses `spdk_app_start()` which initializes:
/// - DPDK environment
/// - All registered SPDK subsystems (bdev, etc.)
/// - JSON-RPC server (optional)
/// - Reactor/poller infrastructure
/// 
/// The application runs inside the framework's main loop.
/// 
/// # Thread Model
/// `spdk_app_start()` takes over the calling thread and runs the SPDK
/// reactor on it. The user callback runs on this "main" reactor thread.
pub struct SpdkApp {
    _private: (),
}

/// Builder for SPDK Application Framework
pub struct SpdkAppBuilder {
    name: Option<String>,
    config_file: Option<String>,   // JSON config file path
    reactor_mask: Option<String>,  // CPU mask for reactors
    main_core: Option<i32>,
    mem_size_mb: Option<i32>,
    no_pci: bool,
    rpc_addr: Option<String>,      // Unix socket for JSON-RPC
    shm_id: Option<i32>,
    log_level: Option<LogLevel>,
}

impl SpdkAppBuilder {
    pub fn new() -> Self;
    
    /// Application name (used for hugepage files, logs)
    pub fn name(self, name: &str) -> Self;
    
    /// Path to JSON config file for bdev/subsystem configuration
    /// 
    /// Example config file:
    /// ```json
    /// {
    ///   "subsystems": [{
    ///     "subsystem": "bdev",
    ///     "config": [{
    ///       "method": "bdev_null_create",
    ///       "params": {
    ///         "name": "Null0",
    ///         "num_blocks": 262144,
    ///         "block_size": 512
    ///       }
    ///     }]
    ///   }]
    /// }
    /// ```
    pub fn config_file(self, path: &str) -> Self;
    
    /// CPU core mask for SPDK reactors (e.g., "0x3" for cores 0,1)
    pub fn reactor_mask(self, mask: &str) -> Self;
    
    /// Main (first) reactor core
    pub fn main_core(self, core: i32) -> Self;
    
    /// Hugepage memory size in MB
    pub fn mem_size_mb(self, mb: i32) -> Self;
    
    /// Disable PCI device scanning
    pub fn no_pci(self, no_pci: bool) -> Self;
    
    /// JSON-RPC server socket path (e.g., "/var/tmp/spdk.sock")
    pub fn rpc_addr(self, addr: &str) -> Self;
    
    /// Shared memory ID for multi-process
    pub fn shm_id(self, id: i32) -> Self;
    
    /// SPDK log level
    pub fn log_level(self, level: LogLevel) -> Self;
    
    /// Run application with synchronous callback
    /// 
    /// The callback runs on the main SPDK reactor thread.
    /// When callback returns, SPDK shuts down.
    /// 
    /// # Example
    /// ```rust
    /// SpdkApp::builder()
    ///     .name("my_app")
    ///     .config_file("./config.json")
    ///     .run(|| {
    ///         // Bdevs from config.json are now available
    ///         let bdev = Bdev::get_by_name("Null0").unwrap();
    ///         let desc = bdev.open(true).unwrap();
    ///         // ...
    ///         SpdkApp::stop(); // Signal shutdown
    ///     })
    ///     .expect("SPDK app failed");
    /// ```
    pub fn run<F>(self, f: F) -> Result<()>
    where
        F: FnOnce() + 'static;
    
    /// Run application with async main function
    /// 
    /// Spawns a poller-based executor and runs the future.
    /// 
    /// # Example
    /// ```rust
    /// SpdkApp::builder()
    ///     .name("my_app")
    ///     .config_file("./config.json")
    ///     .block_on(async {
    ///         let bdev = Bdev::get_by_name("Null0").unwrap();
    ///         let desc = bdev.open(true).unwrap();
    ///         let channel = desc.get_io_channel().unwrap();
    ///         
    ///         // Async I/O
    ///         desc.read(&channel, 0, &mut buf).await?;
    ///         
    ///         Ok(())
    ///     })
    ///     .expect("SPDK app failed");
    /// ```
    pub fn block_on<F, T>(self, future: F) -> Result<T>
    where
        F: Future<Output = Result<T>> + 'static,
        T: 'static;
}

impl SpdkApp {
    pub fn builder() -> SpdkAppBuilder {
        SpdkAppBuilder::new()
    }
    
    /// Signal SPDK to shut down gracefully
    /// 
    /// Can be called from any reactor thread.
    pub fn stop() {
        unsafe { spdk_app_stop(0); }
    }
    
    /// Request application shutdown (alternative to stop)
    pub fn start_shutdown() {
        unsafe { spdk_app_start_shutdown(); }
    }
}
```

#### SpdkApp vs SpdkEnv: When to Use Which

**Use `SpdkApp` when:**
- Building a typical SPDK application
- Need bdev/nvmf/other subsystems
- Want JSON config for bdevs
- Need graceful signal handling
- Want the standard SPDK application model

**Use `SpdkEnv` when:**
- Embedding SPDK in an existing application
- Only need NVMe driver (no bdev layer)
- Custom threading model required
- Need fine-grained control over initialization
- Building a custom subsystem

#### Implementation Notes

```
┌─────────────────────────────────────────────────────────────────────┐
│                      spdk_app_start() Flow                          │
├─────────────────────────────────────────────────────────────────────┤
│  1. Parse options (core mask, mem size, config file)                │
│  2. Call spdk_env_init() internally                                 │
│  3. Initialize SPDK thread library                                  │
│  4. Initialize all registered subsystems (bdev, nvmf, etc.)         │
│  5. Load JSON config if provided (creates bdevs, etc.)              │
│  6. Start JSON-RPC server if configured                             │
│  7. Call user's start callback                                      │
│  8. Run reactor main loop (polling)                                 │
│  9. On shutdown: finalize subsystems, cleanup                       │
│ 10. Call spdk_app_fini() and return                                 │
└─────────────────────────────────────────────────────────────────────┘
```

**Async Executor Integration:**

For `block_on()`, we use SPDK's poller mechanism:

```rust
impl SpdkAppBuilder {
    pub fn block_on<F, T>(self, future: F) -> Result<T> {
        let output: Cell<Option<T>> = Cell::new(None);
        let output_ptr = &output as *const _ as *mut Option<T>;
        
        self.run(move || {
            // Register a poller that drives the future
            let waker = spdk_poller_waker();
            let mut future = pin!(future);
            
            // Use spdk_poller_register to poll the future
            let poller = Poller::register(move || {
                let cx = &mut Context::from_waker(&waker);
                match future.as_mut().poll(cx) {
                    Poll::Ready(result) => {
                        unsafe { *output_ptr = Some(result?); }
                        SpdkApp::stop();
                        PollStatus::Stop
                    }
                    Poll::Pending => PollStatus::Busy,
                }
            });
        })?;
        
        output.into_inner().ok_or(Error::NoOutput)
    }
}
```

### Thread API

```rust
/// Constants for message mempool sizing
pub const DEFAULT_MSG_MEMPOOL_SIZE: usize = 262144 - 1;  // Production
pub const SMALL_MSG_MEMPOOL_SIZE: usize = 1023;          // Testing (no hugepages)

// Note: thread_lib_init(), thread_lib_init_ext(), assume_thread_lib_initialized()
// are internal (pub(crate)). SpdkThread::current() and SpdkApp handle initialization
// automatically.

/// SPDK thread context - !Send + !Sync, must stay on creating OS thread.
/// This is a lightweight scheduling context, NOT an OS thread.
pub struct SpdkThread {
    ptr: NonNull<spdk_thread>,
    _marker: PhantomData<*mut ()>,  // Prevents Send/Sync
}

impl SpdkThread {
    /// Attach an SPDK thread to the current OS thread.
    /// Thread library is initialized automatically if needed.
    pub fn current(name: &str) -> Result<Self>;
    
    /// Alias for current() - for familiarity with std::thread::spawn
    pub fn new(name: &str) -> Result<Self>;
    
    /// Attach with custom message mempool size (for testing without hugepages).
    pub fn current_with_mempool_size(name: &str, size: usize) -> Result<Self>;
    pub fn new_with_mempool_size(name: &str, size: usize) -> Result<Self>;
    
    /// Get the SPDK thread attached to the current OS thread.
    pub fn get_current() -> Option<CurrentThread>;
    
    /// Get the app thread (first thread created).
    pub fn app_thread() -> Option<CurrentThread>;
    
    /// Poll to process messages and run pollers. Returns work count.
    pub fn poll(&self) -> i32;
    pub fn poll_max(&self, max_msgs: u32) -> i32;
    
    /// Query thread state
    pub fn has_active_pollers(&self) -> bool;
    pub fn has_pollers(&self) -> bool;
    pub fn is_idle(&self) -> bool;
    pub fn is_running(&self) -> bool;
    pub fn name(&self) -> &str;
    pub fn id(&self) -> u64;
    
    /// Get total number of SPDK threads
    pub fn count() -> u32;
    
    /// Raw pointer access
    pub fn as_ptr(&self) -> *mut spdk_thread;
}

impl Drop for SpdkThread {
    fn drop(&mut self) {
        // 1. Request thread exit
        // 2. Poll until exited
        // 3. Clear current thread
        // 4. Destroy thread
        // 5. If last thread, finalize library
    }
}

/// Borrowed reference to an SPDK thread (does not own it).
/// Returned by SpdkThread::get_current().
pub struct CurrentThread {
    ptr: NonNull<spdk_thread>,
    _marker: PhantomData<*mut ()>,
}

impl CurrentThread {
    pub fn poll(&self) -> i32;
    pub fn name(&self) -> &str;
    pub fn id(&self) -> u64;
    pub fn as_ptr(&self) -> *mut spdk_thread;
}
```

### Cross-Thread Messaging

SPDK provides `spdk_thread_send_msg()` for sending callbacks between threads.
This is essential for multi-core scenarios where work needs to be dispatched
across reactor cores.

**Use cases:**
- Dispatch I/O requests to worker threads
- Return results to the requester's thread
- Execute thread-bound operations (channel ops) on the correct thread
- Load balancing across cores

```rust
/// Thread-safe handle for sending messages to an SPDK thread.
///
/// Unlike `SpdkThread` (which is `!Send`), this can be cloned and
/// sent across OS threads. Use it to dispatch work to specific
/// SPDK threads from anywhere.
///
/// Wraps `spdk_thread_send_msg()` which is thread-safe.
#[derive(Clone)]
pub struct ThreadHandle {
    ptr: *const spdk_thread,
}

// Safe to send/share - spdk_thread_send_msg is thread-safe
unsafe impl Send for ThreadHandle {}
unsafe impl Sync for ThreadHandle {}

impl ThreadHandle {
    /// Send a closure to execute on the target thread.
    ///
    /// Returns immediately. The closure runs during the target's next poll.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let main_handle = main_thread.handle();
    ///
    /// SpdkThread::spawn("worker", move |worker| {
    ///     // Do work on worker thread
    ///     let result = compute_something();
    ///
    ///     // Send result back to main thread
    ///     main_handle.send(move || {
    ///         println!("Result: {}", result);
    ///     });
    ///
    ///     for _ in 0..100 { worker.poll(); }
    /// });
    /// ```
    pub fn send<F>(&self, f: F)
    where
        F: FnOnce() + Send + 'static;

    /// Send a closure and await the result.
    ///
    /// Must be called from within an SPDK thread context.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Call a function on another thread and get result
    /// let result = worker_handle.call(|| expensive_computation()).await;
    /// ```
    pub async fn call<F, T>(&self, f: F) -> T
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static;

    /// Get the target thread's ID.
    pub fn id(&self) -> u64;
}

impl SpdkThread {
    /// Get a clonable, thread-safe handle for message passing.
    ///
    /// The handle can be sent to other threads and used to dispatch
    /// work back to this thread.
    pub fn handle(&self) -> ThreadHandle;
}
```

**Implementation notes:**
- `send()` boxes the closure and passes pointer via `spdk_thread_send_msg()`
- `call()` uses a oneshot channel: sends closure that sends result back
- Handle is cheap to clone (just a pointer)

### I/O Channel Design

SPDK uses **per-thread I/O channels** for lock-free I/O submission. Each channel is:
- Bound to the OS thread that created it
- Reference-counted (getting the same channel twice returns the same pointer)
- Released asynchronously via `spdk_put_io_channel()`

**SPDK APIs:**
- `spdk_bdev_get_io_channel(desc)` - Get channel for block device
- `spdk_bs_alloc_io_channel(bs)` - Get channel for blobstore  
- `spdk_get_io_channel(io_device)` - Generic (rarely used directly)
- `spdk_put_io_channel(ch)` - Release channel
- `spdk_io_channel_get_thread(ch)` - Get thread owning channel

```rust
/// Per-thread I/O channel.
/// 
/// Must be created and used on the same OS thread. Implements Drop
/// to release via spdk_put_io_channel().
/// 
/// # Thread Safety
/// `!Send + !Sync` - must stay on creating thread.
pub struct IoChannel {
    ptr: NonNull<spdk_io_channel>,
    _marker: PhantomData<*mut ()>,
}

impl IoChannel {
    /// Get the thread this channel is bound to.
    pub fn thread(&self) -> CurrentThread;
    
    /// Get the raw pointer.
    pub fn as_ptr(&self) -> *mut spdk_io_channel;
}

impl Drop for IoChannel {
    fn drop(&mut self) {
        unsafe { spdk_put_io_channel(self.ptr.as_ptr()) };
    }
}
```

### Block Device API

```rust
/// Block device handle (does not own the device).
/// 
/// Obtained via `Bdev::get_by_name()` after bdevs are created via JSON config.
/// The bdev itself is managed by SPDK's bdev layer.
/// 
/// # Creating Bdevs
/// Bdevs are created at SPDK init time via `SpdkAppBuilder::config_file()`:
/// ```rust
/// // config.json:
/// // {"subsystems": [{"subsystem": "bdev", "config": [
/// //   {"method": "bdev_null_create", "params": {"name": "Null0", "num_blocks": 262144, "block_size": 512}}
/// // ]}]}
/// 
/// SpdkApp::builder()
///     .name("app")
///     .config_file("./config.json")
///     .mem_size_mb(512)
///     .run(|| {
///         let bdev = Bdev::get_by_name("Null0").unwrap();
///         // ... use bdev
///         SpdkApp::stop();
///     })?;
/// ```
/// 
/// # Thread Safety
/// `!Send + !Sync` - conservative default, may relax later.
pub struct Bdev {
    ptr: NonNull<spdk_bdev>,
    _marker: PhantomData<*mut ()>,
}

impl Bdev {
    /// Look up a bdev by name.
    /// 
    /// Returns `None` if no bdev with that name exists.
    pub fn get_by_name(name: &str) -> Option<Self>;
    
    /// Open this bdev for I/O.
    /// 
    /// # Arguments
    /// * `write` - true for read/write access, false for read-only
    pub fn open(&self, write: bool) -> Result<BdevDesc>;
    
    /// Get bdev name.
    pub fn name(&self) -> &str;
    
    /// Get block size in bytes.
    pub fn block_size(&self) -> u32;
    
    /// Get number of blocks.
    pub fn num_blocks(&self) -> u64;
    
    /// Get total size in bytes.
    pub fn size_bytes(&self) -> u64;
    
    /// Get raw pointer.
    pub fn as_ptr(&self) -> *mut spdk_bdev;
}

/// Open descriptor to a bdev (like a file descriptor).
/// 
/// Use `get_io_channel()` to obtain a thread-local channel for I/O.
/// Must be closed on the same thread it was opened on.
/// 
/// # Thread Safety
/// `!Send + !Sync` - must stay on opening thread for close.
pub struct BdevDesc {
    ptr: NonNull<spdk_bdev_desc>,
    _marker: PhantomData<*mut ()>,
}

impl BdevDesc {
    /// Get an I/O channel for this descriptor on the current thread.
    pub fn get_io_channel(&self) -> Result<IoChannel>;
    
    /// Get the underlying bdev.
    pub fn bdev(&self) -> Bdev;
    
    /// Get raw pointer.
    pub fn as_ptr(&self) -> *mut spdk_bdev_desc;
}

impl Drop for BdevDesc {
    fn drop(&mut self) {
        // Must be called on same thread as open
        unsafe { spdk_bdev_close(self.ptr.as_ptr()) };
    }
}
```


### Blobstore API

```rust
/// Blobstore instance.
/// 
/// Use `alloc_io_channel()` to get a thread-local channel.
pub struct Blobstore {
    ptr: NonNull<spdk_blob_store>,
}

impl Blobstore {
    /// Allocate an I/O channel for this blobstore on the current thread.
    pub fn alloc_io_channel(&self) -> Result<IoChannel> {
        let ch = unsafe { spdk_bs_alloc_io_channel(self.ptr.as_ptr()) };
        NonNull::new(ch)
            .map(|ptr| IoChannel { ptr, _marker: PhantomData })
            .ok_or(Error::ChannelAlloc)
    }
}

/// Blob handle.
pub struct Blob {
    ptr: NonNull<spdk_blob>,
}

/// Blob identifier.
pub struct BlobId(spdk_blob_id);
```

### NVMe API

Direct NVMe access bypassing the bdev layer. Use this for:
- Custom admin commands
- Namespace management
- Maximum performance (bypasses bdev abstraction)
- E2E tests with real NVMe devices

#### Core Types

```rust
/// NVMe transport identifier.
/// 
/// Identifies how to connect to an NVMe controller (PCIe, TCP, RDMA, etc.)
#[derive(Debug, Clone)]
pub struct TransportId {
    inner: spdk_nvme_transport_id,
}

impl TransportId {
    /// Create a PCIe transport ID from BDF address.
    /// 
    /// # Example
    /// 
    /// ```no_run
    /// use spdk_io::nvme::TransportId;
    /// 
    /// // Connect to NVMe at PCI address 0000:00:04.0
    /// let trid = TransportId::pcie("0000:00:04.0")?;
    /// ```
    pub fn pcie(addr: &str) -> Result<Self>;
    
    /// Create a TCP transport ID.
    /// 
    /// # Example
    /// 
    /// ```no_run
    /// let trid = TransportId::tcp("192.168.1.100", "4420", "nqn.2024-01.io.spdk:cnode1")?;
    /// ```
    pub fn tcp(addr: &str, port: &str, subnqn: &str) -> Result<Self>;
    
    /// Create a RDMA transport ID.
    pub fn rdma(addr: &str, port: &str, subnqn: &str) -> Result<Self>;
    
    /// Parse from string (SPDK format).
    /// 
    /// Format: `trtype:PCIe traddr:0000:00:04.0`
    pub fn parse(s: &str) -> Result<Self>;
}
```

```rust
/// NVMe controller handle.
/// 
/// Represents a connected NVMe controller. Obtained via [`connect()`](Self::connect)
/// or [`probe()`](Self::probe).
/// 
/// # Thread Safety
/// 
/// `!Send + !Sync` - controller operations must remain on the thread that connected.
/// 
/// # Example
/// 
/// ```no_run
/// use spdk_io::{SpdkApp, nvme::{NvmeController, TransportId}};
/// 
/// SpdkApp::builder()
///     .name("nvme_test")
///     .run(|| {
///         let trid = TransportId::pcie("0000:00:04.0").unwrap();
///         let ctrlr = NvmeController::connect(&trid, None).unwrap();
///         
///         println!("Controller: {} namespaces", ctrlr.num_namespaces());
///         
///         let ns = ctrlr.namespace(1).unwrap();
///         println!("NS1: {} sectors, {} bytes/sector", 
///                  ns.num_sectors(), ns.sector_size());
///         
///         SpdkApp::stop();
///     })
///     .unwrap();
/// ```
pub struct NvmeController {
    ptr: NonNull<spdk_nvme_ctrlr>,
    _marker: PhantomData<*mut ()>, // !Send + !Sync
}

impl NvmeController {
    /// Connect to an NVMe controller.
    /// 
    /// This is the synchronous connect API. For multiple controllers,
    /// prefer [`probe()`](Self::probe) which can discover and attach
    /// to multiple controllers efficiently.
    /// 
    /// # Arguments
    /// 
    /// * `trid` - Transport identifier (PCIe, TCP, RDMA)
    /// * `opts` - Optional controller options (queue depth, etc.)
    /// 
    /// # Errors
    /// 
    /// Returns error if connection fails (device not found, permission denied, etc.)
    pub fn connect(trid: &TransportId, opts: Option<&NvmeCtrlrOpts>) -> Result<Self>;
    
    /// Connect to an NVMe controller asynchronously.
    /// 
    /// Returns a future that completes when connection is established.
    pub async fn connect_async(trid: &TransportId, opts: Option<&NvmeCtrlrOpts>) -> Result<Self>;
    
    /// Probe for NVMe controllers matching the transport ID.
    /// 
    /// This scans for controllers and calls the callback for each one found.
    /// More efficient than multiple [`connect()`](Self::connect) calls.
    /// 
    /// # Arguments
    /// 
    /// * `trid` - Transport ID filter (None = probe all)
    /// * `probe_cb` - Called for each controller found; return true to attach
    /// * `attach_cb` - Called when controller is attached
    pub fn probe<F, G>(trid: Option<&TransportId>, probe_cb: F, attach_cb: G) -> Result<()>
    where
        F: FnMut(&TransportId, &NvmeCtrlrOpts) -> bool,
        G: FnMut(&TransportId, NvmeController);
    
    /// Get the number of namespaces.
    /// 
    /// Note: Some namespace IDs may be inactive.
    pub fn num_namespaces(&self) -> u32;
    
    /// Get a namespace by ID (1-indexed).
    /// 
    /// Returns `None` if the namespace ID is invalid or inactive.
    pub fn namespace(&self, ns_id: u32) -> Option<NvmeNamespace>;
    
    /// Allocate an I/O queue pair for submitting commands.
    /// 
    /// Each thread should have its own qpair for lock-free I/O.
    pub fn alloc_io_qpair(&self, opts: Option<&NvmeQpairOpts>) -> Result<NvmeQpair>;
    
    /// Process admin command completions.
    /// 
    /// Call periodically to process admin command responses and keep-alive.
    pub fn process_admin_completions(&self) -> i32;
    
    /// Get controller data (identify controller).
    pub fn data(&self) -> &spdk_nvme_ctrlr_data;
    
    /// Get transport ID used to connect.
    pub fn transport_id(&self) -> TransportId;
}

impl Drop for NvmeController {
    fn drop(&mut self) {
        // Detach from controller
        unsafe { spdk_nvme_detach(self.ptr.as_ptr()) };
    }
}
```

```rust
/// NVMe namespace handle.
/// 
/// Represents a namespace on a controller. Obtained via 
/// [`NvmeController::namespace()`].
/// 
/// # Lifetime
/// 
/// The namespace is borrowed from the controller and becomes invalid
/// when the controller is dropped.
pub struct NvmeNamespace<'a> {
    ptr: NonNull<spdk_nvme_ns>,
    ctrlr: &'a NvmeController,
}

impl<'a> NvmeNamespace<'a> {
    /// Get namespace ID.
    pub fn id(&self) -> u32;
    
    /// Get sector size in bytes.
    pub fn sector_size(&self) -> u32;
    
    /// Get total number of sectors.
    pub fn num_sectors(&self) -> u64;
    
    /// Get total size in bytes.
    pub fn size(&self) -> u64;
    
    /// Get maximum I/O transfer size in bytes.
    pub fn max_io_xfer_size(&self) -> u32;
    
    /// Check if namespace is active.
    pub fn is_active(&self) -> bool;
    
    /// Submit a read command.
    /// 
    /// # Arguments
    /// 
    /// * `qpair` - Queue pair for submission
    /// * `buf` - DMA buffer to read into
    /// * `lba` - Starting logical block address
    /// * `num_blocks` - Number of blocks to read
    /// 
    /// # Example
    /// 
    /// ```no_run
    /// let ns = ctrlr.namespace(1).unwrap();
    /// let qpair = ctrlr.alloc_io_qpair(None).unwrap();
    /// let mut buf = DmaBuf::alloc(4096, 4096)?;
    /// 
    /// // Read block 0
    /// ns.read(&qpair, &mut buf, 0, 1).await?;
    /// ```
    pub async fn read(&self, qpair: &NvmeQpair, buf: &mut DmaBuf, lba: u64, num_blocks: u32) -> Result<()>;
    
    /// Submit a write command.
    /// 
    /// # Arguments
    /// 
    /// * `qpair` - Queue pair for submission
    /// * `buf` - DMA buffer with data to write
    /// * `lba` - Starting logical block address
    /// * `num_blocks` - Number of blocks to write
    pub async fn write(&self, qpair: &NvmeQpair, buf: &DmaBuf, lba: u64, num_blocks: u32) -> Result<()>;
    
    /// Submit a flush command.
    pub async fn flush(&self, qpair: &NvmeQpair) -> Result<()>;
}
```

```rust
/// NVMe I/O queue pair.
/// 
/// Used to submit I/O commands to a namespace. Each thread should
/// have its own qpair for lock-free operation.
/// 
/// # Thread Safety
/// 
/// `!Send + !Sync` - qpair must stay on the allocating thread.
pub struct NvmeQpair {
    ptr: NonNull<spdk_nvme_qpair>,
    ctrlr_ptr: *mut spdk_nvme_ctrlr, // For freeing
    _marker: PhantomData<*mut ()>,
}

impl NvmeQpair {
    /// Process I/O completions.
    /// 
    /// Call this to check for completed I/O operations. Returns the
    /// number of completions processed.
    /// 
    /// # Arguments
    /// 
    /// * `max_completions` - Max completions to process (0 = unlimited)
    pub fn process_completions(&self, max_completions: u32) -> i32;
}

impl Drop for NvmeQpair {
    fn drop(&mut self) {
        unsafe { spdk_nvme_ctrlr_free_io_qpair(self.ptr.as_ptr()) };
    }
}
```

```rust
/// NVMe controller options.
#[derive(Debug, Default, Clone)]
pub struct NvmeCtrlrOpts {
    /// Number of I/O queues to request
    pub num_io_queues: Option<u32>,
    /// I/O queue depth
    pub io_queue_size: Option<u32>,
    /// Admin queue depth  
    pub admin_queue_size: Option<u16>,
    /// Keep-alive timeout in ms (0 = disabled)
    pub keep_alive_timeout_ms: Option<u32>,
}

/// NVMe queue pair options.
#[derive(Debug, Default, Clone)]
pub struct NvmeQpairOpts {
    /// Queue depth
    pub io_queue_size: Option<u32>,
    /// Queue requests
    pub io_queue_requests: Option<u32>,
}
```

#### Async I/O Implementation

The async read/write methods use the same completion pattern as bdev:

```rust
impl<'a> NvmeNamespace<'a> {
    pub async fn read(
        &self, 
        qpair: &NvmeQpair, 
        buf: &mut DmaBuf, 
        lba: u64, 
        num_blocks: u32
    ) -> Result<()> {
        let (tx, rx) = completion();
        
        let rc = unsafe {
            spdk_nvme_ns_cmd_read(
                self.ptr.as_ptr(),
                qpair.ptr.as_ptr(),
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
}

/// C callback for NVMe I/O completion.
unsafe extern "C" fn nvme_io_complete(ctx: *mut c_void, cpl: *const spdk_nvme_cpl) {
    let tx = unsafe { CompletionSender::<()>::from_raw(ctx) };
    
    let cpl = unsafe { &*cpl };
    if spdk_nvme_cpl_is_success(cpl) {
        tx.success(());
    } else {
        // Extract status code for error reporting
        let sct = cpl.status.sct();
        let sc = cpl.status.sc();
        tx.error(Error::NvmeError { sct, sc });
    }
}
```

#### Error Types

Additional error variant for NVMe-specific errors:

```rust
#[derive(Debug, thiserror::Error)]
pub enum Error {
    // ... existing variants ...
    
    #[error("NVMe error: SCT={sct}, SC={sc}")]
    NvmeError { sct: u8, sc: u8 },
    
    #[error("NVMe controller not found")]
    ControllerNotFound,
    
    #[error("NVMe namespace not found: {0}")]
    NamespaceNotFound(u32),
    
    #[error("NVMe qpair allocation failed")]
    QpairAlloc,
}
```

#### E2E Test Example

```rust
//! tests/e2e/nvme_test.rs
use spdk_io::{SpdkApp, DmaBuf, nvme::{NvmeController, TransportId}};

#[test]
fn test_nvme_read_write() {
    // NVMe PCI address from test environment (bound to vfio-pci)
    let pci_addr = std::env::var("NVME_PCI_ADDR")
        .unwrap_or_else(|_| "0000:00:04.0".to_string());
    
    SpdkApp::builder()
        .name("nvme_e2e_test")
        .mem_size_mb(256)
        .run_async(async {
            // Connect to NVMe controller
            let trid = TransportId::pcie(&pci_addr).unwrap();
            let ctrlr = NvmeController::connect(&trid, None).unwrap();
            
            // Get namespace 1
            let ns = ctrlr.namespace(1).expect("Namespace 1 not found");
            let sector_size = ns.sector_size() as usize;
            
            // Allocate qpair and buffer
            let qpair = ctrlr.alloc_io_qpair(None).unwrap();
            let mut buf = DmaBuf::alloc(sector_size, sector_size).unwrap();
            
            // Write test pattern
            buf.as_mut_slice().fill(0xAB);
            ns.write(&qpair, &buf, 0, 1).await.unwrap();
            
            // Read back
            buf.as_mut_slice().fill(0x00);
            ns.read(&qpair, &mut buf, 0, 1).await.unwrap();
            
            // Verify
            assert!(buf.as_slice().iter().all(|&b| b == 0xAB));
            
            SpdkApp::stop();
        })
        .unwrap();
}
```

#### Module Structure

```
spdk-io/src/
├── nvme/
│   ├── mod.rs        # Module exports
│   ├── controller.rs # NvmeController
│   ├── namespace.rs  # NvmeNamespace
│   ├── qpair.rs      # NvmeQpair
│   ├── transport.rs  # TransportId
│   └── opts.rs       # NvmeCtrlrOpts, NvmeQpairOpts
└── lib.rs            # pub mod nvme
```

### NVMf Target API (In-Process Testing)

SPDK exposes the NVMe-oF target as a library, allowing target + initiator to run
**in the same process**. This enables NVMe driver testing without real hardware
and without running a separate daemon.

```
┌─────────────────────────────────────────────────────────────┐
│  Same Process (spdk-io test)                                │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  ┌─────────────────────┐      ┌─────────────────────────┐  │
│  │  NVMf Target        │      │  NVMe Initiator         │  │
│  │                     │      │                         │  │
│  │  Null Bdev          │◄────►│  NvmeController         │  │
│  │    ▼                │ TCP  │    ▼                    │  │
│  │  Subsystem          │loopbk│  NvmeNamespace          │  │
│  │    ▼                │      │    ▼                    │  │
│  │  TCP Listener       │      │  NvmeQpair              │  │
│  │  127.0.0.1:4420     │      │                         │  │
│  └─────────────────────┘      └─────────────────────────┘  │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

#### Core Types

```rust
/// NVMe-oF target instance.
/// 
/// Creates and manages subsystems, transports, and listeners.
pub struct NvmfTarget {
    ptr: NonNull<spdk_nvmf_tgt>,
    _marker: PhantomData<*mut ()>,
}

impl NvmfTarget {
    /// Create a new NVMf target.
    pub async fn create(name: &str) -> Result<Self>;
    
    /// Add a transport (TCP, RDMA, etc.)
    pub async fn add_transport(&self, transport: NvmfTransport) -> Result<()>;
    
    /// Create a subsystem.
    pub fn create_subsystem(&self, nqn: &str, opts: NvmfSubsystemOpts) -> Result<NvmfSubsystem>;
    
    /// Get subsystem by NQN.
    pub fn get_subsystem(&self, nqn: &str) -> Option<NvmfSubsystem>;
}

impl Drop for NvmfTarget {
    fn drop(&mut self) {
        // spdk_nvmf_tgt_destroy is async - must be called properly
    }
}
```

```rust
/// NVMf transport (TCP, RDMA, etc.)
pub struct NvmfTransport {
    ptr: NonNull<spdk_nvmf_transport>,
}

impl NvmfTransport {
    /// Create a TCP transport.
    pub async fn tcp(opts: Option<&NvmfTransportOpts>) -> Result<Self>;
    
    /// Create an RDMA transport.
    pub async fn rdma(opts: Option<&NvmfTransportOpts>) -> Result<Self>;
}

/// Transport options.
#[derive(Debug, Default, Clone)]
pub struct NvmfTransportOpts {
    pub max_io_size: Option<u32>,
    pub io_unit_size: Option<u32>,
    pub max_qpairs_per_ctrlr: Option<u16>,
    pub in_capsule_data_size: Option<u32>,
}
```

```rust
/// NVMf subsystem.
/// 
/// Represents a namespace container that can be exported to initiators.
pub struct NvmfSubsystem {
    ptr: NonNull<spdk_nvmf_subsystem>,
}

impl NvmfSubsystem {
    /// Add a bdev as a namespace.
    /// 
    /// Returns the namespace ID.
    pub fn add_namespace(&self, bdev_name: &str) -> Result<u32>;
    
    /// Add a listener address.
    pub async fn add_listener(&self, trid: &TransportId) -> Result<()>;
    
    /// Allow any host to connect.
    pub fn allow_any_host(&self, allow: bool);
    
    /// Start the subsystem (begin accepting connections).
    pub async fn start(&self) -> Result<()>;
    
    /// Stop the subsystem.
    pub async fn stop(&self) -> Result<()>;
    
    /// Get the NQN.
    pub fn nqn(&self) -> &str;
}

/// Subsystem options.
#[derive(Debug, Default, Clone)]
pub struct NvmfSubsystemOpts {
    /// Serial number
    pub serial_number: Option<String>,
    /// Model number
    pub model_number: Option<String>,
    /// Allow any host
    pub allow_any_host: bool,
}
```

#### ⚠️ Threading Warning

**In-process NVMf targets can have threading issues.** Running the NVMf target and NVMe
initiator on the same SPDK thread can cause deadlocks. SPDK expects target and initiator
to run on separate reactor cores.

**Recommended:** Use the subprocess approach for testing (see below).

#### Subprocess Testing (Recommended)

For testing, spawn `nvmf_tgt` as a separate process. This avoids threading issues
and provides better isolation. See `tests/nvmf_test.rs` for the full implementation.

```rust
//! tests/nvmf_test.rs - Subprocess approach (recommended)

/// Helper to manage nvmf_tgt subprocess
mod nvmf_subprocess {
    pub struct NvmfTarget { /* ... */ }
    
    impl NvmfTarget {
        /// Clean up stale processes from previous runs via PID file
        pub fn cleanup_stale(port: u16);
        
        /// Start nvmf_tgt subprocess, configure via RPC
        pub fn start(port: u16) -> Result<(Self, String), String>;
    }
}

#[test]
fn test_nvmf_subprocess() {
    const TEST_PORT: u16 = 4421;
    
    // Clean up any stale processes
    nvmf_subprocess::NvmfTarget::cleanup_stale(TEST_PORT);
    
    // Start nvmf_tgt as subprocess (configures bdev, transport, subsystem via RPC)
    let (target, nqn) = nvmf_subprocess::NvmfTarget::start(TEST_PORT).unwrap();
    
    SpdkApp::builder()
        .name("test")
        .no_pci(true)
        .no_huge(true)
        .mem_size_mb(1024)
        .run(|| {
            // Connect to external nvmf_tgt
            let trid = TransportId::tcp("127.0.0.1", &TEST_PORT.to_string(), &nqn).unwrap();
            let ctrlr = NvmeController::connect(&trid, None).unwrap();
            
            // Perform I/O...
            
            SpdkApp::stop();
        })
        .unwrap();
    
    // target dropped here, kills subprocess
}
```

#### In-Process Example (Use with Caution)

```rust
//! WARNING: May have threading issues. Use subprocess approach instead.
use spdk_io::{SpdkApp, DmaBuf};
use spdk_io::nvme::{NvmeController, TransportId};
use spdk_io::nvmf::{NvmfTarget, NvmfTransport, NvmfSubsystemOpts};

#[test]
fn test_nvme_over_tcp_loopback() {
    // This test works with --no-huge (vdev mode)!
    SpdkApp::builder()
        .name("nvmf_loopback")
        .no_huge(true)
        .mem_size_mb(256)
        .json_data(r#"{
            "subsystems": [{
                "subsystem": "bdev",
                "config": [{
                    "method": "bdev_null_create",
                    "params": {"name": "Null0", "num_blocks": 1024, "block_size": 512}
                }]
            }]
        }"#)
        .run_async(async {
            // === Set up NVMf target ===
            let tgt = NvmfTarget::create("test_tgt").await.unwrap();
            
            // Add TCP transport
            let transport = NvmfTransport::tcp(None).await.unwrap();
            tgt.add_transport(transport).await.unwrap();
            
            // Create subsystem
            let subsys = tgt.create_subsystem(
                "nqn.2024-01.io.spdk:test",
                NvmfSubsystemOpts {
                    allow_any_host: true,
                    ..Default::default()
                }
            ).unwrap();
            
            // Add null bdev as namespace
            subsys.add_namespace("Null0").unwrap();
            
            // Add TCP listener on loopback
            let listen_trid = TransportId::tcp("127.0.0.1", "4420", "").unwrap();
            subsys.add_listener(&listen_trid).await.unwrap();
            
            // Start subsystem
            subsys.start().await.unwrap();
            
            // === Connect as initiator ===
            let connect_trid = TransportId::tcp(
                "127.0.0.1", 
                "4420", 
                "nqn.2024-01.io.spdk:test"
            ).unwrap();
            let ctrlr = NvmeController::connect(&connect_trid, None).unwrap();
            
            // Get namespace and qpair
            let ns = ctrlr.namespace(1).expect("NS1 not found");
            let qpair = ctrlr.alloc_io_qpair(None).unwrap();
            let mut buf = DmaBuf::alloc(512, 512).unwrap();
            
            // Write test pattern
            buf.as_mut_slice().fill(0xCD);
            ns.write(&qpair, &buf, 0, 1).await.unwrap();
            
            // Read back and verify
            buf.as_mut_slice().fill(0x00);
            ns.read(&qpair, &mut buf, 0, 1).await.unwrap();
            assert!(buf.as_slice().iter().all(|&b| b == 0xCD));
            
            // Cleanup (drop order matters)
            drop(qpair);
            drop(ctrlr);
            subsys.stop().await.unwrap();
            
            SpdkApp::stop();
        })
        .unwrap();
}
```

#### Module Structure

```
spdk-io/src/
├── nvmf/
│   ├── mod.rs        # Module exports
│   ├── target.rs     # NvmfTarget
│   ├── transport.rs  # NvmfTransport
│   ├── subsystem.rs  # NvmfSubsystem
│   └── opts.rs       # NvmfTransportOpts, NvmfSubsystemOpts
└── lib.rs            # pub mod nvmf
```

### Error Handling

```rust
#[derive(Debug, thiserror::Error)]
pub enum SpdkError {
    #[error("SPDK error: {0}")]
    Errno(#[from] nix::errno::Errno),
    
    #[error("I/O operation failed")]
    IoError,
    
    #[error("Device not found: {0}")]
    DeviceNotFound(String),
    
    #[error("Channel allocation failed")]
    ChannelAlloc,
    
    #[error("Operation cancelled")]
    Cancelled,
    
    // ... more variants
}

pub type Result<T> = std::result::Result<T, SpdkError>;
```

## Memory Management

### DMA Buffers

All I/O buffers must be DMA-capable (allocated via SPDK):

```rust
impl DmaBuf {
    /// Allocate DMA buffer
    pub fn alloc(size: usize, align: usize) -> Result<Self>;
    
    /// Allocate zeroed DMA buffer
    pub fn alloc_zeroed(size: usize, align: usize) -> Result<Self>;
    
    /// Get slice view
    pub fn as_slice(&self) -> &[u8];
    
    /// Get mutable slice view
    pub fn as_mut_slice(&mut self) -> &mut [u8];
}

impl Drop for DmaBuf {
    fn drop(&mut self) {
        unsafe { spdk_dma_free(self.ptr) };
    }
}
```

### Resource Cleanup

All SPDK resources implement `Drop` for RAII cleanup:

```rust
impl Drop for BdevDesc {
    fn drop(&mut self) {
        unsafe { spdk_bdev_close(self.ptr) };
    }
}

impl Drop for IoChannel {
    fn drop(&mut self) {
        unsafe { spdk_put_io_channel(self.ptr) };
    }
}
```

## Thread Safety

### Send/Sync Considerations

- `SpdkThread`: `!Send + !Sync` (bound to OS thread)
- `Bdev`: `!Send + !Sync` (conservative default, may relax later)
- `BdevDesc`: `!Send + !Sync` (must close on opening thread)
- `IoChannel`: `!Send + !Sync` (must stay on creating thread)
- `DmaBuf`: `Send` (can be moved, but not shared during I/O)

### Explicit Handle Model

No thread-local statics needed. Channels are obtained from the device, not the thread:

```rust
/// SPDK thread handle - !Send + !Sync, bound to creating OS thread
/// 
/// IMPLEMENTED:
/// - Lightweight scheduling context (not an OS thread)
/// - Polling for message processing and poller execution
/// - Thread state queries (is_idle, is_running, etc.)
/// - Automatic cleanup on drop
/// 
/// PLANNED:
/// - SpdkThread::spawn() for spawning new OS thread + SPDK thread
pub struct SpdkThread {
    ptr: NonNull<spdk_thread>,
    _marker: PhantomData<*mut ()>,
}

impl SpdkThread {
    // === IMPLEMENTED ===
    
    pub fn current(name: &str) -> Result<Self>;
    pub fn new(name: &str) -> Result<Self>;
    pub fn current_with_mempool_size(name: &str, size: usize) -> Result<Self>;
    pub fn get_current() -> Option<CurrentThread>;
    pub fn app_thread() -> Option<CurrentThread>;
    pub fn poll(&self) -> i32;
    pub fn poll_max(&self, max_msgs: u32) -> i32;
    pub fn has_active_pollers(&self) -> bool;
    pub fn has_pollers(&self) -> bool;
    pub fn is_idle(&self) -> bool;
    pub fn is_running(&self) -> bool;
    pub fn name(&self) -> &str;
    pub fn id(&self) -> u64;
    pub fn count() -> u32;
    
    // === PLANNED ===
    
    pub fn spawn<F, T>(name: &str, f: F) -> JoinHandle<T>
    where
        F: FnOnce(&SpdkThread) -> T + Send + 'static,
        T: Send + 'static;
}

// Channel acquisition is per-device, not per-thread:

impl BdevDesc {
    /// Get thread-local I/O channel for this device
    pub fn get_io_channel(&self) -> Result<IoChannel>;
}

impl Blobstore {
    /// Allocate thread-local I/O channel for this blobstore
    pub fn alloc_io_channel(&self) -> Result<IoChannel>;
}
```

### Usage Pattern

```rust
use spdk_io::{SpdkEnv, SpdkThread};

fn main() {
    let _env = SpdkEnv::init().unwrap();
    
    // Option 1: Attach to current OS thread (no new thread)
    let thread = SpdkThread::current("main").unwrap();
    run_with_thread(&thread);
    
    // Option 2: Spawn new thread (like std::thread::spawn) - PLANNED
    let handle = SpdkThread::spawn("worker", |thread| {
        run_with_thread(thread)
    });
    handle.join().unwrap();
}

fn run_with_thread(thread: &SpdkThread) -> Result<()> {
    let ex = smol::LocalExecutor::new();
    futures_lite::future::block_on(ex.run(async {
        ex.spawn(poller_task(thread)).detach();
        
        // Get device and open it (sync)
        let bdev = Bdev::get_by_name("Nvme0n1")
            .ok_or(Error::DeviceNotFound("Nvme0n1".into()))?;
        let desc = bdev.open(true)?;
        
        // Get I/O channel FROM THE DESCRIPTOR (not the thread)
        let channel = desc.get_io_channel()?;
        
        // Use channel for I/O (async read/write - future work)
        let mut buf = DmaBuf::alloc(4096, 4096)?;
        desc.read(&channel, &mut buf, 0, 4096).await?;
        
        Ok(())
    }))
}
```

## Testing

### Virtual Block Devices (vdevs)

SPDK provides virtual bdev modules for testing without real NVMe hardware:

| Module | Description | Use Case |
|--------|-------------|----------|
| **Malloc** | RAM-backed block device | Unit tests, no persistence |
| **Null** | Discards writes, returns zeros | Throughput benchmarks |
| **Error** | Injects I/O errors | Failure path testing |
| **Delay** | Adds configurable latency | Timeout testing |
| **AIO** | Linux AIO on regular files | File-backed tests |
| **Passthru** | Proxy to another bdev | Layer testing |

### Creating Test Bdevs

Test bdevs are created via JSON config file with `SpdkApp`:

```rust
// config.json
// {
//   "subsystems": [{
//     "subsystem": "bdev",
//     "config": [{
//       "method": "bdev_null_create",
//       "params": {"name": "Null0", "num_blocks": 262144, "block_size": 512}
//     }]
//   }]
// }

SpdkApp::builder()
    .name("test")
    .config_file("./config.json")
    .run(|| {
        // Bdev is now available
        let bdev = Bdev::get_by_name("Null0").unwrap();
        // ...
        SpdkApp::stop();
    })?;
```

JSON config methods for bdevs:
- `bdev_null_create` - discards writes, returns zeros (simplest)
- `bdev_malloc_create` - RAM-backed block device
- `bdev_error_create` - injects I/O errors
- `bdev_delay_create` - adds configurable latency

### Unit Test Example

```rust
#[cfg(test)]
mod tests {
    use spdk_io::{SpdkApp, Bdev};
    use std::fs;
    
    // Write test config to temp file
    fn create_test_config() -> String {
        let config = r#"{
            "subsystems": [{
                "subsystem": "bdev",
                "config": [{
                    "method": "bdev_null_create",
                    "params": {"name": "test0", "num_blocks": 1024, "block_size": 512}
                }]
            }]
        }"#;
        let path = "/tmp/spdk_test_config.json";
        fs::write(path, config).unwrap();
        path.to_string()
    }
    
    #[test]
    fn test_null_bdev() {
        let config_path = create_test_config();
        
        SpdkApp::builder()
            .name("test")
            .config_file(&config_path)
            .run(|| {
                // Null bdev was created via JSON config at init
                let bdev = Bdev::get_by_name("test0").unwrap();
                assert_eq!(bdev.name(), "test0");
                assert_eq!(bdev.block_size(), 512);
                assert_eq!(bdev.num_blocks(), 1024);
                
                // Open for read/write
                let desc = bdev.open(true).unwrap();
                
                // Get I/O channel
                let channel = desc.get_io_channel().unwrap();
                
                // Channel obtained successfully
                drop(channel);
                drop(desc);
                
                SpdkApp::stop();
            })
            .expect("SPDK test failed");
        
        fs::remove_file(config_path).ok();
    }
    
    #[test]
    fn test_async_bdev_io() {
        let config_path = create_test_config();
        
        SpdkApp::builder()
            .name("test_async")
            .config_file(&config_path)
            .block_on(async {
                let bdev = Bdev::get_by_name("test0").unwrap();
                let desc = bdev.open(true).unwrap();
                let channel = desc.get_io_channel().unwrap();
                
                // Async read (returns zeros for null bdev)
                let mut buf = DmaBuf::alloc(512, 512)?;
                desc.read(&channel, 0, &mut buf).await?;
                
                // Verify zeros
                assert!(buf.iter().all(|&b| b == 0));
                
                Ok(())
            })
            .expect("Async test failed");
        
        fs::remove_file(config_path).ok();
    }
}
```

### Integration Testing with Real Devices

For tests requiring actual NVMe:
```rust
#[test]
#[ignore]  // Run with: cargo test -- --ignored
fn test_with_real_nvme() {
    // Requires: sudo, NVMe device bound to SPDK
    let bdev = Bdev::get_by_name("Nvme0n1").await.unwrap();
    // ...
}
```

## Future Considerations

### Completed
- [x] spdk-io-sys bindings generation
- [x] Environment initialization (`SpdkEnv`, `SpdkApp`)
- [x] SPDK thread creation/management (`SpdkThread`, `ThreadHandle`)
- [x] Bdev open/close/read/write
- [x] Runtime-agnostic poller task (`spdk_poller`, `block_on`)
- [x] DMA buffer management (`DmaBuf`)
- [x] Callback-to-future utilities (`completion`, `io_completion`)
- [x] NVMe driver direct access (`nvme` module)
- [x] NVMf target API (`nvmf` module)
- [x] Cross-thread messaging (`ThreadHandle::send`, `ThreadHandle::call`)
- [x] Thread spawning (`SpdkThread::spawn`, `JoinHandle`)

### Planned
- [ ] Blobstore support
- [ ] Better error context with spans
- [ ] Tracing/metrics integration
- [ ] Optional Tokio/smol convenience wrappers

## Dependencies

```toml
[dependencies]
spdk-io-sys = { path = "../spdk-io-sys" }

# Async utilities (runtime-agnostic)
futures-util = "0.3"
futures-channel = "0.3"
futures-core = "0.3"

# Error handling
thiserror = "1"
nix = { version = "0.27", features = ["fs"] }

# Optional: pin utilities
pin-project-lite = "0.2"

[dev-dependencies]
# For testing with Tokio
tokio = { version = "1", features = ["rt", "macros"] }
# For testing with smol
smol = "2"
futures-lite = "2"

[build-dependencies]
bindgen = "0.69"
pkg-config = "0.3"
```

## References

- [SPDK Documentation](https://spdk.io/doc/)
- [futures-rs](https://docs.rs/futures/latest/futures/) - Core async utilities
- [futures-util](https://docs.rs/futures-util/latest/futures_util/) - Future combinators
- [Background.md](Background.md) - SPDK concepts and APIs
- [Reference.md](Reference.md) - Existing Rust SPDK projects
