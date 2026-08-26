//! Request and response bodies of the trigger routes.
//!
//! One struct per wire shape, `camelCase` on the JSON side, kept apart from
//! the handlers so the contract reads on its own.

use serde::{Deserialize, Serialize};

use apollia_triggers::TriggerHistoryEntry;

// ─── Request types ───────────────────────────────────────────────────────

/// Request body for `POST /api/v1/triggers`, trigger creation.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CreateTriggerRequest {
    /// Unique trigger identifier.
    pub id: String,
    /// Target agent.
    pub agent: Option<String>,
    /// Whether the trigger is active (default: `true`).
    pub enabled: Option<bool>,
    /// Policy when the agent is busy (default: `"queue"`).
    pub on_busy: Option<String>,
    /// Trigger source configuration.
    pub source: TriggerSourceInput,
    /// Input message template.
    pub input_template: Option<String>,
}

/// Trigger source description used in create/update requests.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct TriggerSourceInput {
    /// Source type: `"cron"`, `"interval"`, `"oneshot"`, `"file_watch"`, `"webhook"`.
    pub r#type: String,
    /// Source-specific configuration (fields flattened via `serde(flatten)`).
    #[serde(flatten)]
    #[schema(value_type = Object)]
    pub config: serde_json::Value,
}

/// Request body for `PUT /api/v1/triggers/:id`, trigger update.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct UpdateTriggerRequest {
    /// Target agent.
    pub agent: Option<String>,
    /// Whether the trigger is active.
    pub enabled: Option<bool>,
    /// Policy when the agent is busy.
    pub on_busy: Option<String>,
    /// Trigger source configuration.
    pub source: TriggerSourceInput,
    /// Input message template.
    pub input_template: Option<String>,
}

// ─── Response types ──────────────────────────────────────────────────────

/// Response for CRUD operations returning a full definition.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct TriggerDefinitionResponse {
    /// Unique trigger identifier.
    pub id: String,
    /// Target agent.
    pub agent: Option<String>,
    /// Whether the trigger is active.
    pub enabled: bool,
    /// Policy when the agent is busy.
    pub on_busy: String,
    /// Source type.
    pub source_type: String,
    /// Source JSON configuration.
    #[schema(value_type = Object)]
    pub source_config: serde_json::Value,
    /// Input message template.
    pub input_template: Option<String>,
    /// Creation timestamp (ISO 8601).
    pub created_at: String,
    /// Last-modified timestamp (ISO 8601).
    pub updated_at: String,
}

/// Response for `DELETE /api/v1/triggers/:id`.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct DeleteResponse {
    /// Identifier of the deleted trigger.
    pub deleted: String,
}

/// Success response body for reload.
#[derive(Serialize, utoipa::ToSchema)]
pub struct ReloadResponse {
    /// Number of active triggers after reload.
    pub reloaded: usize,
}

/// Error response body.
#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorResponse {
    /// Error description.
    pub error: String,
}

/// Response for `GET /api/v1/triggers/:id`, detailed status (legacy).
#[derive(Serialize)]
pub struct TriggerDetailResponse {
    /// Trigger identifier.
    pub id: String,
    /// Target agent name.
    pub agent: String,
    /// Source type.
    pub source_kind: String,
    /// Source configuration detail (e.g. cron expression, interval).
    pub source_detail: String,
    /// Policy when the agent is busy.
    pub on_busy: String,
    /// Whether the trigger is active.
    pub enabled: bool,
    /// Number of successful fires.
    pub fire_count: u64,
    /// Number of skips.
    pub skip_count: u64,
    /// Last fire timestamp (RFC3339) or `null`.
    pub last_fired: Option<String>,
}

/// Response for `POST /api/v1/triggers/:id/fire`.
#[derive(Serialize, utoipa::ToSchema)]
pub struct FireResponse {
    /// Identifier of the submitted task.
    pub task_id: String,
}

/// Response for enable/disable.
#[derive(Serialize, utoipa::ToSchema)]
pub struct OkResponse {
    /// Confirmation flag.
    pub ok: bool,
}

/// Response for `GET /api/v1/triggers/:id/logs`.
#[derive(Serialize, utoipa::ToSchema)]
pub struct LogsResponse {
    /// History entries.
    #[schema(value_type = Vec<Object>)]
    pub entries: Vec<TriggerHistoryEntry>,
}

/// Query parameters for `GET /api/v1/triggers/:id/logs`.
#[derive(Debug, Deserialize)]
pub struct LogsQuery {
    /// Maximum number of entries to return (default: 20).
    #[serde(default = "default_last")]
    pub last: usize,
}

pub(super) fn default_last() -> usize {
    20
}
