//! The synchronous work behind the memory pymethods.
//!
//! Split out of `memory.rs`: the pyclass and its `#[pymethods]` block stay in
//! the parent, the functions each method delegates to, and the JSON
//! conversions they share, live here.

use std::sync::{Arc, Mutex};

use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::PyDict;

use apollia_memory::episodic::EpisodicMemory;
use apollia_memory::export::{export_namespace, import_namespace, ImportMode, MemoryExport};
use apollia_memory::injection_tracker::{global_record, preview, InjectedEntry};
use apollia_memory::manager::{MemoryAccess, MemoryManager};
use apollia_memory::procedural::ProceduralMemory;
use apollia_memory::search::{MemorySearch, SearchQuery, SearchSource};
use apollia_memory::semantic::{RememberInput, SemanticMemory};

use crate::memory::MemoryInterfaceError;

/// Extracts `id`, textual `value`/`content`, and relevance fields from a
/// semantic/episodic JSON value and records them into the global injection
/// tracker.
///
/// Errors are swallowed (fire-and-forget): injection tracking must never
/// break an agent's recall path.
pub(super) fn record_injected_entry(
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
/// Records an episodic event in the agent's namespace.
// REASON: shared worker behind the two pymethods above, taking their flattened keyword arguments.
#[allow(clippy::too_many_arguments)]
pub(super) fn record_inner(
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
pub(super) struct RememberArgs<'a> {
    pub(super) key: &'a str,
    pub(super) value: &'a str,
    pub(super) source: Option<&'a str>,
    pub(super) confidence: Option<f64>,
}
/// Stores a key/value pair in semantic memory.
///
/// When `confidence` is `Some`, an existing entry with strictly higher
/// confidence is preserved: the write is silently skipped.
/// When `None`, defaults to 1.0 (backward-compatible unconditional upsert).
pub(super) fn remember_inner(
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
                    reason = "existing entry has a higher confidence",
                    "memory.semantic.write.skipped"
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
pub(super) fn recall_inner(
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
pub(super) fn search_inner(
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
pub(super) fn forget_inner(
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
pub(super) fn recall_entry_inner(
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
pub(super) fn recall_all_inner(
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
pub(super) fn semantic_entry_to_json(
    entry: apollia_memory::semantic::SemanticEntry,
) -> serde_json::Value {
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
pub(super) fn lock(
    manager: &Arc<Mutex<MemoryManager>>,
) -> Result<std::sync::MutexGuard<'_, MemoryManager>, MemoryInterfaceError> {
    manager
        .lock()
        .map_err(|e| MemoryInterfaceError::OperationFailed(format!("lock poisoned: {e}")))
}
/// Checks that the namespace allows write operations.
pub(super) fn check_write_access(
    mgr: &MemoryManager,
    namespace: &str,
) -> Result<(), MemoryInterfaceError> {
    match mgr.access_level(namespace) {
        Some(MemoryAccess::ReadWrite) => Ok(()),
        Some(MemoryAccess::ReadOnly) => Err(MemoryInterfaceError::ReadOnly(namespace.to_string())),
        None => Err(MemoryInterfaceError::NoNamespace),
    }
}
/// Recalls learned procedures matching the exact trigger.
///
/// Returns a list (0 or 1 entry), matching the Python `List[dict]` semantics.
pub(super) fn recall_procedure_inner(
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
pub(super) fn learn_procedure_inner(
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
pub(super) fn procedure_entry_to_json(
    entry: apollia_memory::procedural::ProcedureEntry,
) -> serde_json::Value {
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
pub(super) fn export_inner(
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
pub(super) fn import_inner(
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
pub(super) fn json_value_to_py(py: Python<'_>, value: &serde_json::Value) -> PyResult<PyObject> {
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
pub(super) fn pyany_to_json(py: Python<'_>, obj: &PyObject) -> PyResult<serde_json::Value> {
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
pub(super) fn pydict_to_json(dict: &Bound<'_, PyDict>) -> PyResult<serde_json::Value> {
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
pub(super) fn expires_in_to_iso(secs: u64) -> String {
    let expiry = chrono::Utc::now() + chrono::Duration::seconds(secs as i64);
    expiry.format("%Y-%m-%dT%H:%M:%SZ").to_string()
}
