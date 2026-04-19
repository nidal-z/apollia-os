//! Session replay (US-SP42-048 — Pattern P12).
//!
//! Stocke en mémoire (et, à terme, en SQLite `session_events (session_id, ts,
//! kind, payload_json)`) les événements d'une session pour permettre un
//! re-jeu chronologique dans le frontend (`SessionReplayControls.svelte`).
//!
//! Ce module expose :
//! - [`SessionEvent`] — une entrée horodatée catégorisée (tool/memory/hitl/a2a/error).
//! - [`SessionEventKind`] — l'enum des catégories affichées par le scrubber.
//! - [`ReplayState`] — l'état partagé avec le frontend (index, vitesse, lecture).
//! - [`SessionEventLog`] — un log append-only en mémoire, cloné par session.
//!
//! Le câblage complet (EventBus → SessionEventLog → SQLite) est suivi dans une
//! story ultérieure. Le contrat sérialisable ici est suffisant pour exercer
//! l'UI via `commands::session_meta` avec des fixtures.

use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

/// Catégorie d'événement affichée comme marqueur dans le scrubber global.
///
/// Le `correlation_id` porté par [`SessionEvent`] permet au frontend de
/// réaliser le drill-down partagé (step_id pour A2A, tool_call_id pour tools,
/// hitl_id pour les approbations).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionEventKind {
    /// Invocation d'outil (correlation_id = tool_call_id).
    Tool,
    /// Écriture/lecture mémoire.
    Memory,
    /// Pause HITL (correlation_id = hitl_id).
    Hitl,
    /// Invocation A2A (correlation_id = step_id).
    A2a,
    /// Erreur tool/LLM/runtime.
    Error,
}

/// Événement horodaté persisté pour le replay et le scrubber.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionEvent {
    /// ISO-8601 UTC ("2026-04-20T12:34:56.789Z").
    pub ts: String,
    /// Catégorie pour le filtrage/marqueurs.
    pub kind: SessionEventKind,
    /// Label court pour le tooltip (≤ 60 chars conseillés).
    pub label: String,
    /// Id partagé pour le drill-down (voir doc [`SessionEventKind`]).
    #[serde(default)]
    pub correlation_id: Option<String>,
    /// Payload sérialisé (détail brut cliquable dans le panneau).
    #[serde(default)]
    pub payload_json: serde_json::Value,
}

/// État de lecture transmis au frontend pour contrôler `SessionReplayControls`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReplayState {
    /// Index de l'événement courant (0-based).
    pub cursor: usize,
    /// Nombre total d'événements du log.
    pub total: usize,
    /// `true` si la lecture automatique est active.
    pub playing: bool,
    /// Multiplicateur de vitesse (1.0 / 2.0 / 5.0).
    pub speed: f32,
}

impl ReplayState {
    /// État initial : curseur à 0, en pause, vitesse 1x.
    pub fn initial(total: usize) -> Self {
        Self {
            cursor: 0,
            total,
            playing: false,
            speed: 1.0,
        }
    }
}

/// Log append-only thread-safe des événements d'une session.
///
/// Cloner le log partage le même stockage interne (Arc+Mutex). Les événements
/// sont triés par horodatage à l'insertion — un `push` hors-ordre coûte un
/// re-tri O(n log n), acceptable pour un flux ≤ quelques centaines d'events.
#[derive(Debug, Clone, Default)]
pub struct SessionEventLog {
    inner: Arc<Mutex<Vec<SessionEvent>>>,
}

impl SessionEventLog {
    /// Crée un log vide.
    pub fn new() -> Self {
        Self::default()
    }

    /// Ajoute un événement. Re-trie si `ts` précède le dernier.
    pub fn push(&self, event: SessionEvent) {
        let mut guard = self.inner.lock().expect("SessionEventLog poisoned");
        let needs_sort = guard
            .last()
            .map(|last| event.ts < last.ts)
            .unwrap_or(false);
        guard.push(event);
        if needs_sort {
            guard.sort_by(|a, b| a.ts.cmp(&b.ts));
        }
    }

    /// Retourne une copie triée des événements.
    pub fn snapshot(&self) -> Vec<SessionEvent> {
        self.inner
            .lock()
            .expect("SessionEventLog poisoned")
            .clone()
    }

    /// Nombre d'événements actuellement stockés.
    pub fn len(&self) -> usize {
        self.inner
            .lock()
            .expect("SessionEventLog poisoned")
            .len()
    }

    /// `true` si aucun événement n'est stocké.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Filtre les événements par catégorie (utile pour les marqueurs du scrubber).
    pub fn filter_kinds(&self, kinds: &[SessionEventKind]) -> Vec<SessionEvent> {
        self.snapshot()
            .into_iter()
            .filter(|e| kinds.contains(&e.kind))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(ts: &str, kind: SessionEventKind, label: &str) -> SessionEvent {
        SessionEvent {
            ts: ts.to_string(),
            kind,
            label: label.to_string(),
            correlation_id: None,
            payload_json: serde_json::Value::Null,
        }
    }

    /// GIVEN a fresh log
    /// WHEN push events in order
    /// THEN snapshot returns them in insertion order.
    #[test]
    fn pushes_events_in_order() {
        let log = SessionEventLog::new();
        log.push(event("2026-04-20T10:00:00Z", SessionEventKind::Tool, "bash"));
        log.push(event("2026-04-20T10:00:01Z", SessionEventKind::Memory, "read"));
        let events = log.snapshot();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].label, "bash");
    }

    /// GIVEN out-of-order pushes
    /// WHEN snapshot
    /// THEN events are sorted by timestamp.
    #[test]
    fn sorts_out_of_order_pushes() {
        let log = SessionEventLog::new();
        log.push(event("2026-04-20T10:00:02Z", SessionEventKind::Tool, "b"));
        log.push(event("2026-04-20T10:00:01Z", SessionEventKind::Tool, "a"));
        let events = log.snapshot();
        assert_eq!(events[0].label, "a");
        assert_eq!(events[1].label, "b");
    }

    /// GIVEN a mix of kinds
    /// WHEN filter_kinds is called with a subset
    /// THEN only matching kinds are returned.
    #[test]
    fn filters_by_kinds() {
        let log = SessionEventLog::new();
        log.push(event("2026-04-20T10:00:00Z", SessionEventKind::Tool, "t"));
        log.push(event("2026-04-20T10:00:01Z", SessionEventKind::Hitl, "h"));
        log.push(event("2026-04-20T10:00:02Z", SessionEventKind::Error, "e"));
        let filtered = log.filter_kinds(&[SessionEventKind::Hitl, SessionEventKind::Error]);
        assert_eq!(filtered.len(), 2);
        assert_eq!(filtered[0].label, "h");
    }

    /// GIVEN a log of N events
    /// WHEN ReplayState::initial(N)
    /// THEN cursor=0, playing=false, speed=1.0.
    #[test]
    fn replay_state_initial_is_paused() {
        let state = ReplayState::initial(5);
        assert_eq!(state.cursor, 0);
        assert_eq!(state.total, 5);
        assert!(!state.playing);
        assert!((state.speed - 1.0).abs() < f32::EPSILON);
    }
}
