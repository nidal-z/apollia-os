//! `RunnerProxy`: adapts the daemon's `CompletionModel` / `SttBackend` calls
//! to the runner via HTTP/JSON.
//!
//! Implements the daemon-side HTTP API. The `CompletionModel` (from
//! `apollia-llm`) and `SttBackend` (from `apollia-stt`) traits are wired up
//! once the runner's RunnerLlmBackend is connected.

use std::sync::Arc;

use serde_json::Value;
use tokio::sync::RwLock;
use uuid::Uuid;

use super::error::RunnerError;

/// Proxy to the runner. Cloneable via `Arc`.
#[derive(Clone)]
pub struct RunnerProxy {
    inner: Arc<RwLock<Option<RunnerInnerHandle>>>,
}

/// Hidden type alias: we do not re-expose the lifecycle::RunnerInner struct here
/// to avoid a circular dep. Instead we keep a generic handle to the
/// RwLock<Option<RunnerInner>> owned by the supervisor.
pub(super) type RunnerInnerHandle = super::lifecycle_inner::RunnerInnerHandle;

impl RunnerProxy {
    pub(super) fn new(
        inner: Arc<RwLock<Option<super::lifecycle_inner::RunnerInnerHandle>>>,
    ) -> Self {
        Self { inner }
    }

    /// Generic HTTP call `POST /llm/...` or `/stt/...`.
    pub async fn post_json<P: serde::Serialize, D: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        params: P,
    ) -> Result<D, RunnerError> {
        let request_id = Uuid::new_v4();
        let envelope = serde_json::json!({
            "request_id": request_id,
            "params": params,
        });

        let guard = self.inner.read().await;
        let inner = guard
            .as_ref()
            .ok_or_else(|| RunnerError::Http("runner not started".into()))?;

        // The runner's response is `{ok, request_id, data | error}`.
        // Parse at the `Value` level then extract the typed `data`.
        let raw: Value = inner.client.post(path, &envelope).await?;
        let ok = raw.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
        if !ok {
            let err = raw.get("error").cloned().unwrap_or(Value::Null);
            let code = err
                .get("code")
                .and_then(|v| v.as_str())
                .unwrap_or("UNKNOWN")
                .to_string();
            let message = err
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            return Err(RunnerError::Ipc { code, message });
        }
        let data = raw
            .get("data")
            .cloned()
            .ok_or_else(|| RunnerError::Http("missing 'data' in response".into()))?;
        serde_json::from_value(data).map_err(RunnerError::Json)
    }

    /// Tokenize `text` for `model_id` via `POST /llm/tokenize`, blocking until
    /// the runner answers, and return the token count.
    ///
    /// `CompletionModel::count_tokens` is synchronous, but the IPC call is
    /// async, so this method bridges the two. It first probes connectivity
    /// synchronously with `try_read`: when the runner is not started it returns
    /// an error immediately, with no Tokio runtime required, so a caller outside
    /// any async context still reaches its proxy fallback. When connected it
    /// drives the HTTP call on the current runtime (the daemon's multi-threaded
    /// runtime in production), falling back to a transient runtime when called
    /// from a non-async context.
    pub fn tokenize_blocking(&self, model_id: &str, text: &str) -> Result<u32, RunnerError> {
        // Synchronous readiness probe so the absent-runner path needs no runtime.
        let connected = matches!(self.inner.try_read(), Ok(guard) if guard.is_some());
        if !connected {
            return Err(RunnerError::Http("runner not started".into()));
        }

        let params = serde_json::json!({ "model_id": model_id, "text": text });
        let fut = self.post_json::<_, Value>("/llm/tokenize", params);

        let data = match tokio::runtime::Handle::try_current() {
            Ok(handle) => tokio::task::block_in_place(|| handle.block_on(fut)),
            Err(_) => tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| RunnerError::Http(format!("tokenize runtime: {e}")))?
                .block_on(fut),
        }?;

        data.get("token_count")
            .and_then(|v| v.as_u64())
            .map(|n| n as u32)
            .ok_or_else(|| RunnerError::Http("tokenize response missing token_count".into()))
    }

    /// Check the runner's health via `GET /health`.
    pub async fn health_check(&self) -> Result<Value, RunnerError> {
        let guard = self.inner.read().await;
        let inner = guard
            .as_ref()
            .ok_or_else(|| RunnerError::Http("runner not started".into()))?;
        inner.client.get("/health").await
    }
}
