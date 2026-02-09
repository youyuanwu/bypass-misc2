//! NVMe driver API.
//!
//! Direct access to NVMe controllers and namespaces, bypassing the bdev layer.
//! Use this for:
//! - Maximum performance (no bdev abstraction overhead)
//! - Custom admin commands
//! - Namespace management
//! - E2E tests with real NVMe devices
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────┐
//! │  NvmeController │  ← Connect via TransportId (PCIe/TCP/RDMA)
//! └────────┬────────┘
//!          │
//!     ┌────┴────┐
//!     ▼         ▼
//! ┌───────┐ ┌───────┐
//! │  NS1  │ │  NS2  │  ← NvmeNamespace (read/write)
//! └───────┘ └───────┘
//!     │
//!     ▼
//! ┌─────────┐
//! │ NvmeQpair│  ← Per-thread I/O queue
//! └─────────┘
//! ```
//!
//! # Example
//!
//! ```no_run
//! use spdk_io::{SpdkApp, DmaBuf};
//! use spdk_io::nvme::{NvmeController, TransportId};
//!
//! SpdkApp::builder()
//!     .name("nvme_example")
//!     .run_async(|| async {
//!         // Connect to NVMe controller
//!         let trid = TransportId::pcie("0000:00:04.0").unwrap();
//!         let ctrlr = NvmeController::connect(&trid, None).unwrap();
//!
//!         // Get namespace and allocate qpair
//!         let ns = ctrlr.namespace(1).expect("NS1 not found");
//!         let qpair = ctrlr.alloc_io_qpair(None).unwrap();
//!
//!         // Allocate DMA buffer
//!         let mut buf = DmaBuf::alloc(ns.sector_size() as usize, 4096).unwrap();
//!
//!         // Read first sector
//!         ns.read(&qpair, &mut buf, 0, 1).await.unwrap();
//!
//!         SpdkApp::stop();
//!     })
//!     .unwrap();
//! ```

mod controller;
mod namespace;
mod opts;
mod qpair;
mod transport;

pub use controller::NvmeController;
pub use namespace::NvmeNamespace;
pub use opts::{NvmeCtrlrOpts, NvmeQpairOpts};
pub use qpair::NvmeQpair;
pub use transport::TransportId;
