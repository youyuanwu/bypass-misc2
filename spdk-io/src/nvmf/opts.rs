//! NVMf target and transport options.

/// NVMf target options.
#[derive(Debug, Default, Clone)]
pub struct NvmfTargetOpts {
    /// Target name
    pub name: Option<String>,
    /// Max subsystems
    pub max_subsystems: Option<u32>,
}

/// NVMf transport options.
#[derive(Debug, Default, Clone)]
pub struct NvmfTransportOpts {
    /// Max I/O size in bytes
    pub max_io_size: Option<u32>,
    /// I/O unit size in bytes
    pub io_unit_size: Option<u32>,
    /// Max queue pairs per controller
    pub max_qpairs_per_ctrlr: Option<u16>,
    /// In-capsule data size
    pub in_capsule_data_size: Option<u32>,
    /// Max AQ depth
    pub max_aq_depth: Option<u32>,
}

/// NVMf subsystem options.
#[derive(Debug, Default, Clone)]
pub struct NvmfSubsystemOpts {
    /// Serial number
    pub serial_number: Option<String>,
    /// Model number  
    pub model_number: Option<String>,
    /// Allow any host to connect
    pub allow_any_host: bool,
}

/// NVMf namespace options.
#[derive(Debug, Default, Clone)]
pub struct NvmfNsOpts {
    /// Namespace ID (0 = auto-assign)
    pub nsid: u32,
    /// UUID (empty = auto-generate)
    pub uuid: Option<String>,
}
