//! High-level async Rust bindings for SPDK (Storage Performance Development Kit)
//!
//! This crate provides safe, ergonomic, async-first bindings to SPDK's
//! high-performance user-space storage stack.
//!
//! # Quick Start
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
//!             SpdkApp::stop();
//!         })
//! }
//! ```
//!
//! # Modules
//!
//! - [`app`] - SPDK Application Framework (recommended for most apps)
//! - [`bdev`] - Block device API
//! - [`complete`] - Callback-to-future utilities
//! - [`dma`] - DMA-capable buffer allocation
//! - [`env`] - Low-level environment initialization  
//! - [`event`] - Event dispatching to specific reactor lcores
//! - [`poller`] - SPDK poller integration for async executors
//! - [`thread`] - SPDK thread management
//! - [`channel`] - I/O channel management
//! - [`error`] - Error types
//! - [`nvme`] - Direct NVMe driver access
//! - [`nvmf`] - NVMe-oF target for in-process testing

pub mod app;
pub mod bdev;
pub mod channel;
pub mod complete;
pub mod dma;
pub mod env;
pub mod error;
pub mod event;
pub mod nvme;
pub mod nvmf;
pub mod poller;
pub mod thread;

// Re-exports
pub use app::{SpdkApp, SpdkAppBuilder};
pub use bdev::{Bdev, BdevDesc};
pub use channel::IoChannel;
pub use complete::{CompletionReceiver, CompletionSender, block_on, completion, io_completion};
pub use dma::DmaBuf;
pub use env::{LogLevel, SpdkEnv, SpdkEnvBuilder};
pub use error::{Error, Result};
pub use event::{CoreIterator, Cores, SpdkEvent};
pub use poller::{spdk_poller, spdk_poller_limited};
pub use thread::{CurrentThread, JoinHandle, SpdkThread, ThreadHandle};
