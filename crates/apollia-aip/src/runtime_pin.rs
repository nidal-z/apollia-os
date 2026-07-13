//! Pins `pyo3-async-runtimes` onto the shared apollia-worker Tokio runtime.
//!
//! Without this, `future_into_py` drives Python-invoked Rust futures on a second
//! runtime that `pyo3-async-runtimes` lazily builds for itself, separate from
//! the runtime that runs the actors. Pinning keeps a single runtime, as required
//! by the crate rulebook (never let `pyo3-async-runtimes` create its own).

use std::sync::Once;

static PIN_ONCE: Once = Once::new();

/// Pins `pyo3-async-runtimes` to the apollia-worker Tokio runtime.
///
/// Idempotent: only the first call takes effect. Call it once at the
/// composition root, after the embedded runtime has started (so
/// [`apollia_runtime::worker_runtime`] returns the runtime) and before any
/// Python agent runs a coroutine (before the first `future_into_py`). If the
/// worker runtime is not yet available, or `pyo3-async-runtimes` has already
/// built its own runtime, the pin is skipped and logged at `warn`.
pub fn pin_async_runtime() {
    PIN_ONCE.call_once(|| {
        let Some(rt) = apollia_runtime::worker_runtime() else {
            tracing::warn!(
                reason = "worker runtime not started",
                "aip.runtime.pin_failed"
            );
            return;
        };
        match pyo3_async_runtimes::tokio::init_with_runtime(rt) {
            Ok(()) => tracing::debug!("aip.runtime.pinned"),
            Err(()) => tracing::warn!(
                reason = "pyo3-async-runtimes already initialized",
                "aip.runtime.pin_failed"
            ),
        }
    });
}

#[cfg(test)]
mod tests {
    use super::pin_async_runtime;

    // GIVEN a unit-test process where the embedded worker runtime never started
    // WHEN pin_async_runtime is called, then called again
    // THEN neither call panics and the Once guard makes the second a no-op;
    // pinning is skipped because the worker runtime is absent
    #[test]
    fn test_pin_async_runtime_idempotent_and_safe_without_worker() {
        // GIVEN: worker_runtime() is None here (no init_embedded in this binary).
        assert!(apollia_runtime::worker_runtime().is_none());

        // WHEN / THEN: repeated calls do not panic (Once guard + graceful skip).
        pin_async_runtime();
        pin_async_runtime();
    }
}
