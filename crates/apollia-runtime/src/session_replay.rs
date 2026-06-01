//! Session replay.
//!
//! Stores session events in memory (and eventually in the SQLite
//! `session_events (session_id, ts, kind, payload_json)` table) so the frontend
//! (`SessionReplayControls.svelte`) can replay them chronologically.
//!
//! This module exposes:
//! - [`SessionEvent`], a categorized timestamped entry (tool/memory/hitl/a2a/error).
//! - [`SessionEventKind`], the enum of categories shown by the scrubber.
//! - [`ReplayState`], the state shared with the frontend (index, speed, playback).
//! - [`SessionEventLog`], an in-memory append-only log, cloned per session.
//!
//! Full wiring (EventBus to SessionEventLog to SQLite) is handled separately.
//! The serializable contract here is enough to exercise the UI via
//! `commands::session_meta` with fixtures.

use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

/// Event category shown as a marker in the global scrubber.
///
/// The `correlation_id` carried by [`SessionEvent`] lets the frontend perform
/// the shared drill-down (step_id for A2A, tool_call_id for tools, hitl_id for
/// approvals).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionEventKind {
    /// Tool invocation (correlation_id = tool_call_id).
    Tool,
    /// Memory read/write.
    Memory,
    /// HITL pause (correlation_id = hitl_id).
    Hitl,
    /// A2A invocation (correlation_id = step_id).
    A2a,
    /// Tool/LLM/runtime error.
    Error,
}

/// Timestamped event persisted for replay and the scrubber.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionEvent {
    /// ISO-8601 UTC ("2026-04-20T12:34:56.789Z").
    pub ts: String,
    /// Category for filtering and markers.
    pub kind: SessionEventKind,
    /// Short label for the tooltip (60 chars or fewer recommended).
    pub label: String,
    /// Shared id for the drill-down (see [`SessionEventKind`] docs).
    #[serde(default)]
    pub correlation_id: Option<String>,
    /// Serialized payload (raw detail, clickable in the panel).
    #[serde(default)]
    pub payload_json: serde_json::Value,
}

/// Playback state sent to the frontend to drive `SessionReplayControls`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReplayState {
    /// Index of the current event (0-based).
    pub cursor: usize,
    /// Total number of events in the log.
    pub total: usize,
    /// `true` when automatic playback is active.
    pub playing: bool,
    /// Speed multiplier (1.0 / 2.0 / 5.0).
    pub speed: f32,
}

impl ReplayState {
    /// Initial state: cursor at 0, paused, speed 1x.
    pub fn initial(total: usize) -> Self {
        Self {
            cursor: 0,
            total,
            playing: false,
            speed: 1.0,
        }
    }
}

/// Thread-safe append-only log of a session's events.
///
/// Cloning the log shares the same internal storage (Arc+Mutex). Events are
/// sorted by timestamp on insertion; an out-of-order `push` costs an O(n log n)
/// re-sort, acceptable for a stream of at most a few hundred events.
#[derive(Debug, Clone, Default)]
pub struct SessionEventLog {
    inner: Arc<Mutex<Vec<SessionEvent>>>,
}

impl SessionEventLog {
    /// Creates an empty log.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds an event. Re-sorts if `ts` precedes the last entry.
    pub fn push(&self, event: SessionEvent) {
        let mut guard = self.inner.lock().expect("SessionEventLog poisoned");
        let needs_sort = guard.last().map(|last| event.ts < last.ts).unwrap_or(false);
        guard.push(event);
        if needs_sort {
            guard.sort_by(|a, b| a.ts.cmp(&b.ts));
        }
    }

    /// Returns a sorted copy of the events.
    pub fn snapshot(&self) -> Vec<SessionEvent> {
        self.inner.lock().expect("SessionEventLog poisoned").clone()
    }

    /// Number of events currently stored.
    pub fn len(&self) -> usize {
        self.inner.lock().expect("SessionEventLog poisoned").len()
    }

    /// `true` when no event is stored.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Filters events by category (useful for scrubber markers).
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
        log.push(event(
            "2026-04-20T10:00:00Z",
            SessionEventKind::Tool,
            "bash",
        ));
        log.push(event(
            "2026-04-20T10:00:01Z",
            SessionEventKind::Memory,
            "read",
        ));
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
