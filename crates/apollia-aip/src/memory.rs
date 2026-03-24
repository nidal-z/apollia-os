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
#[pyclass]
pub struct MemoryInterface {
    manager: Arc<Mutex<MemoryManager>>,
    namespace: String,
    agent_id: String,
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
    #[pyo3(signature = (key, value, source=None))]
    fn remember<'py>(
        &self,
        py: Python<'py>,
        key: String,
        value: String,
        source: Option<String>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let manager = Arc::clone(&self.manager);
        let namespace = self.namespace.clone();

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let result = tokio::task::spawn_blocking(move || {
                remember_inner(&manager, &namespace, &key, &value, source.as_deref())
            })
            .await
            .map_err(|e| PyRuntimeError::new_err(format!("spawn_blocking failed: {e}")))?;

            result.map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
            Ok(Python::with_gil(|py| py.None()))
        })
    }

    /// Retrieves a value by key from semantic memory.
    ///
    /// Returns the value (str) or None if the key doesn't exist.
    fn recall<'py>(&self, py: Python<'py>, key: String) -> PyResult<Bound<'py, PyAny>> {
        let manager = Arc::clone(&self.manager);
        let namespace = self.namespace.clone();

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let result =
                tokio::task::spawn_blocking(move || recall_inner(&manager, &namespace, &key))
                    .await
                    .map_err(|e| PyRuntimeError::new_err(format!("spawn_blocking failed: {e}")))?;

            match result {
                Ok(Some(value)) => Ok(Python::with_gil(|py| value.into_py(py))),
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
                        .import_bound("json")
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
    /// Returns None if the namespace is empty or absent.
    pub fn new(manager: MemoryManager, namespace: String, agent_id: String) -> Option<Self> {
        if namespace.is_empty() {
            return None;
        }
        Some(Self {
            manager: Arc::new(Mutex::new(manager)),
            namespace,
            agent_id,
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
fn remember_inner(
    manager: &Arc<Mutex<MemoryManager>>,
    namespace: &str,
    key: &str,
    value: &str,
    source: Option<&str>,
) -> Result<String, MemoryInterfaceError> {
    let mut mgr = lock(manager)?;
    check_write_access(&mgr, namespace)?;

    let store = mgr
        .store(namespace)
        .map_err(|e| MemoryInterfaceError::OperationFailed(e.to_string()))?;

    let sem = SemanticMemory::new(store);
    let json_value = serde_json::Value::String(value.to_string());
    sem.remember(namespace, key, &json_value, 1.0, source, None)
        .map_err(|e| MemoryInterfaceError::OperationFailed(e.to_string()))
}

/// Retrieves a value by key from semantic memory.
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
        )
        .expect("record");
        remember_inner(
            &iface.manager,
            &iface.namespace,
            "client.dupont.budget",
            "15000",
            Some("crm"),
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
        );

        // THEN we get ReadOnly error
        assert!(matches!(
            record_result,
            Err(MemoryInterfaceError::ReadOnly(_))
        ));

        // WHEN we try to write (remember)
        let remember_result =
            remember_inner(&iface.manager, &iface.namespace, "key", "value", None);
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
        let recall_result = recall_inner(&iface.manager, &iface.namespace, "nonexistent");
        assert!(recall_result.is_ok());

        // WHEN we try to search — should work (empty results is ok)
        let search_result = search_inner(&iface.manager, &iface.namespace, "test", 5);
        assert!(search_result.is_ok());
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
