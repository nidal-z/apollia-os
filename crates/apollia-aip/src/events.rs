//! ctx.events — typed event emission for agents (LOT 4 — ADR-105).
//!
//! Encapsule l'`EventBus` runtime dans un `#[pyclass]` consommable depuis
//! Python via `ctx.events.<verb>(...)`. Chaque méthode est un no-op
//! silencieux lorsque le contexte ne dispose pas du bus (mode test/CLI
//! dry-run sans persistor) — l'agent reste portable sans test conditionnel.
//!
//! Successeur additif des méthodes flat existantes sur `RuntimeContext`
//! (`emit_token`, `emit_thought`, `emit_retry`, `emit_action_parse_error`).
//! Les anciennes restent fonctionnelles mais marquées `#[deprecated]` —
//! suppression effective en LOT 9 après migration des agents (LOT 13).

use apollia_core::events::{AgentId, EventBusSender, RuntimeEvent, TaskId};
use pyo3::prelude::*;

/// Interface typée d'émission d'événements runtime exposée via `ctx.events`.
///
/// Le pyclass capture une vue immuable de la chaîne d'émission au moment de
/// la construction du `RuntimeContext` : bus, task_id, agent_id, et l'éventuel
/// `message_id` du tour de chat en cours. Si l'un de ces champs manque, les
/// méthodes deviennent des no-op silencieux — l'agent n'a jamais besoin de
/// vérifier la présence du bus avant d'émettre.
#[pyclass(name = "EventsInterface", module = "apollia._native")]
pub struct EventsInterface {
    /// Bus broadcast cible (`apollia_core::events`). `None` désactive
    /// l'émission sans casser la sémantique no-op.
    event_bus: Option<EventBusSender>,
    /// Identifiant de tâche pour les événements typés (Thought, Retry,
    /// ActionParseError).
    task_id: Option<TaskId>,
    /// Identifiant d'agent émetteur.
    agent_id: AgentId,
    /// Session de chat en cours (pour `emit_token`). `None` en mode task.
    chat_session_id: Option<String>,
    /// Message courant à taguer sur les tokens streamés. `None` en mode task.
    chat_message_id: Option<String>,
}

#[pymethods]
impl EventsInterface {
    /// Émet un token streamé vers le frontend en mode chat (`ChatToken`).
    ///
    /// No-op silencieux en mode task ou si la session/message manque.
    /// Forme un sandwich avec `ChatTokenStreamed` côté SSE — le filtrage par
    /// `session_id` se fait dans `routes_chat.rs`.
    fn emit_token(&self, token: String) -> PyResult<()> {
        let (Some(session_id), Some(message_id), Some(bus)) = (
            self.chat_session_id.as_ref(),
            self.chat_message_id.as_ref(),
            self.event_bus.as_ref(),
        ) else {
            return Ok(());
        };
        // fire-and-forget : on ignore l'erreur si le bus est saturé.
        let _ = bus.send(RuntimeEvent::ChatToken {
            session_id: session_id.clone(),
            message_id: message_id.clone(),
            token,
        });
        Ok(())
    }

    /// Émet une `Thought` ReAct (chaîne de raisonnement).
    ///
    /// Capturée par le SDK Python (`react.py`) à chaque tour. Affiché en
    /// mode builder, masqué en mode operator par défaut.
    ///
    /// Signature alignée sur le Protocol Python (ADR-105) :
    /// `emit_thought(text: str, *, step: int)`. Le paramètre `step` est
    /// keyword-only côté Python, ce qui empêche les confusions positionnelles.
    #[pyo3(signature = (text, *, step))]
    fn emit_thought(&self, text: String, step: u32) -> PyResult<()> {
        let (Some(task_id), Some(bus)) = (self.task_id.as_ref(), self.event_bus.as_ref()) else {
            // Fallback structuré pour les tests sans bus.
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

    /// Émet un événement `Retry` (parse error, tool error, llm error).
    ///
    /// Signature alignée sur le Protocol Python (ADR-105) :
    /// `emit_retry(*, step: int, reason: str, count: int)`. Mapping
    /// vers `RuntimeEvent::Retry { step_num, cause, attempt }` :
    /// `reason → cause`, `count → attempt`.
    ///
    /// `reason` doit être l'une des chaînes normalisées :
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

    /// Émet un `ActionParseError` (JSON action invalide, non-réparable).
    ///
    /// Signature alignée sur le Protocol Python (ADR-105) :
    /// `emit_action_parse_error(*, step: int, raw: str, fatal: bool = False)`.
    /// Mapping vers `RuntimeEvent::ActionParseError` :
    /// `raw → raw_content`, `fatal → repair_attempted`.
    #[pyo3(signature = (*, step, raw, fatal = false))]
    fn emit_action_parse_error(
        &self,
        step: u32,
        raw: String,
        fatal: bool,
    ) -> PyResult<()> {
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
    /// Construit une nouvelle interface évènements typée.
    ///
    /// `event_bus = None` ⇒ toutes les méthodes deviennent no-op silencieux.
    /// `task_id = None` est toléré : les variants typés non-token retombent
    /// sur `tracing::*`.
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

    fn make_bus(cap: usize) -> (
        apollia_core::events::EventBusSender,
        broadcast::Receiver<RuntimeEvent>,
    ) {
        broadcast::channel::<RuntimeEvent>(cap)
    }

    /// LOT 8 (ADR-105) — `emit_thought` propage bien sur le bus avec le
    /// shape attendu et la signature `(text, *, step)` du Protocol Python.
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

    /// `emit_retry` mappe correctement `reason → cause`, `count → attempt`.
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

    /// `emit_action_parse_error` mappe `raw → raw_content`,
    /// `fatal → repair_attempted` (interop ADR-105 ↔ ADR-088).
    #[test]
    fn test_emit_action_parse_error_maps_fields() {
        pyo3::prepare_freethreaded_python();
        let (tx, mut rx) = make_bus(16);
        let task_id = TaskId::from("task-7".to_string());
        let iface = EventsInterface::new(
            Some(tx),
            Some(task_id),
            AgentId::new_v4(),
            None,
            None,
        );

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

    /// Sans bus, `emit_thought` reste un no-op silencieux (fallback tracing).
    #[test]
    fn test_emit_thought_noop_without_bus() {
        pyo3::prepare_freethreaded_python();
        let iface = EventsInterface::new(None, None, AgentId::new_v4(), None, None);
        // Just verify it doesn't error — tracing fallback covers stderr.
        iface
            .emit_thought("anything".to_string(), 1)
            .expect("noop should succeed");
    }
}
