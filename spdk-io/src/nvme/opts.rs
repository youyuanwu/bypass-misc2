//! NVMe controller and queue pair options.

/// NVMe controller options.
///
/// Configure controller behavior when connecting.
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
///
/// Configure queue pair behavior when allocating.
#[derive(Debug, Default, Clone)]
pub struct NvmeQpairOpts {
    /// Queue depth
    pub io_queue_size: Option<u32>,
    /// Queue requests
    pub io_queue_requests: Option<u32>,
}
