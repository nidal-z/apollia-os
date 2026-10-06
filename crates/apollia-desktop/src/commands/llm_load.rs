//! Tauri IPC commands for loading a local model by hand.
//!
//! Each command delegates to the internal REST API:
//! `POST /api/v1/llm/loaded-models/estimate` says what a load would take,
//! `POST /api/v1/llm/loaded-models` loads, `GET /api/v1/llm/loaded-models/last`
//! names the last model loaded by hand and
//! `POST /api/v1/llm/loaded-models/last/reload` loads it again. The answers
//! are the routes' own JSON; the interface types them.

use std::time::Duration;

use crate::attached::RuntimeLink;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use tauri::State;

const ESTIMATE_PATH: &str = "/api/v1/llm/loaded-models/estimate";
const LOAD_PATH: &str = "/api/v1/llm/loaded-models";
const LAST_PATH: &str = "/api/v1/llm/loaded-models/last";
const RELOAD_LAST_PATH: &str = "/api/v1/llm/loaded-models/last/reload";

/// A load answers once the engine serves the model, which a large model on a
/// slow disk takes minutes to reach.
const LOAD_DEADLINE: Duration = Duration::from_secs(600);

/// What the loading window asks: the model and the settings of this load.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelLoadRequest {
    /// Identifier of the model in the model index.
    pub model_id: String,
    /// Context window in tokens; absent for the one the model's defaults give.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub n_ctx: Option<u32>,
    /// Layers placed on the accelerator; absent for the planned count.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub n_gpu_layers: Option<i32>,
    /// Advanced load settings, by the keys of a preset's load object.
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub advanced: Map<String, Value>,
    /// Write these settings to the model's default preset.
    #[serde(default)]
    pub remember: bool,
    /// The user confirmed a load the guardrail refuses.
    #[serde(default)]
    pub confirmed: bool,
}

impl ModelLoadRequest {
    /// The body of an estimate: the model and its settings, nothing a load
    /// alone reads.
    fn estimate_body(&self) -> Value {
        let mut body = serde_json::json!({ "modelId": self.model_id });
        if let Some(n_ctx) = self.n_ctx {
            body["nCtx"] = Value::from(n_ctx);
        }
        if let Some(layers) = self.n_gpu_layers {
            body["nGpuLayers"] = Value::from(layers);
        }
        if !self.advanced.is_empty() {
            body["advanced"] = Value::Object(self.advanced.clone());
        }
        body
    }

    fn load_body(&self) -> Result<Value, String> {
        serde_json::to_value(self).map_err(|e| format!("invalid load request: {e}"))
    }
}

fn reload_body(confirmed: bool) -> Value {
    serde_json::json!({ "confirmed": confirmed })
}

/// Says what loading a model under these settings would take: the memory on
/// the accelerator and in total, and the verdict of the load guardrail.
///
/// Delegates to `POST /api/v1/llm/loaded-models/estimate`. Nothing is loaded.
#[tauri::command]
pub async fn estimate_model_load(
    state: State<'_, RuntimeLink>,
    request: ModelLoadRequest,
) -> Result<Value, String> {
    state
        .post_json(ESTIMATE_PATH, &request.estimate_body())
        .await
}

/// Loads a model under these settings and answers the instance serving it.
///
/// Delegates to `POST /api/v1/llm/loaded-models`. Rejected with the runtime's
/// message when the load guardrail refuses the load and `confirmed` is false.
#[tauri::command]
pub async fn load_model(
    state: State<'_, RuntimeLink>,
    request: ModelLoadRequest,
) -> Result<Value, String> {
    state
        .post_json_within(LOAD_PATH, &request.load_body()?, LOAD_DEADLINE)
        .await
}

/// The last model loaded by hand and its settings, `None` when there is none.
///
/// Delegates to `GET /api/v1/llm/loaded-models/last`.
#[tauri::command]
pub async fn get_last_loaded_model(state: State<'_, RuntimeLink>) -> Result<Option<Value>, String> {
    state.get_json_or_none(LAST_PATH).await
}

/// Loads the last model loaded by hand again, under the settings it had.
///
/// Delegates to `POST /api/v1/llm/loaded-models/last/reload`.
#[tauri::command]
pub async fn reload_last_loaded_model(
    state: State<'_, RuntimeLink>,
    confirmed: bool,
) -> Result<Value, String> {
    state
        .post_json_within(RELOAD_LAST_PATH, &reload_body(confirmed), LOAD_DEADLINE)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(json: Value) -> ModelLoadRequest {
        serde_json::from_value(json).unwrap()
    }

    #[test]
    fn test_an_estimate_sends_the_settings_and_never_the_consent_of_a_load() {
        // GIVEN what the window holds: a window, an offload, one advanced
        // setting, and the two choices only a load reads
        let asked = request(serde_json::json!({
            "modelId": "Qwen3-8B",
            "nCtx": 16384,
            "nGpuLayers": 12,
            "advanced": { "n_threads": 4 },
            "remember": true,
            "confirmed": true
        }));

        // WHEN the body of the estimate is built
        let body = asked.estimate_body();

        // THEN it names the model and the settings, in the route's casing,
        // and neither choice
        assert_eq!(
            body,
            serde_json::json!({
                "modelId": "Qwen3-8B",
                "nCtx": 16384,
                "nGpuLayers": 12,
                "advanced": { "n_threads": 4 }
            })
        );
    }

    #[test]
    fn test_a_load_without_settings_sends_the_model_alone_and_no_consent() {
        // GIVEN a load asked for a model, nothing set
        let asked = request(serde_json::json!({ "modelId": "Qwen3-8B" }));

        // WHEN its bodies are built
        let estimate = asked.estimate_body();
        let load = asked.load_body().unwrap();

        // THEN no setting is sent, so the model's defaults apply, and the
        // load is neither remembered nor confirmed
        assert_eq!(estimate, serde_json::json!({ "modelId": "Qwen3-8B" }));
        assert_eq!(
            load,
            serde_json::json!({ "modelId": "Qwen3-8B", "remember": false, "confirmed": false })
        );
    }

    #[test]
    fn test_a_confirmed_load_carries_the_consent_and_the_wish_to_remember() {
        // GIVEN a load the user confirmed past the guardrail, to remember
        let asked = request(serde_json::json!({
            "modelId": "Qwen3-32B",
            "nCtx": 8192,
            "remember": true,
            "confirmed": true
        }));

        // WHEN the body of the load is built
        let body = asked.load_body().unwrap();

        // THEN the route reads both
        assert_eq!(body["confirmed"], true);
        assert_eq!(body["remember"], true);
        assert_eq!(body["nCtx"], 8192);
        assert_eq!(reload_body(true), serde_json::json!({ "confirmed": true }));
    }
}
