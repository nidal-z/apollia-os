//! `RunnerLlmBackend` : adapte `CompletionModel` (apollia-llm) sur le
//! [`RunnerProxy`] via HTTP/JSON IPC.
//!
//! Cf. ADR-113 §4 (lifecycle) et IPC-PROTOCOL §3.4-3.6.

use std::pin::Pin;
use std::sync::Arc;
use std::time::Instant;

use apollia_llm::types::{
    ChatMessage, CompletionModel, CompletionRequest, CompletionResponse, FinishReason,
    MessageContent, Role, StreamChunk, TokenUsage,
};
use apollia_llm::LlmError;
use futures::Stream;
use serde_json::Value;

use super::proxy::RunnerProxy;

/// Backend `CompletionModel` qui route les appels vers le runner sidecar.
///
/// Instancié par le `Supervisor` au boot quand un `LlmBackendConfig` de
/// provider `LlamaCpp` est trouvé en DB ET qu'un runner est dispo.
pub struct RunnerLlmBackend {
    proxy: RunnerProxy,
    backend_name: String,
    model_id: String,
    model_path: String,
    /// True après le premier `load_model` réussi côté runner.
    loaded: std::sync::Mutex<bool>,
}

impl RunnerLlmBackend {
    pub fn new(
        proxy: RunnerProxy,
        backend_name: String,
        model_id: String,
        model_path: String,
    ) -> Arc<Self> {
        Arc::new(Self {
            proxy,
            backend_name,
            model_id,
            model_path,
            loaded: std::sync::Mutex::new(false),
        })
    }

    /// Charge le modèle côté runner si pas déjà fait. Idempotent.
    async fn ensure_loaded(&self) -> Result<(), LlmError> {
        {
            let guard = self.loaded.lock().unwrap();
            if *guard {
                return Ok(());
            }
        }

        let params = serde_json::json!({
            "model_id": self.model_id,
            "model_path": self.model_path,
            "n_ctx": 4096,
            "n_gpu_layers": -1,
            "use_mmap": true,
            "use_mlock": false,
        });

        let _data: Value = self
            .proxy
            .post_json("/llm/load_model", params)
            .await
            .map_err(|e| LlmError::BackendUnavailable {
                backend: self.backend_name.clone(),
                reason: format!("load_model via runner: {e}"),
            })?;

        *self.loaded.lock().unwrap() = true;
        Ok(())
    }

    fn map_messages(messages: &[ChatMessage]) -> Vec<Value> {
        messages
            .iter()
            .map(|m| {
                let role = match m.role {
                    Role::System => "system",
                    Role::User => "user",
                    Role::Assistant => "assistant",
                    // Tool results : pas encore plombés via IPC (limitation Phase 2),
                    // sérialiser en message user avec le contenu textuel.
                    Role::Tool => "user",
                };
                let content = match &m.content {
                    MessageContent::Text(t) => t.clone(),
                    MessageContent::WithToolCalls { text, .. } => text.clone(),
                    MessageContent::ToolResult { content, .. } => content.clone(),
                };
                serde_json::json!({
                    "role": role,
                    "content": content,
                })
            })
            .collect()
    }

    fn complete_params(&self, req: &CompletionRequest) -> Value {
        serde_json::json!({
            "model_id": self.model_id,
            "messages": Self::map_messages(&req.messages),
            "max_tokens": req.max_tokens.unwrap_or(512),
            "temperature": req.temperature.unwrap_or(0.7),
            "top_p": 0.95,
            "top_k": 40,
            "repeat_penalty": 1.1,
            "seed": req.seed,
            "stop": Vec::<String>::new(),
        })
    }
}

#[async_trait::async_trait]
impl CompletionModel for RunnerLlmBackend {
    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        self.ensure_loaded().await?;

        let started = Instant::now();
        let params = self.complete_params(&req);

        let data: Value = self
            .proxy
            .post_json("/llm/complete", params)
            .await
            .map_err(|e| LlmError::InferenceError(format!("runner /llm/complete: {e}")))?;

        let text = data
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let usage = data
            .get("usage")
            .and_then(|u| {
                Some(TokenUsage {
                    prompt_tokens: u.get("prompt_tokens")?.as_u64()? as u32,
                    completion_tokens: u.get("completion_tokens")?.as_u64()? as u32,
                    cost_usd: None,
                    cache_read_input_tokens: 0,
                    cache_write_input_tokens: 0,
                })
            })
            .unwrap_or_default();

        let finish_reason = match data
            .get("finish_reason")
            .and_then(|v| v.as_str())
            .unwrap_or("stop")
        {
            "length" => FinishReason::Length,
            _ => FinishReason::Stop,
        };

        Ok(CompletionResponse {
            content: text,
            tool_calls: Vec::new(),
            usage,
            finish_reason,
            latency_ms: started.elapsed().as_millis() as u64,
            ttft_ms: None,
        })
    }

    async fn stream(
        &self,
        req: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk, LlmError>> + Send>>, LlmError> {
        // Phase 3 minimal : déléguer à complete(), retourner le texte en un
        // seul chunk. Le vrai streaming SSE sera plombé en finalisation
        // (parser le flux text/event-stream et ré-émettre des StreamChunk::Text).
        let resp = self.complete(req).await?;
        let chunk = StreamChunk::Text(resp.content);
        let stream = futures::stream::once(async move { Ok(chunk) });
        Ok(Box::pin(stream))
    }

    fn is_available(&self) -> bool {
        true
    }

    fn backend_name(&self) -> &str {
        &self.backend_name
    }

    fn model_id(&self) -> &str {
        &self.model_id
    }
}
