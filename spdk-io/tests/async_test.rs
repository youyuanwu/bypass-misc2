//! Integration test for async API
//!
//! Tests run_async and spdk_poller integration.

use spdk_io::{Bdev, DmaBuf, Result, SpdkApp};
use std::sync::atomic::{AtomicBool, Ordering};

#[test]
fn test_run_async() -> Result<()> {
    static CALLBACK_RAN: AtomicBool = AtomicBool::new(false);

    let config = r#"{
        "subsystems": [{
            "subsystem": "bdev",
            "config": [{
                "method": "bdev_null_create",
                "params": {
                    "name": "Null1",
                    "num_blocks": 1024,
                    "block_size": 512
                }
            }]
        }]
    }"#;

    async fn app_main() {
        let bdev = Bdev::get_by_name("Null1").expect("Bdev 'Null1' not found");
        let desc = bdev.open(true).expect("Failed to open bdev");
        let channel = desc.get_io_channel().expect("Failed to get I/O channel");

        let mut buf = DmaBuf::alloc_zeroed(512, 512).expect("Failed to allocate DmaBuf");

        // Async write
        buf.as_mut_slice()[..5].copy_from_slice(b"async");
        desc.write(&channel, &buf, 0).await.expect("Write failed");

        // Async read
        buf.as_mut_slice().fill(0);
        desc.read(&channel, &mut buf, 0).await.expect("Read failed");

        eprintln!("run_async test completed!");
    }

    let result = SpdkApp::builder()
        .name("test_run_async")
        .json_data(config)
        .no_pci(true)
        .no_huge(true)
        .mem_size_mb(512)
        .run_async(|| async {
            CALLBACK_RAN.store(true, Ordering::SeqCst);
            app_main().await;
        });

    assert!(CALLBACK_RAN.load(Ordering::SeqCst), "Callback did not run");
    result
}
