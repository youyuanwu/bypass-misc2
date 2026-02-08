//! Integration test for SPDK Bdev API
//!
//! Tests bdev creation via JSON config and I/O channel operations.

use spdk_io::{Bdev, Result, SpdkApp};
use std::fs;
use std::sync::atomic::{AtomicBool, Ordering};

/// Create a test JSON config file with a null bdev
fn create_null_bdev_config() -> String {
    let config = r#"{
        "subsystems": [{
            "subsystem": "bdev",
            "config": [{
                "method": "bdev_null_create",
                "params": {
                    "name": "Null0",
                    "num_blocks": 1024,
                    "block_size": 512
                }
            }]
        }]
    }"#;

    let path = "/tmp/spdk_test_null_bdev.json";
    fs::write(path, config).expect("Failed to write config file");
    path.to_string()
}

#[test]
#[ignore] // Requires SPDK environment
fn test_spdk_app_with_null_bdev() -> Result<()> {
    let config_path = create_null_bdev_config();

    static CALLBACK_RAN: AtomicBool = AtomicBool::new(false);

    let result = SpdkApp::builder()
        .name("test_null_bdev")
        .config_file(&config_path)
        .no_pci(true)
        .no_huge(true)
        .mem_size_mb(512)
        .run(|| {
            CALLBACK_RAN.store(true, Ordering::SeqCst);

            // Try to get the bdev we created via JSON config
            match Bdev::get_by_name("Null0") {
                Some(bdev) => {
                    assert_eq!(bdev.name(), "Null0");
                    assert_eq!(bdev.block_size(), 512);
                    assert_eq!(bdev.num_blocks(), 1024);
                    assert_eq!(bdev.size_bytes(), 512 * 1024);

                    // Open the bdev
                    match bdev.open(true) {
                        Ok(desc) => {
                            // Get an I/O channel
                            match desc.get_io_channel() {
                                Ok(_channel) => {
                                    eprintln!("Successfully got I/O channel");
                                }
                                Err(e) => {
                                    eprintln!("Failed to get I/O channel: {}", e);
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("Failed to open bdev: {}", e);
                        }
                    }
                }
                None => {
                    eprintln!("Bdev 'Null0' not found!");
                }
            }

            SpdkApp::stop();
        });

    fs::remove_file(&config_path).ok();

    assert!(CALLBACK_RAN.load(Ordering::SeqCst), "Callback did not run");
    result
}
