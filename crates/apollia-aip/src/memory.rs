//! MemoryInterface — Python-facing proxy for agent memory operations.
//!
//! Exposes a `#[pyclass]` that agents use via `ctx.memory.record()`,
//! `ctx.memory.remember()`, `ctx.memory.recall()`, `ctx.memory.search()`,
//! and `ctx.memory.forget()`.
//!
//! Respects Principle #6: memory at agent's initiative — no automatic injection.

use std::sync::{Arc, Mutex};

use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;

use apollia_memory::episodic::EpisodicMemory;
use apollia_memory::manager::{MemoryAccess, MemoryManager};
use apollia_memory::search::{MemorySearch, SearchSource};
use apollia_memory::semantic::SemanticMemory;

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
/// When `user_memory_read_only` is `true` (A2A invocation context),
/// `recall()` falls back to the global `__user__` namespace if the key
/// is not found in the agent's own namespace. Writes always target the
/// agent's namespace — the namespace is enforced by the runtime.
#[pyclass]
pub struct MemoryInterface {
    manager: Arc<Mutex<MemoryManager>>,
    namespace: String,
    agent_id: String,
    /// When `true`, `recall()` also reads from the global user memory namespace.
    ///
    /// Set by the runtime when the agent is invoked via A2A. The agent Python
    /// code is not aware of this flag — it calls `ctx.memory.recall()` normally.
    user_memory_read_only: bool,
    /// Secondary memory manager pointing at the `__user__` namespace.
    ///
    /// `None` when `user_memory_read_only` is `false` or when user memory
    /// is not available in the current runtime environment.
    user_manager: Option<Arc<Mutex<MemoryManager>>>,
}

#[pymethods]
impl MemoryInterface {
    /// Records an episodic memory event.
    ///
    /// importance: score between 0.0 and 1.0 (default 0.5)
    /// task_id: current task identifier (optional)
    #[pyo3(signature = (content, importance=None, task_id=None))]
    fn record<'py>(
        &self,
        py: Python<'py>,
        content: String,
        importance: Option<f64>,
        task_id: Option<String>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let manager = Arc::clone(&self.manager);
        let namespace = self.namespace.clone();
        let agent_id = self.agent_id.clone();
        let importance = importance.unwrap_or(0.5);

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let result = tokio::task::spawn_blocking(move || {
                record_inner(
                    &manager,
                    &namespace,
                    &agent_id,
                    &content,
                    importance,
                    task_id.as_deref(),
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
                    &key,
                    &value,
                    source.as_deref(),
                    confidence,
                )
            })
            .await
            .map_err(|e| PyRuntimeError::new_err(format!("spawn_blocking failed: {e}")))?;

            result.map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
            Ok(Python::with_gil(|py| py.None()))
        })
    }

    /// Retrieves a value by key from semantic memory.
    ///
    /// Searches the agent's own namespace first. When `user_memory_read_only`
    /// is `true` and the key is absent from the agent namespace, also reads
    /// from the global user memory namespace.
    ///
    /// Returns the value (str) or None if the key doesn't exist.
    fn recall<'py>(&self, py: Python<'py>, key: String) -> PyResult<Bound<'py, PyAny>> {
        let manager = Arc::clone(&self.manager);
        let namespace = self.namespace.clone();
        let user_memory_read_only = self.user_memory_read_only;
        let user_manager = self.user_manager.clone();

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let result = tokio::task::spawn_blocking(move || {
                recall_inner(
                    &manager,
                    &namespace,
                    &key,
                    user_memory_read_only,
                    user_manager.as_ref(),
                )
            })
            .await
            .map_err(|e| PyRuntimeError::new_err(format!("spawn_blocking failed: {e}")))?;

            match result {
                Ok(Some(value)) => Ok(Python::with_gil(|py| value.into_pyobject(py).unwrap().into_any().unbind())),
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
}

impl MemoryInterface {
    /// Creates a new MemoryInterface for a given agent.
    ///
    /// `user_memory_read_only` enables read-through to the global `__user__`
    /// namespace on `recall()` misses. Pass `None` for `user_manager` when
    /// user memory is not needed or not available.
    ///
    /// Returns `None` if `namespace` is empty.
    pub fn new(
        manager: MemoryManager,
        namespace: String,
        agent_id: String,
        user_memory_read_only: bool,
        user_manager: Option<MemoryManager>,
    ) -> Option<Self> {
        if namespace.is_empty() {
            return None;
        }
        Some(Self {
            manager: Arc::new(Mutex::new(manager)),
            namespace,
            agent_id,
            user_memory_read_only,
            user_manager: user_manager.map(|m| Arc::new(Mutex::new(m))),
        })
    }
}

// ---------------------------------------------------------------------------
// Pure Rust internals — testable without PyO3
// ---------------------------------------------------------------------------

/// Records an episodic event in the agent's namespace.
fn record_inner(
    manager: &Arc<Mutex<MemoryManager>>,
    namespace: &str,
    agent_id: &str,
    content: &str,
    importance: f64,
    task_id: Option<&str>,
) -> Result<String, MemoryInterfaceError> {
    let mut mgr = lock(manager)?;
    check_write_access(&mgr, namespace)?;

    let store = mgr
        .store(namespace)
        .map_err(|e| MemoryInterfaceError::OperationFailed(e.to_string()))?;

    let ep = EpisodicMemory::new(store);
    ep.record(
        namespace, agent_id, content, importance, task_id, None, None,
    )
    .map_err(|e| MemoryInterfaceError::OperationFailed(e.to_string()))
}

/// Stores a key/value pair in semantic memory.
///
/// When `confidence` is `Some`, an existing entry with strictly higher
/// confidence is preserved — the write is silently skipped.
/// When `None`, defaults to 1.0 (backward-compatible unconditional upsert).
fn remember_inner(
    manager: &Arc<Mutex<MemoryManager>>,
    namespace: &str,
    key: &str,
    value: &str,
    source: Option<&str>,
    confidence: Option<f64>,
) -> Result<String, MemoryInterfaceError> {
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
    sem.remember(namespace, key, &json_value, conf, source, None)
        .map_err(|e| MemoryInterfaceError::OperationFailed(e.to_string()))
}

/// The reserved namespace for global user memory.
const USER_MEMORY_NAMESPACE: &str = "__user__";

/// Retrieves a value by key from semantic memory.
///
/// Checks the agent's primary namespace first. When `user_memory_read_only`
/// is `true` and the key is absent, also reads from the `__user__` namespace
/// via `user_manager` if one is provided.
fn recall_inner(
    manager: &Arc<Mutex<MemoryManager>>,
    namespace: &str,
    key: &str,
    user_memory_read_only: bool,
    user_manager: Option<&Arc<Mutex<MemoryManager>>>,
) -> Result<Option<String>, MemoryInterfaceError> {
    // Search agent's own namespace first.
    let agent_result = {
        let mut mgr = lock(manager)?;
        let store = mgr
            .store(namespace)
            .map_err(|e| MemoryInterfaceError::OperationFailed(e.to_string()))?;
        let sem = SemanticMemory::new(store);
        let entry = sem
            .recall(namespace, key)
            .map_err(|e| MemoryInterfaceError::OperationFailed(e.to_string()))?;
        entry.map(|e| match &e.value {
            serde_json::Value::String(s) => s.clone(),
            other => other.to_string(),
        })
    };

    if agent_result.is_some() || !user_memory_read_only {
        return Ok(agent_result);
    }

    // Key absent from agent namespace — try global user memory as fallback.
    let Some(umgr) = user_manager else {
        return Ok(None);
    };

    let mut mgr = lock(umgr)?;
    let store = match mgr.store(USER_MEMORY_NAMESPACE) {
        Ok(s) => s,
        // User memory DB not available — treat as not found, not an error.
        Err(_) => return Ok(None),
    };
    let sem = SemanticMemory::new(store);
    let entry = sem
        .recall(USER_MEMORY_NAMESPACE, key)
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
        .query(namespace, query, limit as u32, None, None)
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup_interface(namespace: &str) -> (MemoryInterface, TempDir) {
        let dir = TempDir::new().expect("create temp dir");
        let manager = MemoryManager::new(dir.path(), Some(namespace.to_string()), vec![]);
        let iface = MemoryInterface::new(
            manager,
            namespace.to_string(),
            "test-agent".to_string(),
            false,
            None,
        )
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
        let iface = MemoryInterface::new(
            manager,
            shared.to_string(),
            "test-agent".to_string(),
            false,
            None,
        )
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
            "client.dupont.email",
            "marie@dupont.fr",
            None,
            None,
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
            "client.dupont.email",
            "marie@dupont.fr",
            None,
            None,
        )
        .expect("remember");

        // WHEN we recall the key
        let result = recall_inner(
            &iface.manager,
            &iface.namespace,
            "client.dupont.email",
            false,
            None,
        );

        // THEN we get the stored value
        assert_eq!(result.expect("recall"), Some("marie@dupont.fr".to_string()));
    }

    // bis: Recall missing key returns None
    #[test]
    fn test_recall_missing_key() {
        // GIVEN a MemoryInterface with no data
        let (iface, _dir) = setup_interface("agent-alpha");

        // WHEN we recall a nonexistent key
        let result = recall_inner(
            &iface.manager,
            &iface.namespace,
            "cle.inexistante",
            false,
            None,
        );

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
        )
        .expect("record");
        remember_inner(
            &iface.manager,
            &iface.namespace,
            "client.dupont.budget",
            "15000",
            Some("crm"),
            None,
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
            "client.dupont.email",
            "marie@dupont.fr",
            None,
            None,
        )
        .expect("remember");

        // WHEN we forget the key
        let removed = forget_inner(&iface.manager, &iface.namespace, "client.dupont.email");
        assert!(removed.expect("forget"));

        // THEN recall returns None
        let result = recall_inner(
            &iface.manager,
            &iface.namespace,
            "client.dupont.email",
            false,
            None,
        );
        assert_eq!(result.expect("recall"), None);
    }

    // No namespace returns None
    #[test]
    fn test_no_namespace_returns_none() {
        // GIVEN a MemoryManager with no namespace
        let dir = TempDir::new().expect("create temp dir");
        let manager = MemoryManager::new(dir.path(), None, vec![]);

        // WHEN we construct with empty namespace
        let result =
            MemoryInterface::new(manager, String::new(), "agent-1".to_string(), false, None);

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
        );

        // THEN we get ReadOnly error
        assert!(matches!(
            record_result,
            Err(MemoryInterfaceError::ReadOnly(_))
        ));

        // WHEN we try to write (remember)
        let remember_result =
            remember_inner(&iface.manager, &iface.namespace, "key", "value", None, None);
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

        // WHEN we try to read (recall) — should work
        let recall_result =
            recall_inner(&iface.manager, &iface.namespace, "nonexistent", false, None);
        assert!(recall_result.is_ok());

        // WHEN we try to search — should work (empty results is ok)
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
            "user.name",
            "Nidal",
            Some("onboarding"),
            Some(0.9),
        );

        // THEN the entry is stored with the given confidence
        assert!(id.is_ok());
        let value = recall_inner(&iface.manager, &iface.namespace, "user.name", false, None);
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
            "user.name",
            "Nidal",
            Some("onboarding"),
            Some(0.9),
        )
        .expect("remember");

        // WHEN we try to overwrite with lower confidence 0.5
        remember_inner(
            &iface.manager,
            &iface.namespace,
            "user.name",
            "Unknown",
            Some("onboarding"),
            Some(0.5),
        )
        .expect("remember");

        // THEN the original value is preserved
        let value = recall_inner(&iface.manager, &iface.namespace, "user.name", false, None);
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
            "user.role",
            "developer",
            Some("onboarding"),
            Some(0.5),
        )
        .expect("remember");

        // WHEN we overwrite with equal confidence
        remember_inner(
            &iface.manager,
            &iface.namespace,
            "user.role",
            "CTO",
            Some("onboarding"),
            Some(0.5),
        )
        .expect("remember");

        // THEN the new value replaces the old one
        let value = recall_inner(&iface.manager, &iface.namespace, "user.role", false, None);
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
        );

        // THEN it succeeds
        assert!(id.is_ok());
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

    /// Stores a key directly in the given manager/namespace using recall_inner helpers.
    fn seed_memory(mgr_arc: &Arc<Mutex<MemoryManager>>, namespace: &str, key: &str, value: &str) {
        remember_inner(mgr_arc, namespace, key, value, None, None).expect("seed memory");
    }

    // Agent invoked via A2A reads global user memory when key is absent from agent namespace
    #[test]
    fn test_a2a_context_reads_user_memory() {
        // GIVEN user memory containing ("user_pref", "dark_mode")
        let dir = TempDir::new().expect("create temp dir");
        let user_mgr = make_manager(&dir, USER_MEMORY_NAMESPACE);
        let user_mgr_arc = Arc::new(Mutex::new(user_mgr));
        seed_memory(
            &user_mgr_arc,
            USER_MEMORY_NAMESPACE,
            "user_pref",
            "dark_mode",
        );

        // AND a MemoryInterface with user_memory_read_only = true
        let agent_mgr = make_manager(&dir, "excel-worker");
        let iface = MemoryInterface::new(
            agent_mgr,
            "excel-worker".to_string(),
            "excel-worker".to_string(),
            true,
            Some(make_manager(&dir, USER_MEMORY_NAMESPACE)),
        )
        .expect("create interface");

        // WHEN recall("user_pref")
        let result = recall_inner(
            &iface.manager,
            &iface.namespace,
            "user_pref",
            iface.user_memory_read_only,
            iface.user_manager.as_ref(),
        );

        // THEN returns "dark_mode" from user memory
        assert_eq!(result.expect("recall"), Some("dark_mode".to_string()));
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
            true,
            None,
        )
        .expect("create interface");

        // WHEN remember("last_file", "ventes.xlsx")
        remember_inner(
            &iface.manager,
            &iface.namespace,
            "last_file",
            "ventes.xlsx",
            None,
            None,
        )
        .expect("remember");

        // THEN the data is in the "excel-worker" namespace
        let in_agent = recall_inner(&iface.manager, &iface.namespace, "last_file", false, None);
        assert_eq!(
            in_agent.expect("recall agent"),
            Some("ventes.xlsx".to_string())
        );

        // AND not visible through a separate "director" namespace manager
        let director_mgr_arc = Arc::new(Mutex::new(make_manager(&dir, "director")));
        let in_director = recall_inner(&director_mgr_arc, "director", "last_file", false, None);
        assert_eq!(in_director.expect("recall director"), None);
    }

    // Agent invoked via A2A cannot see a key that is only in agent namespace through user-memory fallback
    #[test]
    fn test_a2a_agent_key_not_leaked_to_user_memory_fallback() {
        // GIVEN "excel-worker" has a key "secret" in its namespace
        let dir = TempDir::new().expect("create temp dir");
        let agent_mgr = make_manager(&dir, "excel-worker");
        let agent_mgr_arc = Arc::new(Mutex::new(agent_mgr));
        seed_memory(&agent_mgr_arc, "excel-worker", "secret", "classified");

        // AND user memory is empty
        let user_mgr = make_manager(&dir, USER_MEMORY_NAMESPACE);
        let user_mgr_arc = Arc::new(Mutex::new(user_mgr));

        // WHEN another interface with user_memory_read_only reads "secret" via user memory only
        // (agent_mgr_arc is moved, so we construct a fresh interface with a different agent_mgr)
        let iface = MemoryInterface::new(
            make_manager(&dir, "excel-worker"),
            "excel-worker".to_string(),
            "excel-worker".to_string(),
            true,
            Some(make_manager(&dir, USER_MEMORY_NAMESPACE)),
        )
        .expect("create interface");

        // Recall from user memory only (simulate: key exists in agent ns — should return it from agent ns)
        let result = recall_inner(
            &iface.manager,
            &iface.namespace,
            "secret",
            iface.user_memory_read_only,
            iface.user_manager.as_ref(),
        );

        // THEN returns the agent's own value (agent namespace takes priority)
        assert_eq!(result.expect("recall"), Some("classified".to_string()));

        // AND user memory does not contain the key
        let user_result = recall_inner(&user_mgr_arc, USER_MEMORY_NAMESPACE, "secret", false, None);
        assert_eq!(user_result.expect("user recall"), None);
    }

    // Agent started directly (not via A2A) has user_memory_read_only = false
    #[test]
    fn test_direct_context_has_normal_permissions() {
        // GIVEN a MemoryInterface with user_memory_read_only = false
        let dir = TempDir::new().expect("create temp dir");
        let iface = MemoryInterface::new(
            make_manager(&dir, "direct-agent"),
            "direct-agent".to_string(),
            "direct-agent".to_string(),
            false,
            None,
        )
        .expect("create interface");

        // THEN user_memory_read_only is false
        assert!(!iface.user_memory_read_only);

        // WHEN remember and recall
        remember_inner(
            &iface.manager,
            &iface.namespace,
            "my_key",
            "my_value",
            None,
            None,
        )
        .expect("remember");
        let result = recall_inner(&iface.manager, &iface.namespace, "my_key", false, None);

        // THEN standard behavior — value is found
        assert_eq!(result.expect("recall"), Some("my_value".to_string()));
    }

    // user_memory_read_only = true but no user_manager — returns None gracefully
    #[test]
    fn test_user_memory_read_only_without_manager_returns_none() {
        // GIVEN user_memory_read_only = true but no user_manager provided
        let dir = TempDir::new().expect("create temp dir");
        let iface = MemoryInterface::new(
            make_manager(&dir, "agent-x"),
            "agent-x".to_string(),
            "agent-x".to_string(),
            true,
            None, // no user memory
        )
        .expect("create interface");

        // WHEN recall a key that does not exist in agent namespace
        let result = recall_inner(
            &iface.manager,
            &iface.namespace,
            "absent_key",
            iface.user_memory_read_only,
            iface.user_manager.as_ref(),
        );

        // THEN None (graceful — no panic)
        assert_eq!(result.expect("recall"), None);
    }
}
