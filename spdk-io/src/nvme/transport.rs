//! NVMe transport identifier.
//!
//! Identifies how to connect to an NVMe controller (PCIe, TCP, RDMA, etc.)

use std::ffi::CString;
use std::fmt;
use std::mem::MaybeUninit;

use spdk_io_sys::*;

use crate::error::{Error, Result};

/// NVMe transport identifier.
///
/// Identifies a unique endpoint on an NVMe fabric or PCIe bus.
///
/// # Example
///
/// ```no_run
/// use spdk_io::nvme::TransportId;
///
/// // PCIe device
/// let trid = TransportId::pcie("0000:00:04.0")?;
///
/// // TCP device  
/// let trid = TransportId::tcp("192.168.1.100", "4420", "nqn.2024-01.io.spdk:cnode1")?;
/// # Ok::<(), spdk_io::Error>(())
/// ```
#[derive(Clone)]
pub struct TransportId {
    inner: spdk_nvme_transport_id,
}

impl TransportId {
    /// Create a PCIe transport ID from BDF address.
    ///
    /// # Arguments
    ///
    /// * `addr` - PCI address in BDF format (e.g., "0000:00:04.0")
    ///
    /// # Example
    ///
    /// ```no_run
    /// use spdk_io::nvme::TransportId;
    ///
    /// let trid = TransportId::pcie("0000:00:04.0")?;
    /// # Ok::<(), spdk_io::Error>(())
    /// ```
    pub fn pcie(addr: &str) -> Result<Self> {
        let mut trid: spdk_nvme_transport_id = unsafe { MaybeUninit::zeroed().assume_init() };

        trid.trtype = spdk_nvme_transport_type_SPDK_NVME_TRANSPORT_PCIE;

        // Copy transport string name
        Self::copy_to_field(&mut trid.trstring, "PCIe", "trstring")?;

        // Copy address to traddr field
        let addr_bytes = addr.as_bytes();
        let max_len = trid.traddr.len() - 1; // Leave room for null terminator
        if addr_bytes.len() > max_len {
            return Err(Error::InvalidArgument(format!(
                "PCI address too long: {} (max {})",
                addr_bytes.len(),
                max_len
            )));
        }

        for (i, &byte) in addr_bytes.iter().enumerate() {
            trid.traddr[i] = byte as i8;
        }

        Ok(Self { inner: trid })
    }

    /// Create a TCP transport ID.
    ///
    /// # Arguments
    ///
    /// * `addr` - IP address or hostname
    /// * `port` - Service port (typically "4420")
    /// * `subnqn` - Subsystem NQN (can be empty for discovery)
    ///
    /// # Example
    ///
    /// ```no_run
    /// use spdk_io::nvme::TransportId;
    ///
    /// let trid = TransportId::tcp("127.0.0.1", "4420", "nqn.2024-01.io.spdk:test")?;
    /// # Ok::<(), spdk_io::Error>(())
    /// ```
    pub fn tcp(addr: &str, port: &str, subnqn: &str) -> Result<Self> {
        let mut trid: spdk_nvme_transport_id = unsafe { MaybeUninit::zeroed().assume_init() };

        trid.trtype = spdk_nvme_transport_type_SPDK_NVME_TRANSPORT_TCP;
        trid.adrfam = spdk_nvmf_adrfam_SPDK_NVMF_ADRFAM_IPV4;

        // Copy transport string name (used by NVMf subsystem lookup)
        Self::copy_to_field(&mut trid.trstring, "TCP", "trstring")?;

        // Copy address
        Self::copy_to_field(&mut trid.traddr, addr, "address")?;

        // Copy port to trsvcid
        Self::copy_to_field(&mut trid.trsvcid, port, "port")?;

        // Copy subnqn
        Self::copy_to_field(&mut trid.subnqn, subnqn, "subnqn")?;

        Ok(Self { inner: trid })
    }

    /// Create an RDMA transport ID.
    ///
    /// # Arguments
    ///
    /// * `addr` - IP address
    /// * `port` - Service port
    /// * `subnqn` - Subsystem NQN
    pub fn rdma(addr: &str, port: &str, subnqn: &str) -> Result<Self> {
        let mut trid: spdk_nvme_transport_id = unsafe { MaybeUninit::zeroed().assume_init() };

        trid.trtype = spdk_nvme_transport_type_SPDK_NVME_TRANSPORT_RDMA;
        trid.adrfam = spdk_nvmf_adrfam_SPDK_NVMF_ADRFAM_IPV4;

        // Copy transport string name (used by NVMf subsystem lookup)
        Self::copy_to_field(&mut trid.trstring, "RDMA", "trstring")?;

        Self::copy_to_field(&mut trid.traddr, addr, "address")?;
        Self::copy_to_field(&mut trid.trsvcid, port, "port")?;
        Self::copy_to_field(&mut trid.subnqn, subnqn, "subnqn")?;

        Ok(Self { inner: trid })
    }

    /// Parse from string (SPDK format).
    ///
    /// Format: `trtype:PCIe traddr:0000:00:04.0`
    /// or: `trtype:TCP adrfam:IPv4 traddr:127.0.0.1 trsvcid:4420 subnqn:nqn.test`
    pub fn parse(s: &str) -> Result<Self> {
        let mut trid: spdk_nvme_transport_id = unsafe { MaybeUninit::zeroed().assume_init() };

        let c_str = CString::new(s)?;

        let rc = unsafe { spdk_nvme_transport_id_parse(&mut trid, c_str.as_ptr()) };

        if rc != 0 {
            return Err(Error::InvalidArgument(format!(
                "Failed to parse transport ID: {}",
                s
            )));
        }

        Ok(Self { inner: trid })
    }

    /// Get the transport type.
    pub fn transport_type(&self) -> u32 {
        self.inner.trtype
    }

    /// Get the address.
    pub fn address(&self) -> &str {
        Self::field_to_str(&self.inner.traddr)
    }

    /// Get the service ID (port).
    pub fn service_id(&self) -> &str {
        Self::field_to_str(&self.inner.trsvcid)
    }

    /// Get the subsystem NQN.
    pub fn subnqn(&self) -> &str {
        Self::field_to_str(&self.inner.subnqn)
    }

    /// Get a pointer to the inner transport ID.
    ///
    /// Used for FFI calls to SPDK NVMe functions.
    pub fn as_ptr(&self) -> *const spdk_nvme_transport_id {
        &self.inner
    }

    /// Get a mutable pointer to the inner transport ID.
    #[allow(dead_code)]
    pub(crate) fn as_mut_ptr(&mut self) -> *mut spdk_nvme_transport_id {
        &mut self.inner
    }

    /// Helper to copy string to fixed-size char array field.
    fn copy_to_field(field: &mut [i8], value: &str, name: &str) -> Result<()> {
        let bytes = value.as_bytes();
        let max_len = field.len() - 1;

        if bytes.len() > max_len {
            return Err(Error::InvalidArgument(format!(
                "{} too long: {} (max {})",
                name,
                bytes.len(),
                max_len
            )));
        }

        for (i, &byte) in bytes.iter().enumerate() {
            field[i] = byte as i8;
        }

        Ok(())
    }

    /// Helper to convert fixed-size char array to string slice.
    fn field_to_str(field: &[i8]) -> &str {
        // Find null terminator
        let len = field.iter().position(|&c| c == 0).unwrap_or(field.len());
        // Safety: SPDK fields are ASCII
        unsafe {
            std::str::from_utf8_unchecked(std::slice::from_raw_parts(
                field.as_ptr() as *const u8,
                len,
            ))
        }
    }
}

impl fmt::Debug for TransportId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let trtype = if self.inner.trtype == spdk_nvme_transport_type_SPDK_NVME_TRANSPORT_PCIE {
            "PCIe"
        } else if self.inner.trtype == spdk_nvme_transport_type_SPDK_NVME_TRANSPORT_TCP {
            "TCP"
        } else if self.inner.trtype == spdk_nvme_transport_type_SPDK_NVME_TRANSPORT_RDMA {
            "RDMA"
        } else {
            "Unknown"
        };

        f.debug_struct("TransportId")
            .field("trtype", &trtype)
            .field("traddr", &self.address())
            .field("trsvcid", &self.service_id())
            .field("subnqn", &self.subnqn())
            .finish()
    }
}
