//! Integration test for SPDK Application Framework
//!
//! Tests SpdkApp with 2 cores and SpdkEvent dispatching.

use spdk_io::{Cores, Result, SpdkApp, SpdkEvent};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// Test running on 2 cores with SpdkEvent dispatching
#[test]
fn test_spdk_app_two_cores_with_event() -> Result<()> {
    static MAIN_CORE_RAN: AtomicBool = AtomicBool::new(false);
    static SECOND_CORE_RAN: AtomicBool = AtomicBool::new(false);
    static MAIN_CORE_ID: AtomicU32 = AtomicU32::new(u32::MAX);
    static SECOND_CORE_ID: AtomicU32 = AtomicU32::new(u32::MAX);

    let result = SpdkApp::builder()
        .name("test_2core")
        .no_pci(true)
        .no_huge(true)
        .mem_size_mb(512)
        .reactor_mask("0x3") // Use cores 0 and 1
        .run(|| {
            MAIN_CORE_RAN.store(true, Ordering::SeqCst);
            let main_core = Cores::current();
            MAIN_CORE_ID.store(main_core, Ordering::SeqCst);

            // Find the other core
            let cores: Vec<u32> = Cores::iter().collect();
            println!("Available cores: {:?}", cores);
            assert!(cores.len() >= 2, "Expected at least 2 cores");

            // Find a core that's not the current one
            let other_core = cores.iter().find(|&&c| c != main_core).copied();

            if let Some(target_core) = other_core {
                println!(
                    "Main core: {}, dispatching to core: {}",
                    main_core, target_core
                );

                // Dispatch work to the other core
                SpdkEvent::call_on(target_core, move || {
                    SECOND_CORE_RAN.store(true, Ordering::SeqCst);
                    SECOND_CORE_ID.store(Cores::current(), Ordering::SeqCst);
                    println!("Running on second core: {}", Cores::current());

                    // Stop from the second core
                    SpdkApp::stop();
                })
                .expect("failed to dispatch event");
            } else {
                println!("Could not find second core, stopping");
                SpdkApp::stop();
            }
        });

    assert!(
        MAIN_CORE_RAN.load(Ordering::SeqCst),
        "Main core callback should have run"
    );
    assert!(
        SECOND_CORE_RAN.load(Ordering::SeqCst),
        "Second core callback should have run"
    );

    let main_id = MAIN_CORE_ID.load(Ordering::SeqCst);
    let second_id = SECOND_CORE_ID.load(Ordering::SeqCst);
    assert_ne!(
        main_id, second_id,
        "Callbacks should run on different cores"
    );
    println!(
        "Main ran on core {}, second ran on core {}",
        main_id, second_id
    );

    result
}
