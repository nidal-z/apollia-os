//! ctx.events: typed event emission for agents.
//!
//! Wraps the runtime `EventBus` in a `#[pyclass]` consumable from Python via
//! `ctx.events.<verb>(...)`. Each method is a silent no-op when the context
//! has no bus (test / CLI dry-run mode without a persistor), so the agent
//! stays portable without conditional checks.
//!
//! Additive successor of the existing flat methods on `RuntimeContext`
//! (`emit_token`, `emit_thought`, `emit_retry`, `emit_action_parse_error`).
//! The old ones remain functional but are marked `#[deprecated]`, to be
//! removed once agents have migrated.

use apollia_core::events::{AgentId, EventBusSender, RuntimeEvent, TaskId};
use pyo3::prelude::*;

/// Typed runtime event-emission interface exposed via `ctx.events`.
///
/// The pyclass captures an immutable view of the emission chain at
/// `RuntimeContext` construction time: bus, task_id, agent_id, and the
/// optional `message_id` of the current chat turn. If any of these fields is
/// missing the methods become silent no-ops, so the agent never has to check
/// for the bus before emitting.
#[pyclass(name = "EventsInterface", module = "apollia._native")]
pub struct EventsInterface {
    /// Target broadcast bus (`apollia_core::events`). `None` disables emission
    /// without breaking the no-op semantics.
    event_bus: Option<EventBusSender>,
    /// Task id for typed events (Thought, Retry, ActionParseError).
    task_id: Option<TaskId>,
    /// Emitting agent id.
    agent_id: AgentId,
    /// Current chat session (for `emit_token`). `None` in task mode.
    chat_session_id: Option<String>,
    /// Current message to tag on streamed tokens. `None` in task mode.
    chat_message_id: Option<String>,
}

#[pymethods]
impl EventsInterface {
    /// Emits a streamed token to the frontend in chat mode (`ChatToken`).
    ///
    /// Silent no-op in task mode or if the session/message is missing.
    /// Pairs with `ChatTokenStreamed` on the SSE side; filtering by
    /// `session_id` happens in `routes_chat.rs`.
    fn emit_token(&self, token: String) -> PyResult<()> {
        let (Some(session_id), Some(message_id), Some(bus)) = (
            self.chat_session_id.as_ref(),
            self.chat_message_id.as_ref(),
            self.event_bus.as_ref(),
        ) else {
            return Ok(());
        };
        // Fire-and-forget: ignore the error if the bus is saturated.
        let _ = bus.send(RuntimeEvent::ChatToken {
            session_id: session_id.clone(),
            message_id: message_id.clone(),
            token,
        });
        Ok(())
    }

    /// Emits a ReAct `Thought` (reasoning chain).
    ///
    /// Captured by the Python SDK (`react.py`) on each turn. Shown in builder
    /// mode, hidden in operator mode by default.
    ///
    /// Signature aligned with the Python Protocol:
    /// `emit_thought(text: str, *, step: int)`. The `step` parameter is
    /// keyword-only on the Python side, preventing positional confusion.
    #[pyo3(signature = (text, *, step))]
    fn emit_thought(&self, text: String, step: u32) -> PyResult<()> {
        let (Some(task_id), Some(bus)) = (self.task_id.as_ref(), self.event_bus.as_ref()) else {
            // Structured fallback for tests without a bus.
            tracing::info!(target: "apollia.agent.thought", step = step, "{}", text);
            return Ok(());
        };
        let _ = bus.send(RuntimeEvent::Thought {
            task_id: task_id.clone(),
            agent_id: self.agent_id.clone(),
            step_num: step,
            text,
        });
        Ok(())
    }

    /// Emits a `Retry` event (parse error, tool error, llm error).
    ///
    /// Signature aligned with the Python Protocol:
    /// `emit_retry(*, step: int, reason: str, count: int)`. Maps to
    /// `RuntimeEvent::Retry { step_num, cause, attempt }`:
    /// `reason -> cause`, `count -> attempt`.
    ///
    /// `reason` must be one of the normalized strings:
    /// `"action_parse_error" | "tool_error" | "llm_error" | "other"`.
    #[pyo3(signature = (*, step, reason, count))]
    fn emit_retry(&self, step: u32, reason: String, count: u32) -> PyResult<()> {
        let (Some(task_id), Some(bus)) = (self.task_id.as_ref(), self.event_bus.as_ref()) else {
            tracing::warn!(
                target: "apollia.agent.retry",
                step = step,
                count = count,
                "{}",
                reason
            );
            return Ok(());
        };
        let _ = bus.send(RuntimeEvent::Retry {
            task_id: task_id.clone(),
            agent_id: self.agent_id.clone(),
            step_num: step,
            cause: reason,
            attempt: count,
        });
        Ok(())
    }

    /// Emits an `ActionParseError` (invalid action JSON, unrepairable).
    ///
    /// Signature aligned with the Python Protocol:
    /// `emit_action_parse_error(*, step: int, raw: str, fatal: bool = False)`.
    /// Maps to `RuntimeEvent::ActionParseError`:
    /// `raw -> raw_content`, `fatal -> repair_attempted`.
    #[pyo3(signature = (*, step, raw, fatal = false))]
    fn emit_action_parse_error(&self, step: u32, raw: String, fatal: bool) -> PyResult<()> {
        let (Some(task_id), Some(bus)) = (self.task_id.as_ref(), self.event_bus.as_ref()) else {
            tracing::warn!(
                target: "apollia.agent.action_parse_error",
                step = step,
                fatal = fatal,
                "{}",
                raw
            );
            return Ok(());
        };
        let _ = bus.send(RuntimeEvent::ActionParseError {
            task_id: task_id.clone(),
            agent_id: self.agent_id.clone(),
            step_num: step,
            raw_content: raw,
            repair_attempted: fatal,
        });
        Ok(())
    }
}

impl EventsInterface {
    /// Builds a new typed events interface.
    ///
    /// `event_bus = None` makes all methods silent no-ops.
    /// `task_id = None` is tolerated: the non-token typed variants fall back
    /// to `tracing::*`.
    pub fn new(
        event_bus: Option<EventBusSender>,
        task_id: Option<TaskId>,
        agent_id: AgentId,
        chat_session_id: Option<String>,
        chat_message_id: Option<String>,
    ) -> Self {
        Self {
            event_bus,
            task_id,
            agent_id,
            chat_session_id,
            chat_message_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_core::events::{AgentId, TaskId};
    use tokio::sync::broadcast;

    fn make_bus(
        cap: usize,
    ) -> (
        apollia_core::events::EventBusSender,
        broadcast::Receiver<RuntimeEvent>,
    ) {
        broadcast::channel::<RuntimeEvent>(cap)
    }

    /// `emit_thought` propagates on the bus with the expected shape and the
    /// `(text, *, step)` signature from the Python Protocol.
    #[test]
    fn test_emit_thought_publishes_to_bus() {
        // GIVEN an EventsInterface attached to a bus + task_id + agent_id
        pyo3::prepare_freethreaded_python();
        let (tx, mut rx) = make_bus(16);
        let task_id = TaskId::from("task-42".to_string());
        let agent_id = AgentId::new_v4();
        let iface = EventsInterface::new(
            Some(tx),
            Some(task_id.clone()),
            agent_id.clone(),
            None,
            None,
        );

        // WHEN emit_thought is invoked
        iface
            .emit_thought("reasoning".to_string(), 3)
            .expect("emit_thought");

        // THEN the bus receives RuntimeEvent::Thought with matching fields.
        let evt = rx.try_recv().expect("event on bus");
        match evt {
            RuntimeEvent::Thought {
                task_id: tid,
                agent_id: aid,
                step_num,
                text,
            } => {
                assert_eq!(tid, task_id);
                assert_eq!(aid, agent_id);
                assert_eq!(step_num, 3);
                assert_eq!(text, "reasoning");
            }
            other => panic!("expected Thought, got {other:?}"),
        }
    }

    /// `emit_retry` correctly maps `reason -> cause`, `count -> attempt`.
    #[test]
    fn test_emit_retry_maps_python_names_to_runtime_event() {
        // GIVEN an EventsInterface with bus + task_id
        pyo3::prepare_freethreaded_python();
        let (tx, mut rx) = make_bus(16);
        let task_id = TaskId::from("task-99".to_string());
        let iface = EventsInterface::new(
            Some(tx),
            Some(task_id.clone()),
            AgentId::new_v4(),
            None,
            None,
        );

        // WHEN emit_retry is called with the Protocol-aligned kwargs
        iface
            .emit_retry(2, "tool_error".to_string(), 1)
            .expect("emit_retry");

        // THEN the bus event carries cause/attempt with the right values.
        let evt = rx.try_recv().expect("event on bus");
        match evt {
            RuntimeEvent::Retry {
                task_id: tid,
                step_num,
                cause,
                attempt,
                ..
            } => {
                assert_eq!(tid, task_id);
                assert_eq!(step_num, 2);
                assert_eq!(cause, "tool_error");
                assert_eq!(attempt, 1);
            }
            other => panic!("expected Retry, got {other:?}"),
        }
    }

    /// `emit_action_parse_error` maps `raw -> raw_content`,
    /// `fatal -> repair_attempted`.
    #[test]
    fn test_emit_action_parse_error_maps_fields() {
        pyo3::prepare_freethreaded_python();
        let (tx, mut rx) = make_bus(16);
        let task_id = TaskId::from("task-7".to_string());
        let iface = EventsInterface::new(Some(tx), Some(task_id), AgentId::new_v4(), None, None);

        iface
            .emit_action_parse_error(5, "{ broken".to_string(), true)
            .expect("emit_action_parse_error");

        let evt = rx.try_recv().expect("event on bus");
        match evt {
            RuntimeEvent::ActionParseError {
                step_num,
                raw_content,
                repair_attempted,
                ..
            } => {
                assert_eq!(step_num, 5);
                assert_eq!(raw_content, "{ broken");
                assert!(repair_attempted);
            }
            other => panic!("expected ActionParseError, got {other:?}"),
        }
    }

    /// Without a bus, `emit_thought` stays a silent no-op (tracing fallback).
    #[test]
    fn test_emit_thought_noop_without_bus() {
        pyo3::prepare_freethreaded_python();
        let iface = EventsInterface::new(None, None, AgentId::new_v4(), None, None);
        // Just verify it doesn't error; tracing fallback covers stderr.
        iface
            .emit_thought("anything".to_string(), 1)
            .expect("noop should succeed");
    }
}
