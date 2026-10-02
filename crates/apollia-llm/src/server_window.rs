//! The context window a server serves a model with, read and never imposed.
//!
//! A remote server owns its configuration: Apollia sends it plain OpenAI chat
//! completions and asks nothing about how the model is loaded. What Apollia
//! does need is the figure, because the router sizes compaction against it.
//! So the window is read where the server exposes it: `POST /api/show` on
//! Ollama, `max_model_len` in `GET /v1/models` on vLLM. When the figure read is
//! shorter than the window the embedded engine runs at, the operator is warned
//! (`llm.server.context_short`), and nothing is changed on the server.

use std::time::Duration;

use serde_json::Value;

use apollia_core::inference_server::openai_api_base;
use apollia_core::net::{configured_endpoint_client_builder_for, read_capped_json};

use crate::server_models::{list_server_model_entries, ListedModel};

/// The window an agent is expected to have: the one the embedded engine
/// launches with when nothing else is set, and the one Apollia used to impose
/// on Ollama. A server serving less is reported, never overridden.
pub const EXPECTED_CONTEXT_WINDOW: u32 = 32_768;

/// Largest `/api/show` body accepted. It carries the model's metadata and
/// template, a few hundred kilobytes for the largest tokenizers listed inline.
const MAX_SHOW_BODY: u64 = 8 * 1024 * 1024;

/// How long a server has to accept the connection of a probe.
const PROBE_CONNECT_TIMEOUT: Duration = Duration::from_secs(2);

/// How long one probe may take in all.
const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// The native API root of an address: its OpenAI base without the `/v1`.
fn native_root(address: &str) -> String {
    let base = openai_api_base(address);
    base.strip_suffix("/v1").unwrap_or(&base).to_string()
}

/// The window an `/api/show` body says the model is served with.
///
/// A model whose parameters set its window (a `PARAMETER num_ctx` line in its
/// Modelfile) is served with that window on every endpoint. Otherwise the
/// figure is the length the model was trained on, `<arch>.context_length` in
/// `model_info`.
#[must_use]
pub fn window_from_show(show: &Value) -> Option<u32> {
    let parameter = show
        .get("parameters")
        .and_then(Value::as_str)
        .and_then(|params| {
            params.lines().find_map(|line| {
                let mut words = line.split_whitespace();
                (words.next() == Some("num_ctx"))
                    .then(|| words.next()?.parse::<u32>().ok())
                    .flatten()
            })
        });
    let trained = || {
        show.get("model_info")?
            .as_object()?
            .iter()
            .find(|(key, _)| key.ends_with(".context_length"))
            .and_then(|(_, v)| v.as_u64())
            .and_then(|n| u32::try_from(n).ok())
    };
    parameter.or_else(trained).filter(|n| *n > 0)
}

/// Ask an Ollama server, by `POST /api/show`, the window it serves `model` with.
///
/// `None` when the server does not answer that route (it is not Ollama, or it
/// is down) or answers without a figure.
pub async fn ollama_window(address: &str, model: &str, api_key: Option<&str>) -> Option<u32> {
    let url = format!("{}/api/show", native_root(address));
    let client = configured_endpoint_client_builder_for(&url)
        .connect_timeout(PROBE_CONNECT_TIMEOUT)
        .timeout(PROBE_TIMEOUT)
        .build()
        .ok()?;
    let mut request = client
        .post(&url)
        .json(&serde_json::json!({ "model": model }));
    if let Some(key) = api_key.map(str::trim).filter(|k| !k.is_empty()) {
        request = request.bearer_auth(key);
    }
    let response = request.send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }
    let body: Value = read_capped_json(response, MAX_SHOW_BODY).await.ok()?;
    window_from_show(&body)
}

/// The window `GET /v1/models` declares for `model`, when the server exposes
/// one (`max_model_len` on vLLM).
pub async fn listed_window(address: &str, model: &str, api_key: Option<&str>) -> Option<u32> {
    let listed = tokio::time::timeout(PROBE_TIMEOUT, list_server_model_entries(address, api_key))
        .await
        .ok()?
        .ok()?;
    listed
        .into_iter()
        .find(|m| m.id == model)
        .and_then(|m| m.context_window)
}

/// Complete the windows a listing left unknown by asking `/api/show`, which
/// only Ollama answers.
///
/// The first model the server does not answer for ends the probing: a server
/// that is not Ollama answers none of them, and is asked once, not once per
/// model.
pub async fn fill_windows(address: &str, api_key: Option<&str>, models: &mut [ListedModel]) {
    for model in models.iter_mut().filter(|m| m.context_window.is_none()) {
        match ollama_window(address, &model.id, api_key).await {
            Some(window) => model.context_window = Some(window),
            None => return,
        }
    }
}

/// Whether a window read on a server is shorter than [`EXPECTED_CONTEXT_WINDOW`],
/// emitting `llm.server.context_short` when it is.
pub fn context_short(server: &str, model: &str, read_window: u32) -> bool {
    let short = read_window < EXPECTED_CONTEXT_WINDOW;
    if short {
        tracing::event!(
            tracing::Level::WARN,
            server = %server,
            model = %model,
            read_window = read_window,
            expected_window = EXPECTED_CONTEXT_WINDOW,
            "llm.server.context_short"
        );
    }
    short
}

#[cfg(test)]
mod tests;
