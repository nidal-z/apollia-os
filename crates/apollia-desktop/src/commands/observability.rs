//! Tauri IPC commands for the Observability view.
//!
//! The global timeline and the inter-agent mailbox are read by the runtime
//! (`apollia_runtime::observability`), which scans its own data directory; the
//! commands reach it over its routes through [`RuntimeLink`], in both modes.

use crate::attached::RuntimeLink;
use serde::Deserialize;
use tauri::State;

/// The daily costs and the hooks live in `audit`, the plan cache in
/// `plan_cache`, the mailbox in `mailbox`.
pub mod audit;
pub mod mailbox;
pub mod plan_cache;

pub use apollia_runtime::observability::global_timeline::GlobalTimelineEvent;

// ---------------------------------------------------------------------------
// Global Timeline
// ---------------------------------------------------------------------------

/// Parameters for `get_global_timeline`.
#[derive(Debug, Deserialize)]
pub struct TimelineParams {
    /// Time window in minutes (30, 60, 360, 720, 1440).
    pub window_minutes: u32,
}

/// Fetches the global timeline of a trailing window, most recent first, from
/// `GET /api/v1/observability/timeline`.
#[tauri::command]
pub async fn get_global_timeline(
    state: State<'_, RuntimeLink>,
    params: TimelineParams,
) -> Result<Vec<GlobalTimelineEvent>, String> {
    timeline_on(&state, params.window_minutes).await
}

/// The timeline read on the runtime `link` drives.
async fn timeline_on(
    link: &RuntimeLink,
    window_minutes: u32,
) -> Result<Vec<GlobalTimelineEvent>, String> {
    let path = format!("/api/v1/observability/timeline?window_minutes={window_minutes}");
    let json = link.get_json(&path).await?;
    serde_json::from_value(json).map_err(|e| format!("unreadable timeline: {e}"))
}

#[cfg(test)]
mod attached_tests;
