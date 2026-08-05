//! Tauri IPC commands for managing triggers.
//!
//! Each command delegates to the internal REST API (`/api/v1/triggers/*`) via
//! the `http_get_json` / `http_post_json` helpers. Data travels as raw JSON
//! (`serde_json::Value`) to avoid duplicating the Rust types already defined in
//! `apollia-triggers`.

use apollia_runtime::embedded::RuntimeHandle;
use serde::{Deserialize, Serialize};
use tauri::State;

use super::{http_delete_json, http_get_json, http_post_json, http_put_json};

/// Trigger status for display in the UI.
#[derive(Debug, Serialize)]
pub struct TriggerStatus {
    /// Trigger identifier.
    pub id: String,
    /// Name of the target agent.
    pub agent: String,
    /// Source type: `"cron"` | `"interval"` | `"file_watch"` | `"webhook"` | `"oneshot"`.
    pub source_kind: String,
    /// Source configuration detail (e.g. cron expression, interval, path).
    pub source_config: String,
    /// Whether the trigger is active.
    pub enabled: bool,
    /// Number of successful fires.
    pub fire_count: u64,
    /// Number of skips.
    pub skip_count: u64,
    /// Timestamp of the last fire (RFC3339) or `null`.
    pub last_fired: Option<String>,
}

/// History entry of a trigger.
#[derive(Debug, Serialize)]
pub struct TriggerLogEntry {
    /// Unique entry identifier.
    pub id: String,
    /// Trigger identifier.
    pub trigger_id: String,
    /// Name of the target agent.
    pub agent_name: String,
    /// Fire timestamp (RFC3339).
    pub fired_at: String,
    /// Identifier of the submitted task (if `status` is `"fired"`).
    pub task_id: Option<String>,
    /// Status: `"fired"` | `"skipped"` | `"error"`.
    pub status: String,
    /// Reason for the skip or error.
    pub reason: Option<String>,
}

/// Result of a manual fire.
#[derive(Debug, Serialize)]
pub struct FireResult {
    /// Identifier of the created task.
    pub task_id: String,
}

/// Result of reloading the configuration.
#[derive(Debug, Serialize)]
pub struct ReloadResult {
    /// Number of active triggers after the reload.
    pub reloaded: u64,
}

/// Lists all configured triggers with their status.
///
/// Delegates to `GET /api/v1/triggers` on the internal REST API.
#[tauri::command]
pub async fn list_triggers(state: State<'_, RuntimeHandle>) -> Result<Vec<TriggerStatus>, String> {
    let json = http_get_json(state.api_port, "/api/v1/triggers").await?;

    let triggers = json
        .get("triggers")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let result = triggers
        .into_iter()
        .map(|t| TriggerStatus {
            id: t
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            agent: t
                .get("agent")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            source_kind: t
                .get("source_kind")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            source_config: t
                .get("source_config")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            enabled: t.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false),
            fire_count: t.get("fire_count").and_then(|v| v.as_u64()).unwrap_or(0),
            skip_count: t.get("skip_count").and_then(|v| v.as_u64()).unwrap_or(0),
            last_fired: t
                .get("last_fired")
                .and_then(|v| v.as_str())
                .map(String::from),
        })
        .collect();

    Ok(result)
}

/// Enables or disables a trigger.
///
/// Delegates to `POST /api/v1/triggers/:id/enable` or `POST /api/v1/triggers/:id/disable`.
#[tauri::command]
pub async fn set_trigger_enabled(
    state: State<'_, RuntimeHandle>,
    id: String,
    enabled: bool,
) -> Result<(), String> {
    let action = if enabled { "enable" } else { "disable" };
    let path = format!("/api/v1/triggers/{id}/{action}");
    let body = serde_json::json!({});
    http_post_json(state.api_port, &path, &body).await?;
    Ok(())
}

/// Fires a trigger manually.
///
/// Delegates to `POST /api/v1/triggers/:id/fire`.
#[tauri::command]
pub async fn fire_trigger(
    state: State<'_, RuntimeHandle>,
    id: String,
) -> Result<FireResult, String> {
    let body = serde_json::json!({});
    let json = http_post_json(
        state.api_port,
        &format!("/api/v1/triggers/{id}/fire"),
        &body,
    )
    .await?;

    let task_id = json
        .get("task_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    Ok(FireResult { task_id })
}

/// Fetches a trigger's logs.
///
/// Delegates to `GET /api/v1/triggers/:id/logs?last=N`.
#[tauri::command]
pub async fn get_trigger_logs(
    state: State<'_, RuntimeHandle>,
    id: String,
    limit: Option<u32>,
) -> Result<Vec<TriggerLogEntry>, String> {
    let l = limit.unwrap_or(20);
    let path = format!("/api/v1/triggers/{id}/logs?last={l}");
    let json = http_get_json(state.api_port, &path).await?;

    let entries = json
        .get("entries")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let result = entries
        .into_iter()
        .map(|e| TriggerLogEntry {
            id: e
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            trigger_id: e
                .get("trigger_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            agent_name: e
                .get("agent_name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            fired_at: e
                .get("fired_at")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            task_id: e.get("task_id").and_then(|v| v.as_str()).map(String::from),
            status: e
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            reason: e.get("reason").and_then(|v| v.as_str()).map(String::from),
        })
        .collect();

    Ok(result)
}

/// Reloads the trigger configuration from apollia.toml.
///
/// Delegates to `POST /api/v1/triggers/reload`.
#[tauri::command]
pub async fn reload_triggers(state: State<'_, RuntimeHandle>) -> Result<ReloadResult, String> {
    let body = serde_json::json!({});
    let json = http_post_json(state.api_port, "/api/v1/triggers/reload", &body).await?;

    let reloaded = json.get("reloaded").and_then(|v| v.as_u64()).unwrap_or(0);

    Ok(ReloadResult { reloaded })
}

// ─── CRUD types & commands ──────────────────────────────────────────────────

/// Full definition of a trigger returned by the CRUD operations.
///
/// `source_config` is redacted before it leaves this process: see
/// [`redact_source_config`].
#[derive(Debug, Serialize)]
pub struct TriggerDefinitionView {
    /// Unique trigger identifier.
    pub id: String,
    /// Target agent (mutually exclusive with `pipeline`).
    pub agent: Option<String>,
    /// Target pipeline (mutually exclusive with `agent`).
    pub pipeline: Option<String>,
    /// Whether the trigger is active.
    pub enabled: bool,
    /// Policy when the agent is busy: `"queue"` or `"drop"`.
    pub on_busy: String,
    /// Source type: `"cron"`, `"interval"`, etc.
    pub source_type: String,
    /// JSON configuration of the source, secret material removed.
    pub source_config: serde_json::Value,
    /// Presence marker replacing the removed secret.
    pub has_secret: bool,
    /// Input message template.
    pub input_template: Option<String>,
    /// Creation timestamp (ISO 8601).
    pub created_at: String,
    /// Last-modification timestamp (ISO 8601).
    pub updated_at: String,
}

/// Trigger source configuration in CRUD requests.
#[derive(Debug, Serialize, Deserialize)]
pub struct TriggerSourceInput {
    /// Source type: `"cron"`, `"interval"`, `"oneshot"`, `"file_watch"`, `"webhook"`.
    pub r#type: String,
    /// Source-specific configuration.
    #[serde(flatten)]
    pub config: serde_json::Value,
}

/// Request body for creating a trigger via `create_trigger`.
#[derive(Debug, Serialize, Deserialize)]
pub struct CreateTriggerRequest {
    /// Unique trigger identifier.
    pub id: String,
    /// Target agent, mutually exclusive with `pipeline`.
    pub agent: Option<String>,
    /// Target pipeline, mutually exclusive with `agent`.
    pub pipeline: Option<String>,
    /// Whether the trigger is active (default: `true`).
    pub enabled: Option<bool>,
    /// Policy when the agent is busy (default: `"queue"`).
    pub on_busy: Option<String>,
    /// Trigger source configuration.
    pub source: TriggerSourceInput,
    /// Input message template.
    pub input_template: Option<String>,
}

/// Request body for updating a trigger via `update_trigger`.
#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateTriggerRequest {
    /// Target agent, mutually exclusive with `pipeline`.
    pub agent: Option<String>,
    /// Target pipeline, mutually exclusive with `agent`.
    pub pipeline: Option<String>,
    /// Whether the trigger is active.
    pub enabled: Option<bool>,
    /// Policy when the agent is busy.
    pub on_busy: Option<String>,
    /// Trigger source configuration.
    pub source: TriggerSourceInput,
    /// Input message template.
    pub input_template: Option<String>,
}

/// Keys of `source_config` that hold secret material.
///
/// Currently only the webhook HMAC shared secret.
const SECRET_SOURCE_KEYS: [&str; 1] = ["secret"];

/// Strips secret material from a `source_config` and reports whether one was there.
///
/// The webview never needs the value: changing a schedule does not require the
/// HMAC secret, and anything handed over IPC lands in the renderer context.
/// A boolean presence marker is enough for the form to say "already stored,
/// leave empty to keep it".
fn redact_source_config(config: &serde_json::Value) -> (serde_json::Value, bool) {
    let Some(map) = config.as_object() else {
        return (config.clone(), false);
    };

    let mut redacted = map.clone();
    let mut has_secret = false;
    for key in SECRET_SOURCE_KEYS {
        if let Some(value) = redacted.remove(key) {
            has_secret |= value.as_str().is_some_and(|s| !s.is_empty());
        }
    }

    (serde_json::Value::Object(redacted), has_secret)
}

/// True when an update payload targets a webhook and carries no replacement secret.
///
/// `PUT /api/v1/triggers/:id` replaces the stored row wholesale and rejects a
/// webhook whose secret is missing or shorter than the required length, so an
/// unchanged secret still has to be resent. The webview cannot do that: it no
/// longer holds the value.
fn needs_stored_secret(body: &serde_json::Value) -> bool {
    let Some(source) = body.get("source") else {
        return false;
    };
    if source.get("type").and_then(|v| v.as_str()) != Some("webhook") {
        return false;
    }
    source
        .get("secret")
        .and_then(|v| v.as_str())
        .is_none_or(|s| s.trim().is_empty())
}

/// Reads the secret currently stored for `id`, `None` when there is none.
async fn fetch_stored_secret(api_port: u16, id: &str) -> Result<Option<String>, String> {
    let json = http_get_json(api_port, &format!("/api/v1/triggers/{id}")).await?;
    Ok(json
        .pointer("/source_config/secret")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(String::from))
}

/// Writes `secret` into the `source` object of an update payload.
fn inject_secret(body: &mut serde_json::Value, secret: String) {
    if let Some(source) = body.get_mut("source").and_then(|s| s.as_object_mut()) {
        source.insert("secret".to_string(), serde_json::Value::String(secret));
    }
}

/// Parses an API response JSON into a `TriggerDefinitionView`.
fn parse_trigger_definition(json: &serde_json::Value) -> TriggerDefinitionView {
    let (source_config, has_secret) = redact_source_config(
        &json
            .get("source_config")
            .cloned()
            .unwrap_or(serde_json::Value::Object(serde_json::Map::new())),
    );

    TriggerDefinitionView {
        id: json
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        agent: json.get("agent").and_then(|v| v.as_str()).map(String::from),
        pipeline: json
            .get("pipeline")
            .and_then(|v| v.as_str())
            .map(String::from),
        enabled: json
            .get("enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        on_busy: json
            .get("on_busy")
            .and_then(|v| v.as_str())
            .unwrap_or("queue")
            .to_string(),
        source_type: json
            .get("source_type")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        source_config,
        has_secret,
        input_template: json
            .get("input_template")
            .and_then(|v| v.as_str())
            .map(String::from),
        created_at: json
            .get("created_at")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        updated_at: json
            .get("updated_at")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
    }
}

/// Creates a new trigger.
///
/// Delegates to `POST /api/v1/triggers`.
#[tauri::command]
pub async fn create_trigger(
    state: State<'_, RuntimeHandle>,
    definition: CreateTriggerRequest,
) -> Result<TriggerDefinitionView, String> {
    let body = serde_json::to_value(&definition)
        .map_err(|e| format!("failed to serialize request: {e}"))?;
    let json = http_post_json(state.api_port, "/api/v1/triggers", &body).await?;
    Ok(parse_trigger_definition(&json))
}

/// Updates an existing trigger.
///
/// Delegates to `PUT /api/v1/triggers/:id`.
///
/// An empty webhook secret means "keep the stored one". Since the update route
/// rewrites the whole row, the stored value is read back here and spliced into
/// the payload, which keeps it inside the host process.
#[tauri::command]
pub async fn update_trigger(
    state: State<'_, RuntimeHandle>,
    id: String,
    definition: UpdateTriggerRequest,
) -> Result<TriggerDefinitionView, String> {
    let mut body = serde_json::to_value(&definition)
        .map_err(|e| format!("failed to serialize request: {e}"))?;

    if needs_stored_secret(&body) {
        if let Some(secret) = fetch_stored_secret(state.api_port, &id).await? {
            inject_secret(&mut body, secret);
            tracing::debug!(trigger_id = %id, "triggers.update.secret_preserved");
        }
    }

    let path = format!("/api/v1/triggers/{id}");
    let json = http_put_json(state.api_port, &path, &body).await?;
    Ok(parse_trigger_definition(&json))
}

/// Fetches the full definition of a trigger by its identifier.
///
/// Delegates to `GET /api/v1/triggers/:id` (CRUD route). The webhook secret is
/// stripped from the response, `has_secret` reports its presence instead.
#[tauri::command]
pub async fn get_trigger_definition(
    state: State<'_, RuntimeHandle>,
    id: String,
) -> Result<TriggerDefinitionView, String> {
    let path = format!("/api/v1/triggers/{id}");
    let json = http_get_json(state.api_port, &path).await?;
    Ok(parse_trigger_definition(&json))
}

/// Deletes a trigger.
///
/// Delegates to `DELETE /api/v1/triggers/:id`.
#[tauri::command]
pub async fn delete_trigger(state: State<'_, RuntimeHandle>, id: String) -> Result<(), String> {
    let path = format!("/api/v1/triggers/{id}");
    http_delete_json(state.api_port, &path).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trigger_status_serializes() {
        // GIVEN a TriggerStatus
        let status = TriggerStatus {
            id: "daily-report".to_string(),
            agent: "report-agent".to_string(),
            source_kind: "cron".to_string(),
            source_config: "0 8 * * MON".to_string(),
            enabled: true,
            fire_count: 42,
            skip_count: 3,
            last_fired: Some("2026-03-13T08:00:00Z".to_string()),
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&status).expect("serialize");

        // THEN all fields are present and correct
        assert_eq!(json["id"], "daily-report");
        assert_eq!(json["agent"], "report-agent");
        assert_eq!(json["source_kind"], "cron");
        assert_eq!(json["source_config"], "0 8 * * MON");
        assert_eq!(json["enabled"], true);
        assert_eq!(json["fire_count"], 42);
        assert_eq!(json["skip_count"], 3);
        assert_eq!(json["last_fired"], "2026-03-13T08:00:00Z");
    }

    #[test]
    fn test_trigger_status_serializes_no_last_fired() {
        // GIVEN a TriggerStatus that never fired
        let status = TriggerStatus {
            id: "watcher".to_string(),
            agent: "file-agent".to_string(),
            source_kind: "file_watch".to_string(),
            source_config: "/home/user/docs".to_string(),
            enabled: false,
            fire_count: 0,
            skip_count: 0,
            last_fired: None,
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&status).expect("serialize");

        // THEN last_fired is null
        assert!(json["last_fired"].is_null());
        assert_eq!(json["enabled"], false);
    }

    #[test]
    fn test_trigger_log_entry_serializes() {
        // GIVEN a TriggerLogEntry for a fired event
        let entry = TriggerLogEntry {
            id: "abc123".to_string(),
            trigger_id: "daily-report".to_string(),
            agent_name: "report-agent".to_string(),
            fired_at: "2026-03-13T08:00:00Z".to_string(),
            task_id: Some("task-456".to_string()),
            status: "fired".to_string(),
            reason: None,
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&entry).expect("serialize");

        // THEN all fields are correct
        assert_eq!(json["status"], "fired");
        assert_eq!(json["task_id"], "task-456");
        assert!(json["reason"].is_null());
    }

    #[test]
    fn test_trigger_log_entry_serializes_error() {
        // GIVEN a TriggerLogEntry for an error event
        let entry = TriggerLogEntry {
            id: "def789".to_string(),
            trigger_id: "webhook-trigger".to_string(),
            agent_name: "api-agent".to_string(),
            fired_at: "2026-03-13T09:00:00Z".to_string(),
            task_id: None,
            status: "error".to_string(),
            reason: Some("agent not running".to_string()),
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&entry).expect("serialize");

        // THEN task_id is null and reason is set
        assert!(json["task_id"].is_null());
        assert_eq!(json["reason"], "agent not running");
    }

    #[test]
    fn test_fire_result_serializes() {
        // GIVEN a FireResult
        let result = FireResult {
            task_id: "task-abc".to_string(),
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&result).expect("serialize");

        // THEN task_id is present
        assert_eq!(json["task_id"], "task-abc");
    }

    #[test]
    fn test_reload_result_serializes() {
        // GIVEN a ReloadResult
        let result = ReloadResult { reloaded: 5 };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&result).expect("serialize");

        // THEN reloaded count is correct
        assert_eq!(json["reloaded"], 5);
    }

    #[test]
    fn test_trigger_definition_view_serializes() {
        // GIVEN a TriggerDefinitionView with an agent target
        let view = TriggerDefinitionView {
            id: "daily-cron".to_string(),
            agent: Some("report-agent".to_string()),
            pipeline: None,
            enabled: true,
            on_busy: "queue".to_string(),
            source_type: "cron".to_string(),
            source_config: serde_json::json!({"expression": "0 8 * * MON"}),
            has_secret: false,
            input_template: Some("Generate weekly report".to_string()),
            created_at: "2026-03-20T10:00:00Z".to_string(),
            updated_at: "2026-03-20T10:00:00Z".to_string(),
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&view).expect("serialize");

        // THEN all fields are present and correct
        assert_eq!(json["id"], "daily-cron");
        assert_eq!(json["agent"], "report-agent");
        assert!(json["pipeline"].is_null());
        assert_eq!(json["enabled"], true);
        assert_eq!(json["on_busy"], "queue");
        assert_eq!(json["source_type"], "cron");
        assert_eq!(json["source_config"]["expression"], "0 8 * * MON");
        assert_eq!(json["input_template"], "Generate weekly report");
        assert_eq!(json["created_at"], "2026-03-20T10:00:00Z");
    }

    #[test]
    fn test_trigger_definition_view_serializes_pipeline_target() {
        // GIVEN a TriggerDefinitionView with a pipeline target
        let view = TriggerDefinitionView {
            id: "file-watcher".to_string(),
            agent: None,
            pipeline: Some("ingestion-pipeline".to_string()),
            enabled: false,
            on_busy: "drop".to_string(),
            source_type: "file_watch".to_string(),
            source_config: serde_json::json!({"path": "/data/inbox"}),
            has_secret: false,
            input_template: None,
            created_at: "2026-03-20T08:00:00Z".to_string(),
            updated_at: "2026-03-20T09:00:00Z".to_string(),
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&view).expect("serialize");

        // THEN pipeline is set, agent is null
        assert!(json["agent"].is_null());
        assert_eq!(json["pipeline"], "ingestion-pipeline");
        assert_eq!(json["enabled"], false);
        assert!(json["input_template"].is_null());
    }

    #[test]
    fn test_create_trigger_request_serializes() {
        // GIVEN a CreateTriggerRequest
        let req = CreateTriggerRequest {
            id: "new-trigger".to_string(),
            agent: Some("my-agent".to_string()),
            pipeline: None,
            enabled: Some(true),
            on_busy: Some("queue".to_string()),
            source: TriggerSourceInput {
                r#type: "interval".to_string(),
                config: serde_json::json!({"seconds": 60}),
            },
            input_template: None,
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&req).expect("serialize");

        // THEN required fields are present
        assert_eq!(json["id"], "new-trigger");
        assert_eq!(json["agent"], "my-agent");
        assert_eq!(json["source"]["type"], "interval");
    }

    #[test]
    fn test_parse_trigger_definition_from_api_response() {
        // GIVEN a JSON response matching API TriggerDefinitionResponse
        let api_json = serde_json::json!({
            "id": "t-abc",
            "agent": "test-agent",
            "pipeline": null,
            "enabled": true,
            "on_busy": "drop",
            "source_type": "webhook",
            "source_config": {"secret": "abc"},
            "input_template": "payload: {{trigger.payload}}",
            "created_at": "2026-03-20T10:00:00Z",
            "updated_at": "2026-03-20T11:00:00Z"
        });

        // WHEN parsed
        let view = parse_trigger_definition(&api_json);

        // THEN all fields are correctly mapped
        assert_eq!(view.id, "t-abc");
        assert_eq!(view.agent.as_deref(), Some("test-agent"));
        assert!(view.pipeline.is_none());
        assert!(view.enabled);
        assert_eq!(view.on_busy, "drop");
        assert_eq!(view.source_type, "webhook");
        assert_eq!(
            view.input_template.as_deref(),
            Some("payload: {{trigger.payload}}")
        );
    }

    #[test]
    fn test_parsed_definition_hides_the_webhook_secret() {
        // GIVEN an API response carrying the HMAC secret of a webhook trigger
        let secret = "d41d8cd98f00b204e9800998ecf8427e";
        let api_json = serde_json::json!({
            "id": "inbound-hook",
            "agent": "intake-agent",
            "enabled": true,
            "on_busy": "queue",
            "source_type": "webhook",
            "source_config": {"secret": secret, "path": "/hooks/intake"},
            "input_template": null,
            "created_at": "2026-03-20T10:00:00Z",
            "updated_at": "2026-03-20T11:00:00Z"
        });

        // WHEN parsed into the view sent to the webview
        let view = parse_trigger_definition(&api_json);
        let serialized = serde_json::to_string(&view).expect("serialize");

        // THEN the secret is gone, only its presence marker survives
        assert!(view.source_config.get("secret").is_none());
        assert!(view.has_secret);
        assert!(
            !serialized.contains(secret),
            "the cleartext secret must not cross the IPC boundary: {serialized}"
        );
        // AND the non-secret keys are untouched
        assert_eq!(view.source_config["path"], "/hooks/intake");
    }

    #[test]
    fn test_parsed_definition_reports_no_secret_when_absent() {
        // GIVEN a cron definition, which holds no secret at all
        let api_json = serde_json::json!({
            "id": "daily",
            "source_type": "cron",
            "source_config": {"schedule": "0 8 * * MON"},
        });

        // WHEN parsed
        let view = parse_trigger_definition(&api_json);

        // THEN the presence marker is false and the config is intact
        assert!(!view.has_secret);
        assert_eq!(view.source_config["schedule"], "0 8 * * MON");
    }

    #[test]
    fn test_redact_source_config_treats_an_empty_secret_as_absent() {
        // GIVEN a webhook config whose stored secret is an empty string
        let config = serde_json::json!({"secret": ""});

        // WHEN redacted
        let (redacted, has_secret) = redact_source_config(&config);

        // THEN nothing is reported as stored, so the form still demands one
        assert!(!has_secret);
        assert!(redacted.get("secret").is_none());
    }

    #[test]
    fn test_needs_stored_secret_only_for_a_blank_webhook_secret() {
        // GIVEN update payloads covering the three interesting shapes
        let blank = serde_json::json!({"source": {"type": "webhook", "secret": ""}});
        let missing = serde_json::json!({"source": {"type": "webhook"}});
        let rotated = serde_json::json!({"source": {"type": "webhook", "secret": "n3w"}});
        let cron = serde_json::json!({"source": {"type": "cron", "schedule": "0 8 * * MON"}});

        // WHEN each is inspected
        // THEN only the blank and missing webhook secrets ask for the stored one
        assert!(needs_stored_secret(&blank));
        assert!(needs_stored_secret(&missing));
        assert!(!needs_stored_secret(&rotated));
        assert!(!needs_stored_secret(&cron));
    }

    #[test]
    fn test_inject_secret_fills_the_source_object() {
        // GIVEN an update payload with a webhook source and no secret
        let mut body = serde_json::json!({"source": {"type": "webhook"}});

        // WHEN the stored secret is spliced in
        inject_secret(&mut body, "s3cr3t".to_string());

        // THEN the payload the API receives carries it
        assert_eq!(body["source"]["secret"], "s3cr3t");
    }
}
