//! Integration test for SPDK Application Framework
//!
//! Tests basic SpdkApp functionality.

use spdk_io::{Result, SpdkApp};
use std::sync::atomic::{AtomicBool, Ordering};

#[test]
#[ignore] // Requires SPDK environment
fn test_spdk_app_simple() -> Result<()> {
    static CALLBACK_RAN: AtomicBool = AtomicBool::new(false);

    let result = SpdkApp::builder()
        .name("test_simple")
        .no_pci(true)
        .no_huge(true)
        .mem_size_mb(512)
        .run(|| {
            CALLBACK_RAN.store(true, Ordering::SeqCst);
            SpdkApp::stop();
        });

    assert!(CALLBACK_RAN.load(Ordering::SeqCst));
    result
}
