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
//! - [`env`] - Low-level environment initialization  
//! - [`thread`] - SPDK thread management
//! - [`channel`] - I/O channel management
//! - [`error`] - Error types

pub mod app;
pub mod bdev;
pub mod channel;
pub mod env;
pub mod error;
pub mod thread;

// Re-exports
pub use app::{SpdkApp, SpdkAppBuilder};
pub use bdev::{Bdev, BdevDesc};
pub use channel::IoChannel;
pub use env::{LogLevel, SpdkEnv, SpdkEnvBuilder};
pub use error::{Error, Result};
pub use thread::{CurrentThread, SpdkThread};
