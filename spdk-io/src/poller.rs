//! SPDK poller integration for async executors.
//!
//! This module provides an async task that polls SPDK while cooperating
//! with any async executor.
//!
//! # Architecture
//!
//! SPDK completions are delivered synchronously during `spdk_thread_poll()`.
//! To integrate with async executors, we run the SPDK polling as a task
//! that yields when idle:
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                    Local Executor                           │
//! │  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────┐  │
//! │  │ App Task 1      │  │ App Task 2      │  │ SPDK Poller │  │
//! │  │ (I/O future)    │  │ (I/O future)    │  │ (this task) │  │
//! │  └────────┬────────┘  └────────┬────────┘  └──────┬──────┘  │
//! │           │                    │                   │         │
//! │           ▼                    ▼                   ▼         │
//! │  ┌─────────────────────────────────────────────────────────┐│
//! │  │         spdk_thread_poll() - processes completions      ││
//! │  └─────────────────────────────────────────────────────────┘│
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Example
//!
//! ```ignore
//! use spdk_io::{SpdkApp, poller::spdk_poller};
//! use async_executor::LocalExecutor;
//! use futures_lite::future;
//!
//! SpdkApp::builder()
//!     .name("app")
//!     .json_data(config)
//!     .run(|| {
//!         let ex = LocalExecutor::new();
//!         future::block_on(ex.run(async {
//!             // Spawn SPDK poller as a background task
//!             ex.spawn(spdk_poller()).detach();
//!             
//!             // Now multiple concurrent I/Os work
//!             let (r1, r2) = futures::join!(
//!                 do_read(&desc, &channel, 0),
//!                 do_read(&desc, &channel, 4096),
//!             );
//!         }));
//!         SpdkApp::stop();
//!     });
//! ```

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use crate::thread::SpdkThread;

/// A future that yields once, then completes.
///
/// This allows other tasks to run before continuing.
struct YieldNow(bool);

impl Future for YieldNow {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if self.0 {
            Poll::Ready(())
        } else {
            self.0 = true;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

/// Yield to other tasks in the executor.
fn yield_now() -> YieldNow {
    YieldNow(false)
}

/// SPDK poller task for use with async executors.
///
/// This future never completes - it runs indefinitely, polling SPDK
/// and yielding to other tasks when idle.
///
/// # How it works
///
/// 1. Calls `spdk_thread_poll()` to process SPDK work (messages, I/O completions)
/// 2. If work was done, immediately polls again (busy polling)
/// 3. If no work was done, yields to let other tasks run
///
/// # Panics
///
/// Panics if called from outside an SPDK thread context.
///
/// # Example
///
/// ```ignore
/// use async_executor::LocalExecutor;
/// use spdk_io::poller::spdk_poller;
///
/// let ex = LocalExecutor::new();
/// futures_lite::future::block_on(ex.run(async {
///     ex.spawn(spdk_poller()).detach();
///     
///     // Your async I/O code here
///     desc.read(&channel, &mut buf, 0).await?;
/// }));
/// ```
pub async fn spdk_poller() {
    let thread = SpdkThread::get_current().expect("spdk_poller called outside SPDK thread context");

    loop {
        let work_done = thread.poll();
        if work_done == 0 {
            // No work done, yield to other tasks
            yield_now().await;
        }
        // If work was done, immediately poll again (hot path)
    }
}

/// SPDK poller task that runs for a limited number of iterations.
///
/// Useful for tests or finite workloads. Returns when `max_iters` is reached.
///
/// # Arguments
///
/// * `max_iters` - Maximum number of poll iterations (0 for infinite, same as `spdk_poller`)
pub async fn spdk_poller_limited(max_iters: u64) {
    if max_iters == 0 {
        return spdk_poller().await;
    }

    let thread =
        SpdkThread::get_current().expect("spdk_poller_limited called outside SPDK thread context");

    for _ in 0..max_iters {
        let work_done = thread.poll();
        if work_done == 0 {
            yield_now().await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_yield_now() {
        // Just verify it compiles and the future logic is correct
        use futures_task::noop_waker;

        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);

        let mut fut = yield_now();
        let mut fut = unsafe { Pin::new_unchecked(&mut fut) };

        // First poll returns Pending
        assert!(fut.as_mut().poll(&mut cx).is_pending());

        // Second poll returns Ready
        assert!(fut.as_mut().poll(&mut cx).is_ready());
    }
}
