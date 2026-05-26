//! `RunnerProxy` : adapte les appels `CompletionModel` / `SttBackend` du
//! daemon vers le runner via HTTP/JSON.
//!
//! Phase 3 : implémente l'API HTTP côté daemon, mais les traits
//! `CompletionModel` (de `apollia-llm`) et `SttBackend` (de `apollia-stt`) ne
//! sont pas encore connectés — c'est STORY-009/finalisation qui le fait
//! quand le RunnerLlmBackend du runner est câblé.

use std::sync::Arc;

use serde_json::Value;
use tokio::sync::RwLock;
use uuid::Uuid;

use super::error::RunnerError;

/// Proxy vers le runner. Cloneable via `Arc`.
#[derive(Clone)]
pub struct RunnerProxy {
    inner: Arc<RwLock<Option<RunnerInnerHandle>>>,
}

/// Type alias caché : on ne réexpose pas la struct lifecycle::RunnerInner ici
/// pour ne pas créer de circular dep. À la place, on garde un handle générique
/// au RwLock<Option<RunnerInner>> qui appartient au supervisor.
pub(super) type RunnerInnerHandle = super::lifecycle_inner::RunnerInnerHandle;

impl RunnerProxy {
    pub(super) fn new(
        inner: Arc<RwLock<Option<super::lifecycle_inner::RunnerInnerHandle>>>,
    ) -> Self {
        Self { inner }
    }

    /// Appel HTTP générique `POST /llm/...` ou `/stt/...`.
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

        // Le response du runner est `{ok, request_id, data | error}`.
        // On parse au niveau `Value` puis on extrait `data` typé.
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

    /// Vérifie la santé du runner via `GET /health`.
    pub async fn health_check(&self) -> Result<Value, RunnerError> {
        let guard = self.inner.read().await;
        let inner = guard
            .as_ref()
            .ok_or_else(|| RunnerError::Http("runner not started".into()))?;
        inner.client.get("/health").await
    }
}
