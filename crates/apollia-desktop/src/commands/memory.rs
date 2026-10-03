//! Tauri IPC commands for inspecting and managing memory.
//!
//! Accesses the SQLite files `~/.apollia/memory/<namespace>.db` directly via the
//! `apollia-memory` APIs (MemoryStore, EpisodicMemory, SemanticMemory,
//! ProceduralMemory, MemorySearch). Admin view: no namespace access control.

use std::path::PathBuf;

use apollia_memory::episodic::EpisodicMemory;
use apollia_memory::procedural::ProceduralMemory;
use apollia_memory::semantic::SemanticMemory;
use apollia_memory::store::MemoryStore;
use apollia_runtime::memory_view;
pub use apollia_runtime::memory_view::{
    InjectedMemoryEntryView, MemoryEntryView as MemoryEntry,
    MemorySearchResultView as MemorySearchResult,
};
use tauri::State;

use crate::attached::{segment, RuntimeLink};

/// Resolves the memory directory path (`~/.apollia/memory/`).
fn memory_base_dir() -> Result<PathBuf, String> {
    let home = apollia_core::paths::home_string_or_err()
        .map_err(|_| "cannot determine home directory: $HOME not set".to_string())?;
    Ok(apollia_core::paths::data_dir_under(home).join("memory"))
}

/// Lists the available memory namespaces (one `.db` file = one namespace).
/// Attached to a daemon, `GET /api/v1/memory/namespaces` answers the same list.
#[tauri::command]
pub async fn list_memory_namespaces(link: State<'_, RuntimeLink>) -> Result<Vec<String>, String> {
    namespaces_on(link.inner()).await
}

pub(crate) async fn namespaces_on(link: &RuntimeLink) -> Result<Vec<String>, String> {
    match link {
        RuntimeLink::Embedded(_) => memory_view::list_namespaces(memory_base_dir()?)
            .await
            .map_err(|e| e.to_string()),
        RuntimeLink::Attached(remote) => Ok(remote.get("/api/v1/memory/namespaces").await?),
    }
}

/// Lists all memory entries of a namespace (episodic + semantic + procedural),
/// newest first.
#[tauri::command]
pub async fn list_memory_entries(
    link: State<'_, RuntimeLink>,
    namespace: String,
) -> Result<Vec<MemoryEntry>, String> {
    entries_on(link.inner(), namespace).await
}

pub(crate) async fn entries_on(
    link: &RuntimeLink,
    namespace: String,
) -> Result<Vec<MemoryEntry>, String> {
    match link {
        RuntimeLink::Embedded(_) => memory_view::namespace_entries(memory_base_dir()?, namespace)
            .await
            .map_err(|e| e.to_string()),
        RuntimeLink::Attached(remote) => {
            let path = format!("/api/v1/memory/namespaces/{}/entries", segment(&namespace));
            Ok(remote.get(&path).await?)
        }
    }
}

/// FTS5 search within a memory namespace, best BM25 score first.
#[tauri::command]
pub async fn search_memory(
    link: State<'_, RuntimeLink>,
    namespace: String,
    query: String,
) -> Result<Vec<MemorySearchResult>, String> {
    search_on(link.inner(), namespace, query).await
}

pub(crate) async fn search_on(
    link: &RuntimeLink,
    namespace: String,
    query: String,
) -> Result<Vec<MemorySearchResult>, String> {
    match link {
        RuntimeLink::Embedded(_) => memory_view::search(memory_base_dir()?, namespace, query)
            .await
            .map_err(|e| e.to_string()),
        RuntimeLink::Attached(remote) => {
            let path = format!(
                "/api/v1/memory/namespaces/{}/search?q={}",
                segment(&namespace),
                segment(&query)
            );
            Ok(remote.get(&path).await?)
        }
    }
}

/// Clears all memory entries of a namespace by type.
///
/// `memory_type` can be `"episodic"`, `"semantic"`, `"procedural"`, or `"all"` (default).
/// Returns the total number of removed entries.
#[tauri::command]
pub async fn clear_memory(namespace: String, memory_type: Option<String>) -> Result<u64, String> {
    let base = memory_base_dir()?;
    let db_path = base.join(format!("{namespace}.db"));

    if !db_path.exists() {
        return Ok(0);
    }

    let store = MemoryStore::open(&db_path).map_err(|e| format!("failed to open store: {e}"))?;
    let kind = memory_type.as_deref().unwrap_or("all");
    let mut total: u64 = 0;

    match kind {
        "episodic" => {
            total += store
                .clear_episodic(&namespace)
                .map_err(|e| format!("clear_episodic failed: {e}"))?;
        }
        "semantic" => {
            total += store
                .clear_semantic(&namespace)
                .map_err(|e| format!("clear_semantic failed: {e}"))?;
        }
        "procedural" => {
            total += store
                .clear_procedural(&namespace)
                .map_err(|e| format!("clear_procedural failed: {e}"))?;
        }
        // "all" or any unrecognized scope: clear everything.
        _ => {
            total += store
                .clear_episodic(&namespace)
                .map_err(|e| format!("clear_episodic failed: {e}"))?;
            total += store
                .clear_semantic(&namespace)
                .map_err(|e| format!("clear_semantic failed: {e}"))?;
            total += store
                .clear_procedural(&namespace)
                .map_err(|e| format!("clear_procedural failed: {e}"))?;
        }
    }

    Ok(total)
}

/// Purges memory entries older than N days.
///
/// `memory_type` can be `"episodic"`, `"semantic"`, `"procedural"`, or `"all"` (default).
/// Returns the total number of purged entries.
#[tauri::command]
pub async fn purge_memory(
    namespace: String,
    older_than_days: u32,
    memory_type: Option<String>,
) -> Result<u64, String> {
    let base = memory_base_dir()?;
    let db_path = base.join(format!("{namespace}.db"));

    if !db_path.exists() {
        return Ok(0);
    }

    let store = MemoryStore::open(&db_path).map_err(|e| format!("failed to open store: {e}"))?;
    let kind = memory_type.as_deref().unwrap_or("all");
    let mut total: u64 = 0;

    match kind {
        "episodic" => {
            let ep = EpisodicMemory::new(&store);
            total += ep
                .purge_older_than(&namespace, older_than_days)
                .map_err(|e| format!("purge_episodic failed: {e}"))?;
        }
        "semantic" => {
            let sem = SemanticMemory::new(&store);
            total += sem
                .purge_older_than(&namespace, older_than_days)
                .map_err(|e| format!("purge_semantic failed: {e}"))?;
        }
        "procedural" => {
            let proc = ProceduralMemory::new(&store);
            total += proc
                .purge_older_than(&namespace, older_than_days)
                .map_err(|e| format!("purge_procedural failed: {e}"))?;
        }
        // "all" or any unrecognized scope: purge everything.
        _ => {
            let ep = EpisodicMemory::new(&store);
            total += ep
                .purge_older_than(&namespace, older_than_days)
                .map_err(|e| format!("purge_episodic failed: {e}"))?;
            let sem = SemanticMemory::new(&store);
            total += sem
                .purge_older_than(&namespace, older_than_days)
                .map_err(|e| format!("purge_semantic failed: {e}"))?;
            let proc = ProceduralMemory::new(&store);
            total += proc
                .purge_older_than(&namespace, older_than_days)
                .map_err(|e| format!("purge_procedural failed: {e}"))?;
        }
    }

    Ok(total)
}

/// Deletes a memory entry by its UUID.
///
/// Searches all tables (episodic, semantic, procedural) and deletes the
/// matching entry along with its FTS5 index.
#[tauri::command]
pub async fn delete_memory_entry(namespace: String, entry_id: String) -> Result<bool, String> {
    let base = memory_base_dir()?;
    let db_path = base.join(format!("{namespace}.db"));

    if !db_path.exists() {
        return Err(format!("namespace '{namespace}' not found"));
    }

    let store = MemoryStore::open(&db_path).map_err(|e| format!("failed to open store: {e}"))?;

    store
        .delete_entry_by_id(&entry_id)
        .map_err(|e| format!("failed to delete entry: {e}"))
}

/// Exports a namespace's memory to a JSON file.
///
/// `output_path`: absolute path of the output file.
/// Returns a confirmation message with the export statistics.
#[tauri::command]
pub async fn memory_export_namespace(
    namespace: String,
    output_path: String,
) -> Result<String, String> {
    let base = memory_base_dir()?;

    let out = std::path::PathBuf::from(&output_path);
    let export = apollia_memory::export::export_namespace(&base, &namespace)
        .map_err(|e| format!("export failed: {e}"))?;

    let json_str =
        serde_json::to_string_pretty(&export).map_err(|e| format!("serialize failed: {e}"))?;
    std::fs::write(&out, &json_str).map_err(|e| format!("write failed: {e}"))?;

    Ok(format!(
        "Exported {} episodic, {} semantic, {} procedural entries to {}",
        export.episodic.len(),
        export.semantic.len(),
        export.procedural.len(),
        out.display(),
    ))
}

/// Imports memory from a JSON file into a namespace.
///
/// `mode` can be `"replace"` or `"merge"` (default: `"merge"`).
/// Returns the number of imported entries.
#[tauri::command]
pub async fn memory_import_namespace(
    namespace: String,
    input_path: String,
    mode: String,
) -> Result<String, String> {
    let base = memory_base_dir()?;

    let content = std::fs::read_to_string(&input_path).map_err(|e| format!("read failed: {e}"))?;
    let export: apollia_memory::export::MemoryExport =
        serde_json::from_str(&content).map_err(|e| format!("parse failed: {e}"))?;

    let import_mode = if mode == "replace" {
        apollia_memory::export::ImportMode::Replace
    } else {
        apollia_memory::export::ImportMode::Merge
    };

    let count = apollia_memory::export::import_namespace(&base, &namespace, &export, import_mode)
        .map_err(|e| format!("import failed: {e}"))?;

    Ok(format!(
        "{count} entries imported into namespace '{namespace}' (mode: {mode})"
    ))
}

/// Returns the memory entries the agent injected into a specific turn.
///
/// Backs the `<InjectedMemorySheet />` panel. The tracker is the one of the
/// process running the agents: this one embedded, the daemon's attached.
#[tauri::command]
pub async fn get_injected_memory_entries(
    link: State<'_, RuntimeLink>,
    turn_id: String,
) -> Result<Vec<InjectedMemoryEntryView>, String> {
    injected_on(link.inner(), turn_id).await
}

pub(crate) async fn injected_on(
    link: &RuntimeLink,
    turn_id: String,
) -> Result<Vec<InjectedMemoryEntryView>, String> {
    match link {
        RuntimeLink::Embedded(_) => Ok(memory_view::injected_entries(&turn_id)),
        RuntimeLink::Attached(remote) => {
            let path = format!("/api/v1/memory/injections/{}", segment(&turn_id));
            Ok(remote.get(&path).await?)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_entry_serializes() {
        // GIVEN a MemoryEntry
        let entry = MemoryEntry {
            id: "e-abc123".to_string(),
            entry_type: "episodic".to_string(),
            key: "agent-1".to_string(),
            value: "Quote sent to Dupont".to_string(),
            created_at: "2026-03-13T10:00:00Z".to_string(),
            expires_at: None,
            score: None,
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&entry).expect("serialize");

        // THEN all fields are present and correct
        assert_eq!(json["id"], "e-abc123");
        assert_eq!(json["entry_type"], "episodic");
        assert_eq!(json["key"], "agent-1");
        assert!(json["expires_at"].is_null());
        assert!(json["score"].is_null());
    }

    #[test]
    fn test_memory_entry_with_ttl_serializes() {
        // GIVEN an entry with TTL and search score
        let entry = MemoryEntry {
            id: "s-def456".to_string(),
            entry_type: "semantic".to_string(),
            key: "client.dupont.budget".to_string(),
            value: "15000".to_string(),
            created_at: "2026-03-13T10:00:00Z".to_string(),
            expires_at: Some("2026-06-13T10:00:00Z".to_string()),
            score: Some(2.5),
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&entry).expect("serialize");

        // THEN TTL and score are present
        assert_eq!(json["expires_at"], "2026-06-13T10:00:00Z");
        assert_eq!(json["score"], 2.5);
    }

    #[test]
    fn test_memory_search_result_serializes() {
        // GIVEN a search result
        let result = MemorySearchResult {
            id: "e-abc123".to_string(),
            entry_type: "episodic".to_string(),
            content: "Quote sent to Dupont SA".to_string(),
            score: 2.5,
            relevance: Some(0.8),
            created_at: "2026-03-13T10:00:00Z".to_string(),
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&result).expect("serialize");

        // THEN all fields are correct
        assert_eq!(json["id"], "e-abc123");
        assert_eq!(json["entry_type"], "episodic");
        assert_eq!(json["score"], 2.5);
        assert_eq!(json["relevance"], 0.8);
    }

    #[test]
    fn test_memory_search_result_without_relevance_serializes() {
        // GIVEN a search result without relevance
        let result = MemorySearchResult {
            id: "p-xyz789".to_string(),
            entry_type: "semantic".to_string(),
            content: "Budget client".to_string(),
            score: 1.2,
            relevance: None,
            created_at: "2026-03-13T10:00:00Z".to_string(),
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&result).expect("serialize");

        // THEN relevance is null
        assert!(json["relevance"].is_null());
    }

    #[test]
    fn test_memory_base_dir_resolves() {
        // GIVEN/WHEN
        let result = memory_base_dir();

        // THEN it resolves to a valid path containing the expected suffix
        assert!(result.is_ok());
        let path = result.expect("path");
        assert!(path.to_str().expect("utf8").contains(".apollia/memory"));
    }

    #[test]
    fn test_memory_entry_procedural_serializes() {
        // GIVEN a procedural memory entry
        let entry = MemoryEntry {
            id: "p-proc001".to_string(),
            entry_type: "procedural".to_string(),
            key: "on_error".to_string(),
            value: "retry -> escalate -> notify".to_string(),
            created_at: "2026-03-14T09:00:00Z".to_string(),
            expires_at: None,
            score: None,
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&entry).expect("serialize");

        // THEN entry_type is "procedural" and expires_at is null
        assert_eq!(json["entry_type"], "procedural");
        assert_eq!(json["key"], "on_error");
        assert!(json["expires_at"].is_null());
    }
}

#[cfg(test)]
mod attached_tests;
