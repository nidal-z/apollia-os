//! PyNotifyInterface: Python-facing proxy for agent notifications.
//!
//! Exposes `ctx.notify.publish(message, severity)` to Python agents via
//! the `NotificationEngineHandle`. When no handle is configured (opt-in),
//! all calls are silent no-ops.

use std::collections::HashMap;

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

use apollia_notifications::{Notification, NotificationEngineHandle, Severity};

/// Python-facing interface for agent notification publishing.
///
/// Each agent optionally receives a `PyNotifyInterface` configured with
/// a handle to the notification engine. When the handle is `None` (no
/// channels configured), all publish calls are silent no-ops.
#[pyclass]
pub struct PyNotifyInterface {
    handle: Option<NotificationEngineHandle>,
}

impl PyNotifyInterface {
    /// Creates a new `PyNotifyInterface`.
    ///
    /// Pass `None` for `handle` when no notification channels are configured.
    pub fn new(handle: Option<NotificationEngineHandle>) -> Self {
        Self { handle }
    }
}

#[pymethods]
impl PyNotifyInterface {
    /// Emits a notification via the NotificationEngine.
    ///
    /// When no channel is configured, the method returns without error
    /// (no-op; channels are opt-in by design).
    ///
    /// severity: "debug" | "info" | "warn" | "warning" | "error" | "critical"
    /// Raises ValueError for any other severity value.
    #[pyo3(signature = (message, severity = "info", title = None, channel = None))]
    fn publish<'py>(
        &self,
        py: Python<'py>,
        message: String,
        severity: &str,
        title: Option<String>,
        channel: Option<String>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let sev = parse_severity(severity)?;
        let _ = channel; // channel override not yet routed, reserved for future use

        let handle = self.handle.clone();
        let agent = title; // use title field as agent context if provided

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            if let Some(h) = handle {
                let notif = Notification {
                    event: "agent.notify".to_string(),
                    timestamp: chrono::Utc::now(),
                    task_id: None,
                    agent,
                    message,
                    metadata: HashMap::new(),
                    severity: sev,
                };
                h.publish(notif).await;
            }
            Ok(Python::with_gil(|py| py.None()))
        })
    }
}

/// Parses a severity string into a [`Severity`] enum value.
///
/// Accepts "warn" as an alias for "warning" for convenience.
/// Returns `PyValueError` for unrecognized severity strings.
fn parse_severity(severity: &str) -> PyResult<Severity> {
    match severity {
        "debug" => Ok(Severity::Debug),
        "info" => Ok(Severity::Info),
        "warn" | "warning" => Ok(Severity::Warning),
        "error" => Ok(Severity::Error),
        "critical" => Ok(Severity::Critical),
        other => Err(PyValueError::new_err(format!(
            "invalid severity: '{}'. Must be one of: debug, info, warn, warning, error, critical",
            other
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // parse_severity maps all valid values
    #[test]
    fn test_parse_severity_valid_values() {
        // GIVEN each valid severity string
        // WHEN parsed
        // THEN no error
        assert!(parse_severity("debug").is_ok());
        assert!(parse_severity("info").is_ok());
        assert!(parse_severity("warn").is_ok());
        assert!(parse_severity("warning").is_ok());
        assert!(parse_severity("error").is_ok());
        assert!(parse_severity("critical").is_ok());
    }

    // parse_severity rejects invalid value
    #[test]
    fn test_parse_severity_invalid_value() {
        // GIVEN an invalid severity
        // WHEN parsed
        // THEN PyValueError
        let result = parse_severity("fatal");
        assert!(result.is_err());
    }

    // PyNotifyInterface::new with None handle constructs successfully
    #[test]
    fn test_new_with_no_handle() {
        // GIVEN no handle
        // WHEN constructed
        // THEN succeeds
        let iface = PyNotifyInterface::new(None);
        assert!(iface.handle.is_none());
    }
}
