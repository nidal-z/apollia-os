//! Tauri IPC commands for the session meta-layer.
//!
//! Exposes:
//! - `compute_session_hallucination_risk`: heuristic score (always-on, no LLM).
//! - `get_session_replay_events`: returns the event log for the scrubber / replay.
//! - `append_session_replay_event`: allows the frontend (or integration tests)
//!   to push fixtures into the log until the EventBus wiring lands.
//!
//! LLM-backed routines (`GenerateSessionSummary`, `GenerateNextSteps`,
//! `GenerateSessionTitle`, `GenerateHallucinationRisk`) are opt-in via
//! `MetaLlmSettings::per_routine` and are called by the runtime when a
//! `MetaOrchestratorHandle` is plumbed through `RuntimeHandle`. That plumbing
//! is tracked separately; this module keeps the contract stable for the UI.

use apollia_runtime::analyzers::{
    compute_session_hallucination_risk, HallucinationRisk, SessionHallucinationInputs,
};
use apollia_runtime::session_replay::{ReplayState, SessionEvent, SessionEventLog};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// In-memory registry mapping `session_id -> SessionEventLog`.
///
/// Scaffolding until the EventBus → SQLite wiring lands. Cleared on process
/// restart; the UI must be resilient to an empty log.
fn session_event_logs() -> &'static Mutex<HashMap<String, SessionEventLog>> {
    static LOGS: OnceLock<Mutex<HashMap<String, SessionEventLog>>> = OnceLock::new();
    LOGS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn get_or_create_log(session_id: &str) -> SessionEventLog {
    let mut guard = session_event_logs()
        .lock()
        .expect("session_event_logs poisoned");
    guard.entry(session_id.to_owned()).or_default().clone()
}

/// Aggregated response from the session meta-layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMetaResponse {
    /// Heuristic score + factors (always-on).
    pub hallucination_risk: HallucinationRisk,
    /// Number of events available for replay.
    pub event_count: usize,
    /// `None` until `GenerateSessionSummary` is triggered.
    #[serde(default)]
    pub summary: Option<String>,
    /// `None` until `GenerateSessionTitle` is triggered.
    #[serde(default)]
    pub title: Option<String>,
    /// `None` until `GenerateNextSteps` is triggered.
    #[serde(default)]
    pub next_steps: Option<Vec<String>>,
}

/// Computes a session's meta state from the signals aggregated frontend-side.
///
/// The frontend provides the counters (P3 flags, citation gaps, contradictions);
/// the command returns the heuristic score + the number of available events.
#[tauri::command]
pub async fn compute_session_meta(
    session_id: String,
    inputs: SessionHallucinationInputs,
) -> Result<SessionMetaResponse, String> {
    let risk = compute_session_hallucination_risk(&inputs);
    let event_count = get_or_create_log(&session_id).len();

    Ok(SessionMetaResponse {
        hallucination_risk: risk,
        event_count,
        summary: None,
        title: None,
        next_steps: None,
    })
}

/// Returns the chronological list of a session's events for the scrubber and
/// replay.
#[tauri::command]
pub async fn get_session_replay_events(session_id: String) -> Result<Vec<SessionEvent>, String> {
    Ok(get_or_create_log(&session_id).snapshot())
}

/// Returns the initial replay state (cursor 0, paused, 1x).
#[tauri::command]
pub async fn get_session_replay_state(session_id: String) -> Result<ReplayState, String> {
    let total = get_or_create_log(&session_id).len();
    Ok(ReplayState::initial(total))
}

/// Appends an event to the session log.
///
/// Used by the upcoming runtime wiring and the UI integration tests.
#[tauri::command]
pub async fn append_session_replay_event(
    session_id: String,
    event: SessionEvent,
) -> Result<usize, String> {
    let log = get_or_create_log(&session_id);
    log.push(event);
    Ok(log.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_runtime::session_replay::SessionEventKind;

    /// GIVEN no signals
    /// WHEN compute_session_meta()
    /// THEN a zero-score response is returned.
    #[tokio::test]
    async fn compute_meta_zero_when_no_signals() {
        let resp = compute_session_meta("s-empty".into(), SessionHallucinationInputs::default())
            .await
            .expect("ok");
        assert_eq!(resp.hallucination_risk.score, 0);
        assert_eq!(resp.event_count, 0);
        assert!(resp.summary.is_none());
    }

    /// GIVEN signals with tool flags + citation gaps
    /// WHEN compute_session_meta()
    /// THEN score > 0 and factors enumerate contributors.
    #[tokio::test]
    async fn compute_meta_combines_signals() {
        let inputs = SessionHallucinationInputs {
            heuristic_flag_count: 2,
            total_tool_outputs: 4,
            assertion_citation_gaps: 3,
            total_assertions: 6,
            thinking_contradictions: 1,
        };
        let resp = compute_session_meta("s-signals".into(), inputs)
            .await
            .expect("ok");
        assert!(resp.hallucination_risk.score > 0);
        assert_eq!(resp.hallucination_risk.factors.len(), 3);
    }

    /// GIVEN events appended to a session log
    /// WHEN get_session_replay_events() is called
    /// THEN events are returned in chronological order.
    #[tokio::test]
    async fn replay_events_round_trip() {
        let sid = "s-replay";
        append_session_replay_event(
            sid.into(),
            SessionEvent {
                ts: "2026-04-20T10:00:00Z".into(),
                kind: SessionEventKind::Tool,
                label: "bash".into(),
                correlation_id: Some("tool-1".into()),
                payload_json: serde_json::json!({"cmd": "ls"}),
            },
        )
        .await
        .expect("ok");
        let events = get_session_replay_events(sid.into()).await.expect("ok");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].label, "bash");

        let state = get_session_replay_state(sid.into()).await.expect("ok");
        assert_eq!(state.total, 1);
        assert!(!state.playing);
    }
}
