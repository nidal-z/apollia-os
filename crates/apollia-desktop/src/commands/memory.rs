//! Tauri IPC commands for inspecting and managing memory.
//!
//! Accesses the SQLite files `~/.apollia/memory/<namespace>.db` directly via the
//! `apollia-memory` APIs (MemoryStore, EpisodicMemory, SemanticMemory,
//! ProceduralMemory, MemorySearch). Admin view: no namespace access control.
//! The commands that change the memory live in [`maintenance`].

use std::path::PathBuf;

use apollia_memory::episodic::EpisodicMemory;
use apollia_memory::injection_tracker::{global_entries_for, InjectedEntry};
use apollia_memory::procedural::ProceduralMemory;
use apollia_memory::search::{MemorySearch, SearchQuery, SearchSource};
use apollia_memory::semantic::SemanticMemory;
use apollia_memory::store::MemoryStore;
use serde::Serialize;

pub mod maintenance;

/// Unified memory entry for display in the UI.
#[derive(Debug, Serialize)]
pub struct MemoryEntry {
    /// Entry UUID.
    pub id: String,
    /// Type: `"episodic"` | `"semantic"` | `"procedural"`.
    pub entry_type: String,
    /// Key or subject (key for semantic, trigger for procedural, agent_id for episodic).
    pub key: String,
    /// Value or text content.
    pub value: String,
    /// ISO 8601 creation date.
    pub created_at: String,
    /// ISO 8601 expiration date, or `null` if no TTL.
    pub expires_at: Option<String>,
    /// BM25 score (search mode only, otherwise `null`).
    pub score: Option<f64>,
}

/// FTS5 search result for the UI.
#[derive(Debug, Serialize)]
pub struct MemorySearchResult {
    /// UUID of the source entry.
    pub id: String,
    /// Type: `"episodic"` | `"semantic"`.
    pub entry_type: String,
    /// Text content of the result.
    pub content: String,
    /// BM25 score (higher = more relevant).
    pub score: f64,
    /// Importance (episodic) or confidence (semantic).
    pub relevance: Option<f64>,
    /// ISO 8601 creation date.
    pub created_at: String,
}

/// Resolves the memory directory path (`~/.apollia/memory/`).
fn memory_base_dir() -> Result<PathBuf, String> {
    let home = apollia_core::paths::home_string_or_err()
        .map_err(|_| "cannot determine home directory: $HOME not set".to_string())?;
    Ok(apollia_core::paths::data_dir_under(home).join("memory"))
}

/// Lists the available memory namespaces (one `.db` file = one namespace).
///
/// Scans `~/.apollia/memory/*.db` and returns the names without the extension.
#[tauri::command]
pub async fn list_memory_namespaces() -> Result<Vec<String>, String> {
    let base = memory_base_dir()?;

    if !base.exists() {
        return Ok(Vec::new());
    }

    let entries =
        std::fs::read_dir(&base).map_err(|e| format!("failed to read memory directory: {e}"))?;

    let mut namespaces: Vec<String> = entries
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("db") {
                path.file_stem().and_then(|s| s.to_str()).map(String::from)
            } else {
                None
            }
        })
        .collect();

    namespaces.sort();
    Ok(namespaces)
}

/// Lists all memory entries of a namespace (episodic + semantic + procedural).
///
/// Returns a unified list of entries sorted by creation date descending.
#[tauri::command]
pub async fn list_memory_entries(namespace: String) -> Result<Vec<MemoryEntry>, String> {
    let base = memory_base_dir()?;
    let db_path = base.join(format!("{namespace}.db"));

    if !db_path.exists() {
        return Ok(Vec::new());
    }

    let store = MemoryStore::open(&db_path).map_err(|e| format!("failed to open store: {e}"))?;
    let mut entries = Vec::new();

    let episodic = EpisodicMemory::new(&store);
    if let Ok(episodes) = episodic.history(&namespace, 500, None) {
        for ep in episodes {
            entries.push(MemoryEntry {
                id: ep.id,
                entry_type: "episodic".to_string(),
                key: ep.agent_id,
                value: ep.content,
                created_at: ep.created_at,
                expires_at: ep.expires_at,
                score: None,
            });
        }
    }

    let semantic = SemanticMemory::new(&store);
    if let Ok(facts) = semantic.recall_all(&namespace, None) {
        for fact in facts {
            entries.push(MemoryEntry {
                id: fact.id,
                entry_type: "semantic".to_string(),
                key: fact.key,
                value: serde_json::to_string(&fact.value).unwrap_or_default(),
                created_at: fact.created_at,
                expires_at: fact.expires_at,
                score: None,
            });
        }
    }

    let procedural = ProceduralMemory::new(&store);
    if let Ok(procedures) = procedural.list(&namespace) {
        for proc in procedures {
            entries.push(MemoryEntry {
                id: proc.id,
                entry_type: "procedural".to_string(),
                key: proc.trigger,
                value: proc.steps.join(" -> "),
                created_at: proc.created_at,
                expires_at: None,
                score: None,
            });
        }
    }

    entries.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    Ok(entries)
}

/// FTS5 search within a memory namespace.
///
/// Returns the results sorted by BM25 score descending.
#[tauri::command]
pub async fn search_memory(
    namespace: String,
    query: String,
) -> Result<Vec<MemorySearchResult>, String> {
    let base = memory_base_dir()?;
    let db_path = base.join(format!("{namespace}.db"));

    if !db_path.exists() {
        return Ok(Vec::new());
    }

    let store = MemoryStore::open(&db_path).map_err(|e| format!("failed to open store: {e}"))?;
    let search = MemorySearch::new(&store);

    let results = search
        .query(SearchQuery {
            namespace: &namespace,
            query: &query,
            limit: 50,
            sources: None,
            min_importance: None,
        })
        .map_err(|e| format!("search failed: {e}"))?;

    Ok(results
        .into_iter()
        .map(|r| MemorySearchResult {
            id: r.source_id,
            entry_type: match r.source {
                SearchSource::Episodic => "episodic".to_string(),
                SearchSource::Semantic => "semantic".to_string(),
            },
            content: r.content,
            score: r.score,
            relevance: r.relevance,
            created_at: r.created_at,
        })
        .collect())
}

/// Returns the memory entries the agent injected into a specific turn.
///
/// Backs the `<InjectedMemorySheet />` panel (memory injection visibility).
/// Reads from the process-local injection
/// tracker populated by the PyO3 `recall_entry()` / `recall_all()` wrappers.
#[tauri::command]
pub async fn get_injected_memory_entries(turn_id: String) -> Result<Vec<InjectedEntry>, String> {
    Ok(global_entries_for(&turn_id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_memory::injection_tracker::{global_record, global_tracker_clear, InjectedEntry};
    use std::sync::{Mutex, OnceLock};

    // Serialises tests that mutate the global injection tracker to prevent races.
    fn tracker_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

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

    #[tokio::test]
    // The tracker lock serializes tests touching the global injected-memory
    // tracker; it is deliberately held across the command await so a concurrent
    // test cannot clear or record between this test's setup and assertion.
    #[allow(clippy::await_holding_lock)]
    async fn test_get_injected_memory_entries_returns_tracked_entries() {
        let _guard = tracker_lock().lock().unwrap();
        // GIVEN a turn with two injected entries recorded in the global tracker
        global_tracker_clear();
        let turn = "turn-p7-test";
        global_record(
            turn,
            InjectedEntry {
                id: "sem-1".to_string(),
                content_preview: "client dupont budget 15000".to_string(),
                namespace: "agent-x".to_string(),
                injection_reason: "Matched query: budget client".to_string(),
                relevance_score: 0.92,
            },
        );
        global_record(
            turn,
            InjectedEntry {
                id: "ep-1".to_string(),
                content_preview: "Quote sent to Dupont".to_string(),
                namespace: "agent-x".to_string(),
                injection_reason: "Contexte devis".to_string(),
                relevance_score: 0.71,
            },
        );

        // WHEN the Tauri command is invoked
        let entries = get_injected_memory_entries(turn.to_string())
            .await
            .expect("command should succeed");

        // THEN both entries are returned with preserved fields
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].id, "sem-1");
        assert_eq!(entries[0].namespace, "agent-x");
        assert_eq!(entries[0].injection_reason, "Matched query: budget client");
        assert!((entries[0].relevance_score - 0.92).abs() < f32::EPSILON);
        assert_eq!(entries[1].id, "ep-1");

        // AND the JSON payload matches the frontend contract
        let json = serde_json::to_value(&entries).expect("serialize");
        assert_eq!(json[0]["content_preview"], "client dupont budget 15000");
        let score = json[0]["relevance_score"]
            .as_f64()
            .expect("score is number");
        assert!((score - 0.92).abs() < 1e-4);
    }

    #[tokio::test]
    // Held across the await to keep global-tracker tests serial (see above).
    #[allow(clippy::await_holding_lock)]
    async fn test_get_injected_memory_entries_unknown_turn_is_empty() {
        let _guard = tracker_lock().lock().unwrap();
        // GIVEN a cleared tracker
        global_tracker_clear();

        // WHEN the command is invoked for a turn that was never tracked
        let entries = get_injected_memory_entries("never-seen".to_string())
            .await
            .expect("command should succeed");

        // THEN the response is empty (not an error)
        assert!(entries.is_empty());
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
