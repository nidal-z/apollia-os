//! Deterministic waiting primitives for the crate's async tests.
//!
//! Timing-dependent tests must never sleep a fixed duration to "wait for
//! propagation": under load the delay loses its race and the suite fails
//! spuriously. These helpers wait for the actual condition, bounded by a
//! generous ceiling, so they succeed the instant the condition holds and stay
//! robust when the machine is saturated.

use std::future::Future;
use std::time::Duration;

/// Interval between two condition checks. Small enough that a satisfied
/// condition is observed almost immediately, large enough not to spin the CPU.
const POLL_INTERVAL: Duration = Duration::from_millis(2);

/// Poll `cond` until it returns `true` or `timeout` elapses.
///
/// Returns the last value of `cond` (`true` on success, `false` on timeout), so
/// a caller can assert the outcome. The condition is checked once before the
/// first wait, so an already-satisfied condition returns without sleeping.
pub(crate) async fn poll_until(timeout: Duration, mut cond: impl FnMut() -> bool) -> bool {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if cond() {
            return true;
        }
        if tokio::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

/// Async variant of [`poll_until`] for conditions that must be awaited, e.g. an
/// HTTP request or an actor query. `cond` is re-evaluated (its future rebuilt)
/// on each iteration.
pub(crate) async fn poll_until_async<F, Fut>(timeout: Duration, mut cond: F) -> bool
where
    F: FnMut() -> Fut,
    Fut: Future<Output = bool>,
{
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if cond().await {
            return true;
        }
        if tokio::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}
