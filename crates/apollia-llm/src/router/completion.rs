//! The router's completion and streaming paths.
//!
//! Split out of `router.rs`: the call that reaches a backend, records the
//! observability fields, and falls back on failure lives here.

use std::time::Instant;

use apollia_core::events::{EventBusSender, RuntimeEvent};

use crate::router::analyse_call_failure;
use crate::router::config::ObservabilityConfig;
use crate::router::{FallbackPlan, LlmRouter};
use crate::types::{CompletionRequest, CompletionResponse, LlmError};

impl LlmRouter {
    /// Call the backend and automatically emit [`RuntimeEvent::LlmCallCompleted`].
    ///
    /// Execution sequence:
    /// 1. Log the prompt at `TRACE` level if `obs.debug_log_prompt` is enabled.
    /// 2. Call `backend.complete(req)`.
    /// 3. Emit `LlmCallCompleted` fire-and-forget on the bus (when present).
    /// 4. Log tokens/latency at `INFO` level per the `obs` flags.
    /// 5. Return `Ok(response)`.
    ///
    /// The `EventBusSender` is optional: `None` disables emission without
    /// changing functional behavior. `send()` errors are silently ignored.
    ///
    /// # Errors
    ///
    /// - [`LlmError::BackendUnavailable`]: the requested backend is not in the router.
    /// - Any error propagated by `backend.complete()`.
    pub async fn complete_with_observability(
        &self,
        backend_name: Option<&str>,
        req: CompletionRequest,
        bus: Option<&EventBusSender>,
        obs: &ObservabilityConfig,
    ) -> Result<CompletionResponse, LlmError> {
        let backend_key = backend_name.unwrap_or(&self.default);

        let backend =
            self.backends
                .get(backend_key)
                .ok_or_else(|| LlmError::BackendUnavailable {
                    backend: backend_key.to_owned(),
                    reason: "not found in router".to_owned(),
                })?;

        // Log the prompt at TRACE only, never at INFO.
        if obs.debug_log_prompt {
            tracing::trace!(prompt = ?req.messages, "llm.request.prompt");
        }

        let started = Instant::now();
        let response = match backend.complete(req).await {
            Ok(response) => response,
            Err(error) => {
                // Fire-and-forget emission: send() errors are silently ignored.
                if let Some(b) = bus {
                    let _ = b.send(RuntimeEvent::LlmCallFailed {
                        backend: backend_key.to_owned(),
                        model: backend.model_id().to_owned(),
                        task_id: None,
                        step_id: None,
                        error: error.to_string(),
                        analysis: analyse_call_failure(&error),
                    });
                }
                return Err(error);
            }
        };
        let latency_ms = started.elapsed().as_millis() as u64;

        // Accumulate into the session budget and emit TokenBudgetUpdated.
        // Lock held only for the duration of record_usage(), never across awaits.
        if let Ok(mut tracker) = self.session_budget.lock() {
            tracker.record_usage(&response.usage, latency_ms, response.ttft_ms);
        }

        // Fire-and-forget emission: send() errors are silently ignored.
        if let Some(b) = bus {
            let _ = b.send(RuntimeEvent::LlmCallCompleted {
                backend: backend_key.to_owned(),
                model: backend.model_id().to_owned(),
                task_id: None,
                step_id: None,
                prompt_tokens: response.usage.prompt_tokens,
                completion_tokens: response.usage.completion_tokens,
                latency_ms,
                cost_usd: response.usage.cost_usd,
                run_id: None,
            });
        }

        if obs.log_token_usage {
            tracing::info!(
                backend = backend_key,
                prompt_tokens = response.usage.prompt_tokens,
                completion_tokens = response.usage.completion_tokens,
                "llm.usage.tokens"
            );
        }

        if obs.log_latency {
            tracing::info!(
                backend = backend_key,
                latency_ms = latency_ms,
                "llm.request.latency"
            );
        }

        Ok(response)
    }
    /// Invoke the primary backend, then on a non-recoverable failure switch to
    /// the first available secondary backend.
    ///
    /// Emits [`RuntimeEvent::LlmFallbackTriggered`] on the bus for each
    /// successful switch. The switch is functionally transparent: the caller
    /// receives either the primary's response, the response of the first
    /// fallback that answers, or the last observed error.
    pub async fn complete_with_fallback(
        &self,
        plan: FallbackPlan<'_>,
        req: CompletionRequest,
    ) -> Result<CompletionResponse, LlmError> {
        let FallbackPlan {
            primary,
            fallbacks,
            bus,
            obs,
        } = plan;
        let primary_result = self
            .complete_with_observability(Some(primary), req.clone(), bus, obs)
            .await;

        let primary_err = match primary_result {
            Ok(response) => return Ok(response),
            Err(e) => e,
        };

        let mut last_err = primary_err;
        for &candidate in fallbacks {
            if candidate == primary || !self.backends.contains_key(candidate) {
                continue;
            }
            if let Some(b) = bus {
                let _ = b.send(RuntimeEvent::LlmFallbackTriggered {
                    from_provider: primary.to_string(),
                    to_provider: candidate.to_string(),
                    reason: last_err.to_string(),
                });
            }
            tracing::warn!(
                from = %primary,
                to = %candidate,
                reason = %last_err,
                reason = "the primary backend failed",
                "llm.fallback.attempted"
            );
            match self
                .complete_with_observability(Some(candidate), req.clone(), bus, obs)
                .await
            {
                Ok(response) => return Ok(response),
                Err(e) => {
                    last_err = e;
                }
            }
        }

        Err(last_err)
    }
    /// Open a stream of [`StreamChunk`]s from the resolved backend.
    ///
    /// Resolves the backend (by name or default), calls `backend.stream(req)`,
    /// and returns the raw stream. The caller is responsible for emitting the
    /// `LlmCallCompleted` event once the stream is consumed.
    ///
    /// # Errors
    ///
    /// - [`LlmError::BackendUnavailable`]: the requested backend is not in the router.
    /// - Any error propagated by `backend.stream()`.
    pub async fn stream_with_observability(
        &self,
        backend_name: Option<&str>,
        req: CompletionRequest,
        obs: &ObservabilityConfig,
    ) -> Result<
        std::pin::Pin<
            Box<dyn futures::Stream<Item = Result<crate::types::StreamChunk, LlmError>> + Send>,
        >,
        LlmError,
    > {
        let backend_key = backend_name.unwrap_or(&self.default);

        let backend =
            self.backends
                .get(backend_key)
                .ok_or_else(|| LlmError::BackendUnavailable {
                    backend: backend_key.to_owned(),
                    reason: "not found in router".to_owned(),
                })?;

        if obs.debug_log_prompt {
            tracing::trace!(prompt = ?req.messages, "llm.stream.prompt");
        }

        let stream = backend.stream(req).await?;
        Ok(stream)
    }
}
