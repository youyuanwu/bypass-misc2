# Background

> **Note**: This file contains background knowledge for the spdk-io project.

## spdk-io

A Rust library for [SPDK](https://github.com/spdk/spdk) (Storage Performance Development Kit).

## What is SPDK?

The **Storage Performance Development Kit (SPDK)** provides a set of tools and libraries for writing high-performance, scalable, user-mode storage applications. It achieves high performance through three key techniques:

1. **User-space drivers**: Moving all necessary drivers into userspace avoids syscalls and enables zero-copy access from the application
2. **Polling instead of interrupts**: Polling hardware for completions instead of relying on interrupts lowers both total latency and latency variance
3. **Lock-free I/O path**: Avoiding all locks in the I/O path, instead relying on message passing

SPDK is designed primarily for "next generation" media (NVMe SSDs) that support fast random reads/writes with no required background garbage collection.

## Core Concepts

### Threading Model

SPDK uses a message-passing concurrency model instead of traditional locks:

- **`spdk_thread`**: Lightweight, stackless thread of execution. Threads are polled via `spdk_thread_poll()` and can be moved between system threads
- **`spdk_poller`**: Function that should be repeatedly called on a given thread
- **`spdk_io_device`**: Any pointer representing a device with global state
- **`spdk_io_channel`**: Per-thread context associated with an `io_device`, used for lock-free I/O submission

Instead of shared data protected by locks, SPDK assigns data to a single thread. Other threads send messages (function pointer + context) via lockless rings.

### Memory Model

- All data buffers must be allocated via `spdk_dma_malloc()` or variants (for DMA compatibility)
- Memory is not copied - Direct Memory Access transfers data directly to/from devices

### SPDK and DPDK

SPDK relies on [DPDK](https://www.dpdk.org/) (Data Plane Development Kit) for its foundational environment abstraction layer. Here's how they relate:

#### What DPDK Provides to SPDK

1. **Hugepage Memory Management**: SPDK uses DPDK to allocate pinned memory via Linux hugepages (2MiB or 1GiB pages). The kernel never changes the physical location of hugepages, which is critical for DMA operations. Regular 4KiB pages can be swapped or moved by the OS, breaking ongoing DMA transfers.

2. **Memory Pinning**: For DMA to work correctly, SPDK needs memory with:
   - Known physical addresses (for programming NVMe devices)
   - Guaranteed stable virtual-to-physical mappings (pinned, not swapped)
   
   DPDK's EAL (Environment Abstraction Layer) handles this complexity.

3. **PCI Device Access**: DPDK provides mechanisms to:
   - Unbind devices from kernel drivers
   - Bind to user-space drivers (`uio` or `vfio`)
   - Map PCI BARs into user-space for MMIO

4. **Lockless Ring Buffers**: SPDK uses DPDK's proven lockless ring implementation for message passing between threads.

5. **IOMMU/VFIO Support**: DPDK integrates with Linux VFIO for IOMMU programming, providing memory-safe DMA with virtual addresses.

#### The Environment Abstraction (`spdk_env`)

SPDK wraps DPDK functionality in its environment abstraction layer (`<spdk/env.h>`):

```c
spdk_env_opts_init()       // Initialize env options (wraps DPDK EAL options)
spdk_env_init()            // Initialize environment (calls DPDK rte_eal_init)
spdk_dma_malloc()          // Allocate DMA memory (uses DPDK hugepage memory)
spdk_dma_free()            // Free DMA memory
spdk_pci_enumerate()       // Enumerate PCI devices
```

#### Why This Matters

When SPDK applications start, they typically:
1. Reserve hugepages (`scripts/setup.sh` or `HUGEMEM` environment variable)
2. Unbind NVMe devices from kernel `nvme` driver
3. Bind devices to `vfio-pci` or `uio_pci_generic`
4. Initialize DPDK EAL via `spdk_env_init()`

This is why:
- SPDK apps require root or specific capabilities
- Devices disappear from `/dev/nvme*` when SPDK takes control
- Memory must come from `spdk_dma_malloc()`, not regular `malloc()`

#### DPDK as Optional Dependency

While DPDK is the default and recommended environment, SPDK's `env` layer is designed as an abstraction. The environment layer could theoretically be replaced with other implementations, though DPDK remains the only production-ready option.

## Main APIs

### 1. NVMe Driver (`<spdk/nvme.h>`)

The foundation of SPDK - a user-space, polled-mode, asynchronous, lockless NVMe driver providing zero-copy, highly parallel access to SSDs.

```c
// Key structures
struct spdk_nvme_ctrlr;     // NVMe controller
struct spdk_nvme_ns;        // NVMe namespace
struct spdk_nvme_qpair;     // I/O queue pair

// Key functions
spdk_nvme_probe()           // Discover and attach to NVMe devices
spdk_nvme_ctrlr_alloc_io_qpair()  // Allocate I/O queue
spdk_nvme_ns_cmd_read()     // Submit read command
spdk_nvme_ns_cmd_write()    // Submit write command
spdk_nvme_qpair_process_completions()  // Poll for completions
```

### 2. Block Device Layer - bdev (`<spdk/bdev.h>`)

A pluggable abstraction layer equivalent to an OS block storage layer:

- **Pluggable module API**: Implement block devices for different storage types
- **Built-in modules**: NVMe, malloc (ramdisk), Linux AIO, io_uring, Ceph RBD, virtio, and more
- **Virtual bdevs (vbdevs)**: Stack devices for RAID, logical volumes, crypto, compression
- **Features**: Request queueing, timeout/reset handling, multiple lockless queues

```c
// Key structures
struct spdk_bdev;           // Block device
struct spdk_bdev_desc;      // Handle/descriptor to a bdev (like file descriptor)
struct spdk_bdev_io;        // Asynchronous I/O request

// Key functions
spdk_bdev_get_by_name()     // Look up bdev by name
spdk_bdev_open_ext()        // Open bdev, get descriptor
spdk_bdev_get_io_channel()  // Get per-thread I/O channel
spdk_bdev_read()            // Async read
spdk_bdev_write()           // Async write
spdk_bdev_free_io()         // Release I/O resources in callback
spdk_bdev_close()           // Close descriptor
```

**Workflow**:
1. `spdk_bdev_open_ext()` → get descriptor
2. `spdk_bdev_get_io_channel()` → get per-thread channel
3. `spdk_bdev_read/write()` → submit async I/O with callback
4. In callback: check status, call `spdk_bdev_free_io()`

### 3. Blobstore (`<spdk/blob.h>`)

A persistent, power-fail safe block allocator for building higher-level storage services (databases, key/value stores, distributed storage).

**Hierarchy**:
- **Logical Block**: 512B or 4KiB from disk
- **Page**: Fixed number of logical blocks (typically 4KiB)
- **Cluster**: Fixed number of pages (typically 1MiB)
- **Blob**: Ordered list of clusters with metadata (xattrs)
- **Blobstore**: Owns entire device, manages blobs

```c
// Key structures
struct spdk_blob_store;     // The blobstore
struct spdk_blob;           // A blob
spdk_blob_id;               // Blob identifier

// Key functions
spdk_bs_init()              // Initialize blobstore on device
spdk_bs_load()              // Load existing blobstore
spdk_bs_create_blob()       // Create new blob
spdk_bs_open_blob()         // Open blob by ID
spdk_blob_resize()          // Change blob size
spdk_blob_io_read()         // Read from blob
spdk_blob_io_write()        // Write to blob
spdk_blob_sync_md()         // Persist blob metadata
spdk_blob_close()           // Close blob
spdk_bs_unload()            // Unload blobstore
```

**Features**:
- Thin provisioning (allocate on write)
- Snapshots and clones with copy-on-write
- External snapshots
- Asynchronous, callback-driven API
- Power-fail safe metadata

### 4. Event Framework (`<spdk/event.h>`)

Application framework providing:
- Polling and scheduling of lightweight threads
- Signal handlers for clean shutdown
- Command line parsing
- Reactor model (one thread per core)

```c
struct spdk_app_opts;       // Application options
spdk_app_opts_init()        // Initialize options
spdk_app_start()            // Start application event loop
spdk_app_stop()             // Stop application
```

### 5. Environment Abstraction (`<spdk/env.h>`)

Platform abstraction for memory allocation, PCI access, etc.

```c
spdk_env_opts_init()        // Initialize environment options
spdk_env_init()             // Initialize SPDK environment
spdk_dma_malloc()           // Allocate DMA-capable memory
spdk_dma_free()             // Free DMA memory
```

## API Characteristics

All SPDK APIs share these characteristics:

1. **Asynchronous & Callback-driven**: Operations don't block; they return immediately and call a callback on completion
2. **Non-blocking**: Functions never block or stall the thread
3. **Thread-affinity**: Must be called from SPDK threads (`spdk_thread`)
4. **Channel-based I/O**: I/O requires an `io_channel` (per-thread, lock-free)
5. **DMA memory**: Data buffers must be allocated via SPDK memory functions

## Storage Targets

SPDK provides complete storage target implementations:

- **NVMe-oF Target**: NVMe over Fabrics (RDMA, TCP)
- **iSCSI Target**: iSCSI block storage
- **vhost Target**: For QEMU/KVM virtual machines

## References

- [SPDK Documentation](https://spdk.io/doc/)
- [GitHub Repository](https://github.com/spdk/spdk)
- [API Reference](https://spdk.io/doc/files.html)