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

    /// Check the runner's health via `GET /health`.
    pub async fn health_check(&self) -> Result<Value, RunnerError> {
        let guard = self.inner.read().await;
        let inner = guard
            .as_ref()
            .ok_or_else(|| RunnerError::Http("runner not started".into()))?;
        inner.client.get("/health").await
    }
}
