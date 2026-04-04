//! `LlmProxy` — bridge PyO3 exposant le moteur LLM à Python via `ctx.llm`.
//!
//! Pattern ADR-014 : toutes les méthodes async
//! utilisent `pyo3_async_runtimes::tokio::future_into_py`.
//!
//! Les tests nécessitant un environnement Python complet
//! sont couverts avec `#[cfg(feature = "python-tests")]`.

use std::sync::Arc;

use futures::StreamExt;
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;

use apollia_core::events::EventBusSender;
use apollia_llm::{
    ChatMessage, CompletionRequest, LlmError, LlmRouter, MessageContent, ObservabilityConfig, Role,
    StepBudgetView, ToolCallHelper, ToolSpec,
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
/// # Cas 3 — Streaming (retourne list[str])
/// chunks = await ctx.llm.stream([{"role": "user", "content": "..."}])
/// for chunk in chunks:
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
        let router = Arc::clone(&self.router);
        let obs = Arc::clone(&self.obs_config);
        let bus = self.event_bus.clone();

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let req = CompletionRequest {
                messages: vec![ChatMessage::system(system), ChatMessage::user(user)],
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

        let router = Arc::clone(&self.router);
        let obs = Arc::clone(&self.obs_config);
        let bus = self.event_bus.clone();

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let req = CompletionRequest {
                messages: chat_messages,
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

    /// Appel LLM en mode streaming — retourne une liste de chunks texte.
    ///
    /// MVP : collecte tous les chunks du stream Rust et les retourne comme `list[str]`.
    /// Fallback : si le backend ne supporte pas le streaming nativement,
    /// appelle `complete()` et retourne le contenu complet comme liste à un seul élément
    /// sans lever d'erreur.
    ///
    /// Retourne un awaitable Python qui résout en `list[str]`.
    ///
    /// # Exemple Python
    /// ```python
    /// chunks = await ctx.llm.stream([{"role": "user", "content": "..."}])
    /// for chunk in chunks:
    ///     await ctx.emit.text(chunk)
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
            let backend_key = backend.as_deref();
            let backend_model = router.get(backend_key);

            let chunks: Vec<String> = match backend_model {
                Some(model) => {
                    let req = CompletionRequest {
                        messages: chat_messages.clone(),
                        ..Default::default()
                    };
                    match model.stream(req).await {
                        Ok(mut stream) => {
                            let mut collected: Vec<String> = Vec::new();
                            while let Some(chunk) = stream.next().await {
                                match chunk {
                                    Ok(apollia_llm::StreamChunk::Text(text)) => {
                                        collected.push(text);
                                    }
                                    Ok(apollia_llm::StreamChunk::ToolCall(_)) => {
                                        // Tool calls in stream not handled in Python bridge
                                    }
                                    Err(_) => break,
                                }
                            }
                            collected
                        }
                        // fallback: stream() not supported → single complete()
                        Err(_) => {
                            let fallback_req = CompletionRequest {
                                messages: chat_messages,
                                ..Default::default()
                            };
                            let resp = router
                                .complete_with_observability(
                                    backend_key,
                                    fallback_req,
                                    bus.as_ref(),
                                    &obs,
                                )
                                .await
                                .map_err(llm_err_to_py)?;
                            vec![resp.content]
                        }
                    }
                }
                // fallback: backend not found → single complete() (may return BackendUnavailable)
                None => {
                    let fallback_req = CompletionRequest {
                        messages: chat_messages,
                        ..Default::default()
                    };
                    let resp = router
                        .complete_with_observability(backend_key, fallback_req, bus.as_ref(), &obs)
                        .await
                        .map_err(llm_err_to_py)?;
                    vec![resp.content]
                }
            };

            Python::with_gil(|py| {
                let py_list = pyo3::types::PyList::new(py, &chunks).unwrap();
                Ok(py_list.into_any().unbind())
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
}

// ─────────────────────────────────────────────
// Fonctions de conversion privées
// ─────────────────────────────────────────────

/// Convertit un dict Python `{"role": "...", "content": "..."}` en `ChatMessage`.
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

    let content_str: String = bound
        .get_item("content")
        .map_err(|_| PyValueError::new_err("message dict missing 'content' key"))?
        .extract()
        .map_err(|e| PyValueError::new_err(format!("'content' must be a str: {e}")))?;

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
