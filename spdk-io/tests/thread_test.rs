//! Integration test for SPDK thread management
//!
//! All thread tests are in one function because SPDK can only be
//! initialized once per process.
//!
//! Uses the simple spdk_thread_lib_init which should work with default SPDK setup.

use spdk_io::{LogLevel, Result, SpdkEnv, SpdkThread, block_on};
use std::sync::atomic::{AtomicU32, Ordering};

// Test thread with hugepages (standard setup)
#[test]
fn test_thread() -> Result<()> {
    // Use hugepages with debug logging
    let _env = SpdkEnv::builder()
        .name("test_thread")
        .no_pci(true)
        .no_huge(true)
        .mem_size_mb(256)
        .log_level(LogLevel::Debug) // Verbose logging
        .build()?;

    // Create a thread using simple init
    let thread = SpdkThread::new("worker")?;

    // Check basic properties
    assert_eq!(thread.name(), "worker");
    assert!(thread.id() > 0);
    assert!(thread.is_running());
    assert!(SpdkThread::count() >= 1);

    // Verify current thread is set
    let current = SpdkThread::get_current().expect("Current thread should be set");
    assert_eq!(current.id(), thread.id());

    // Poll should work (returns 0 when no work)
    let work = thread.poll();
    assert!(work >= 0);

    // Thread is idle when no pollers registered
    assert!(thread.is_idle());
    assert!(!thread.has_pollers());

    // Poll multiple times
    for _ in 0..10 {
        thread.poll();
    }

    // Poll with max_msgs limit
    let work = thread.poll_max(100);
    assert!(work >= 0);

    // Drop the thread
    drop(thread);

    // Current thread should be cleared
    assert!(SpdkThread::get_current().is_none());

    // === Test spawn within same SPDK session ===
    // Re-create main thread
    let main_thread = SpdkThread::new("main")?;
    eprintln!(
        "Main thread created for spawn test: id={}",
        main_thread.id()
    );

    // Spawn a worker thread
    let handle = SpdkThread::spawn("worker-1", |thread| {
        eprintln!(
            "Worker thread running: name={}, id={}",
            thread.name(),
            thread.id()
        );

        // Verify we have a valid thread
        assert_eq!(thread.name(), "worker-1");
        assert!(thread.id() > 0);
        assert!(thread.is_running());

        // Poll a few times
        for _ in 0..10 {
            thread.poll();
        }

        42 // Return value
    });

    eprintln!("Waiting for worker to complete...");

    // Wait for spawn to complete
    let result = handle.join()?;
    assert_eq!(result, 42);

    eprintln!("Worker completed with result: {}", result);

    // Main thread still valid
    assert!(main_thread.is_running());

    // === Test ThreadHandle message passing ===
    static MSG_COUNTER: AtomicU32 = AtomicU32::new(0);

    eprintln!("Testing ThreadHandle message passing...");

    // Get handle to main thread
    let main_handle = main_thread.handle();

    // Spawn worker that sends message back to main
    let worker_handle = SpdkThread::spawn("msg-worker", move |worker| {
        eprintln!("Message worker started");

        // Send a message to main thread
        main_handle.send(|| {
            MSG_COUNTER.fetch_add(1, Ordering::SeqCst);
            eprintln!("Message received on main thread!");
        });

        // Poll worker a bit to let it run
        for _ in 0..10 {
            worker.poll();
        }

        eprintln!("Message worker done");
    });

    // Wait for worker to finish sending
    worker_handle.join()?;

    // Poll main thread to process the message
    for _ in 0..100 {
        main_thread.poll();
    }

    assert_eq!(
        MSG_COUNTER.load(Ordering::SeqCst),
        1,
        "Message should have been received"
    );
    eprintln!("ThreadHandle test passed!");

    // === Test ThreadHandle::call() ===
    eprintln!("Testing ThreadHandle::call()...");

    // Create a worker thread
    let worker_thread = SpdkThread::new("call-worker")?;
    let _worker_handle = worker_thread.handle();

    // Get main thread handle
    let main_handle = main_thread.handle();

    // From main thread, call a function on worker and await result
    // We need to spawn the worker to poll it
    let result_handle = SpdkThread::spawn("call-test", move |_| {
        // Send a call to main thread and get receiver

        // Poll to process - use block_on (but we don't have main thread here)
        // For this test, we'll just check the receiver is created
        main_handle.call(|| {
            eprintln!("Computing on main thread...");
            123
        })
    });

    // Join to get the receiver
    let receiver = result_handle.join()?;

    // Poll main thread to run the closure
    for _ in 0..100 {
        main_thread.poll();
    }

    // Now poll receiver with block_on from main thread context
    // The closure should have run and sent the result
    let result = block_on(receiver)?;
    assert_eq!(result, 123);
    eprintln!("ThreadHandle::call() test passed with result: {}", result);

    drop(worker_thread);
    drop(main_thread);

    Ok(())
}
