//! Tauri IPC commands for the local models resident in memory.
//!
//! Each command delegates to the internal REST API:
//! `GET /api/v1/llm/loaded-models` lists the resident models,
//! `POST /api/v1/llm/loaded-models/unload` stops one of them. The runtime
//! refuses an unload while the model is answering, and that refusal reaches
//! the webview as the rejected promise's message.

use crate::attached::RuntimeLink;
use serde::{Deserialize, Serialize};
use tauri::State;

const LOADED_MODELS_PATH: &str = "/api/v1/llm/loaded-models";
const UNLOAD_PATH: &str = "/api/v1/llm/loaded-models/unload";

/// One resident model, as the runtime reports it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadedModelView {
    /// The engine serving the model, chosen by its format: `llama-server` for
    /// GGUF, `vllm-mlx` for MLX.
    pub engine: String,
    /// What the model serves: `chat`, or `embeddings` for the instance that
    /// serves search by meaning outside the residency ceiling.
    #[serde(default = "chat_kind")]
    pub kind: String,
    /// Display name, the model file name without its extension.
    pub model: String,
    /// Path the model was loaded from; the key an unload takes.
    pub model_path: String,
    /// Loopback port of the process serving it.
    pub port: u16,
    /// Context window in tokens.
    pub n_ctx: u32,
    /// Layers placed on the GPU.
    pub n_gpu_layers: i32,
    /// Requests the process may serve in parallel, absent for the engine default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub n_parallel: Option<u32>,
    /// When the model finished loading, RFC 3339.
    pub loaded_at: String,
    /// When a request last used the model, RFC 3339.
    pub last_used_at: String,
    /// Requests being answered right now; an unload is refused above zero.
    pub active_requests: usize,
}

/// The resident models and the engine that holds them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadedModelsView {
    /// Whether a local engine exists on this host.
    pub engine_available: bool,
    /// The explicit ceiling on resident models; absent when the load
    /// guardrail alone decides how many stay loaded, and without an engine.
    #[serde(default)]
    pub max_loaded: Option<usize>,
    /// The resident models, most recently used first.
    pub models: Vec<LoadedModelView>,
}

/// Outcome of an unload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnloadedModelView {
    /// The model that was unloaded.
    pub model_path: String,
    /// Processes stopped.
    pub stopped: usize,
}

/// The kind a runtime that predates the field reports: every model it lists
/// serves chat.
fn chat_kind() -> String {
    "chat".to_owned()
}

fn parse_loaded_models(json: serde_json::Value) -> Result<LoadedModelsView, String> {
    serde_json::from_value(json).map_err(|e| format!("invalid loaded-models response: {e}"))
}

fn parse_unloaded_model(json: serde_json::Value) -> Result<UnloadedModelView, String> {
    serde_json::from_value(json).map_err(|e| format!("invalid unload response: {e}"))
}

fn unload_body(model_path: &str) -> serde_json::Value {
    serde_json::json!({ "modelPath": model_path })
}

/// Lists the local models resident in memory.
///
/// Delegates to `GET /api/v1/llm/loaded-models`.
#[tauri::command]
pub async fn list_loaded_models(state: State<'_, RuntimeLink>) -> Result<LoadedModelsView, String> {
    let json = state.get_json(LOADED_MODELS_PATH).await?;
    parse_loaded_models(json)
}

/// Unloads one resident model, by the path the list reports.
///
/// Delegates to `POST /api/v1/llm/loaded-models/unload`. Rejected with the
/// runtime's message when the model is answering a request, is not loaded, or
/// when no local engine exists on this host.
#[tauri::command]
pub async fn unload_loaded_model(
    state: State<'_, RuntimeLink>,
    model_path: String,
) -> Result<UnloadedModelView, String> {
    let json = state
        .post_json(UNLOAD_PATH, &unload_body(&model_path))
        .await?;
    parse_unloaded_model(json)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_the_runtime_list_body_parses_into_the_view_with_its_fields() {
        // GIVEN the body the runtime answers for two resident models
        let json = serde_json::json!({
            "engineAvailable": true,
            "maxLoaded": 2,
            "models": [
                {
                    "engine": "llama-server",
                    "kind": "embeddings",
                    "model": "b", "modelPath": "/models/b.gguf", "port": 41002,
                    "nCtx": 8192, "nGpuLayers": 99, "nParallel": 2,
                    "loadedAt": "2026-10-01T10:00:00+00:00",
                    "lastUsedAt": "2026-10-01T10:05:00+00:00",
                    "activeRequests": 1
                },
                {
                    "engine": "vllm-mlx",
                    "model": "a", "modelPath": "/models/a.gguf", "port": 41001,
                    "nCtx": 4096, "nGpuLayers": 0,
                    "loadedAt": "2026-10-01T09:00:00+00:00",
                    "lastUsedAt": "2026-10-01T09:30:00+00:00",
                    "activeRequests": 0
                }
            ]
        });

        // WHEN the command parses it
        let view = parse_loaded_models(json);

        // THEN both models are there, in order, with the absent parallelism left empty
        let view = view.unwrap();
        assert!(view.engine_available);
        assert_eq!(view.max_loaded, Some(2));
        assert_eq!(view.models.len(), 2);
        assert_eq!(view.models[0].model_path, "/models/b.gguf");
        assert_eq!(view.models[0].n_parallel, Some(2));
        assert_eq!(view.models[0].active_requests, 1);
        assert_eq!(view.models[1].n_parallel, None);
        assert_eq!(view.models[0].engine, "llama-server");
        assert_eq!(view.models[1].engine, "vllm-mlx");
        // AND the kind is read, a runtime that sends none meaning chat
        assert_eq!(view.models[0].kind, "embeddings");
        assert_eq!(view.models[1].kind, "chat");
    }

    #[test]
    fn test_the_view_reaches_the_webview_in_camel_case() {
        // GIVEN a parsed view of one model
        let view = LoadedModelsView {
            engine_available: true,
            max_loaded: None,
            models: vec![LoadedModelView {
                engine: "vllm-mlx".into(),
                kind: "chat".into(),
                model: "a".into(),
                model_path: "/models/a.gguf".into(),
                port: 41001,
                n_ctx: 4096,
                n_gpu_layers: 0,
                n_parallel: None,
                loaded_at: "2026-10-01T09:00:00+00:00".into(),
                last_used_at: "2026-10-01T09:30:00+00:00".into(),
                active_requests: 0,
            }],
        };

        // WHEN it is serialised for the webview
        let json = serde_json::to_value(&view).unwrap();

        // THEN the keys are the ones the TypeScript type reads
        assert_eq!(json["engineAvailable"], true);
        assert_eq!(json["maxLoaded"], serde_json::Value::Null);
        assert_eq!(json["models"][0]["modelPath"], "/models/a.gguf");
        assert_eq!(json["models"][0]["activeRequests"], 0);
        assert_eq!(json["models"][0]["engine"], "vllm-mlx");
        assert!(json["models"][0].get("nParallel").is_none());
    }

    #[test]
    fn test_a_list_body_without_its_models_is_an_error() {
        // GIVEN a body missing the model list
        let json = serde_json::json!({ "engineAvailable": true, "maxLoaded": 1 });

        // WHEN the command parses it
        let result = parse_loaded_models(json);

        // THEN the command rejects with a readable message
        assert!(result
            .unwrap_err()
            .contains("invalid loaded-models response"));
    }

    #[test]
    fn test_the_unload_body_carries_the_model_path_under_the_runtime_key() {
        // GIVEN the path of a resident model
        let path = "/models/a.gguf";

        // WHEN the command builds the request body
        let body = unload_body(path);

        // THEN it is the camelCase body the runtime route reads
        assert_eq!(body, serde_json::json!({ "modelPath": "/models/a.gguf" }));
    }

    #[test]
    fn test_the_unload_answer_parses_into_the_stopped_count() {
        // GIVEN the runtime's answer to an unload
        let json = serde_json::json!({ "modelPath": "/models/a.gguf", "stopped": 1 });

        // WHEN the command parses it
        let view = parse_unloaded_model(json).unwrap();

        // THEN the path and the stopped count are carried
        assert_eq!(view.model_path, "/models/a.gguf");
        assert_eq!(view.stopped, 1);
    }
}
