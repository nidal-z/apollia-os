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
    fn emit_thought(&self, text: String, step_num: u32) -> PyResult<()> {
        let (Some(task_id), Some(bus)) = (self.task_id.as_ref(), self.event_bus.as_ref()) else {
            // Fallback structuré pour les tests sans bus.
            tracing::info!(target: "apollia.agent.thought", step = step_num, "{}", text);
            return Ok(());
        };
        let _ = bus.send(RuntimeEvent::Thought {
            task_id: task_id.clone(),
            agent_id: self.agent_id.clone(),
            step_num,
            text,
        });
        Ok(())
    }

    /// Émet un événement `Retry` (parse error, tool error, llm error).
    ///
    /// `cause` doit être l'une des chaînes normalisées :
    /// `"action_parse_error" | "tool_error" | "llm_error" | "other"`.
    fn emit_retry(&self, step_num: u32, cause: String, attempt: u32) -> PyResult<()> {
        let (Some(task_id), Some(bus)) = (self.task_id.as_ref(), self.event_bus.as_ref()) else {
            tracing::warn!(
                target: "apollia.agent.retry",
                step = step_num,
                attempt = attempt,
                "{}",
                cause
            );
            return Ok(());
        };
        let _ = bus.send(RuntimeEvent::Retry {
            task_id: task_id.clone(),
            agent_id: self.agent_id.clone(),
            step_num,
            cause,
            attempt,
        });
        Ok(())
    }

    /// Émet un `ActionParseError` (JSON action invalide, non-réparable).
    fn emit_action_parse_error(
        &self,
        step_num: u32,
        raw_content: String,
        repair_attempted: bool,
    ) -> PyResult<()> {
        let (Some(task_id), Some(bus)) = (self.task_id.as_ref(), self.event_bus.as_ref()) else {
            tracing::warn!(
                target: "apollia.agent.action_parse_error",
                step = step_num,
                repair_attempted = repair_attempted,
                "{}",
                raw_content
            );
            return Ok(());
        };
        let _ = bus.send(RuntimeEvent::ActionParseError {
            task_id: task_id.clone(),
            agent_id: self.agent_id.clone(),
            step_num,
            raw_content,
            repair_attempted,
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
