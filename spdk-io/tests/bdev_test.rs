//! Integration test for SPDK Bdev API
//!
//! Tests bdev creation via JSON config and I/O channel operations.

use spdk_io::{Bdev, DmaBuf, Result, SpdkApp, block_on};
use std::sync::atomic::{AtomicBool, Ordering};

#[test]
fn test_spdk_app_with_null_bdev() -> Result<()> {
    static CALLBACK_RAN: AtomicBool = AtomicBool::new(false);

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

    let result = SpdkApp::builder()
        .name("test_null_bdev")
        .json_data(config)
        .no_pci(true)
        .no_huge(true)
        .mem_size_mb(512)
        .run(|| {
            CALLBACK_RAN.store(true, Ordering::SeqCst);

            // Get the bdev we created via JSON config
            let bdev = Bdev::get_by_name("Null0").expect("Bdev 'Null0' not found");
            assert_eq!(bdev.name(), "Null0");
            assert_eq!(bdev.block_size(), 512);
            assert_eq!(bdev.num_blocks(), 1024);
            assert_eq!(bdev.size_bytes(), 512 * 1024);

            // Open the bdev
            let desc = bdev.open(true).expect("Failed to open bdev");

            // Get an I/O channel
            let channel = desc.get_io_channel().expect("Failed to get I/O channel");
            eprintln!("Successfully got I/O channel");

            // Test DmaBuf allocation and async I/O
            let mut buf =
                DmaBuf::alloc_zeroed(bdev.block_size() as usize, bdev.block_size() as usize)
                    .expect("Failed to allocate DmaBuf");

            eprintln!(
                "DmaBuf allocated: {} bytes at {:p}",
                buf.len(),
                buf.as_ptr()
            );

            // Write test data
            buf.as_mut_slice()[..5].copy_from_slice(b"hello");

            // Write to bdev (async with block_on)
            block_on(desc.write(&channel, &buf, 0)).expect("Write failed");
            eprintln!("Write succeeded!");

            // Clear buffer and read back
            buf.as_mut_slice().fill(0);

            // Read from bdev (async with block_on)
            block_on(desc.read(&channel, &mut buf, 0)).expect("Read failed");
            eprintln!("Read succeeded!");
            // Null bdev doesn't actually persist data, but the I/O should complete successfully

            eprintln!("Async I/O test completed!");

            SpdkApp::stop();
        });

    assert!(CALLBACK_RAN.load(Ordering::SeqCst), "Callback did not run");
    result
}
