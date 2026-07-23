//! Tauri IPC commands for managing LLM backends.
//!
//! Each command delegates to the internal REST API (`/api/v1/llm/*`) via the
//! parent module's HTTP helpers. CRUD operations target
//! `/api/v1/llm/backends`; ping and statistics use their own dedicated routes.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use apollia_core::LlmBackendRepository;
use apollia_llm::LlmRouter;
use apollia_runtime::embedded::RuntimeHandle;
use serde::{Deserialize, Serialize};
use tauri::State;

use super::{http_delete_json, http_get_json, http_post_json, http_put_json};
use crate::SharedLlmRouter;

/// Snapshot of the most recent ping result for a single backend.
///
/// Kept in memory only; re-evaluated on the next ping after a restart.
#[derive(Debug, Clone, Serialize)]
pub struct PingState {
    /// Error message returned by the last ping, `None` if the last ping succeeded.
    pub last_error: Option<String>,
    /// Round-trip latency of the last ping in milliseconds.
    pub last_latency_ms: Option<u64>,
    /// RFC 3339 timestamp of the last ping (UTC).
    pub last_ping_at: Option<String>,
    /// `true` if the last ping reported the backend as reachable.
    pub last_available: bool,
}

/// Process-wide cache of last ping outcomes, keyed by backend name.
///
/// Populated by `ping_llm_backend` and projected onto `LlmBackendView`
/// entries returned by `list_llm_backends` so the UI can surface the
/// most recent error without a fresh ping.
pub type LlmPingCache = Arc<RwLock<HashMap<String, PingState>>>;

/// View of an LLM backend for CRUD operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmBackendView {
    /// Unique logical backend name (e.g. `"local-code"`).
    pub name: String,
    /// Provider: `"llama-cpp"`, `"openai"`, `"mistral"`, `"anthropic"`, `"ollama"`.
    pub provider: String,
    /// Configured model identifier.
    pub model: String,
    /// Provider-specific extra JSON configuration.
    pub config_json: serde_json::Value,
    /// `true` if this backend is active.
    pub enabled: bool,
    /// `true` if this is the default backend.
    pub is_default: bool,
    /// Error message from the last ping (`None` if the last ping was OK or never pinged).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub last_ping_error: Option<String>,
    /// RFC 3339 timestamp of the last ping (`None` if never pinged).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub last_ping_at: Option<String>,
}

/// Request body for creating an LLM backend.
#[derive(Debug, Deserialize)]
pub struct CreateLlmBackendPayload {
    /// Unique backend name (pattern `^[a-z0-9_-]+$`).
    pub name: String,
    /// Backend provider.
    pub provider: String,
    /// Model identifier.
    pub model: String,
    /// Extra JSON configuration (must be a JSON object).
    pub config_json: serde_json::Value,
    /// Enable the backend on creation.
    pub enabled: bool,
    /// Set as the default backend on creation.
    pub is_default: bool,
}

/// Request body for updating an existing LLM backend.
#[derive(Debug, Deserialize)]
pub struct UpdateLlmBackendPayload {
    /// New provider.
    pub provider: String,
    /// New model identifier.
    pub model: String,
    /// New JSON configuration (must be a JSON object).
    pub config_json: serde_json::Value,
    /// Enable or disable the backend.
    pub enabled: bool,
    /// Mark as the default backend.
    pub is_default: bool,
}

/// Result of a ping on an LLM backend.
#[derive(Debug, Serialize)]
pub struct PingResult {
    /// Name of the pinged backend.
    pub backend: String,
    /// `true` if the backend responded.
    pub available: bool,
    /// Latency in milliseconds (if available).
    pub latency_ms: Option<u64>,
    /// Error message if the ping failed.
    pub error: Option<String>,
}

/// Cost/token statistics row for a backend+model.
#[derive(Debug, Serialize)]
pub struct CostStatsRow {
    /// Backend name.
    pub backend: String,
    /// Model identifier.
    pub model: String,
    /// Number of LLM calls.
    pub call_count: u64,
    /// Total tokens (prompt + completion).
    pub total_tokens: u64,
    /// Estimated total cost in USD.
    pub total_cost_usd: f64,
}

/// Aggregated cost/token statistics response.
#[derive(Debug, Serialize)]
pub struct CostStatsResponse {
    /// Rows per backend+model.
    pub rows: Vec<CostStatsRow>,
    /// Number of aggregated days.
    pub days: u32,
}

/// Deserializes a JSON value into an `LlmBackendView`.
fn parse_backend_view(json: serde_json::Value) -> Result<LlmBackendView, String> {
    serde_json::from_value(json).map_err(|e| format!("invalid backend response: {e}"))
}

/// Lists all configured LLM backends.
///
/// Delegates to `GET /api/v1/llm/backends`, then enriches each view with the
/// latest ping data from the in-memory `LlmPingCache`.
#[tauri::command]
pub async fn list_llm_backends(
    state: State<'_, RuntimeHandle>,
    cache: State<'_, LlmPingCache>,
) -> Result<Vec<LlmBackendView>, String> {
    let json = http_get_json(state.api_port, "/api/v1/llm/backends").await?;

    let backends = json
        .get("backends")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut views: Vec<LlmBackendView> = backends
        .into_iter()
        .map(parse_backend_view)
        .collect::<Result<Vec<_>, _>>()?;

    if let Ok(guard) = cache.read() {
        for view in views.iter_mut() {
            if let Some(ping) = guard.get(&view.name) {
                view.last_ping_error = ping.last_error.clone();
                view.last_ping_at = ping.last_ping_at.clone();
            }
        }
    }

    Ok(views)
}

/// Creates a new LLM backend.
///
/// Delegates to `POST /api/v1/llm/backends`.
#[tauri::command]
pub async fn create_llm_backend(
    state: State<'_, RuntimeHandle>,
    payload: CreateLlmBackendPayload,
) -> Result<LlmBackendView, String> {
    let body = serde_json::json!({
        "name": payload.name,
        "provider": payload.provider,
        "model": payload.model,
        "config_json": payload.config_json,
        "enabled": payload.enabled,
        "is_default": payload.is_default,
    });

    let json = http_post_json(state.api_port, "/api/v1/llm/backends", &body).await?;
    parse_backend_view(json)
}

/// Updates an existing LLM backend.
///
/// Delegates to `PUT /api/v1/llm/backends/:name`.
#[tauri::command]
pub async fn update_llm_backend(
    state: State<'_, RuntimeHandle>,
    name: String,
    payload: UpdateLlmBackendPayload,
) -> Result<LlmBackendView, String> {
    let path = format!("/api/v1/llm/backends/{name}");
    let body = serde_json::json!({
        "provider": payload.provider,
        "model": payload.model,
        "config_json": payload.config_json,
        "enabled": payload.enabled,
        "is_default": payload.is_default,
    });

    let json = http_put_json(state.api_port, &path, &body).await?;
    parse_backend_view(json)
}

/// Deletes an LLM backend.
///
/// Delegates to `DELETE /api/v1/llm/backends/:name`.
/// Returns a 409 error if the backend is the default backend.
#[tauri::command]
pub async fn delete_llm_backend(
    state: State<'_, RuntimeHandle>,
    name: String,
) -> Result<(), String> {
    let path = format!("/api/v1/llm/backends/{name}");
    http_delete_json(state.api_port, &path).await?;
    Ok(())
}

/// Sets an LLM backend as the default backend.
///
/// Delegates to `POST /api/v1/llm/backends/:name/set-default`.
#[tauri::command]
pub async fn set_default_llm_backend(
    state: State<'_, RuntimeHandle>,
    name: String,
) -> Result<(), String> {
    let path = format!("/api/v1/llm/backends/{name}/set-default");
    let body = serde_json::json!({});
    http_post_json(state.api_port, &path, &body).await?;
    Ok(())
}

/// Pings an LLM backend and returns the latency.
///
/// Delegates to `POST /api/v1/llm/ping`. The result is also written to the
/// shared `LlmPingCache` so `list_llm_backends` can project
/// `last_ping_error` / `last_ping_at` onto the returned views.
#[tauri::command]
pub async fn ping_llm_backend(
    state: State<'_, RuntimeHandle>,
    cache: State<'_, LlmPingCache>,
    name: String,
) -> Result<PingResult, String> {
    let body = serde_json::json!({ "backend": name });
    let json = http_post_json(state.api_port, "/api/v1/llm/ping", &body).await;

    let result = match json {
        Ok(resp) => PingResult {
            backend: resp
                .get("backend")
                .and_then(|v| v.as_str())
                .unwrap_or(&name)
                .to_string(),
            available: resp
                .get("available")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            latency_ms: resp.get("latency_ms").and_then(|v| v.as_u64()),
            error: resp.get("error").and_then(|v| v.as_str()).map(String::from),
        },
        Err(e) => PingResult {
            backend: name.clone(),
            available: false,
            latency_ms: None,
            error: Some(e),
        },
    };

    if let Ok(mut guard) = cache.write() {
        guard.insert(
            name.clone(),
            PingState {
                last_error: result.error.clone(),
                last_latency_ms: result.latency_ms,
                last_ping_at: Some(chrono::Utc::now().to_rfc3339()),
                last_available: result.available,
            },
        );
    }

    Ok(result)
}

/// Returns the configured LLM cost-alert threshold in USD.
///
/// Returns `None` if `cost_alert_threshold_usd` is not configured in `apollia.toml`.
#[tauri::command]
pub async fn get_cost_alert_threshold(
    state: State<'_, RuntimeHandle>,
) -> Result<Option<f64>, String> {
    Ok(state
        .llm_router
        .as_ref()
        .and_then(|router| router.cost_alert_threshold_usd()))
}

/// Fetches the cost/token statistics aggregated over N days.
///
/// Delegates to `GET /api/v1/llm/costs?days=N`.
#[tauri::command]
pub async fn get_llm_cost_stats(
    state: State<'_, RuntimeHandle>,
    days: Option<u32>,
) -> Result<CostStatsResponse, String> {
    let d = days.unwrap_or(7);
    let path = format!("/api/v1/llm/costs?days={d}");
    let json = http_get_json(state.api_port, &path).await;

    match json {
        Ok(resp) => {
            let rows = resp
                .get("rows")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .map(|r| CostStatsRow {
                    backend: r
                        .get("backend")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    model: r
                        .get("model")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    call_count: r.get("call_count").and_then(|v| v.as_u64()).unwrap_or(0),
                    total_tokens: r.get("total_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
                    total_cost_usd: r
                        .get("total_cost_usd")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0),
                })
                .collect();

            Ok(CostStatsResponse { rows, days: d })
        }
        Err(_) => Ok(CostStatsResponse {
            rows: vec![],
            days: d,
        }),
    }
}

/// Reloads the LLM router from `system.db`.
///
/// Alias of `reload_llm_from_db`, kept for compatibility with existing
/// frontend callers. All LLM configuration now lives in `system.db`;
/// `apollia.toml` is no longer the source of truth.
#[tauri::command]
pub async fn reload_llm(
    shared: State<'_, SharedLlmRouter>,
    runtime: State<'_, RuntimeHandle>,
) -> Result<(), String> {
    reload_llm_from_db(shared, runtime).await
}

/// Rebuilds the `SharedLlmRouter` from `system.db` (SQLite).
///
/// Called by the Settings frontend after `create_llm_backend` / `update_llm_backend`
/// so agents immediately use the new configuration. Unlike `reload_llm` (which
/// reads TOML), this reads the CRUD repository in `system.db`.
///
/// The old `Arc<LlmRouter>` is explicitly dropped before writing the new one,
/// which frees the GGUF model from RAM as soon as no requests are in flight.
#[tauri::command]
pub async fn reload_llm_from_db(
    shared: State<'_, SharedLlmRouter>,
    runtime: State<'_, RuntimeHandle>,
) -> Result<(), String> {
    let home = std::env::var("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir());
    let db_path = home.join(".apollia").join("system.db");

    // LlmBackendRepository is !Send (uses RefCell<Connection>), so all DB work
    // must stay inside spawn_blocking. We extract the raw configs and return them.
    let (all_configs, default_name) = tokio::task::spawn_blocking(move || {
        let repo = LlmBackendRepository::open(&db_path)
            .map_err(|e| format!("failed to open system.db: {e}"))?;
        let all = repo
            .list()
            .map_err(|e| format!("failed to list LLM backends: {e}"))?;
        let default_cfg = repo
            .find_default()
            .map_err(|e| format!("failed to find default backend: {e}"))?
            .ok_or_else(|| "no default LLM backend configured in system.db".to_string())?;
        Ok::<_, String>((all, default_cfg.name))
    })
    .await
    .map_err(|e| format!("spawn_blocking failed: {e}"))??;

    // Re-inject the runner override for local LlamaCpp backends, otherwise the
    // reload would rebuild a router without the local backend (the runner would
    // become unreachable). We reuse the same factory as the supervisor.
    let runner_proxy = runtime.runner_supervisor.as_ref().map(|s| s.proxy());
    let factory = apollia_runtime::runner_supervisor::runner_llm_override(runner_proxy);

    let new_router = Arc::new(
        LlmRouter::from_backend_configs_with_override(all_configs, default_name, factory)
            .await
            .map_err(|e| format!("failed to build LLM router: {e}"))?,
    );

    // Drop the old router before writing the new one so the GGUF is freed from RAM
    // as soon as no other Arc references remain (e.g. in-flight agent requests).
    {
        let mut guard = shared.write().map_err(|e| format!("lock poisoned: {e}"))?;
        let old = guard.take();
        drop(old);
        *guard = Some(new_router);
    }

    // The runtime keeps its OWN LlmRouter cell, consumed by the REST API
    // (`/api/v1/llm/ping`, `/chat`, `/complete`). It is rebuilt only by
    // `POST /api/v1/llm/reload`; the desktop cell swap above does not touch it.
    // Without this, Settings "test/ping" reports "no LLM router configured" even
    // after a backend is created. Best-effort: a failure here must not fail the
    // desktop reload, which already succeeded.
    if let Err(e) = http_post_json(
        runtime.api_port,
        "/api/v1/llm/reload",
        &serde_json::json!({}),
    )
    .await
    {
        tracing::warn!(error = %e, "failed to rebuild runtime LLM router after reload");
    }

    tracing::info!("LLM router reloaded from system.db");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_llm_backend_view_round_trips() {
        // GIVEN a JSON object matching LlmBackendView
        let json = serde_json::json!({
            "name": "local-code",
            "provider": "llama-cpp",
            "model": "qwen3-0.6b-q8_0",
            "config_json": { "device": "metal" },
            "enabled": true,
            "is_default": true,
        });

        // WHEN deserialized and re-serialized
        let view: LlmBackendView = serde_json::from_value(json.clone()).expect("deserialize");
        let back = serde_json::to_value(&view).expect("serialize");

        // THEN all fields match
        assert_eq!(back["name"], "local-code");
        assert_eq!(back["provider"], "llama-cpp");
        assert_eq!(back["model"], "qwen3-0.6b-q8_0");
        assert_eq!(back["enabled"], true);
        assert_eq!(back["is_default"], true);
    }

    #[test]
    fn test_parse_backend_view_returns_error_on_invalid_json() {
        // GIVEN invalid JSON (missing required fields)
        let json = serde_json::json!({ "name": "only-name" });

        // WHEN parsing
        let result = parse_backend_view(json);

        // THEN an error is returned
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("invalid backend response"));
    }

    #[test]
    fn test_ping_result_serializes_success() {
        // GIVEN a successful ping result
        let result = PingResult {
            backend: "local".to_string(),
            available: true,
            latency_ms: Some(42),
            error: None,
        };

        // WHEN serialized
        let json = serde_json::to_value(&result).expect("serialize");

        // THEN latency_ms is present and error is null
        assert_eq!(json["available"], true);
        assert_eq!(json["latency_ms"], 42);
        assert!(json["error"].is_null());
    }

    #[test]
    fn test_ping_result_serializes_failure() {
        // GIVEN a failed ping result
        let result = PingResult {
            backend: "anthropic".to_string(),
            available: false,
            latency_ms: None,
            error: Some("connection refused".to_string()),
        };

        // WHEN serialized
        let json = serde_json::to_value(&result).expect("serialize");

        // THEN available is false and error is set
        assert_eq!(json["available"], false);
        assert!(json["latency_ms"].is_null());
        assert_eq!(json["error"], "connection refused");
    }

    #[test]
    fn test_cost_stats_response_serializes() {
        // GIVEN a CostStatsResponse with one row
        let resp = CostStatsResponse {
            rows: vec![CostStatsRow {
                backend: "anthropic".to_string(),
                model: "sonnet".to_string(),
                call_count: 15,
                total_tokens: 3000,
                total_cost_usd: 0.045,
            }],
            days: 7,
        };

        // WHEN serialized
        let json = serde_json::to_value(&resp).expect("serialize");

        // THEN rows and days are correct
        assert_eq!(json["days"], 7);
        let rows = json["rows"].as_array().expect("rows is array");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["call_count"], 15);
        assert_eq!(rows[0]["total_tokens"], 3000);
    }

    #[test]
    fn test_cost_stats_response_empty() {
        // GIVEN an empty CostStatsResponse
        let resp = CostStatsResponse {
            rows: vec![],
            days: 30,
        };

        // WHEN serialized
        let json = serde_json::to_value(&resp).expect("serialize");

        // THEN rows is empty array
        assert_eq!(json["days"], 30);
        assert_eq!(json["rows"].as_array().expect("rows").len(), 0);
    }
}
