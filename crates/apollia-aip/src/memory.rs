//! MemoryInterface: Python-facing proxy for agent memory operations.
//!
//! Exposes a `#[pyclass]` that agents use via `ctx.memory.record()`,
//! `ctx.memory.remember()`, `ctx.memory.recall()`, `ctx.memory.search()`,
//! and `ctx.memory.forget()`.
//!
//! Memory is recalled at the agent's initiative: there is no automatic injection.

use std::sync::{Arc, Mutex};

use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::PyDict;

use apollia_core::events::{AgentId, EventBusSender, RuntimeEvent};
use apollia_memory::episodic::EpisodicMemory;
use apollia_memory::export::{export_namespace, import_namespace, ImportMode, MemoryExport};
use apollia_memory::injection_tracker::{global_record, preview, InjectedEntry};
use apollia_memory::manager::{MemoryAccess, MemoryManager};
use apollia_memory::procedural::ProceduralMemory;
use apollia_memory::search::{MemorySearch, SearchQuery, SearchSource};
use apollia_memory::semantic::{RememberInput, SemanticMemory};

/// Errors from memory operations via the Python proxy.
#[derive(Debug, thiserror::Error)]
pub enum MemoryInterfaceError {
    /// A memory operation failed.
    #[error("memory operation failed: {0}")]
    OperationFailed(String),

    /// The namespace is read-only (shared namespace).
    #[error("namespace is read-only: {0}")]
    ReadOnly(String),

    /// No memory namespace configured for this agent.
    #[error("no memory namespace configured")]
    NoNamespace,
}

/// Python-facing interface for agent memory operations.
///
/// Each agent receives its own `MemoryInterface` configured with
/// its namespace. Write operations are only allowed on the agent's
/// primary namespace (ReadWrite). Shared namespaces are read-only.
///
/// Reads and writes are scoped to the agent's own namespace.  Access to the
/// global user profile (`__user__`) is handled separately via
/// `ctx.profile` ([`crate::profile::ProfileInterface`]).
#[pyclass]
pub struct MemoryInterface {
    manager: Arc<Mutex<MemoryManager>>,
    namespace: String,
    agent_id: String,
    /// Current turn id, set by the runtime before each agent turn so that
    /// `recall_entry()` / `recall_all()` can record their results into the
    /// global injection tracker.
    ///
    /// `None` outside of a turn (e.g. during bootstrap); injections are then
    /// not tracked.
    current_turn_id: Arc<Mutex<Option<String>>>,
}

#[pymethods]
impl MemoryInterface {
    /// Records a memory in episodic memory.
    ///
    /// importance: score between 0.0 and 1.0 (default 0.5)
    /// task_id: id of the current task (optional)
    /// metadata: Python dict of arbitrary metadata (optional)
    /// expires_in: lifetime in seconds; `None` = no expiration
    #[pyo3(signature = (content, importance=None, task_id=None, metadata=None, expires_in=None))]
    fn record<'py>(
        &self,
        py: Python<'py>,
        content: String,
        importance: Option<f64>,
        task_id: Option<String>,
        metadata: Option<Bound<'py, PyDict>>,
        expires_in: Option<u64>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let manager = Arc::clone(&self.manager);
        let namespace = self.namespace.clone();
        let agent_id = self.agent_id.clone();
        let importance = importance.unwrap_or(0.5);
        let metadata_json = metadata.map(|d| pydict_to_json(&d)).transpose()?;

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let result = tokio::task::spawn_blocking(move || {
                record_inner(
                    &manager,
                    &namespace,
                    &agent_id,
                    &content,
                    importance,
                    task_id.as_deref(),
                    metadata_json,
                    expires_in,
                )
            })
            .await
            .map_err(|e| PyRuntimeError::new_err(format!("spawn_blocking failed: {e}")))?;

            result.map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
            Ok(Python::with_gil(|py| py.None()))
        })
    }

    /// Stores a key/value pair in semantic memory.
    ///
    /// source: provenance of the information (optional)
    /// confidence: score between 0.0 and 1.0 (default 1.0).
    ///     When provided, an existing entry with strictly higher confidence
    ///     is preserved (no overwrite).
    #[pyo3(signature = (key, value, source=None, confidence=None))]
    fn remember<'py>(
        &self,
        py: Python<'py>,
        key: String,
        value: String,
        source: Option<String>,
        confidence: Option<f64>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let manager = Arc::clone(&self.manager);
        let namespace = self.namespace.clone();

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let result = tokio::task::spawn_blocking(move || {
                remember_inner(
                    &manager,
                    &namespace,
                    RememberArgs {
                        key: &key,
                        value: &value,
                        source: source.as_deref(),
                        confidence,
                    },
                )
            })
            .await
            .map_err(|e| PyRuntimeError::new_err(format!("spawn_blocking failed: {e}")))?;

            result.map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
            Ok(Python::with_gil(|py| py.None()))
        })
    }

    /// Retrieves a value by key from the agent's semantic memory namespace.
    ///
    /// Returns the value (str) or `None` if the key does not exist.  Reads
    /// are isolated to the agent's own namespace; the global user profile
    /// is exposed separately via `ctx.profile`.
    fn recall<'py>(&self, py: Python<'py>, key: String) -> PyResult<Bound<'py, PyAny>> {
        let manager = Arc::clone(&self.manager);
        let namespace = self.namespace.clone();

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let result =
                tokio::task::spawn_blocking(move || recall_inner(&manager, &namespace, &key))
                    .await
                    .map_err(|e| PyRuntimeError::new_err(format!("spawn_blocking failed: {e}")))?;

            match result {
                Ok(Some(value)) => Ok(Python::with_gil(|py| {
                    value.into_pyobject(py).unwrap().into_any().unbind()
                })),
                Ok(None) => Ok(Python::with_gil(|py| py.None())),
                Err(e) => Err(PyRuntimeError::new_err(e.to_string())),
            }
        })
    }

    /// Full-text search across agent memory.
    ///
    /// Returns a list of dicts {content, score, source, timestamp}.
    /// limit: max number of results (default 10)
    #[pyo3(signature = (query, limit=None))]
    fn search<'py>(
        &self,
        py: Python<'py>,
        query: String,
        limit: Option<usize>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let manager = Arc::clone(&self.manager);
        let namespace = self.namespace.clone();
        let limit = limit.unwrap_or(10);

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let result = tokio::task::spawn_blocking(move || {
                search_inner(&manager, &namespace, &query, limit)
            })
            .await
            .map_err(|e| PyRuntimeError::new_err(format!("spawn_blocking failed: {e}")))?;

            match result {
                Ok(items) => Python::with_gil(|py| {
                    let json_mod = py
                        .import("json")
                        .map_err(|e| PyRuntimeError::new_err(format!("import json: {e}")))?;

                    let json_str = serde_json::to_string(&items)
                        .map_err(|e| PyRuntimeError::new_err(format!("serialize: {e}")))?;

                    let py_obj: PyObject = json_mod
                        .call_method1("loads", (json_str,))
                        .map_err(|e| PyRuntimeError::new_err(format!("json.loads: {e}")))?
                        .unbind();

                    Ok(py_obj)
                }),
                Err(e) => Err(PyRuntimeError::new_err(e.to_string())),
            }
        })
    }

    /// Retrieves a semantic entry with full metadata.
    ///
    /// Returns a dict with keys `{key, value, confidence, source, updated_at, expires_at}`
    /// or `None` if the key does not exist or is expired.
    #[pyo3(signature = (key, injection_reason=None))]
    fn recall_entry<'py>(
        &self,
        py: Python<'py>,
        key: String,
        injection_reason: Option<String>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let manager = Arc::clone(&self.manager);
        let namespace = self.namespace.clone();
        let turn_id = self.current_turn_id.lock().ok().and_then(|g| g.clone());
        let ns_for_log = namespace.clone();
        let key_for_log = key.clone();

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let result = tokio::task::spawn_blocking({
                let manager = Arc::clone(&manager);
                let namespace = namespace.clone();
                let key = key.clone();
                move || recall_entry_inner(&manager, &namespace, &key)
            })
            .await
            .map_err(|e| PyRuntimeError::new_err(format!("spawn_blocking failed: {e}")))?;

            if let (Some(turn), Ok(Some(ref json_val))) = (turn_id.as_ref(), result.as_ref()) {
                let reason = injection_reason
                    .clone()
                    .unwrap_or_else(|| format!("Matched query: {key_for_log}"));
                record_injected_entry(turn, &ns_for_log, json_val, reason, None);
            }

            match result {
                Ok(Some(json_val)) => Python::with_gil(|py| {
                    let json_mod = py
                        .import("json")
                        .map_err(|e| PyRuntimeError::new_err(format!("import json: {e}")))?;
                    let json_str = serde_json::to_string(&json_val)
                        .map_err(|e| PyRuntimeError::new_err(format!("serialize: {e}")))?;
                    let py_obj: PyObject = json_mod
                        .call_method1("loads", (json_str,))
                        .map_err(|e| PyRuntimeError::new_err(format!("json.loads: {e}")))?
                        .unbind();
                    Ok(py_obj)
                }),
                Ok(None) => Ok(Python::with_gil(|py| py.None())),
                Err(e) => Err(PyRuntimeError::new_err(e.to_string())),
            }
        })
    }

    /// Lists all semantic entries in the agent namespace.
    ///
    /// Returns `list[dict]` with the same structure as `recall_entry()`.
    /// `limit` caps the result count (defaults to 100).
    #[pyo3(signature = (limit=None, injection_reason=None))]
    fn recall_all<'py>(
        &self,
        py: Python<'py>,
        limit: Option<usize>,
        injection_reason: Option<String>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let manager = Arc::clone(&self.manager);
        let namespace = self.namespace.clone();
        let limit = limit.unwrap_or(100);
        let turn_id = self.current_turn_id.lock().ok().and_then(|g| g.clone());
        let ns_for_log = namespace.clone();

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let result = tokio::task::spawn_blocking({
                let manager = Arc::clone(&manager);
                let namespace = namespace.clone();
                move || recall_all_inner(&manager, &namespace, limit)
            })
            .await
            .map_err(|e| PyRuntimeError::new_err(format!("spawn_blocking failed: {e}")))?;

            if let (Some(turn), Ok(items)) = (turn_id.as_ref(), result.as_ref()) {
                let reason = injection_reason
                    .clone()
                    .unwrap_or_else(|| "Matched query: recall_all".to_string());
                for item in items.iter() {
                    record_injected_entry(turn, &ns_for_log, item, reason.clone(), None);
                }
            }

            match result {
                Ok(items) => Python::with_gil(|py| {
                    let json_mod = py
                        .import("json")
                        .map_err(|e| PyRuntimeError::new_err(format!("import json: {e}")))?;
                    let json_str = serde_json::to_string(&items)
                        .map_err(|e| PyRuntimeError::new_err(format!("serialize: {e}")))?;
                    let py_obj: PyObject = json_mod
                        .call_method1("loads", (json_str,))
                        .map_err(|e| PyRuntimeError::new_err(format!("json.loads: {e}")))?
                        .unbind();
                    Ok(py_obj)
                }),
                Err(e) => Err(PyRuntimeError::new_err(e.to_string())),
            }
        })
    }

    /// Recalls learned procedures matching the trigger (exact match).
    ///
    /// Returns a list of Python dicts `[{id, trigger, steps, success_count, ...}]`.
    /// Empty list if no procedure exists for this trigger.
    fn recall_procedure<'py>(
        &self,
        py: Python<'py>,
        trigger: String,
    ) -> PyResult<Bound<'py, PyAny>> {
        let manager = Arc::clone(&self.manager);
        let namespace = self.namespace.clone();

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let result = tokio::task::spawn_blocking(move || {
                recall_procedure_inner(&manager, &namespace, &trigger)
            })
            .await
            .map_err(|e| PyRuntimeError::new_err(format!("spawn_blocking failed: {e}")))?;

            match result {
                Ok(items) => Python::with_gil(|py| {
                    let json_mod = py
                        .import("json")
                        .map_err(|e| PyRuntimeError::new_err(format!("import json: {e}")))?;
                    let json_str = serde_json::to_string(&items)
                        .map_err(|e| PyRuntimeError::new_err(format!("serialize: {e}")))?;
                    let py_obj: PyObject = json_mod
                        .call_method1("loads", (json_str,))
                        .map_err(|e| PyRuntimeError::new_err(format!("json.loads: {e}")))?
                        .unbind();
                    Ok(py_obj)
                }),
                Err(e) => Err(PyRuntimeError::new_err(e.to_string())),
            }
        })
    }

    /// Records a procedure in the namespace's procedural memory.
    ///
    /// trigger: exact trigger (exact match for recall)
    /// steps: ordered list of steps
    /// Returns the UUID of the recorded procedure.
    /// Raises `ValueError` if `steps` is empty.
    fn learn_procedure<'py>(
        &self,
        py: Python<'py>,
        trigger: String,
        steps: Vec<String>,
    ) -> PyResult<Bound<'py, PyAny>> {
        if steps.is_empty() {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "steps must not be empty",
            ));
        }

        let manager = Arc::clone(&self.manager);
        let namespace = self.namespace.clone();

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let result = tokio::task::spawn_blocking(move || {
                learn_procedure_inner(&manager, &namespace, &trigger, &steps)
            })
            .await
            .map_err(|e| PyRuntimeError::new_err(format!("spawn_blocking failed: {e}")))?;

            match result {
                Ok(id) => Ok(Python::with_gil(|py| {
                    id.into_pyobject(py).unwrap().into_any().unbind()
                })),
                Err(e) => Err(PyRuntimeError::new_err(e.to_string())),
            }
        })
    }

    /// Removes a key/value pair from semantic memory.
    fn forget<'py>(&self, py: Python<'py>, key: String) -> PyResult<Bound<'py, PyAny>> {
        let manager = Arc::clone(&self.manager);
        let namespace = self.namespace.clone();

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let result =
                tokio::task::spawn_blocking(move || forget_inner(&manager, &namespace, &key))
                    .await
                    .map_err(|e| PyRuntimeError::new_err(format!("spawn_blocking failed: {e}")))?;

            result.map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
            Ok(Python::with_gil(|py| py.None()))
        })
    }

    /// Exports the agent's entire memory (episodic + semantic + procedural)
    /// as a JSON-serializable dict.
    ///
    /// The export covers **only** the calling agent's private namespace;
    /// shared (read-only) namespaces are not included. The returned structure
    /// matches [`apollia_memory::export::MemoryExport`] with
    /// `schema_version = 1` (alias `format_version` for compatibility):
    ///
    /// ```python
    /// dump = await ctx.memory.export()
    /// # dump == {
    /// #     "schema_version": 1,
    /// #     "namespace": "my-agent",
    /// #     "exported_at": "2026-05-19T10:00:00Z",
    /// #     "episodic": [...],
    /// #     "semantic": [...],
    /// #     "procedural": [...],
    /// # }
    /// ```
    fn export<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let manager = Arc::clone(&self.manager);
        let namespace = self.namespace.clone();

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let dump = tokio::task::spawn_blocking(move || export_inner(&manager, &namespace))
                .await
                .map_err(|e| PyRuntimeError::new_err(format!("spawn_blocking failed: {e}")))?
                .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;

            Python::with_gil(|py| json_value_to_py(py, &dump))
        })
    }

    /// Imports a dump produced by [`MemoryInterface::export`] into the
    /// agent's private namespace.
    ///
    /// `data` must be a dict following the `format_version = 1` schema (or its
    /// `schema_version` alias). The default mode is `merge`: entries whose
    /// `id` already exists are ignored (`INSERT OR IGNORE`). Pass
    /// `replace=True` to clear the namespace before importing.
    ///
    /// Returns the number of entries actually imported.
    #[pyo3(signature = (data, replace = false))]
    fn import_data<'py>(
        &self,
        py: Python<'py>,
        data: PyObject,
        replace: bool,
    ) -> PyResult<Bound<'py, PyAny>> {
        // Convert PyObject -> serde_json::Value synchronously (needs GIL).
        let dump_json = pyany_to_json(py, &data)?;

        let manager = Arc::clone(&self.manager);
        let namespace = self.namespace.clone();
        let mode = if replace {
            ImportMode::Replace
        } else {
            ImportMode::Merge
        };

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let count = tokio::task::spawn_blocking(move || {
                import_inner(&manager, &namespace, dump_json, mode)
            })
            .await
            .map_err(|e| PyRuntimeError::new_err(format!("spawn_blocking failed: {e}")))?
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;

            Python::with_gil(|py| Ok(count.into_pyobject(py).unwrap().into_any().unbind()))
        })
    }
}

impl MemoryInterface {
    /// Creates a new MemoryInterface for a given agent.
    ///
    /// Returns `None` if `namespace` is empty. Global user profile access
    /// is configured separately via [`crate::profile::ProfileInterface`].
    pub fn new(manager: MemoryManager, namespace: String, agent_id: String) -> Option<Self> {
        if namespace.is_empty() {
            return None;
        }
        Some(Self {
            manager: Arc::new(Mutex::new(manager)),
            namespace,
            agent_id,
            current_turn_id: Arc::new(Mutex::new(None)),
        })
    }

    /// Announces on the EventBus each shared namespace the agent can access.
    ///
    /// Emits one [`RuntimeEvent::SharedNamespaceAdded`] per shared namespace
    /// configured in the underlying `MemoryManager`. Called by the runtime
    /// right after the interface is built, once the list of allowed
    /// namespaces is fixed. `broadcast` errors are ignored (fire-and-forget:
    /// no receiver means no effect).
    pub fn announce_shared_namespaces(&self, bus: &EventBusSender) {
        let Ok(guard) = self.manager.lock() else {
            return;
        };
        for ns in guard.shared_namespaces() {
            let _ = bus.send(RuntimeEvent::SharedNamespaceAdded {
                agent_id: AgentId::from(self.agent_id.as_str()),
                namespace: ns.clone(),
            });
        }
    }

    /// Sets (or clears) the current turn id used to correlate injections.
    ///
    /// The runtime should call this at turn boundaries so that
    /// `recall_entry()` / `recall_all()` record into the correct turn. A
    /// `None` value clears the tracking window.
    pub fn set_turn_id(&self, turn_id: Option<String>) {
        if let Ok(mut guard) = self.current_turn_id.lock() {
            *guard = turn_id;
        }
    }

    /// Current turn id (test helper).
    #[doc(hidden)]
    pub fn current_turn_id(&self) -> Option<String> {
        self.current_turn_id.lock().ok().and_then(|g| g.clone())
    }
}

// ---------------------------------------------------------------------------
// Injection tracking helpers
// ---------------------------------------------------------------------------

/// Extracts `id`, textual `value`/`content`, and relevance fields from a
/// semantic/episodic JSON value and records them into the global injection
/// tracker.
///
/// Errors are swallowed (fire-and-forget): injection tracking must never
/// break an agent's recall path.
fn record_injected_entry(
    turn_id: &str,
    namespace: &str,
    json_val: &serde_json::Value,
    injection_reason: String,
    extra_score: Option<f32>,
) {
    let id = json_val
        .get("id")
        .and_then(|v| v.as_str())
        .or_else(|| json_val.get("key").and_then(|v| v.as_str()))
        .unwrap_or("")
        .to_string();
    if id.is_empty() {
        return;
    }

    let content = json_val
        .get("content")
        .and_then(|v| v.as_str())
        .or_else(|| json_val.get("value").and_then(|v| v.as_str()))
        .map(|s| s.to_string())
        .unwrap_or_else(|| json_val.to_string());

    let relevance_score = extra_score
        .or_else(|| {
            json_val
                .get("confidence")
                .and_then(|v| v.as_f64())
                .map(|v| v as f32)
        })
        .or_else(|| {
            json_val
                .get("importance")
                .and_then(|v| v.as_f64())
                .map(|v| v as f32)
        })
        .unwrap_or(0.0);

    global_record(
        turn_id,
        InjectedEntry {
            id,
            content_preview: preview(&content, 160),
            namespace: namespace.to_string(),
            injection_reason,
            relevance_score: relevance_score.clamp(0.0, 1.0),
        },
    );
}

// ---------------------------------------------------------------------------
// Pure Rust internals, testable without PyO3
// ---------------------------------------------------------------------------

/// Records an episodic event in the agent's namespace.
#[allow(clippy::too_many_arguments)]
fn record_inner(
    manager: &Arc<Mutex<MemoryManager>>,
    namespace: &str,
    agent_id: &str,
    content: &str,
    importance: f64,
    task_id: Option<&str>,
    metadata_json: Option<serde_json::Value>,
    expires_in: Option<u64>,
) -> Result<String, MemoryInterfaceError> {
    let mut mgr = lock(manager)?;
    check_write_access(&mgr, namespace)?;

    let store = mgr
        .store(namespace)
        .map_err(|e| MemoryInterfaceError::OperationFailed(e.to_string()))?;

    let expires_at = expires_in.map(expires_in_to_iso);

    let ep = EpisodicMemory::new(store);
    ep.record(
        namespace,
        agent_id,
        content,
        importance,
        task_id,
        expires_at.as_deref(),
        metadata_json.as_ref(),
    )
    .map_err(|e| MemoryInterfaceError::OperationFailed(e.to_string()))
}

/// Key/value payload for [`remember_inner`], grouped to avoid too many
/// function arguments.
struct RememberArgs<'a> {
    key: &'a str,
    value: &'a str,
    source: Option<&'a str>,
    confidence: Option<f64>,
}

/// Stores a key/value pair in semantic memory.
///
/// When `confidence` is `Some`, an existing entry with strictly higher
/// confidence is preserved: the write is silently skipped.
/// When `None`, defaults to 1.0 (backward-compatible unconditional upsert).
fn remember_inner(
    manager: &Arc<Mutex<MemoryManager>>,
    namespace: &str,
    args: RememberArgs<'_>,
) -> Result<String, MemoryInterfaceError> {
    let RememberArgs {
        key,
        value,
        source,
        confidence,
    } = args;
    let mut mgr = lock(manager)?;
    check_write_access(&mgr, namespace)?;

    let store = mgr
        .store(namespace)
        .map_err(|e| MemoryInterfaceError::OperationFailed(e.to_string()))?;

    let sem = SemanticMemory::new(store);
    let conf = confidence.unwrap_or(1.0);

    if confidence.is_some() {
        if let Ok(Some(existing)) = sem.recall(namespace, key) {
            if existing.confidence > conf {
                tracing::debug!(
                    namespace = %namespace,
                    key = %key,
                    existing_confidence = existing.confidence,
                    new_confidence = conf,
                    "skipping write: existing entry has higher confidence"
                );
                return Ok(existing.id);
            }
        }
    }

    let json_value = serde_json::Value::String(value.to_string());
    sem.remember(RememberInput {
        namespace,
        key,
        value: &json_value,
        confidence: conf,
        source,
        expires_at: None,
    })
    .map_err(|e| MemoryInterfaceError::OperationFailed(e.to_string()))
}

/// Retrieves a value by key from the agent's semantic memory namespace.
fn recall_inner(
    manager: &Arc<Mutex<MemoryManager>>,
    namespace: &str,
    key: &str,
) -> Result<Option<String>, MemoryInterfaceError> {
    let mut mgr = lock(manager)?;
    let store = mgr
        .store(namespace)
        .map_err(|e| MemoryInterfaceError::OperationFailed(e.to_string()))?;
    let sem = SemanticMemory::new(store);
    let entry = sem
        .recall(namespace, key)
        .map_err(|e| MemoryInterfaceError::OperationFailed(e.to_string()))?;
    Ok(entry.map(|e| match &e.value {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }))
}

/// Full-text search across the agent's namespace.
fn search_inner(
    manager: &Arc<Mutex<MemoryManager>>,
    namespace: &str,
    query: &str,
    limit: usize,
) -> Result<Vec<serde_json::Value>, MemoryInterfaceError> {
    let mut mgr = lock(manager)?;

    let store = mgr
        .store(namespace)
        .map_err(|e| MemoryInterfaceError::OperationFailed(e.to_string()))?;

    let search = MemorySearch::new(store);
    let results = search
        .query(SearchQuery {
            namespace,
            query,
            limit: limit as u32,
            sources: None,
            min_importance: None,
        })
        .map_err(|e| MemoryInterfaceError::OperationFailed(e.to_string()))?;

    Ok(results
        .into_iter()
        .map(|r| {
            let source_str = match r.source {
                SearchSource::Episodic => "episodic",
                SearchSource::Semantic => "semantic",
            };
            serde_json::json!({
                "content": r.content,
                "score": r.score,
                "source": source_str,
                "timestamp": r.created_at,
            })
        })
        .collect())
}

/// Removes a key/value pair from semantic memory.
fn forget_inner(
    manager: &Arc<Mutex<MemoryManager>>,
    namespace: &str,
    key: &str,
) -> Result<bool, MemoryInterfaceError> {
    let mut mgr = lock(manager)?;
    check_write_access(&mgr, namespace)?;

    let store = mgr
        .store(namespace)
        .map_err(|e| MemoryInterfaceError::OperationFailed(e.to_string()))?;

    let sem = SemanticMemory::new(store);
    sem.forget(namespace, key)
        .map_err(|e| MemoryInterfaceError::OperationFailed(e.to_string()))
}

/// Retrieves a semantic entry with full metadata.
///
/// Returns the entry as a JSON value containing `{key, value, confidence,
/// source, updated_at, expires_at}`, or `None` if absent or expired.
fn recall_entry_inner(
    manager: &Arc<Mutex<MemoryManager>>,
    namespace: &str,
    key: &str,
) -> Result<Option<serde_json::Value>, MemoryInterfaceError> {
    let mut mgr = lock(manager)?;
    let store = mgr
        .store(namespace)
        .map_err(|e| MemoryInterfaceError::OperationFailed(e.to_string()))?;

    let sem = SemanticMemory::new(store);
    let entry = sem
        .recall_entry(namespace, key)
        .map_err(|e| MemoryInterfaceError::OperationFailed(e.to_string()))?;

    Ok(entry.map(semantic_entry_to_json))
}

/// Lists all non-expired semantic entries in the namespace.
///
/// Returns a vector of JSON values with the same structure as `recall_entry_inner`.
fn recall_all_inner(
    manager: &Arc<Mutex<MemoryManager>>,
    namespace: &str,
    limit: usize,
) -> Result<Vec<serde_json::Value>, MemoryInterfaceError> {
    let mut mgr = lock(manager)?;
    let store = mgr
        .store(namespace)
        .map_err(|e| MemoryInterfaceError::OperationFailed(e.to_string()))?;

    let sem = SemanticMemory::new(store);
    let entries = sem
        .recall_all(namespace, Some(limit as u64))
        .map_err(|e| MemoryInterfaceError::OperationFailed(e.to_string()))?;

    Ok(entries.into_iter().map(semantic_entry_to_json).collect())
}

/// Converts a [`SemanticEntry`] to the dict shape exposed to Python.
fn semantic_entry_to_json(entry: apollia_memory::semantic::SemanticEntry) -> serde_json::Value {
    serde_json::json!({
        "key": entry.key,
        "value": entry.value,
        "confidence": entry.confidence,
        "source": entry.source,
        "updated_at": entry.updated_at,
        "expires_at": entry.expires_at,
    })
}

/// Locks the manager, converting poison errors.
fn lock(
    manager: &Arc<Mutex<MemoryManager>>,
) -> Result<std::sync::MutexGuard<'_, MemoryManager>, MemoryInterfaceError> {
    manager
        .lock()
        .map_err(|e| MemoryInterfaceError::OperationFailed(format!("lock poisoned: {e}")))
}

/// Checks that the namespace allows write operations.
fn check_write_access(mgr: &MemoryManager, namespace: &str) -> Result<(), MemoryInterfaceError> {
    match mgr.access_level(namespace) {
        Some(MemoryAccess::ReadWrite) => Ok(()),
        Some(MemoryAccess::ReadOnly) => Err(MemoryInterfaceError::ReadOnly(namespace.to_string())),
        None => Err(MemoryInterfaceError::NoNamespace),
    }
}

/// Recalls learned procedures matching the exact trigger.
///
/// Returns a list (0 or 1 entry), matching the Python `List[dict]` semantics.
fn recall_procedure_inner(
    manager: &Arc<Mutex<MemoryManager>>,
    namespace: &str,
    trigger: &str,
) -> Result<Vec<serde_json::Value>, MemoryInterfaceError> {
    let mut mgr = lock(manager)?;
    let store = mgr
        .store(namespace)
        .map_err(|e| MemoryInterfaceError::OperationFailed(e.to_string()))?;

    let proc = ProceduralMemory::new(store);
    let entry = proc
        .recall(namespace, trigger)
        .map_err(|e| MemoryInterfaceError::OperationFailed(e.to_string()))?;

    Ok(entry.into_iter().map(procedure_entry_to_json).collect())
}

/// Records or updates a procedure in procedural memory.
fn learn_procedure_inner(
    manager: &Arc<Mutex<MemoryManager>>,
    namespace: &str,
    trigger: &str,
    steps: &[String],
) -> Result<String, MemoryInterfaceError> {
    let mut mgr = lock(manager)?;
    check_write_access(&mgr, namespace)?;

    let store = mgr
        .store(namespace)
        .map_err(|e| MemoryInterfaceError::OperationFailed(e.to_string()))?;

    let proc = ProceduralMemory::new(store);
    proc.learn(namespace, trigger, steps)
        .map_err(|e| MemoryInterfaceError::OperationFailed(e.to_string()))
}

/// Converts a [`ProcedureEntry`] to the dict shape exposed to Python.
fn procedure_entry_to_json(entry: apollia_memory::procedural::ProcedureEntry) -> serde_json::Value {
    serde_json::json!({
        "id": entry.id,
        "trigger": entry.trigger,
        "steps": entry.steps,
        "success_count": entry.success_count,
        "last_used_at": entry.last_used_at,
        "created_at": entry.created_at,
    })
}

/// Exports the agent's private namespace as a JSON-serializable dict.
///
/// Wraps [`apollia_memory::export::export_namespace`] and tags the dump
/// with the alias `schema_version` (alongside `format_version`) so the
/// Python SDK can refer to either key going forward.
fn export_inner(
    manager: &Arc<Mutex<MemoryManager>>,
    namespace: &str,
) -> Result<serde_json::Value, MemoryInterfaceError> {
    let mgr = lock(manager)?;
    // Only the agent's primary namespace may be exported through this surface.
    if mgr.access_level(namespace) != Some(MemoryAccess::ReadWrite) {
        return Err(MemoryInterfaceError::NoNamespace);
    }
    let base_dir = mgr.base_dir().to_path_buf();
    // Drop the lock before touching disk: export_namespace opens its own
    // store handle (read-only flow) and could deadlock if we held the manager.
    drop(mgr);

    let export = export_namespace(&base_dir, namespace)
        .map_err(|e| MemoryInterfaceError::OperationFailed(e.to_string()))?;

    let mut value = serde_json::to_value(&export)
        .map_err(|e| MemoryInterfaceError::OperationFailed(e.to_string()))?;
    if let Some(obj) = value.as_object_mut() {
        // Surface a canonical `schema_version` alias: the SDK Protocol uses
        // this name in its docstrings.
        obj.insert(
            "schema_version".to_string(),
            serde_json::Value::from(export.format_version),
        );
    }
    Ok(value)
}

/// Imports a previously-exported dump into the agent's private namespace.
fn import_inner(
    manager: &Arc<Mutex<MemoryManager>>,
    namespace: &str,
    data: serde_json::Value,
    mode: ImportMode,
) -> Result<usize, MemoryInterfaceError> {
    let mgr = lock(manager)?;
    check_write_access(&mgr, namespace)?;
    let base_dir = mgr.base_dir().to_path_buf();
    drop(mgr);

    // Be lenient about the format_version / schema_version alias.
    let mut value = data;
    if let Some(obj) = value.as_object_mut() {
        if !obj.contains_key("format_version") {
            if let Some(v) = obj.remove("schema_version") {
                obj.insert("format_version".to_string(), v);
            }
        }
        // Default to current namespace if the payload doesn't carry one: an
        // agent re-importing its own dump shouldn't need to remember.
        obj.entry("namespace".to_string())
            .or_insert_with(|| serde_json::Value::String(namespace.to_string()));
        obj.entry("exported_at".to_string())
            .or_insert_with(|| serde_json::Value::String(String::new()));
        for key in ["episodic", "semantic", "procedural"] {
            obj.entry(key.to_string())
                .or_insert_with(|| serde_json::Value::Array(Vec::new()));
        }
    }

    let export: MemoryExport = serde_json::from_value(value)
        .map_err(|e| MemoryInterfaceError::OperationFailed(format!("invalid dump: {e}")))?;

    import_namespace(&base_dir, namespace, &export, mode)
        .map_err(|e| MemoryInterfaceError::OperationFailed(e.to_string()))
}

/// Converts a `serde_json::Value` to a Python object via `json.loads`.
fn json_value_to_py(py: Python<'_>, value: &serde_json::Value) -> PyResult<PyObject> {
    let json_str = serde_json::to_string(value)
        .map_err(|e| PyRuntimeError::new_err(format!("serialize: {e}")))?;
    let json_mod = py
        .import("json")
        .map_err(|e| PyRuntimeError::new_err(format!("import json: {e}")))?;
    let obj: PyObject = json_mod
        .call_method1("loads", (json_str,))
        .map_err(|e| PyRuntimeError::new_err(format!("json.loads: {e}")))?
        .unbind();
    Ok(obj)
}

/// Converts an arbitrary Python object to a `serde_json::Value` by round-tripping
/// through `json.dumps`; accepts dicts, lists, primitives transparently.
fn pyany_to_json(py: Python<'_>, obj: &PyObject) -> PyResult<serde_json::Value> {
    let json_mod = py
        .import("json")
        .map_err(|e| PyRuntimeError::new_err(format!("import json: {e}")))?;
    let bound = obj.bind(py);
    let json_str: String = json_mod
        .call_method1("dumps", (bound,))
        .map_err(|e| PyRuntimeError::new_err(format!("json.dumps: {e}")))?
        .extract()
        .map_err(|e| PyRuntimeError::new_err(format!("extract: {e}")))?;
    serde_json::from_str(&json_str).map_err(|e| PyRuntimeError::new_err(format!("json parse: {e}")))
}

/// Converts a Python dict to a `serde_json::Value`.
///
/// Uses `json.dumps` via the Python interpreter to handle nested structures.
fn pydict_to_json(dict: &Bound<'_, PyDict>) -> PyResult<serde_json::Value> {
    let py = dict.py();
    let json_mod = py
        .import("json")
        .map_err(|e| PyRuntimeError::new_err(format!("import json: {e}")))?;
    let json_str: String = json_mod
        .call_method1("dumps", (dict,))
        .map_err(|e| PyRuntimeError::new_err(format!("json.dumps: {e}")))?
        .extract()
        .map_err(|e| PyRuntimeError::new_err(format!("extract: {e}")))?;
    serde_json::from_str(&json_str).map_err(|e| PyRuntimeError::new_err(format!("json parse: {e}")))
}

/// Converts `expires_in` seconds from now to an ISO 8601 UTC timestamp string.
fn expires_in_to_iso(secs: u64) -> String {
    let expiry = chrono::Utc::now() + chrono::Duration::seconds(secs as i64);
    expiry.format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_memory::injection_tracker::{global_entries_for, global_tracker_clear};
    use tempfile::TempDir;

    #[test]
    fn set_turn_id_round_trips() {
        // GIVEN an interface with no turn set
        let (iface, _dir) = setup_interface("ns-turn");
        assert!(iface.current_turn_id().is_none());

        // WHEN a turn id is assigned and later cleared
        iface.set_turn_id(Some("turn-42".to_string()));
        // THEN the interface reports it
        assert_eq!(iface.current_turn_id(), Some("turn-42".to_string()));

        iface.set_turn_id(None);
        assert!(iface.current_turn_id().is_none());
    }

    #[test]
    fn record_injected_entry_pushes_to_tracker_with_fallback_reason() {
        // GIVEN a cleared global tracker
        global_tracker_clear();
        let json = serde_json::json!({
            "id": "sem-abc",
            "key": "budget",
            "value": "15000",
            "confidence": 0.8,
        });

        // WHEN an entry is recorded with the fallback reason pattern
        record_injected_entry(
            "turn-p7-aip",
            "agent-x",
            &json,
            "Matched query: budget".to_string(),
            None,
        );

        // THEN the tracker exposes the entry with preview/namespace/reason/score
        let entries = global_entries_for("turn-p7-aip");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, "sem-abc");
        assert_eq!(entries[0].namespace, "agent-x");
        assert_eq!(entries[0].content_preview, "15000");
        assert_eq!(entries[0].injection_reason, "Matched query: budget");
        assert!((entries[0].relevance_score - 0.8).abs() < f32::EPSILON);
    }

    #[test]
    fn record_injected_entry_skips_when_id_missing() {
        // GIVEN a cleared tracker
        global_tracker_clear();

        // WHEN the JSON has no id/key fields
        let json = serde_json::json!({ "value": "orphan" });
        record_injected_entry("turn-orphan", "ns", &json, "r".to_string(), None);

        // THEN nothing is recorded
        assert!(global_entries_for("turn-orphan").is_empty());
    }

    fn setup_interface(namespace: &str) -> (MemoryInterface, TempDir) {
        let dir = TempDir::new().expect("create temp dir");
        let manager = MemoryManager::new(dir.path(), Some(namespace.to_string()), vec![]);
        let iface = MemoryInterface::new(manager, namespace.to_string(), "test-agent".to_string())
            .expect("should create interface");
        (iface, dir)
    }

    fn setup_shared_interface(primary: &str, shared: &str) -> (MemoryInterface, TempDir) {
        let dir = TempDir::new().expect("create temp dir");
        // Pre-create the shared namespace DB
        let shared_path = dir.path().join(format!("{shared}.db"));
        apollia_memory::store::MemoryStore::open(&shared_path).expect("pre-create shared db");

        let manager = MemoryManager::new(
            dir.path(),
            Some(primary.to_string()),
            vec![shared.to_string()],
        );
        let iface = MemoryInterface::new(manager, shared.to_string(), "test-agent".to_string())
            .expect("should create interface");
        (iface, dir)
    }

    // Record episodic memory
    #[test]
    fn test_record_episodic_memory() {
        // GIVEN a MemoryInterface with namespace "agent-alpha"
        let (iface, _dir) = setup_interface("agent-alpha");

        // WHEN we record an event
        let id = record_inner(
            &iface.manager,
            &iface.namespace,
            &iface.agent_id,
            "le client a valide le devis",
            0.9,
            Some("task-123"),
            None,
            None,
        );

        // THEN the event is stored successfully
        assert!(id.is_ok());
        assert!(!id.expect("should succeed").is_empty());
    }

    // Remember semantic memory
    #[test]
    fn test_remember_semantic_memory() {
        // GIVEN a MemoryInterface with namespace "agent-alpha"
        let (iface, _dir) = setup_interface("agent-alpha");

        // WHEN we remember a key/value pair
        let id = remember_inner(
            &iface.manager,
            &iface.namespace,
            RememberArgs {
                key: "client.dupont.email",
                value: "marie@dupont.fr",
                source: None,
                confidence: None,
            },
        );

        // THEN the value is stored
        assert!(id.is_ok());
    }

    // Recall existing key
    #[test]
    fn test_recall_existing_key() {
        // GIVEN a MemoryInterface with a stored key
        let (iface, _dir) = setup_interface("agent-alpha");
        remember_inner(
            &iface.manager,
            &iface.namespace,
            RememberArgs {
                key: "client.dupont.email",
                value: "marie@dupont.fr",
                source: None,
                confidence: None,
            },
        )
        .expect("remember");

        // WHEN we recall the key
        let result = recall_inner(&iface.manager, &iface.namespace, "client.dupont.email");

        // THEN we get the stored value
        assert_eq!(result.expect("recall"), Some("marie@dupont.fr".to_string()));
    }

    // bis: Recall missing key returns None
    #[test]
    fn test_recall_missing_key() {
        // GIVEN a MemoryInterface with no data
        let (iface, _dir) = setup_interface("agent-alpha");

        // WHEN we recall a nonexistent key
        let result = recall_inner(&iface.manager, &iface.namespace, "cle.inexistante");

        // THEN we get None
        assert_eq!(result.expect("recall"), None);
    }

    // Search FTS with results
    #[test]
    fn test_search_fts_with_results() {
        // GIVEN a MemoryInterface with entries containing "Dupont"
        let (iface, _dir) = setup_interface("agent-alpha");
        record_inner(
            &iface.manager,
            &iface.namespace,
            &iface.agent_id,
            "Devis envoye a Dupont SA",
            0.8,
            None,
            None,
            None,
        )
        .expect("record");
        remember_inner(
            &iface.manager,
            &iface.namespace,
            RememberArgs {
                key: "client.dupont.budget",
                value: "15000",
                source: Some("crm"),
                confidence: None,
            },
        )
        .expect("remember");

        // WHEN we search for "Dupont"
        let results = search_inner(&iface.manager, &iface.namespace, "Dupont", 5);

        // THEN we get results with scores
        let items = results.expect("search");
        assert!(!items.is_empty());
        for item in &items {
            assert!(item["score"].as_f64().expect("score") > 0.0);
            assert!(!item["content"].as_str().expect("content").is_empty());
            assert!(!item["source"].as_str().expect("source").is_empty());
            assert!(!item["timestamp"].as_str().expect("timestamp").is_empty());
        }
    }

    // Forget removes key
    #[test]
    fn test_forget_removes_key() {
        // GIVEN a MemoryInterface with a stored key
        let (iface, _dir) = setup_interface("agent-alpha");
        remember_inner(
            &iface.manager,
            &iface.namespace,
            RememberArgs {
                key: "client.dupont.email",
                value: "marie@dupont.fr",
                source: None,
                confidence: None,
            },
        )
        .expect("remember");

        // WHEN we forget the key
        let removed = forget_inner(&iface.manager, &iface.namespace, "client.dupont.email");
        assert!(removed.expect("forget"));

        // THEN recall returns None
        let result = recall_inner(&iface.manager, &iface.namespace, "client.dupont.email");
        assert_eq!(result.expect("recall"), None);
    }

    // No namespace returns None
    #[test]
    fn test_no_namespace_returns_none() {
        // GIVEN a MemoryManager with no namespace
        let dir = TempDir::new().expect("create temp dir");
        let manager = MemoryManager::new(dir.path(), None, vec![]);

        // WHEN we construct with empty namespace
        let result = MemoryInterface::new(manager, String::new(), "agent-1".to_string());

        // THEN we get None
        assert!(result.is_none());
    }

    // Shared namespace is read-only
    #[test]
    fn test_shared_namespace_read_only() {
        // GIVEN a MemoryInterface on a shared (ReadOnly) namespace
        let (iface, _dir) = setup_shared_interface("private", "shared-ns");

        // WHEN we try to write (record)
        let record_result = record_inner(
            &iface.manager,
            &iface.namespace,
            &iface.agent_id,
            "should fail",
            0.5,
            None,
            None,
            None,
        );

        // THEN we get ReadOnly error
        assert!(matches!(
            record_result,
            Err(MemoryInterfaceError::ReadOnly(_))
        ));

        // WHEN we try to write (remember)
        let remember_result = remember_inner(
            &iface.manager,
            &iface.namespace,
            RememberArgs {
                key: "key",
                value: "value",
                source: None,
                confidence: None,
            },
        );
        assert!(matches!(
            remember_result,
            Err(MemoryInterfaceError::ReadOnly(_))
        ));

        // WHEN we try to write (forget)
        let forget_result = forget_inner(&iface.manager, &iface.namespace, "key");
        assert!(matches!(
            forget_result,
            Err(MemoryInterfaceError::ReadOnly(_))
        ));

        // WHEN we try to read (recall): should work
        let recall_result = recall_inner(&iface.manager, &iface.namespace, "nonexistent");
        assert!(recall_result.is_ok());

        // WHEN we try to search: should work (empty results is ok)
        let search_result = search_inner(&iface.manager, &iface.namespace, "test", 5);
        assert!(search_result.is_ok());
    }

    // Remember with explicit confidence
    #[test]
    fn test_remember_with_confidence() {
        // GIVEN a MemoryInterface
        let (iface, _dir) = setup_interface("agent-alpha");

        // WHEN we remember with explicit confidence
        let id = remember_inner(
            &iface.manager,
            &iface.namespace,
            RememberArgs {
                key: "user.name",
                value: "Nidal",
                source: Some("onboarding"),
                confidence: Some(0.9),
            },
        );

        // THEN the entry is stored with the given confidence
        assert!(id.is_ok());
        let value = recall_inner(&iface.manager, &iface.namespace, "user.name");
        assert_eq!(value.expect("recall"), Some("Nidal".to_string()));
    }

    // No overwrite when existing confidence is strictly higher
    #[test]
    fn test_no_overwrite_higher_confidence() {
        // GIVEN a key stored with confidence 0.9
        let (iface, _dir) = setup_interface("agent-alpha");
        remember_inner(
            &iface.manager,
            &iface.namespace,
            RememberArgs {
                key: "user.name",
                value: "Nidal",
                source: Some("onboarding"),
                confidence: Some(0.9),
            },
        )
        .expect("remember");

        // WHEN we try to overwrite with lower confidence 0.5
        remember_inner(
            &iface.manager,
            &iface.namespace,
            RememberArgs {
                key: "user.name",
                value: "Unknown",
                source: Some("onboarding"),
                confidence: Some(0.5),
            },
        )
        .expect("remember");

        // THEN the original value is preserved
        let value = recall_inner(&iface.manager, &iface.namespace, "user.name");
        assert_eq!(value.expect("recall"), Some("Nidal".to_string()));
    }

    // Overwrite when new confidence is higher or equal
    #[test]
    fn test_overwrite_when_equal_or_higher_confidence() {
        // GIVEN a key stored with confidence 0.5
        let (iface, _dir) = setup_interface("agent-alpha");
        remember_inner(
            &iface.manager,
            &iface.namespace,
            RememberArgs {
                key: "user.role",
                value: "developer",
                source: Some("onboarding"),
                confidence: Some(0.5),
            },
        )
        .expect("remember");

        // WHEN we overwrite with equal confidence
        remember_inner(
            &iface.manager,
            &iface.namespace,
            RememberArgs {
                key: "user.role",
                value: "CTO",
                source: Some("onboarding"),
                confidence: Some(0.5),
            },
        )
        .expect("remember");

        // THEN the new value replaces the old one
        let value = recall_inner(&iface.manager, &iface.namespace, "user.role");
        assert_eq!(value.expect("recall"), Some("CTO".to_string()));
    }

    // Record with default importance (0.5)
    #[test]
    fn test_record_default_importance() {
        // GIVEN a MemoryInterface
        let (iface, _dir) = setup_interface("agent-alpha");

        // WHEN we record with default importance (0.5 applied by caller)
        let id = record_inner(
            &iface.manager,
            &iface.namespace,
            &iface.agent_id,
            "event with default importance",
            0.5,
            None,
            None,
            None,
        );

        // THEN it succeeds
        assert!(id.is_ok());
    }

    // recall_entry_inner returns full metadata as JSON
    #[test]
    fn test_recall_entry_inner_returns_metadata() {
        // GIVEN a stored entry with confidence 0.95
        let (iface, _dir) = setup_interface("agent-alpha");
        remember_inner(
            &iface.manager,
            &iface.namespace,
            RememberArgs {
                key: "bootstrap.meta",
                value: "data",
                source: Some("agent"),
                confidence: Some(0.95),
            },
        )
        .expect("remember");

        // WHEN we recall the entry
        let result = recall_entry_inner(&iface.manager, &iface.namespace, "bootstrap.meta");

        // THEN the dict contains all metadata fields
        let entry = result.expect("recall_entry").expect("should be Some");
        assert_eq!(entry["key"], "bootstrap.meta");
        assert_eq!(entry["confidence"], 0.95);
        assert_eq!(entry["source"], "agent");
        assert!(!entry["updated_at"].as_str().expect("updated_at").is_empty());
    }

    // recall_entry_inner returns None for missing key
    #[test]
    fn test_recall_entry_inner_returns_none_for_missing() {
        // GIVEN a namespace with no matching key
        let (iface, _dir) = setup_interface("agent-alpha");

        // WHEN we recall a nonexistent key
        let result = recall_entry_inner(&iface.manager, &iface.namespace, "nonexistent");

        // THEN we get None
        assert!(result.expect("recall_entry").is_none());
    }

    // recall_all_inner lists namespace entries
    #[test]
    fn test_recall_all_inner_lists_entries() {
        // GIVEN 3 entries in the namespace
        let (iface, _dir) = setup_interface("agent-alpha");
        for i in 0..3 {
            remember_inner(
                &iface.manager,
                &iface.namespace,
                RememberArgs {
                    key: &format!("key.{i}"),
                    value: &format!("val{i}"),
                    source: None,
                    confidence: None,
                },
            )
            .expect("remember");
        }

        // WHEN we recall all
        let result = recall_all_inner(&iface.manager, &iface.namespace, 100);

        // THEN we get 3 entries
        assert_eq!(result.expect("recall_all").len(), 3);
    }

    // recall_all_inner respects limit
    #[test]
    fn test_recall_all_inner_respects_limit() {
        // GIVEN 10 entries in the namespace
        let (iface, _dir) = setup_interface("agent-alpha");
        for i in 0..10 {
            remember_inner(
                &iface.manager,
                &iface.namespace,
                RememberArgs {
                    key: &format!("key.{i}"),
                    value: &format!("val{i}"),
                    source: None,
                    confidence: None,
                },
            )
            .expect("remember");
        }

        // WHEN we recall with limit 5
        let result = recall_all_inner(&iface.manager, &iface.namespace, 5);

        // THEN we get at most 5 entries
        assert_eq!(result.expect("recall_all").len(), 5);
    }

    // recall_procedure with existing trigger returns list with one entry
    #[test]
    fn test_recall_procedure_existing_trigger() {
        // GIVEN a namespace with a learned procedure
        use apollia_memory::procedural::ProceduralMemory;

        let (iface, _dir) = setup_interface("agent-proc");
        let mut mgr = iface.manager.lock().expect("lock");
        let store = mgr.store("agent-proc").expect("store");
        let proc = ProceduralMemory::new(store);
        proc.learn(
            "agent-proc",
            "handle_summary",
            &["step1".to_string(), "step2".to_string()],
        )
        .expect("learn");
        drop(mgr);

        // WHEN recall_procedure_inner is called with the exact trigger
        let results = recall_procedure_inner(&iface.manager, "agent-proc", "handle_summary");

        // THEN a list with one entry is returned
        let items = results.expect("recall_procedure");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["trigger"], "handle_summary");
        assert_eq!(items[0]["steps"][0], "step1");
    }

    // recall_procedure with unknown trigger returns empty list
    #[test]
    fn test_recall_procedure_unknown_trigger_returns_empty() {
        // GIVEN a namespace with no procedures
        let (iface, _dir) = setup_interface("agent-proc-empty");

        // WHEN recall_procedure_inner is called with an unknown trigger
        let results = recall_procedure_inner(&iface.manager, "agent-proc-empty", "unknown_trigger");

        // THEN an empty list is returned (no exception)
        assert!(results.expect("recall_procedure").is_empty());
    }

    // record with metadata persists metadata in the entry
    #[test]
    fn test_record_with_metadata() {
        // GIVEN a MemoryInterface
        let (iface, _dir) = setup_interface("agent-meta");

        // WHEN we record with metadata
        let metadata = serde_json::json!({"source": "web_search", "url": "https://example.com"});
        let id = record_inner(
            &iface.manager,
            &iface.namespace,
            &iface.agent_id,
            "content with metadata",
            0.7,
            Some("task-meta"),
            Some(metadata.clone()),
            None,
        );

        // THEN the record succeeds
        assert!(id.is_ok(), "record with metadata should succeed");
    }

    // record without metadata is backward compatible
    #[test]
    fn test_record_without_metadata_backward_compat() {
        // GIVEN a MemoryInterface
        let (iface, _dir) = setup_interface("agent-compat");

        // WHEN we record without metadata or expires_in (old signature)
        let id = record_inner(
            &iface.manager,
            &iface.namespace,
            &iface.agent_id,
            "backward compat content",
            0.5,
            None,
            None,
            None,
        );

        // THEN it succeeds as before
        assert!(id.is_ok());
    }

    // record with expires_in stores an expiration timestamp
    #[test]
    fn test_record_with_expires_in() {
        // GIVEN a MemoryInterface
        let (iface, _dir) = setup_interface("agent-expiry");

        // WHEN we record with a 3600s expiry
        let id = record_inner(
            &iface.manager,
            &iface.namespace,
            &iface.agent_id,
            "content with expiry",
            0.8,
            None,
            None,
            Some(3600),
        );

        // THEN the record succeeds (expiry is set)
        assert!(id.is_ok(), "record with expires_in should succeed");
    }

    // export() round-trips all entry types as JSON
    #[test]
    fn test_export_round_trips_entries() {
        // GIVEN a namespace seeded with episodic + semantic entries
        let (iface, _dir) = setup_interface("agent-export");
        remember_inner(
            &iface.manager,
            &iface.namespace,
            RememberArgs {
                key: "user.name",
                value: "Nidal",
                source: Some("onboarding"),
                confidence: Some(0.9),
            },
        )
        .expect("remember");
        record_inner(
            &iface.manager,
            &iface.namespace,
            &iface.agent_id,
            "Welcome event",
            0.7,
            None,
            None,
            None,
        )
        .expect("record");

        // WHEN export_inner is called
        let dump = export_inner(&iface.manager, &iface.namespace).expect("export");

        // THEN the dump exposes the schema alias and both entry buckets
        assert_eq!(dump["format_version"], 1);
        assert_eq!(dump["schema_version"], 1);
        assert_eq!(dump["namespace"], "agent-export");
        assert_eq!(dump["episodic"].as_array().expect("episodic").len(), 1);
        assert_eq!(dump["semantic"].as_array().expect("semantic").len(), 1);
        assert_eq!(dump["procedural"].as_array().expect("procedural").len(), 0);
    }

    // import_data(replace=True) restores into an empty namespace
    #[test]
    fn test_export_then_import_replace_round_trip() {
        // GIVEN namespace A with one semantic + one episodic entry, dumped
        let (src, src_dir) = setup_interface("agent-src");
        remember_inner(
            &src.manager,
            &src.namespace,
            RememberArgs {
                key: "k.budget",
                value: "15000",
                source: Some("crm"),
                confidence: Some(0.8),
            },
        )
        .expect("remember src");
        record_inner(
            &src.manager,
            &src.namespace,
            &src.agent_id,
            "Initial signal",
            0.6,
            None,
            None,
            None,
        )
        .expect("record src");
        let dump = export_inner(&src.manager, &src.namespace).expect("export");

        // WHEN we import the dump into an empty target namespace with Replace
        // (using the same agent identity to keep write access).
        let dest_base = src_dir.path().to_path_buf();
        let manager = MemoryManager::new(&dest_base, Some("agent-dest".to_string()), vec![]);
        let dest =
            MemoryInterface::new(manager, "agent-dest".to_string(), "test-agent".to_string())
                .expect("dest iface");

        let imported = import_inner(&dest.manager, &dest.namespace, dump, ImportMode::Replace)
            .expect("import");

        // THEN both entries are restored and recall sees the semantic value
        assert_eq!(imported, 2, "expected 2 entries imported");
        let value =
            recall_inner(&dest.manager, &dest.namespace, "k.budget").expect("recall after import");
        assert_eq!(value, Some("15000".to_string()));
    }

    // import_data without replace keeps existing entries intact (merge)
    #[test]
    fn test_import_data_merge_does_not_overwrite() {
        // GIVEN namespace with an existing entry and a dump from another agent
        let (iface, _dir) = setup_interface("agent-merge");
        remember_inner(
            &iface.manager,
            &iface.namespace,
            RememberArgs {
                key: "user.role",
                value: "operator",
                source: Some("local"),
                confidence: Some(0.9),
            },
        )
        .expect("seed");

        // Craft a minimal dump that would overwrite (same key). INSERT OR
        // IGNORE on `id` means the same primary key collides only when the
        // entry id is identical, so we forge a fresh id but same key.
        let dump = serde_json::json!({
            "format_version": 1,
            "namespace": "agent-merge",
            "exported_at": "2026-05-19T00:00:00Z",
            "episodic": [],
            "semantic": [{
                "id": "imported-1",
                "key": "user.role",
                "value": "\"intruder\"",
                "source": "external",
                "confidence": 0.1,
                "created_at": "2026-05-19T00:00:00Z",
                "updated_at": "2026-05-19T00:00:00Z",
            }],
            "procedural": [],
        });

        // WHEN we merge: semantic_memories has UNIQUE(namespace, key), so the
        // imported row is silently ignored and the original survives.
        let imported = import_inner(&iface.manager, &iface.namespace, dump, ImportMode::Merge)
            .expect("import");

        // THEN nothing is overwritten and recall still returns the original.
        assert_eq!(imported, 0, "merge must not insert colliding rows");
        let value = recall_inner(&iface.manager, &iface.namespace, "user.role").expect("recall");
        assert_eq!(value, Some("operator".to_string()));
    }

    // Accept the `schema_version` alias in addition to `format_version`
    #[test]
    fn test_import_accepts_schema_version_alias() {
        // GIVEN a dump that only carries `schema_version` (SDK-facing alias)
        let (iface, _dir) = setup_interface("agent-alias");
        let dump = serde_json::json!({
            "schema_version": 1,
            "episodic": [],
            "semantic": [{
                "id": "alias-1",
                "key": "from.alias",
                "value": "\"ok\"",
                "source": null,
                "confidence": 1.0,
                "created_at": "2026-05-19T00:00:00Z",
                "updated_at": "2026-05-19T00:00:00Z",
            }],
            "procedural": [],
        });

        // WHEN imported
        let imported = import_inner(&iface.manager, &iface.namespace, dump, ImportMode::Merge)
            .expect("import alias");

        // THEN the row landed despite the missing `format_version` key
        assert_eq!(imported, 1);
        let value = recall_inner(&iface.manager, &iface.namespace, "from.alias").expect("recall");
        assert_eq!(value, Some("ok".to_string()));
    }
}

#[cfg(test)]
mod trust_model_tests {
    use super::*;
    use tempfile::TempDir;

    /// Builds a MemoryManager with the given namespace as primary.
    fn make_manager(dir: &TempDir, namespace: &str) -> MemoryManager {
        MemoryManager::new(dir.path(), Some(namespace.to_string()), vec![])
    }

    /// Builds a MemoryInterface for the given namespace (used by procedure tests).
    fn setup_interface(namespace: &str) -> (MemoryInterface, TempDir) {
        let dir = TempDir::new().expect("create temp dir");
        let manager = make_manager(&dir, namespace);
        let iface = MemoryInterface::new(manager, namespace.to_string(), "test-agent".to_string())
            .expect("should create interface");
        (iface, dir)
    }

    /// Stores a key directly in the given manager/namespace using recall_inner helpers.
    fn seed_memory(mgr_arc: &Arc<Mutex<MemoryManager>>, namespace: &str, key: &str, value: &str) {
        remember_inner(
            mgr_arc,
            namespace,
            RememberArgs {
                key,
                value,
                source: None,
                confidence: None,
            },
        )
        .expect("seed memory");
    }

    // Agent invoked via A2A writes only into its own namespace
    #[test]
    fn test_a2a_context_writes_own_namespace_only() {
        // GIVEN a MemoryInterface for "excel-worker" with user_memory_read_only = true
        let dir = TempDir::new().expect("create temp dir");
        let iface = MemoryInterface::new(
            make_manager(&dir, "excel-worker"),
            "excel-worker".to_string(),
            "excel-worker".to_string(),
        )
        .expect("create interface");

        // WHEN remember("last_file", "ventes.xlsx")
        remember_inner(
            &iface.manager,
            &iface.namespace,
            RememberArgs {
                key: "last_file",
                value: "ventes.xlsx",
                source: None,
                confidence: None,
            },
        )
        .expect("remember");

        // THEN the data is in the "excel-worker" namespace
        let in_agent = recall_inner(&iface.manager, &iface.namespace, "last_file");
        assert_eq!(
            in_agent.expect("recall agent"),
            Some("ventes.xlsx".to_string())
        );

        // AND not visible through a separate "director" namespace manager
        let director_mgr_arc = Arc::new(Mutex::new(make_manager(&dir, "director")));
        let in_director = recall_inner(&director_mgr_arc, "director", "last_file");
        assert_eq!(in_director.expect("recall director"), None);
    }

    // learn_procedure_inner stores a procedure retrievable by recall_procedure_inner
    #[test]
    fn test_learn_procedure_round_trip() {
        // GIVEN a MemoryInterface
        let (iface, _dir) = setup_interface("agent-learn");

        // WHEN we learn a procedure
        let id = learn_procedure_inner(
            &iface.manager,
            "agent-learn",
            "analyse rapport financier",
            &[
                "Ouvrir le PDF".to_string(),
                "Extraire le CA".to_string(),
                "Calculer la marge".to_string(),
            ],
        );
        assert!(id.is_ok());

        // THEN recall_procedure_inner returns the procedure
        let results =
            recall_procedure_inner(&iface.manager, "agent-learn", "analyse rapport financier");
        let items = results.expect("recall");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["trigger"], "analyse rapport financier");
        assert_eq!(items[0]["steps"][0], "Ouvrir le PDF");
    }

    // learn_procedure_inner with empty steps returns error
    #[test]
    fn test_learn_procedure_empty_steps_error() {
        // GIVEN a MemoryInterface
        let (iface, _dir) = setup_interface("agent-learn-err");

        // WHEN we learn with empty steps (would be caught by PyO3 layer, but test the Rust layer too)
        // The ProceduralMemory::learn returns EmptySteps error
        let result = learn_procedure_inner(&iface.manager, "agent-learn-err", "trigger", &[]);
        // THEN error
        assert!(result.is_err());
    }
}
