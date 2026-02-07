//! Error types for spdk-io

use std::ffi::NulError;

/// Result type for spdk-io operations
pub type Result<T> = std::result::Result<T, Error>;

/// Error type for spdk-io operations
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// SPDK environment initialization failed
    #[error("SPDK environment initialization failed: {0}")]
    EnvInit(String),

    /// SPDK environment already initialized
    #[error("SPDK environment already initialized")]
    AlreadyInitialized,

    /// SPDK environment not initialized
    #[error("SPDK environment not initialized")]
    NotInitialized,

    /// Invalid argument provided
    #[error("Invalid argument: {0}")]
    InvalidArgument(String),

    /// String contains null byte
    #[error("String contains null byte")]
    NulError(#[from] NulError),

    /// I/O operation failed
    #[error("I/O operation failed")]
    IoError,

    /// Device not found
    #[error("Device not found: {0}")]
    DeviceNotFound(String),

    /// Channel allocation failed
    #[error("I/O channel allocation failed")]
    ChannelAlloc,

    /// Memory allocation failed
    #[error("Memory allocation failed")]
    MemoryAlloc,

    /// Operation was cancelled
    #[error("Operation cancelled")]
    Cancelled,

    /// OS error with errno
    #[error("OS error: {0}")]
    Os(i32),
}

impl Error {
    /// Create an OS error from errno
    pub fn from_errno(errno: i32) -> Self {
        Error::Os(errno)
    }

    /// Create from SPDK return code (negative errno)
    pub fn from_rc(rc: i32) -> Self {
        if rc < 0 {
            Error::Os(-rc)
        } else {
            Error::Os(rc)
        }
    }
}
