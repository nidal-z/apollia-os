//! ctx.mail: asynchronous inter-agent messaging surface.
//!
//! Nested facade over the durable [`AgentMailboxHandle`]. Distinct from
//! `ctx.a2a` (synchronous skill RPC): `ctx.mail.send` posts a message into
//! another agent's durable inbox and returns immediately, while the recipient
//! pulls it at its own initiative. Delivery is at-least-once: `receive` leases a
//! message rather than deleting it; the consuming context acknowledges it on
//! success, and `ack`/`nack` are available for explicit control.
//!
//! The interface shares the same `AgentMailboxHandle` as `RuntimeContext`; it is
//! rebuilt by the `ctx.mail` getter on each access so the caller's current
//! `run_id` (set after construction via `with_run_id`) always flows into the
//! emitted events for auditability. The agent never instantiates it directly.

use std::sync::Arc;
use std::time::{Duration, Instant};

use apollia_core::events::{EventBusSender, RunId, RuntimeEvent};
use apollia_runtime::a2a::A2AInvoker;
use apollia_runtime::mailbox::AgentMailboxHandle;
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;

/// Polling granularity while `receive` waits for a message.
const RECEIVE_POLL_INTERVAL: Duration = Duration::from_millis(50);
/// Bound applied to a single actor round-trip inside the poll loop.
const RECEIVE_ROUND_TRIP: Duration = Duration::from_millis(100);

/// Typed facade exposed to the Python agent via `ctx.mail`.
#[pyclass(name = "MailInterface", module = "apollia._native")]
pub struct MailInterface {
    /// Durable mailbox handle shared with `RuntimeContext`. `None` = minimal
    /// runtime without a mailbox (tests, CLI dry-run).
    mailbox: Option<AgentMailboxHandle>,
    /// Name of the agent owning this context (the sender, and the inbox owner).
    caller_agent_name: String,
    /// Current run of the caller, propagated into emitted events for audit.
    run_id: Option<RunId>,
    /// Whether the agent declared the mailbox capability. When `false`, every
    /// method raises: the surface is opt-in.
    supports_mailbox: bool,
    /// Optional recipient allowlist for `send`. `None` means any recipient.
    allowlist: Option<Vec<String>>,
    /// Whether `send` is gated behind human approval (opt-in via the manifest
    /// `tools_requiring_approval` containing `mailbox:send`, or a host policy).
    /// When set, a send emits `PermissionRequired` and is denied, mirroring the
    /// deny-and-notify HITL semantics of the tool executor.
    send_requires_approval: bool,
    /// Event bus for emitting `PermissionRequired` when a gated send is denied.
    event_bus: Option<EventBusSender>,
    /// Registry-backed directory used to fail-fast on an unknown recipient.
    /// Shares the `A2AInvoker` already held by `RuntimeContext`; `None` in a
    /// minimal runtime (no registry available), where validation is skipped.
    directory: Option<Arc<A2AInvoker>>,
}

impl MailInterface {
    /// Builds the interface from the shared handle, the caller identity, the
    /// caller's current run, the declared mailbox capability, the send gate, and
    /// the registry-backed directory for recipient validation.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        mailbox: Option<AgentMailboxHandle>,
        caller_agent_name: String,
        run_id: Option<RunId>,
        supports_mailbox: bool,
        allowlist: Option<Vec<String>>,
        send_requires_approval: bool,
        event_bus: Option<EventBusSender>,
        directory: Option<Arc<A2AInvoker>>,
    ) -> Self {
        Self {
            mailbox,
            caller_agent_name,
            run_id,
            supports_mailbox,
            allowlist,
            send_requires_approval,
            event_bus,
            directory,
        }
    }

    /// Emits a `PermissionRequired` event for a gated send and returns the
    /// request id. Fire-and-forget; a missing event bus is not an error.
    fn emit_approval_required(&self, to: &str) -> String {
        let request_id = uuid::Uuid::new_v4().to_string();
        if let Some(bus) = &self.event_bus {
            let _ = bus.send(RuntimeEvent::PermissionRequired {
                tool_name: "mailbox:send".to_string(),
                input: serde_json::json!({ "from": self.caller_agent_name, "to": to }),
                request_id: request_id.clone(),
            });
        }
        request_id
    }

    /// Returns the handle after checking the mailbox capability was declared.
    fn require_handle(&self) -> PyResult<AgentMailboxHandle> {
        if !self.supports_mailbox {
            return Err(PyRuntimeError::new_err(
                "ctx.mail requires the mailbox capability; declare it with @agent(mailbox=True)",
            ));
        }
        self.mailbox
            .clone()
            .ok_or_else(|| PyRuntimeError::new_err("mailbox not available in this runtime context"))
    }

    /// Rejects a recipient that is outside a declared allowlist.
    fn check_recipient(&self, to: &str) -> PyResult<()> {
        match &self.allowlist {
            Some(list) if !list.iter().any(|a| a == to) => Err(PyRuntimeError::new_err(format!(
                "recipient '{to}' is not in this agent's mailbox allowlist"
            ))),
            _ => Ok(()),
        }
    }
}

/// Converts a Python object to a `serde_json::Value` via `json.dumps`.
fn pyobj_to_json(py: Python<'_>, obj: &PyObject) -> PyResult<serde_json::Value> {
    let json_mod = py
        .import("json")
        .map_err(|e| PyRuntimeError::new_err(format!("failed to import json: {e}")))?;
    let json_str: String = json_mod
        .call_method1("dumps", (obj.bind(py),))
        .map_err(|e| PyRuntimeError::new_err(format!("json.dumps failed: {e}")))?
        .extract()
        .map_err(|e| PyRuntimeError::new_err(format!("extract failed: {e}")))?;
    serde_json::from_str(&json_str)
        .map_err(|e| PyRuntimeError::new_err(format!("JSON parse failed: {e}")))
}

/// Converts a `serde_json::Value` to a Python object via `json.loads`.
fn json_to_pyobj(py: Python<'_>, value: &serde_json::Value) -> PyResult<PyObject> {
    let json_str = serde_json::to_string(value)
        .map_err(|e| PyRuntimeError::new_err(format!("serialization error: {e}")))?;
    let json_mod = py
        .import("json")
        .map_err(|e| PyRuntimeError::new_err(format!("import json: {e}")))?;
    Ok(json_mod
        .call_method1("loads", (json_str,))
        .map_err(|e| PyRuntimeError::new_err(format!("json.loads: {e}")))?
        .unbind())
}

#[pymethods]
impl MailInterface {
    /// Sends a message to `to`'s inbox, returning the assigned message id.
    ///
    /// Non-blocking. Raises `RuntimeError` if a guard blocks the send (queue
    /// full, per-run quota, oversized payload) or the mailbox is unavailable.
    fn send<'py>(
        &self,
        py: Python<'py>,
        to: String,
        payload: PyObject,
    ) -> PyResult<Bound<'py, PyAny>> {
        let mailbox = self.require_handle()?;
        self.check_recipient(&to)?;
        if self.send_requires_approval {
            let request_id = self.emit_approval_required(&to);
            return Err(PyRuntimeError::new_err(format!(
                "mailbox send to '{to}' requires human approval (mailbox:send gated, request {request_id})"
            )));
        }
        let value = pyobj_to_json(py, &payload)?;
        let from = self.caller_agent_name.clone();
        let run_id = self.run_id.clone();
        let directory = self.directory.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            // Fail-fast on an unknown recipient, when a
            // registry-backed directory is available.
            if let Some(dir) = &directory {
                if !dir.agent_exists(&to).await {
                    return Err(PyRuntimeError::new_err(format!(
                        "unknown mailbox recipient '{to}'"
                    )));
                }
            }
            mailbox
                .send(&from, &to, value, run_id)
                .await
                .map_err(|e| PyRuntimeError::new_err(format!("mailbox send failed: {e}")))
        })
    }

    /// Receives (leases) the next message, waiting up to `timeout` seconds.
    ///
    /// Returns a `dict` (`message_id`, `from_agent`, `payload`, `sent_at`) or
    /// `None` if nothing arrives. The message is leased, not deleted; it is
    /// acknowledged automatically when the context completes, or explicitly via
    /// `ack`/`nack`.
    #[pyo3(signature = (timeout_secs=None))]
    fn receive<'py>(
        &self,
        py: Python<'py>,
        timeout_secs: Option<f64>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let mailbox = self.require_handle()?;
        let agent = self.caller_agent_name.clone();
        let run_id = self.run_id.clone();
        let deadline =
            timeout_secs.map(|secs| Instant::now() + Duration::from_secs_f64(secs.max(0.0)));
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            loop {
                if let Some(msg) = mailbox
                    .receive(&agent, run_id.clone(), RECEIVE_ROUND_TRIP)
                    .await
                {
                    return Python::with_gil(|py| {
                        json_to_pyobj(py, &message_to_json(&msg)).map(Some)
                    });
                }
                match deadline {
                    Some(d) if Instant::now() < d => {
                        tokio::time::sleep(RECEIVE_POLL_INTERVAL).await;
                    }
                    _ => return Ok(None::<PyObject>),
                }
            }
        })
    }

    /// Non-blocking receive: returns the next leased message or `None`.
    fn poll<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let mailbox = self.require_handle()?;
        let agent = self.caller_agent_name.clone();
        let run_id = self.run_id.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            match mailbox.receive(&agent, run_id, RECEIVE_ROUND_TRIP).await {
                Some(msg) => {
                    Python::with_gil(|py| json_to_pyobj(py, &message_to_json(&msg)).map(Some))
                }
                None => Ok(None::<PyObject>),
            }
        })
    }

    /// Returns the number of non-expired messages in the caller's inbox.
    fn pending<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let mailbox = self.require_handle()?;
        let agent = self.caller_agent_name.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            Ok(mailbox.pending_count(&agent).await)
        })
    }

    /// Lists up to `limit` messages (most recent first) without consuming them.
    #[pyo3(signature = (limit=50))]
    fn list<'py>(&self, py: Python<'py>, limit: usize) -> PyResult<Bound<'py, PyAny>> {
        let mailbox = self.require_handle()?;
        let agent = self.caller_agent_name.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let messages = mailbox.list_messages(&agent, limit).await;
            let arr = serde_json::Value::Array(messages.iter().map(message_to_json).collect());
            Python::with_gil(|py| json_to_pyobj(py, &arr))
        })
    }

    /// Acknowledges a leased message, deleting it from the store.
    fn ack<'py>(&self, py: Python<'py>, message_id: String) -> PyResult<Bound<'py, PyAny>> {
        let mailbox = self.require_handle()?;
        let agent = self.caller_agent_name.clone();
        let run_id = self.run_id.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            mailbox
                .ack(&agent, &message_id, run_id)
                .await
                .map_err(|e| PyRuntimeError::new_err(format!("mailbox ack failed: {e}")))
        })
    }

    /// Refuses a leased message, making it immediately deliverable again.
    fn nack<'py>(&self, py: Python<'py>, message_id: String) -> PyResult<Bound<'py, PyAny>> {
        let mailbox = self.require_handle()?;
        let agent = self.caller_agent_name.clone();
        let run_id = self.run_id.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            mailbox
                .nack(&agent, &message_id, run_id)
                .await
                .map_err(|e| PyRuntimeError::new_err(format!("mailbox nack failed: {e}")))
        })
    }
}

/// Shapes an [`apollia_runtime::mailbox::AgentMessage`] as the SDK `Message`.
fn message_to_json(msg: &apollia_runtime::mailbox::AgentMessage) -> serde_json::Value {
    serde_json::json!({
        "message_id": msg.message_id,
        "from_agent": msg.from,
        "payload": msg.payload,
        "sent_at": msg.sent_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_runtime::EventBus;

    /// A gated interface emits a PermissionRequired event for `mailbox:send`.
    #[test]
    fn test_gated_send_emits_permission_required() {
        // GIVEN a gated mail interface wired to an event bus
        let (event_tx, mut event_rx) = EventBus::new();
        let mail = MailInterface::new(
            None,
            "director".to_string(),
            None,
            true,
            None,
            true, // send_requires_approval
            Some(event_tx),
            None,
        );

        // WHEN an approval-required event is emitted for a recipient
        let req = mail.emit_approval_required("worker-b");

        // THEN a PermissionRequired event carries the mailbox:send tool name
        let event = event_rx.try_recv().expect("event emitted");
        assert!(
            matches!(
                event,
                RuntimeEvent::PermissionRequired { ref tool_name, ref request_id, ref input }
                if tool_name == "mailbox:send"
                    && *request_id == req
                    && input["to"] == serde_json::json!("worker-b")
                    && input["from"] == serde_json::json!("director")
            ),
            "unexpected event: {event:?}"
        );
    }

    /// An ungated interface has no approval requirement flagged.
    #[test]
    fn test_ungated_interface_flag_false() {
        // GIVEN a non-gated mail interface
        let mail = MailInterface::new(None, "a".to_string(), None, true, None, false, None, None);

        // THEN the send gate is not active
        assert!(!mail.send_requires_approval);
    }
}
