//! Tauri IPC commands for the engines the runtime routes requests to.
//!
//! Each command delegates to the runtime's REST API (`/api/v1/llm/*`) through
//! [`RuntimeLink`], so it serves the runtime in this process and the daemon
//! alike: the list of the configured engines, and the reload of the router.
//! The inference servers have their own commands in `llm_servers.rs`.

use std::sync::Arc;

use crate::attached::RuntimeLink;
use apollia_core::LlmBackendRepository;
use apollia_llm::LlmRouter;
use apollia_runtime::embedded::RuntimeHandle;
use serde::{Deserialize, Serialize};
use tauri::State;

use super::http_post_json;
use crate::SharedLlmRouter;

/// One configured engine, as the list of the interface reads it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmBackendView {
    /// Unique logical backend name (e.g. `"local-code"`).
    pub name: String,
    /// Provider: `"llama-cpp"`, `"openai"` (OpenAI-compatible server), `"ollama"`.
    pub provider: String,
    /// Configured model identifier.
    pub model: String,
    /// Provider-specific extra JSON configuration.
    pub config_json: serde_json::Value,
    /// `true` if this backend is active.
    pub enabled: bool,
    /// `true` if this is the default backend.
    pub is_default: bool,
}

/// Deserializes a JSON value into an `LlmBackendView`.
fn parse_backend_view(json: serde_json::Value) -> Result<LlmBackendView, String> {
    serde_json::from_value(json).map_err(|e| format!("invalid backend response: {e}"))
}

/// Lists all configured LLM backends.
///
/// Delegates to `GET /api/v1/llm/backends`.
#[tauri::command]
pub async fn list_llm_backends(
    link: State<'_, RuntimeLink>,
) -> Result<Vec<LlmBackendView>, String> {
    listed_backends(&link).await
}

/// The backends of the runtime `link` drives.
pub(crate) async fn listed_backends(link: &RuntimeLink) -> Result<Vec<LlmBackendView>, String> {
    let json = link.get_json("/api/v1/llm/backends").await?;
    json.get("backends")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(parse_backend_view)
        .collect()
}

/// Reloads the LLM router from `system.db`.
///
/// Alias of `reload_llm_from_db`, kept for compatibility with existing
/// frontend callers. All LLM configuration now lives in `system.db`;
/// `apollia.toml` is no longer the source of truth.
#[tauri::command]
pub async fn reload_llm(app: tauri::AppHandle, link: State<'_, RuntimeLink>) -> Result<(), String> {
    reload_llm_from_db(app, link).await
}

/// Rebuilds the `SharedLlmRouter` from `system.db` (SQLite).
///
/// Called by the interface after a change of the default model
/// so agents immediately use the new configuration. Unlike `reload_llm` (which
/// reads TOML), this reads the CRUD repository in `system.db`.
///
/// The old `Arc<LlmRouter>` is explicitly dropped before writing the new one,
/// which frees the GGUF model from RAM as soon as no requests are in flight.
///
/// Attached to a daemon, this window holds no router: the daemon rebuilds its
/// own through `POST /api/v1/llm/reload`, and its refusal is the error shown.
#[tauri::command]
pub async fn reload_llm_from_db(
    app: tauri::AppHandle,
    link: State<'_, RuntimeLink>,
) -> Result<(), String> {
    match &*link {
        RuntimeLink::Embedded(runtime) => {
            let shared = tauri::Manager::try_state::<SharedLlmRouter>(&app)
                .ok_or_else(|| "LLM router not available".to_string())?;
            rebuild_desktop_router(&shared, runtime).await
        }
        RuntimeLink::Attached(_) => reload_runtime_router(&link).await,
    }
}

/// Asks the runtime `link` drives to rebuild its router from `system.db`.
pub(crate) async fn reload_runtime_router(link: &RuntimeLink) -> Result<(), String> {
    link.post_json("/api/v1/llm/reload", &serde_json::json!({}))
        .await
        .map(|_| ())
}

/// Rebuilds the router of this process, then the embedded runtime's.
pub(crate) async fn rebuild_desktop_router(
    shared: &SharedLlmRouter,
    runtime: &RuntimeHandle,
) -> Result<(), String> {
    let home = apollia_core::paths::home_dir_or_temp();
    let db_path = apollia_core::paths::data_dir_under(home)
        .join(apollia_core::paths::DataFile::System.file_name());

    // A row naming a server the organisation policy refuses is turned off.
    let governance =
        apollia_runtime::governance::InferenceGovernance::new(runtime.org_policy.clone());

    // LlmBackendRepository is !Send (uses RefCell<Connection>), so all DB work
    // must stay inside spawn_blocking. We extract the raw configs and return them.
    let (all_configs, default_name) = tokio::task::spawn_blocking(move || {
        let repo = LlmBackendRepository::open(&db_path)
            .map_err(|e| format!("failed to open system.db: {e}"))?;
        // A key kept in the secret store is put back in memory for the router.
        let (all, default_name) = apollia_runtime::backend_keys::routable_configs(
            &repo,
            apollia_runtime::inference_servers::default_store,
            &governance,
        )
        .map_err(|e| format!("failed to list LLM backends: {e}"))?;
        let default_name = default_name
            .ok_or_else(|| "no default LLM backend configured in system.db".to_string())?;
        Ok::<_, String>((all, default_name))
    })
    .await
    .map_err(|e| format!("spawn_blocking failed: {e}"))??;

    // Surface the reload on the EventBus: the boot-time router emits
    // LlmModelLoading / Ready / Failed itself, but this desktop rebuild runs
    // without a bus, so nothing reached the UI. Without these events the
    // `llm-changed` SSE category stays silent after onboarding registers the
    // local model, and the chat step shows "no engine available" until the
    // 10 s watchdog refreshes the store.
    let default_model = all_configs
        .iter()
        .find(|c| c.name == default_name)
        .map(|c| c.model.clone())
        .unwrap_or_default();
    let _ = runtime
        .event_sender
        .send(apollia_core::RuntimeEvent::LlmModelLoading {
            backend: default_name.clone(),
            model_path: default_model.clone(),
        });

    // Re-inject the llama-server override for local LlamaCpp backends, otherwise
    // the reload would rebuild a router without the local backend (local
    // inference would become unreachable). We reuse the same factory as the
    // supervisor at boot.
    let factory = apollia_runtime::local_engine::local_engine_override(
        runtime.llama_server_supervisor.clone(),
        runtime.mlx_engine.clone(),
        apollia_core::paths::data_dir().map(|d| apollia_runtime::installed_models::models_dir(&d)),
    );

    // `system.db` holds neither `[llm.routing]` nor the agents' last
    // fallbacks: both come from the router this one replaces.
    let previous = shared
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone();
    let new_router = match LlmRouter::from_backend_configs_with_override(
        all_configs,
        default_name.clone(),
        factory,
    )
    .await
    {
        Ok(router) => Arc::new(match previous.as_ref() {
            Some(previous) => router.carry_over_from(previous),
            None => router,
        }),
        Err(e) => {
            let reason = format!("failed to build LLM router: {e}");
            emit_reload_outcome(
                &runtime.event_sender,
                Err((default_name.as_str(), reason.as_str())),
            );
            return Err(reason);
        }
    };
    emit_reload_outcome(
        &runtime.event_sender,
        Ok((default_name.as_str(), default_model.as_str())),
    );

    // Drop the old router before writing the new one so the GGUF is freed from RAM
    // as soon as no other Arc references remain (e.g. in-flight agent requests).
    {
        let mut guard = shared.write().map_err(|e| format!("lock poisoned: {e}"))?;
        let old = guard.take();
        drop(old);
        *guard = Some(Arc::clone(&new_router));
    }
    // A running agent holds the router it started with; it reaches this one
    // at its next call.
    if let Some(previous) = previous {
        previous.supersede(&new_router);
    }

    // The runtime keeps its OWN LlmRouter cell, consumed by the REST API
    // (`/api/v1/llm/ping`, `/chat`, `/complete`). It is rebuilt only by
    // `POST /api/v1/llm/reload`; the desktop cell swap above does not touch it.
    // Without this, a request through those routes reports "no LLM router configured" even
    // after a backend is created. Best-effort: a failure here must not fail the
    // desktop reload, which already succeeded.
    if let Err(e) = http_post_json(
        runtime.api_port,
        "/api/v1/llm/reload",
        &serde_json::json!({}),
    )
    .await
    {
        tracing::warn!(error = %e, "llm.router.rebuild.failed");
    }

    tracing::info!("llm.router.reloaded");
    Ok(())
}

/// Emit the terminal LLM lifecycle event for a desktop router reload.
///
/// `Ok((backend, model_id))` becomes [`RuntimeEvent::LlmModelReady`],
/// `Err((backend, reason))` becomes [`RuntimeEvent::LlmModelFailed`]. Both map
/// to the `llm-changed` SSE category, which the frontend answers by refreshing
/// its backend list. Best-effort: an unsubscribed bus is not an error.
fn emit_reload_outcome(
    sender: &apollia_runtime::eventbus::EventBusSender,
    outcome: Result<(&str, &str), (&str, &str)>,
) {
    let event = match outcome {
        Ok((backend, model_id)) => apollia_core::RuntimeEvent::LlmModelReady {
            backend: backend.to_owned(),
            model_id: model_id.to_owned(),
        },
        Err((backend, reason)) => apollia_core::RuntimeEvent::LlmModelFailed {
            backend: backend.to_owned(),
            reason: reason.to_owned(),
        },
    };
    let _ = sender.send(event);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reload_outcome_emits_llm_model_ready() {
        // GIVEN a subscribed event bus
        let (tx, mut rx) = tokio::sync::broadcast::channel(8);

        // WHEN the reload reports success
        emit_reload_outcome(&tx, Ok(("local-code", "qwen3.gguf")));

        // THEN one LlmModelReady with the backend name is received
        match rx.try_recv() {
            Ok(apollia_core::RuntimeEvent::LlmModelReady { backend, model_id }) => {
                assert_eq!(backend, "local-code");
                assert_eq!(model_id, "qwen3.gguf");
            }
            other => panic!("expected LlmModelReady, got {other:?}"),
        }
    }

    #[test]
    fn test_reload_outcome_emits_llm_model_failed() {
        // GIVEN a subscribed event bus
        let (tx, mut rx) = tokio::sync::broadcast::channel(8);

        // WHEN the reload reports a build failure
        emit_reload_outcome(&tx, Err(("local-code", "boom")));

        // THEN one LlmModelFailed with the reason is received
        match rx.try_recv() {
            Ok(apollia_core::RuntimeEvent::LlmModelFailed { backend, reason }) => {
                assert_eq!(backend, "local-code");
                assert_eq!(reason, "boom");
            }
            other => panic!("expected LlmModelFailed, got {other:?}"),
        }
    }

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
}
