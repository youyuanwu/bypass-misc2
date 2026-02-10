# Core Types

## Environment & Initialization

### SpdkEnv

```rust
/// SPDK environment guard - initialized ONCE per process
/// 
/// Like DPDK's EAL, the SPDK environment can only be initialized once.
/// This struct acts as a guard: when dropped, SPDK is cleaned up but
/// CANNOT be re-initialized (DPDK limitation).
pub struct SpdkEnv {
    _private: (),
}

impl SpdkEnv {
    pub fn init() -> Result<SpdkEnv>;
    pub fn builder() -> SpdkEnvBuilder;
}

impl Drop for SpdkEnv {
    fn drop(&mut self) {
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
```

> **Note:** `SpdkEnv` only initializes the DPDK environment. For bdev subsystem and
> JSON configuration support, use `SpdkApp` instead.

### Privilege Requirements

SPDK/DPDK typically requires elevated privileges for:
- **Hugepage access** - allocating/mapping hugepages
- **PCI device access** - binding to VFIO/UIO drivers  
- **Memory locking** - `mlockall()` to prevent DMA buffers from swapping

| Method | Command | Notes |
|--------|---------|-------|
| Root | `sudo ./app` | Simplest for development |
| Capabilities | `sudo setcap cap_ipc_lock,cap_sys_rawio+ep ./app` | Per-binary grant |
| Systemd | `AmbientCapabilities=CAP_IPC_LOCK CAP_SYS_RAWIO` | Production services |
| Pre-setup | Run `spdk/scripts/setup.sh` first | Prepares hugepages & drivers |

## Application Framework (SpdkApp)

Most SPDK applications should use the **SPDK Application Framework** which handles:

1. Environment initialization (DPDK/hugepages)
2. All subsystem initialization (bdev, nvmf, etc.)
3. JSON configuration loading (bdevs, etc.)
4. Reactor/poller infrastructure
5. Signal handling and graceful shutdown

### SpdkApp vs SpdkEnv

| Feature | `SpdkEnv` (low-level) | `SpdkApp` (framework) |
|---------|----------------------|----------------------|
| Init via | `spdk_env_init()` | `spdk_app_start()` |
| Subsystems | Manual init required | Auto-initialized |
| JSON config | Not supported | Native support |
| Bdev creation | Need internal headers | Via JSON config |
| Main loop | User-managed | Framework-managed |
| Use case | Embedding, custom apps | Typical SPDK apps |

### SpdkAppBuilder

```rust
impl SpdkAppBuilder {
    pub fn name(self, name: &str) -> Self;
    pub fn config_file(self, path: &str) -> Self;
    pub fn json_data(self, json: &str) -> Self;
    pub fn reactor_mask(self, mask: &str) -> Self;
    pub fn main_core(self, core: i32) -> Self;
    pub fn mem_size_mb(self, mb: i32) -> Self;
    pub fn no_pci(self, no_pci: bool) -> Self;
    pub fn no_huge(self, no_huge: bool) -> Self;
    pub fn rpc_addr(self, addr: &str) -> Self;
    
    /// Run with synchronous callback
    pub fn run<F>(self, f: F) -> Result<()>
    where F: FnOnce() + 'static;
    
    /// Run with async main function
    pub fn run_async<F, Fut>(self, f: F) -> Result<()>
    where
        F: FnOnce() -> Fut + 'static,
        Fut: Future<Output = ()> + 'static;
}
```

### Example

```rust
SpdkApp::builder()
    .name("my_app")
    .json_data(r#"{
        "subsystems": [{
            "subsystem": "bdev",
            "config": [{
                "method": "bdev_null_create",
                "params": {"name": "Null0", "num_blocks": 1024, "block_size": 512}
            }]
        }]
    }"#)
    .run(|| {
        let bdev = Bdev::get_by_name("Null0").unwrap();
        // ... use bdev
        SpdkApp::stop();
    })?;
```

## Thread API

### SpdkThread

An `SpdkThread` is a lightweight scheduling context (like a green thread), NOT an OS thread.
It runs on whatever OS thread calls `poll()` on it.

```rust
/// SPDK thread context - !Send + !Sync, must stay on creating OS thread.
/// This is a lightweight scheduling context, NOT an OS thread.
pub struct SpdkThread {
    ptr: NonNull<spdk_thread>,
    _marker: PhantomData<*mut ()>,
}

impl SpdkThread {
    // === Creation ===
    
    /// Attach an SPDK thread to the current OS thread (initializes thread lib if needed)
    pub fn current(name: &str) -> Result<Self>;
    
    /// Alias for current()
    pub fn new(name: &str) -> Result<Self>;
    
    /// Attach without initializing thread lib (use when already initialized by SpdkApp)
    pub fn attach(name: &str) -> Result<Self>;
    
    /// Attach with custom mempool size (for testing without hugepages)
    pub fn current_with_mempool_size(name: &str, size: usize) -> Result<Self>;
    pub fn new_with_mempool_size(name: &str, size: usize) -> Result<Self>;
    
    // === Thread Lookup ===
    
    /// Get SPDK thread attached to current OS thread
    pub fn get_current() -> Option<CurrentThread>;
    
    /// Get the app thread (first thread created, ID=1)
    pub fn app_thread() -> Option<CurrentThread>;
    
    /// Get total number of SPDK threads
    pub fn count() -> u32;
    
    // === Polling ===
    
    /// Poll to process messages and run pollers. Returns work count.
    pub fn poll(&self) -> i32;
    
    /// Poll with max messages limit
    pub fn poll_max(&self, max_msgs: u32) -> i32;
    
    // === State Queries ===
    
    pub fn has_active_pollers(&self) -> bool;
    pub fn has_pollers(&self) -> bool;
    pub fn is_idle(&self) -> bool;
    pub fn is_running(&self) -> bool;
    pub fn name(&self) -> &str;
    pub fn id(&self) -> u64;
    
    // === Thread Spawning ===
    
    /// Spawn a new OS thread with an SPDK thread attached
    pub fn spawn<F, T>(name: &str, f: F) -> JoinHandle<T>
    where
        F: FnOnce(&SpdkThread) -> T + Send + 'static,
        T: Send + 'static;
    
    // === Cross-Thread Messaging ===
    
    /// Get a thread-safe handle for message passing
    pub fn handle(&self) -> ThreadHandle;
}
```

### CurrentThread

Borrowed reference to an SPDK thread (does not own it).

```rust
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

### JoinHandle

Handle for spawned OS thread with SPDK context.

```rust
pub struct JoinHandle<T> {
    handle: std::thread::JoinHandle<T>,
}

impl<T> JoinHandle<T> {
    /// Wait for thread to complete and return result
    pub fn join(self) -> Result<T, Box<dyn Any + Send>>;
}
```

## Cross-Thread Messaging

```rust
/// Thread-safe handle for sending messages to an SPDK thread.
/// Unlike SpdkThread (which is !Send), this can be sent across threads.
/// Uses spdk_thread_send_msg() internally.
#[derive(Clone)]
pub struct ThreadHandle {
    ptr: *mut spdk_thread,
}

unsafe impl Send for ThreadHandle {}
unsafe impl Sync for ThreadHandle {}

impl ThreadHandle {
    /// Send a closure to execute on the target thread.
    /// Returns immediately; closure runs on next poll().
    pub fn send<F>(&self, f: F)
    where F: FnOnce() + Send + 'static;

    /// Send a closure and await the result.
    pub fn call<F, T>(&self, f: F) -> CompletionReceiver<T>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static;

    pub fn id(&self) -> u64;
    pub fn as_ptr(&self) -> *mut spdk_thread;
}
```

---

## SpdkEvent

### Overview

`SpdkEvent` enables dispatching work to a specific reactor (lcore). This is the SPDK-native
way to do multi-core I/O - each reactor runs on a dedicated CPU core and processes events
from its queue.

```
┌─────────────────────────────────────────────────────────────────┐
│  SpdkApp with reactor_mask("0x3") = cores 0 and 1               │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  ┌─────────────────────┐      ┌─────────────────────────────┐  │
│  │  Reactor 0 (lcore 0)│      │  Reactor 1 (lcore 1)        │  │
│  │  ┌───────────────┐  │      │  ┌───────────────────────┐  │  │
│  │  │ Event Queue   │  │      │  │ Event Queue           │  │  │
│  │  └───────────────┘  │      │  └───────────────────────┘  │  │
│  │         ▼           │      │           ▼                 │  │
│  │  ┌───────────────┐  │      │  ┌───────────────────────┐  │  │
│  │  │ SPDK Thread   │  │      │  │ SPDK Thread           │  │  │
│  │  │ + Pollers     │  │      │  │ + Pollers             │  │  │
│  │  └───────────────┘  │      │  └───────────────────────┘  │  │
│  │         ▼           │      │           ▼                 │  │
│  │  NvmeController     │      │  NvmeController             │  │
│  │  + QpairA           │      │  + QpairB                   │  │
│  │  (LBAs 0-99)        │      │  (LBAs 100-199)             │  │
│  └─────────────────────┘      └─────────────────────────────┘  │
│                                                                 │
│       SpdkEvent::call_on(1, || { ... })  ──────────────────►   │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### API

```rust
/// Event for dispatching work to a specific reactor (lcore).
///
/// Unlike `ThreadHandle::send()` which targets a specific SPDK thread,
/// `SpdkEvent` dispatches to an lcore's reactor.
///
/// Use `SpdkEvent` when you want core affinity for I/O operations.
pub struct SpdkEvent { /* ... */ }

unsafe impl Send for SpdkEvent {}

impl SpdkEvent {
    /// Allocate an event to run on a specific lcore.
    ///
    /// The closure will execute on the target lcore's reactor thread.
    /// The event is NOT dispatched until `call()` is invoked.
    ///
    /// # Arguments
    ///
    /// * `lcore` - Target logical core ID (must be in reactor_mask)
    /// * `f` - Closure to execute on the target lcore
    ///
    /// # Errors
    ///
    /// Returns error if event allocation fails (invalid lcore, out of memory).
    pub fn new<F>(lcore: u32, f: F) -> Result<Self>
    where
        F: FnOnce() + Send + 'static;

    /// Dispatch the event to the target lcore.
    ///
    /// The event is placed on the target reactor's event queue and will
    /// be processed during the reactor's next poll cycle.
    ///
    /// This consumes the event (can only be called once).
    pub fn call(mut self);

    /// Allocate and dispatch in one step (convenience method).
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Dispatch work to lcore 1
    /// SpdkEvent::call_on(1, || {
    ///     let ctrlr = NvmeController::connect(&trid, None).unwrap();
    ///     let qpair = ctrlr.alloc_io_qpair(None).unwrap();
    ///     // I/O on lcore 1...
    /// })?;
    /// ```
    pub fn call_on<F>(lcore: u32, f: F) -> Result<()>
    where
        F: FnOnce() + Send + 'static;

    /// Dispatch and await completion.
    ///
    /// Sends the closure to the target lcore and returns a receiver that
    /// completes when the closure finishes.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Run computation on lcore 1 and get result
    /// let result = SpdkEvent::call_on_async(1, || {
    ///     expensive_computation()
    /// })?.await;
    /// ```
    pub fn call_on_async<F, T>(lcore: u32, f: F) -> Result<CompletionReceiver<T>>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static;
}
```

### Core Utilities

```rust
/// CPU core utilities for reactor management.
pub struct Cores;

impl Cores {
    /// Get current lcore ID.
    pub fn current() -> u32;

    /// Get first reactor lcore.
    pub fn first() -> u32;

    /// Get last reactor lcore.
    pub fn last() -> u32;

    /// Get total number of reactor cores.
    pub fn count() -> u32;

    /// Iterate over all reactor lcores.
    pub fn iter() -> CoreIterator;
}

/// Iterator over reactor lcores.
pub struct CoreIterator {
    current: u32,
}

impl Iterator for CoreIterator {
    type Item = u32;
    fn next(&mut self) -> Option<u32>;
}
```

### Usage Example: Multi-Queue NVMe I/O

```rust
use spdk_io::{SpdkApp, SpdkEvent, Cores};
use spdk_io::nvme::{NvmeController, TransportId};
use std::sync::{Arc, Barrier};

fn main() {
    let nqn = "nqn.2024-01.io.spdk:test";
    
    SpdkApp::builder()
        .name("multi_qpair")
        .reactor_mask("0x3")  // Cores 0 and 1
        .run(move || {
            // Connect once on core 0, share via Arc (NvmeController is Sync)
            let trid = TransportId::tcp("127.0.0.1", "4420", nqn).unwrap();
            let ctrlr = Arc::new(NvmeController::connect(&trid, None).unwrap());
            
            let barrier = Arc::new(Barrier::new(2));
            let barrier_clone = barrier.clone();
            let ctrlr_clone = ctrlr.clone();

            // === Dispatch I/O to lcore 1 ===
            SpdkEvent::call_on(1, move || {
                // Allocate qpair on core 1 - uses &self, thread-safe
                let qpair1 = ctrlr_clone.alloc_io_qpair(None).unwrap();
                // qpair1 stays on core 1, cannot be moved

                let ns = ctrlr_clone.namespace(1).unwrap();

                // Write to LBAs 100-199 on lcore 1
                // ... async I/O with poller ...

                barrier_clone.wait();
            }).unwrap();

            // === I/O on lcore 0 (current) ===
            let qpair0 = ctrlr.alloc_io_qpair(None).unwrap();
            let ns = ctrlr.namespace(1).unwrap();

            // Write to LBAs 0-99 on lcore 0
            // ... async I/O with poller ...

            barrier.wait();
            SpdkApp::stop();
        })
        .unwrap();
}
```

## I/O Channel

```rust
/// Per-thread I/O channel.
/// Must be created and used on the same OS thread.
/// 
/// # Thread Safety
/// `!Send + !Sync` - must stay on creating thread.
pub struct IoChannel {
    ptr: NonNull<spdk_io_channel>,
    _marker: PhantomData<*mut ()>,
}

impl IoChannel {
    pub fn thread(&self) -> CurrentThread;
    pub fn as_ptr(&self) -> *mut spdk_io_channel;
}

impl Drop for IoChannel {
    fn drop(&mut self) {
        unsafe { spdk_put_io_channel(self.ptr.as_ptr()) };
    }
}
```

## DMA Buffer

```rust
/// DMA-capable buffer for I/O operations.
pub struct DmaBuf {
    ptr: *mut u8,
    size: usize,
}

impl DmaBuf {
    pub fn alloc(size: usize, align: usize) -> Result<Self>;
    pub fn alloc_zeroed(size: usize, align: usize) -> Result<Self>;
    pub fn as_slice(&self) -> &[u8];
    pub fn as_mut_slice(&mut self) -> &mut [u8];
    pub fn len(&self) -> usize;
}

impl Drop for DmaBuf {
    fn drop(&mut self) {
        unsafe { spdk_dma_free(self.ptr as *mut c_void) };
    }
}
```

## Completion Utilities

```rust
/// Create a completion sender/receiver pair.
pub fn completion<T>() -> (CompletionSender<T>, CompletionReceiver<T>);

/// Block on a future, polling the SPDK thread while waiting.
pub fn block_on<F, T>(future: F) -> T
where F: Future<Output = T>;
```
