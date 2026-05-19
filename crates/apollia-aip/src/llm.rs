//! `LlmProxy` — bridge PyO3 exposant le moteur LLM à Python via `ctx.llm`.
//!
//! Pattern ADR-014 : toutes les méthodes async
//! utilisent `pyo3_async_runtimes::tokio::future_into_py`.
//!
//! Les tests nécessitant un environnement Python complet
//! sont couverts avec `#[cfg(feature = "python-tests")]`.

use std::sync::Arc;

use futures::StreamExt;
use pyo3::exceptions::{PyRuntimeError, PyStopAsyncIteration, PyValueError};
use pyo3::prelude::*;
use tokio::sync::{mpsc, Mutex as AsyncMutex};

use apollia_core::events::EventBusSender;
use apollia_llm::{
    ChatMessage, CompletionRequest, LlmError, LlmRouter, MessageContent, ObservabilityConfig, Role,
    StepBudgetView, StreamChunk, ToolCallHelper, ToolSpec,
};

// ─────────────────────────────────────────────
// Types PyO3
// ─────────────────────────────────────────────

/// Statistiques de consommation de tokens retournées à Python.
///
/// Accessible via `response.usage.prompt_tokens`, etc.
#[pyclass(name = "TokenUsage")]
#[derive(Clone)]
pub struct PyTokenUsage {
    /// Nombre de tokens dans le prompt (entrée).
    #[pyo3(get)]
    pub prompt_tokens: u32,
    /// Nombre de tokens générés (sortie).
    #[pyo3(get)]
    pub completion_tokens: u32,
    /// Coût estimatif en USD — `None` pour les backends locaux.
    #[pyo3(get)]
    pub cost_usd: Option<f64>,
}

/// Réponse LLM exposée à Python.
///
/// Retournée par `ctx.llm.chat()` et `ctx.llm.complete()`.
///
/// # Exemple Python
/// ```python
/// response = await ctx.llm.chat(system="Sois utile.", user="Bonjour")
/// print(response.content)          # str
/// print(response.usage.prompt_tokens)  # int
/// print(response.latency_ms)       # int
/// ```
#[pyclass(name = "LlmResponse")]
pub struct PyLlmResponse {
    /// Contenu textuel généré par le modèle.
    #[pyo3(get)]
    pub content: String,
    /// Latence totale de l'appel en millisecondes.
    #[pyo3(get)]
    pub latency_ms: u64,
    /// Statistiques de consommation de tokens.
    #[pyo3(get)]
    pub usage: PyTokenUsage,
}

// ─────────────────────────────────────────────
// LlmProxy
// ─────────────────────────────────────────────

/// Bridge PyO3 vers le `LlmRouter` Rust — exposé à Python via `ctx.llm`.
///
/// Injecté dans `RuntimeContext`. Wrappé via `Arc<LlmRouter>`
/// pour permettre le partage sans copie entre le runtime et les agents.
///
/// # Exemple Python complet
///
/// ```python
/// # Cas 1 — API simple (80% des agents)
/// response = await ctx.llm.chat(
///     system="Tu es un assistant commercial.",
///     user=task.input.parts[0].text,
/// )
/// # response.content : str
/// # response.usage.prompt_tokens : int
///
/// # Cas 2 — Multi-tour avec historique
/// response = await ctx.llm.complete([
///     {"role": "system",    "content": "Sois concis."},
///     {"role": "user",      "content": "Résume en 3 points."},
/// ])
///
/// # Cas 3 — Streaming (async iterator)
/// async for chunk in await ctx.llm.stream([{"role": "user", "content": "..."}]):
///     print(chunk)
///
/// # Cas 4 — Boucle ReAct automatique
/// result = await ctx.llm.run_tools(
///     messages=[{"role": "user", "content": "Lis le fichier."}],
///     tools=[{"name": "file_io", "description": "...", "parameters": {}}],
///     max_iterations=5,
/// )
///
/// # Override ponctuel du backend
/// response = await ctx.llm.chat(
///     system="...", user="...", backend="anthropic"
/// )
/// ```
#[pyclass(name = "LlmProxy")]
#[derive(Clone)]
pub struct LlmProxy {
    router: Arc<LlmRouter>,
    tool_helper: Arc<ToolCallHelper>,
    budget_view: Arc<StepBudgetView>,
    obs_config: Arc<ObservabilityConfig>,
    event_bus: Option<EventBusSender>,
    /// Tâche courante — injecté via [`with_task_context`] pour étiqueter
    /// les événements `LlmCallStarted` (ADR-088, Lot 2). `None` empêche
    /// l'émission sans casser le dispatch.
    task_id: Option<String>,
    /// Agent courant — pareil que `task_id`.
    agent_id: Option<String>,
}

impl LlmProxy {
    /// Crée un `LlmProxy` à injecter dans le `RuntimeContext` Python.
    ///
    /// Appelé lors de l'initialisation du contexte agent.
    /// `event_bus` est `None` si aucun Supervisor n'est actif (tests unitaires).
    pub fn new(
        router: Arc<LlmRouter>,
        tool_helper: Arc<ToolCallHelper>,
        budget_view: Arc<StepBudgetView>,
        obs_config: Arc<ObservabilityConfig>,
        event_bus: Option<EventBusSender>,
    ) -> Self {
        Self {
            router,
            tool_helper,
            budget_view,
            obs_config,
            event_bus,
            task_id: None,
            agent_id: None,
        }
    }

    /// Lie ce proxy à l'identité de l'exécution en cours pour l'observabilité
    /// (ADR-088, Lot 2). Sans ce setter, les événements `LlmCallStarted` ne
    /// sont pas émis (mais `complete_with_observability` continue d'émettre
    /// `LlmCallCompleted`/`LlmCallFailed` côté router).
    pub fn with_task_context(mut self, task_id: String, agent_id: String) -> Self {
        self.task_id = Some(task_id);
        self.agent_id = Some(agent_id);
        self
    }

    /// Émet `RuntimeEvent::LlmCallStarted` si bus + task_id sont configurés.
    fn emit_started(&self, backend: &str, model: &str, messages_count: u32, prompt_chars: u64) {
        if let (Some(bus), Some(task_id), Some(agent_id)) = (
            self.event_bus.as_ref(),
            self.task_id.as_ref(),
            self.agent_id.as_ref(),
        ) {
            let _ = bus.send(apollia_core::events::RuntimeEvent::LlmCallStarted {
                task_id: task_id.clone().into(),
                agent_id: agent_id.clone().into(),
                step_id: None,
                backend: backend.to_string(),
                model: model.to_string(),
                messages_count,
                prompt_chars,
            });
        }
    }
}

#[pymethods]
impl LlmProxy {
    /// Nom du backend LLM par défaut configuré dans `apollia.toml`.
    ///
    /// Propriété Python `ctx.llm.default_backend`.
    /// Retourne une chaîne vide si aucun backend n'est configuré.
    #[getter]
    fn default_backend(&self) -> String {
        self.router.default_name().to_string()
    }

    /// Appel LLM simplifié : prompt système + message utilisateur.
    ///
    /// Construit automatiquement `[ChatMessage::system(system), ChatMessage::user(user)]`.
    /// `backend` permet d'override le backend par défaut configuré dans `apollia.toml`.
    ///
    /// Retourne un awaitable Python qui résout en `LlmResponse`.
    #[pyo3(signature = (system, user, backend = None))]
    fn chat<'py>(
        &self,
        py: Python<'py>,
        system: String,
        user: String,
        backend: Option<String>,
    ) -> PyResult<Bound<'py, PyAny>> {
        // ADR-088 — émettre LlmCallStarted avant le dispatch.
        let prompt_chars = (system.chars().count() + user.chars().count()) as u64;
        let backend_label = backend
            .as_deref()
            .unwrap_or_else(|| self.router.default_name())
            .to_string();
        self.emit_started(&backend_label, "<resolved-by-router>", 2, prompt_chars);

        let router = Arc::clone(&self.router);
        let obs = Arc::clone(&self.obs_config);
        let bus = self.event_bus.clone();

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            // Inject the authoritative current-date / system block at the
            // top of the system message so the LLM never falls back to its
            // training cutoff for "today"-type queries. Mirrors what
            // Chat Libre's `build_system_prompt` does (ADR-096 Step 0).
            let system_with_context =
                apollia_core::temporal_context::prepend_temporal_context(&system);
            let req = CompletionRequest {
                messages: vec![
                    ChatMessage::system(system_with_context),
                    ChatMessage::user(user),
                ],
                ..Default::default()
            };
            let resp = router
                .complete_with_observability(backend.as_deref(), req, bus.as_ref(), &obs)
                .await
                .map_err(llm_err_to_py)?;

            Python::with_gil(|py| {
                let usage = PyTokenUsage {
                    prompt_tokens: resp.usage.prompt_tokens,
                    completion_tokens: resp.usage.completion_tokens,
                    cost_usd: resp.usage.cost_usd,
                };
                let py_resp = PyLlmResponse {
                    content: resp.content,
                    latency_ms: resp.latency_ms,
                    usage,
                };
                Py::new(py, py_resp).map(|p| p.into_any())
            })
        })
    }

    /// Appel LLM avec historique multi-tour complet.
    ///
    /// `messages` est une liste de dicts `{"role": "system"|"user"|"assistant"|"tool", "content": "..."}`.
    /// Retourne un awaitable Python qui résout en `LlmResponse`.
    ///
    /// Retourne `PyValueError` si un dict de message est invalide (role manquant ou inconnu).
    #[pyo3(signature = (messages, backend = None))]
    fn complete<'py>(
        &self,
        py: Python<'py>,
        messages: Vec<PyObject>,
        backend: Option<String>,
    ) -> PyResult<Bound<'py, PyAny>> {
        // Conversion synchrone avant de passer la frontière async
        let chat_messages = messages
            .iter()
            .map(|obj| py_dict_to_chat_message(py, obj))
            .collect::<PyResult<Vec<_>>>()?;

        // ADR-088 — émettre LlmCallStarted avant le dispatch.
        // MessageContent peut porter du texte simple ou des tool_calls ; on
        // additionne les chars comptables pour donner un proxy de taille.
        let prompt_chars: u64 = chat_messages
            .iter()
            .map(|m| match &m.content {
                apollia_llm::types::MessageContent::Text(t) => t.chars().count() as u64,
                apollia_llm::types::MessageContent::ToolResult { content, .. } => {
                    content.chars().count() as u64
                }
                apollia_llm::types::MessageContent::WithToolCalls { text, .. } => {
                    text.chars().count() as u64
                }
            })
            .sum();
        let backend_label = backend
            .as_deref()
            .unwrap_or_else(|| self.router.default_name())
            .to_string();
        self.emit_started(
            &backend_label,
            "<resolved-by-router>",
            chat_messages.len() as u32,
            prompt_chars,
        );

        let router = Arc::clone(&self.router);
        let obs = Arc::clone(&self.obs_config);
        let bus = self.event_bus.clone();

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            // Same temporal-context injection as `chat()`. If the first
            // message is a system message, prepend the block to its text.
            // Otherwise insert a fresh system message at index 0. Keeps
            // multi-turn agent calls grounded on the real wall clock.
            let messages = inject_temporal_context_into_messages(chat_messages);
            let req = CompletionRequest {
                messages,
                ..Default::default()
            };
            let resp = router
                .complete_with_observability(backend.as_deref(), req, bus.as_ref(), &obs)
                .await
                .map_err(llm_err_to_py)?;

            Python::with_gil(|py| {
                let usage = PyTokenUsage {
                    prompt_tokens: resp.usage.prompt_tokens,
                    completion_tokens: resp.usage.completion_tokens,
                    cost_usd: resp.usage.cost_usd,
                };
                let py_resp = PyLlmResponse {
                    content: resp.content,
                    latency_ms: resp.latency_ms,
                    usage,
                };
                Py::new(py, py_resp).map(|p| p.into_any())
            })
        })
    }

    /// Boucle ReAct automatique : LLM → outil(s) → LLM → ... → réponse finale.
    ///
    /// `messages` : liste de dicts `{"role": "...", "content": "..."}`.
    /// `tools` : liste de dicts `{"name": "...", "description": "...", "parameters": {...}}`.
    /// `max_iterations` : garde-fou — nombre maximal d'appels LLM (défaut : 5).
    ///
    /// Retourne un awaitable Python qui résout en `str` (réponse finale du LLM après
    /// que tous les appels d'outils aient été exécutés).
    ///
    /// Lève `PyRuntimeError` si le budget d'agent est épuisé ou si
    /// `max_iterations` est atteint.
    #[pyo3(signature = (messages, tools, max_iterations = 5))]
    fn run_tools<'py>(
        &self,
        py: Python<'py>,
        messages: Vec<PyObject>,
        tools: Vec<PyObject>,
        max_iterations: u32,
    ) -> PyResult<Bound<'py, PyAny>> {
        let chat_messages = messages
            .iter()
            .map(|obj| py_dict_to_chat_message(py, obj))
            .collect::<PyResult<Vec<_>>>()?;

        let tool_specs = tools
            .iter()
            .map(|obj| py_dict_to_tool_spec(py, obj))
            .collect::<PyResult<Vec<_>>>()?;

        let helper = Arc::clone(&self.tool_helper);
        let budget = Arc::clone(&self.budget_view);

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let result = helper
                .run_tools(chat_messages, tool_specs, max_iterations, &budget)
                .await
                .map_err(llm_err_to_py)?;

            Python::with_gil(|py| Ok(result.into_pyobject(py).unwrap().into_any().unbind()))
        })
    }

    /// Streaming LLM call — returns an async iterator yielding text chunks.
    ///
    /// Canonical streaming surface. Returns a [`PyTokenStream`]
    /// implementing the async iteration protocol (`__aiter__` / `__anext__`).
    /// Each chunk is yielded as soon as it arrives from the backend so the
    /// agent can emit real-time `ChatToken` events.
    ///
    /// Backends that cannot stream natively fall back to a single
    /// `complete()` call yielded as one chunk.
    ///
    /// # Example
    /// ```python
    /// async for chunk in await ctx.llm.stream(messages):
    ///     buffer += chunk
    ///     await ctx.emit_token(chunk)  # route to frontend
    /// ```
    #[pyo3(signature = (messages, backend = None))]
    fn stream<'py>(
        &self,
        py: Python<'py>,
        messages: Vec<PyObject>,
        backend: Option<String>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let chat_messages = messages
            .iter()
            .map(|obj| py_dict_to_chat_message(py, obj))
            .collect::<PyResult<Vec<_>>>()?;

        let router = Arc::clone(&self.router);
        let obs = Arc::clone(&self.obs_config);
        let bus = self.event_bus.clone();

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let (tx, rx) = mpsc::unbounded_channel::<Result<String, LlmError>>();

            // Background task : streame depuis le backend et forward sur tx.
            tokio::spawn(async move {
                let backend_key = backend.as_deref();
                let backend_model = router.get(backend_key);

                match backend_model {
                    Some(model) => {
                        let req = CompletionRequest {
                            messages: chat_messages.clone(),
                            ..Default::default()
                        };
                        match model.stream(req).await {
                            Ok(mut stream) => {
                                while let Some(chunk) = stream.next().await {
                                    match chunk {
                                        Ok(StreamChunk::Text(text)) => {
                                            if tx.send(Ok(text)).is_err() {
                                                break;
                                            }
                                        }
                                        Ok(StreamChunk::ToolCall(_)) => {
                                            // Tool calls within stream not surfaced
                                            // to Python — assistants parse JSON
                                            // from the accumulated text.
                                        }
                                        Err(e) => {
                                            let _ = tx.send(Err(e));
                                            break;
                                        }
                                    }
                                }
                            }
                            // Fallback : backend ne supporte pas stream()
                            // → appel complete() et forward en un seul chunk.
                            Err(_) => {
                                let fallback_req = CompletionRequest {
                                    messages: chat_messages,
                                    ..Default::default()
                                };
                                match router
                                    .complete_with_observability(
                                        backend_key,
                                        fallback_req,
                                        bus.as_ref(),
                                        &obs,
                                    )
                                    .await
                                {
                                    Ok(resp) => {
                                        let _ = tx.send(Ok(resp.content));
                                    }
                                    Err(e) => {
                                        let _ = tx.send(Err(e));
                                    }
                                }
                            }
                        }
                    }
                    None => {
                        let _ = tx.send(Err(LlmError::BackendUnavailable {
                            backend: backend_key.unwrap_or("(default)").to_string(),
                            reason: "no matching backend".to_string(),
                        }));
                    }
                }
                // Le drop de tx ferme le channel → __anext__ lève
                // PyStopAsyncIteration.
            });

            Python::with_gil(|py| {
                let stream = PyTokenStream {
                    rx: Arc::new(AsyncMutex::new(rx)),
                };
                Py::new(py, stream).map(|p| p.into_any())
            })
        })
    }

}

// ─────────────────────────────────────────────
// PyTokenStream — async iterator Python exposant un stream LLM chunk-par-chunk
// ─────────────────────────────────────────────

/// Async iterator Python qui yield des chunks texte d'un appel LLM streamé.
///
/// Implémente le protocole `__aiter__` + `__anext__`. Lève
/// `StopAsyncIteration` quand le stream est épuisé, `RuntimeError` sur
/// erreur backend.
#[pyclass(name = "TokenStream")]
pub struct PyTokenStream {
    rx: Arc<AsyncMutex<mpsc::UnboundedReceiver<Result<String, LlmError>>>>,
}

#[pymethods]
impl PyTokenStream {
    fn __aiter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __anext__<'py>(slf: PyRef<'py, Self>, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
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

// ─────────────────────────────────────────────
// Fonctions de conversion privées
// ─────────────────────────────────────────────

/// Inject the authoritative temporal/environment block into a multi-turn
/// message list. If the first message is a `system` role, prepend the block
/// to its text. Otherwise insert a brand-new `system` message at index 0.
///
/// Keeps Python agents on the real wall clock for "today"/"now"/relative
/// dates without requiring the agent author to remember anything (ADR-096
/// Step 0).
fn inject_temporal_context_into_messages(mut messages: Vec<ChatMessage>) -> Vec<ChatMessage> {
    use apollia_llm::types::{MessageContent, Role};

    let is_first_system = messages
        .first()
        .map(|m| matches!(m.role, Role::System))
        .unwrap_or(false);

    if is_first_system {
        if let Some(first) = messages.first_mut() {
            if let MessageContent::Text(ref text) = first.content {
                let merged =
                    apollia_core::temporal_context::prepend_temporal_context(text);
                first.content = MessageContent::Text(merged);
            }
            // Non-text system content (rare) — leave untouched and prepend
            // a fresh system block ahead of it as a safety net.
        }
    } else {
        messages.insert(
            0,
            ChatMessage::system(
                apollia_core::temporal_context::temporal_context_block(),
            ),
        );
    }
    messages
}

/// Convertit un dict Python `{"role": "...", "content": ...}` en `ChatMessage`.
///
/// `content` peut être :
/// - une `str` (fast path texte) ;
/// - une `list[dict]` de blocs typés (vision typing) — chaque bloc est
///   soit `{"type": "text", "text": "..."}`, soit
///   `{"type": "image", "source": {"type": "base64"|"url", ...}}`.
///   Les blocs texte sont concaténés (jointure par `\n\n`). Les blocs image
///   sont annotés en `[image: <media_type|url>]` car aucun backend LLM
///   accessible via `LlmProxy` ne supporte la vision aujourd'hui
///   (llama-cpp-2 = text-only — cf. mémoire `project_local_llm_engine`).
///   Le shape JSON est conservé tel que reçu pour qu'un futur backend cloud
///   vision puisse être branché sans casser l'API.
///
/// Retourne `PyValueError` si `role` est absent ou ne correspond à aucun rôle connu
/// (`system` / `user` / `assistant` / `tool`).
fn py_dict_to_chat_message(py: Python<'_>, obj: &PyObject) -> PyResult<ChatMessage> {
    let bound = obj.bind(py);

    let role_str: String = bound
        .get_item("role")
        .map_err(|_| PyValueError::new_err("message dict missing 'role' key"))?
        .extract()
        .map_err(|e| PyValueError::new_err(format!("'role' must be a str: {e}")))?;

    let content_obj = bound
        .get_item("content")
        .map_err(|_| PyValueError::new_err("message dict missing 'content' key"))?;

    // Fast path: plain string content.
    let content_str: String = match content_obj.extract::<String>() {
        Ok(s) => s,
        Err(_) => {
            // Slow path: list[MessageContent] — flatten to text representation.
            flatten_multimodal_content(py, &content_obj)?
        }
    };

    let role = match role_str.as_str() {
        "system" => Role::System,
        "user" => Role::User,
        "assistant" => Role::Assistant,
        "tool" => Role::Tool,
        other => {
            return Err(PyValueError::new_err(format!(
                "unknown role '{other}' — expected system/user/assistant/tool"
            )))
        }
    };

    Ok(ChatMessage {
        role,
        content: MessageContent::Text(content_str),
        cache_control: None,
    })
}

/// Flattens a list of multimodal content blocks (TextContent / ImageContent)
/// to a textual representation suitable for text-only backends.
///
/// - `{"type": "text", "text": "..."}` ⇒ raw text fragment.
/// - `{"type": "image", "source": {...}}` ⇒ `[image: <descriptor>]` marker
///   where descriptor is the `media_type` for base64 sources or the truncated
///   URL for URL sources.
///
/// Returns `PyValueError` if a block is malformed (missing `type`, unknown
/// kind, or non-iterable input).
fn flatten_multimodal_content(py: Python<'_>, content: &Bound<'_, PyAny>) -> PyResult<String> {
    use pyo3::types::PyList;

    if !content.is_instance_of::<PyList>() {
        return Err(PyValueError::new_err(
            "'content' must be either a str or a list of content blocks",
        ));
    }
    let list = content.downcast::<PyList>().map_err(|e| {
        PyValueError::new_err(format!("'content' is not a valid list: {e}"))
    })?;

    let json_mod = py
        .import("json")
        .map_err(|e| PyRuntimeError::new_err(format!("import json: {e}")))?;

    let mut fragments: Vec<String> = Vec::with_capacity(list.len());
    for item in list.iter() {
        // Each block must be a dict with a 'type' key.
        let type_str: String = item
            .get_item("type")
            .map_err(|_| PyValueError::new_err("content block missing 'type' key"))?
            .extract()
            .map_err(|e| PyValueError::new_err(format!("'type' must be a str: {e}")))?;

        match type_str.as_str() {
            "text" => {
                let text: String = item
                    .get_item("text")
                    .map_err(|_| PyValueError::new_err("text block missing 'text' key"))?
                    .extract()
                    .map_err(|e| PyValueError::new_err(format!("'text' must be a str: {e}")))?;
                fragments.push(text);
            }
            "image" => {
                // Pull a short descriptor from `source` for the placeholder. The
                // full JSON is also captured (truncated) so a future vision-capable
                // backend can reconstruct the payload if we expose a richer
                // MessageContent variant later.
                let source = item.get_item("source").map_err(|_| {
                    PyValueError::new_err("image block missing 'source' key")
                })?;
                let descriptor = source
                    .get_item("media_type")
                    .ok()
                    .and_then(|v| v.extract::<String>().ok())
                    .or_else(|| {
                        source
                            .get_item("url")
                            .ok()
                            .and_then(|v| v.extract::<String>().ok())
                            .map(|u| truncate(&u, 80))
                    })
                    .unwrap_or_else(|| "unknown".to_string());
                fragments.push(format!("[image: {descriptor}]"));
                // Tracing — let operators discover that an image was sent to a
                // text-only backend so they can switch to a cloud vision model.
                tracing::warn!(
                    media = %descriptor,
                    "vision content received by LlmProxy — current backends are text-only, \
                     image was flattened to a placeholder"
                );
                // Drop the dumped JSON if we ever need it for debugging.
                let _ = json_mod;
            }
            other => {
                return Err(PyValueError::new_err(format!(
                    "unknown content block type '{other}' — expected 'text' or 'image'"
                )));
            }
        }
    }

    Ok(fragments.join("\n\n"))
}

/// Truncates a string to at most `max_chars` characters with an ellipsis.
fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max_chars).collect();
    out.push('…');
    out
}

/// Convertit un dict Python `{"name": "...", "description": "...", "parameters": {...}}`
/// en `ToolSpec`.
///
/// `parameters` est optionnel — défaut `{}` si absent.
/// Retourne `PyValueError` si `name` ou `description` sont manquants.
fn py_dict_to_tool_spec(py: Python<'_>, obj: &PyObject) -> PyResult<ToolSpec> {
    let bound = obj.bind(py);

    let name: String = bound
        .get_item("name")
        .map_err(|_| PyValueError::new_err("tool spec dict missing 'name' key"))?
        .extract()
        .map_err(|e| PyValueError::new_err(format!("'name' must be a str: {e}")))?;

    let description: String = bound
        .get_item("description")
        .map_err(|_| PyValueError::new_err("tool spec dict missing 'description' key"))?
        .extract()
        .map_err(|e| PyValueError::new_err(format!("'description' must be a str: {e}")))?;

    // `parameters` est optionnel — on sérialise via json.dumps pour gérer
    // n'importe quel type Python (dict, list, etc.)
    let parameters: serde_json::Value = match bound.get_item("parameters") {
        Ok(params_obj) => {
            let json_mod = py
                .import("json")
                .map_err(|e| PyRuntimeError::new_err(format!("import json: {e}")))?;
            let json_str: String = json_mod
                .call_method1("dumps", (params_obj,))
                .map_err(|e| PyRuntimeError::new_err(format!("json.dumps failed: {e}")))?
                .extract()
                .map_err(|e| PyRuntimeError::new_err(format!("extract failed: {e}")))?;
            serde_json::from_str(&json_str)
                .map_err(|e| PyValueError::new_err(format!("parameters JSON parse: {e}")))?
        }
        Err(_) => serde_json::json!({}),
    };

    Ok(ToolSpec {
        name,
        description,
        parameters,
    })
}

/// Mappe un [`LlmError`] vers une `PyRuntimeError`.
fn llm_err_to_py(e: LlmError) -> PyErr {
    PyRuntimeError::new_err(e.to_string())
}

// ─────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// `py_dict_to_chat_message` — role "user" converti correctement.
    #[test]
    fn test_py_dict_to_chat_message_user() {
        // GIVEN
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            let dict = pyo3::types::PyDict::new(py);
            dict.set_item("role", "user").unwrap();
            dict.set_item("content", "bonjour").unwrap();
            // WHEN
            let msg = py_dict_to_chat_message(py, &dict.into()).unwrap();
            // THEN
            assert_eq!(msg.role, Role::User);
            assert!(matches!(msg.content, MessageContent::Text(ref t) if t == "bonjour"));
        });
    }

    /// role invalide → `PyValueError`.
    #[test]
    fn test_py_dict_to_chat_message_invalid_role() {
        // GIVEN
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            let dict = pyo3::types::PyDict::new(py);
            dict.set_item("role", "invalid_role").unwrap();
            dict.set_item("content", "...").unwrap();
            // WHEN
            let result = py_dict_to_chat_message(py, &dict.into());
            // THEN
            assert!(result.is_err());
            let err = result.unwrap_err();
            assert!(err.to_string().contains("unknown role"));
        });
    }

    /// `py_dict_to_tool_spec` — champs extraits correctement.
    #[test]
    fn test_py_dict_to_tool_spec() {
        // GIVEN
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            let dict = pyo3::types::PyDict::new(py);
            dict.set_item("name", "file_io").unwrap();
            dict.set_item("description", "lit un fichier").unwrap();
            dict.set_item("parameters", pyo3::types::PyDict::new(py))
                .unwrap();
            // WHEN
            let spec = py_dict_to_tool_spec(py, &dict.into()).unwrap();
            // THEN
            assert_eq!(spec.name, "file_io");
            assert_eq!(spec.description, "lit un fichier");
        });
    }

    /// clé "role" absente → `PyValueError`.
    #[test]
    fn test_py_dict_to_chat_message_missing_role() {
        // GIVEN
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            let dict = pyo3::types::PyDict::new(py);
            dict.set_item("content", "test").unwrap();
            // WHEN — pas de clé "role"
            let result = py_dict_to_chat_message(py, &dict.into());
            // THEN
            assert!(result.is_err());
        });
    }

    /// role "system" converti correctement.
    #[test]
    fn test_py_dict_to_chat_message_system() {
        // GIVEN
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            let dict = pyo3::types::PyDict::new(py);
            dict.set_item("role", "system").unwrap();
            dict.set_item("content", "tu es utile").unwrap();
            // WHEN
            let msg = py_dict_to_chat_message(py, &dict.into()).unwrap();
            // THEN
            assert_eq!(msg.role, Role::System);
        });
    }

    /// clé "name" absente dans tool spec → `PyValueError`.
    #[test]
    fn test_py_dict_to_tool_spec_missing_name() {
        // GIVEN
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            let dict = pyo3::types::PyDict::new(py);
            dict.set_item("description", "test").unwrap();
            // WHEN — pas de clé "name"
            let result = py_dict_to_tool_spec(py, &dict.into());
            // THEN
            assert!(result.is_err());
        });
    }

    /// Multi-modal content list with a text block is accepted and flattened.
    #[test]
    fn test_py_dict_to_chat_message_multimodal_text_only() {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            // GIVEN a message whose `content` is a list[TextContent]
            let dict = pyo3::types::PyDict::new(py);
            dict.set_item("role", "user").unwrap();
            let text_block = pyo3::types::PyDict::new(py);
            text_block.set_item("type", "text").unwrap();
            text_block.set_item("text", "describe").unwrap();
            let parts = pyo3::types::PyList::new(py, &[text_block]).unwrap();
            dict.set_item("content", parts).unwrap();

            // WHEN converted
            let msg = py_dict_to_chat_message(py, &dict.into()).unwrap();

            // THEN the text fragment survives intact
            assert_eq!(msg.role, Role::User);
            assert!(matches!(msg.content, MessageContent::Text(ref t) if t == "describe"));
        });
    }

    /// Image content blocks are flattened to a placeholder.
    #[test]
    fn test_py_dict_to_chat_message_multimodal_with_image() {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            // GIVEN a message with [text, image_base64]
            let dict = pyo3::types::PyDict::new(py);
            dict.set_item("role", "user").unwrap();

            let text_block = pyo3::types::PyDict::new(py);
            text_block.set_item("type", "text").unwrap();
            text_block.set_item("text", "what is in this picture?").unwrap();

            let source = pyo3::types::PyDict::new(py);
            source.set_item("type", "base64").unwrap();
            source.set_item("media_type", "image/png").unwrap();
            source.set_item("data", "AAAA").unwrap();
            let image_block = pyo3::types::PyDict::new(py);
            image_block.set_item("type", "image").unwrap();
            image_block.set_item("source", source).unwrap();

            let parts = pyo3::types::PyList::new(py, &[text_block, image_block]).unwrap();
            dict.set_item("content", parts).unwrap();

            // WHEN converted
            let msg = py_dict_to_chat_message(py, &dict.into()).unwrap();

            // THEN the image is annotated as a placeholder alongside the text
            assert_eq!(msg.role, Role::User);
            let text = match msg.content {
                MessageContent::Text(t) => t,
                _ => panic!("expected text content"),
            };
            assert!(text.contains("what is in this picture?"));
            assert!(text.contains("[image: image/png]"));
        });
    }

    /// Unknown block type raises PyValueError.
    #[test]
    fn test_py_dict_to_chat_message_unknown_block_type() {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            let dict = pyo3::types::PyDict::new(py);
            dict.set_item("role", "user").unwrap();
            let bogus = pyo3::types::PyDict::new(py);
            bogus.set_item("type", "video").unwrap();
            let parts = pyo3::types::PyList::new(py, &[bogus]).unwrap();
            dict.set_item("content", parts).unwrap();

            let result = py_dict_to_chat_message(py, &dict.into());
            assert!(result.is_err());
            let err = result.unwrap_err();
            assert!(err.to_string().contains("unknown content block type"));
        });
    }

    /// `parameters` absent → défaut `{}`.
    #[test]
    fn test_py_dict_to_tool_spec_parameters_optional() {
        // GIVEN
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            let dict = pyo3::types::PyDict::new(py);
            dict.set_item("name", "echo").unwrap();
            dict.set_item("description", "echo input").unwrap();
            // WHEN — pas de clé "parameters"
            let spec = py_dict_to_tool_spec(py, &dict.into()).unwrap();
            // THEN
            assert_eq!(spec.parameters, serde_json::json!({}));
        });
    }
}
