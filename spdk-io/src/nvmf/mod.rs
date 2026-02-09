//! NVMe-oF Target API for in-process targets.
//!
//! This module provides an embedded NVMe-oF target that runs in the same
//! process as your application. It wraps the SPDK nvmf library APIs.
//!
//! # ⚠️ Known Issues
//!
//! **Threading conflicts:** Running the NVMf target in-process with an NVMe
//! initiator on the same SPDK thread can cause deadlocks or hangs. SPDK's
//! threading model expects the target and initiator to run on separate reactors
//! (cores), which is difficult to configure correctly in a single-process setup.
//!
//! **Recommendation:** Use the subprocess approach instead (see `tests/nvmf_test.rs`).
//! Spawning `nvmf_tgt` as a separate process avoids these threading issues and
//! provides better isolation.
//!
//! # When to Use
//!
//! - **Production:** Run `nvmf_tgt` as a separate process (recommended)
//! - **Testing:** Use the subprocess approach (see `tests/nvmf_test.rs`)
//! - **Embedded:** Use this module only if you have multiple cores and can
//!   dedicate separate reactors to target vs initiator
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │  Same Process                                               │
//! ├─────────────────────────────────────────────────────────────┤
//! │                                                             │
//! │  ┌─────────────────────┐      ┌─────────────────────────┐  │
//! │  │  NvmfTarget         │      │  NvmeController         │  │
//! │  │                     │      │                         │  │
//! │  │  Bdev (Malloc/Null) │◄────►│  NvmeNamespace          │  │
//! │  │    ▼                │ TCP  │    ▼                    │  │
//! │  │  NvmfSubsystem      │loopbk│  NvmeQpair              │  │
//! │  │    ▼                │      │                         │  │
//! │  │  TCP Listener       │      │                         │  │
//! │  └─────────────────────┘      └─────────────────────────┘  │
//! │                                                             │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! See [`NvmfTarget`] for creating in-process targets.

mod opts;
mod subsystem;
mod target;
mod transport;

pub use opts::{NvmfNsOpts, NvmfSubsystemOpts, NvmfTargetOpts, NvmfTransportOpts};
pub use subsystem::NvmfSubsystem;
pub use target::NvmfTarget;
pub use transport::NvmfTransport;
