//! Token streaming from the router to a Python async iterator.
//!
//! Split out of `llm.rs`: the proxy stays in the parent, the forwarding task
//! and the `TokenStream` pyclass it feeds live here.

use std::sync::Arc;

use futures::StreamExt;
use pyo3::exceptions::{PyRuntimeError, PyStopAsyncIteration};
use pyo3::prelude::*;
use tokio::sync::{mpsc, Mutex as AsyncMutex};

use apollia_core::events::EventBusSender;
use apollia_llm::types::{ChatMessage, CompletionRequest, LlmError, StreamChunk};
use apollia_llm::{router::ObservabilityConfig, LlmRouter};

/// Emit `LlmResponseCaptured` so a task-mode LLM completion is journaled (the
/// audit journal only captures completions carrying a `run_id`), enabling
/// `audit replay` and putting the model's output in the tamper-evident trail,
/// mirroring what the chat agent does.
pub(super) fn emit_llm_capture(
    bus: &Option<EventBusSender>,
    run_id: &Option<apollia_core::events::RunId>,
    backend: &str,
    resp: &apollia_llm::types::CompletionResponse,
) {
    let (Some(bus), Some(run_id)) = (bus.as_ref(), run_id.as_ref()) else {
        return;
    };
    let tool_calls: Vec<serde_json::Value> = resp
        .tool_calls
        .iter()
        .map(|tc| serde_json::json!({ "id": tc.id, "name": tc.name, "arguments": tc.arguments }))
        .collect();
    let _ = bus.send(apollia_core::events::RuntimeEvent::LlmResponseCaptured {
        run_id: run_id.clone(),
        backend: backend.to_string(),
        model: String::new(),
        content: resp.content.clone(),
        tool_calls,
        prompt_tokens: resp.usage.prompt_tokens,
        completion_tokens: resp.usage.completion_tokens,
        cost_usd: resp.usage.cost_usd,
        stream_truncated: false,
    });
}
/// Owned inputs for [`forward_stream`], the background task feeding a
/// [`PyTokenStream`].
pub(super) struct StreamForward {
    pub(super) router: Arc<LlmRouter>,
    pub(super) obs: Arc<ObservabilityConfig>,
    pub(super) bus: Option<EventBusSender>,
    pub(super) backend: Option<String>,
    pub(super) chat_messages: Vec<ChatMessage>,
    pub(super) temperature: Option<f32>,
    pub(super) max_tokens: Option<u32>,
    pub(super) seed: Option<u64>,
    pub(super) tx: mpsc::Sender<Result<String, LlmError>>,
}
/// Streams chunks from the resolved backend and forwards each one onto `tx`.
///
/// Backends that cannot stream natively fall back to a single `complete()`
/// call forwarded as one chunk. Sends a `BackendUnavailable` error when no
/// backend matches.
pub(super) async fn forward_stream(f: StreamForward) {
    let StreamForward {
        router,
        obs,
        bus,
        backend,
        chat_messages,
        temperature,
        max_tokens,
        seed,
        tx,
    } = f;
    let backend_key = backend.as_deref();
    let Some(model) = router.get(backend_key) else {
        let _ = tx
            .send(Err(LlmError::BackendUnavailable {
                backend: backend_key.unwrap_or("(default)").to_string(),
                reason: "no matching backend".to_string(),
            }))
            .await;
        return;
    };

    let req = CompletionRequest {
        messages: chat_messages.clone(),
        temperature,
        max_tokens,
        seed,
        ..Default::default()
    };
    match model.stream(req).await {
        Ok(mut stream) => {
            while let Some(chunk) = stream.next().await {
                match chunk {
                    Ok(StreamChunk::Text(text)) => {
                        if tx.send(Ok(text)).await.is_err() {
                            break;
                        }
                    }
                    Ok(StreamChunk::ToolCall(_)) => {
                        // Tool calls within the stream are not surfaced
                        // to Python; assistants parse JSON from the
                        // accumulated text.
                    }
                    Ok(StreamChunk::Usage(_)) => {
                        // Token accounting is not surfaced on this text-only
                        // Python bridge; drop the terminal usage chunk.
                    }
                    Ok(StreamChunk::Timings(_)) => {
                        // Same: engine timings are observed where they are
                        // produced, not forwarded to Python.
                    }
                    Err(e) => {
                        let _ = tx.send(Err(e)).await;
                        break;
                    }
                }
            }
        }
        // Fallback: backend does not support stream(),
        // so call complete() and forward as a single chunk.
        Err(_) => {
            let fallback_req = CompletionRequest {
                messages: chat_messages,
                temperature,
                max_tokens,
                seed,
                ..Default::default()
            };
            match router
                .complete_with_observability(backend_key, fallback_req, bus.as_ref(), &obs)
                .await
            {
                Ok(resp) => {
                    let _ = tx.send(Ok(resp.content)).await;
                }
                Err(e) => {
                    let _ = tx.send(Err(e)).await;
                }
            }
        }
    }
}
/// Python async iterator yielding text chunks from a streamed LLM call.
///
/// Implements the `__aiter__` + `__anext__` protocol. Raises
/// `StopAsyncIteration` when the stream is exhausted, `RuntimeError` on a
/// backend error.
#[pyclass(name = "TokenStream")]
pub struct PyTokenStream {
    pub(super) rx: Arc<AsyncMutex<mpsc::Receiver<Result<String, LlmError>>>>,
}
#[pymethods]
impl PyTokenStream {
    pub(super) fn __aiter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    pub(super) fn __anext__<'py>(
        slf: PyRef<'py, Self>,
        py: Python<'py>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let rx = Arc::clone(&slf.rx);
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let mut guard = rx.lock().await;
            match guard.recv().await {
                Some(Ok(chunk)) => Ok(chunk),
                Some(Err(e)) => Err(PyRuntimeError::new_err(e.to_string())),
                None => Err(PyStopAsyncIteration::new_err("stream exhausted")),
            }
        })
    }
}
