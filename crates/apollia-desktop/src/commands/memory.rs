//! Commandes IPC Tauri pour l'inspection et la gestion de la mémoire.
//!
//! Accède directement aux fichiers SQLite `~/.apollia/memory/<namespace>.db`
//! via les APIs `apollia-memory` (MemoryStore, EpisodicMemory, SemanticMemory,
//! ProceduralMemory, MemorySearch). Vue admin — pas de contrôle d'accès namespace.

use std::path::PathBuf;

use apollia_memory::episodic::EpisodicMemory;
use apollia_memory::procedural::ProceduralMemory;
use apollia_memory::search::{MemorySearch, SearchSource};
use apollia_memory::semantic::SemanticMemory;
use apollia_memory::store::MemoryStore;
use serde::Serialize;

/// Répertoire par défaut des fichiers mémoire.
const MEMORY_DIR: &str = ".apollia/memory";

/// Entrée mémoire unifiée pour l'affichage dans l'UI.
#[derive(Debug, Serialize)]
pub struct MemoryEntry {
    /// UUID de l'entrée.
    pub id: String,
    /// Type : `"episodic"` | `"semantic"` | `"procedural"`.
    pub entry_type: String,
    /// Clé ou sujet (key pour semantic, trigger pour procedural, agent_id pour episodic).
    pub key: String,
    /// Valeur ou contenu textuel.
    pub value: String,
    /// Date de création ISO 8601.
    pub created_at: String,
    /// Date d'expiration ISO 8601 ou `null` si pas de TTL.
    pub expires_at: Option<String>,
    /// Score BM25 (uniquement en mode recherche, sinon `null`).
    pub score: Option<f64>,
}

/// Résultat de recherche FTS5 pour l'UI.
#[derive(Debug, Serialize)]
pub struct MemorySearchResult {
    /// UUID de l'entrée source.
    pub id: String,
    /// Type : `"episodic"` | `"semantic"`.
    pub entry_type: String,
    /// Contenu textuel du résultat.
    pub content: String,
    /// Score BM25 (plus élevé = plus pertinent).
    pub score: f64,
    /// Importance (episodic) ou confiance (semantic).
    pub relevance: Option<f64>,
    /// Date de création ISO 8601.
    pub created_at: String,
}

/// Résout le chemin du répertoire mémoire (`~/.apollia/memory/`).
fn memory_base_dir() -> Result<PathBuf, String> {
    let home = std::env::var("HOME")
        .map_err(|_| "cannot determine home directory: $HOME not set".to_string())?;
    Ok(PathBuf::from(home).join(MEMORY_DIR))
}

/// Liste les namespaces mémoire disponibles (un fichier `.db` = un namespace).
///
/// Scanne `~/.apollia/memory/*.db` et retourne les noms sans extension.
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

/// Liste toutes les entrées mémoire d'un namespace (episodic + semantic + procedural).
///
/// Retourne une liste unifiée d'entrées triées par date de création décroissante.
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

/// Recherche FTS5 dans un namespace mémoire.
///
/// Retourne les résultats triés par score BM25 décroissant.
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
        .query(&namespace, &query, 50, None, None)
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

/// Efface toutes les entrées mémoire d'un namespace par type.
///
/// `memory_type` peut être `"episodic"`, `"semantic"`, `"procedural"`, ou `"all"` (défaut).
/// Retourne le nombre total d'entrées supprimées.
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
        "all" | _ => {
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

/// Purge les entrées mémoire plus anciennes que N jours.
///
/// `memory_type` peut être `"episodic"`, `"semantic"`, `"procedural"`, ou `"all"` (défaut).
/// Retourne le nombre total d'entrées purgées.
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
        "all" | _ => {
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

/// Supprime une entrée mémoire par son UUID.
///
/// Cherche dans toutes les tables (episodic, semantic, procedural) et supprime
/// l'entrée correspondante ainsi que son index FTS5.
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
            value: "Devis envoyé à Dupont".to_string(),
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
            score: Some(3.14),
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&entry).expect("serialize");

        // THEN TTL and score are present
        assert_eq!(json["expires_at"], "2026-06-13T10:00:00Z");
        assert_eq!(json["score"], 3.14);
    }

    #[test]
    fn test_memory_search_result_serializes() {
        // GIVEN a search result
        let result = MemorySearchResult {
            id: "e-abc123".to_string(),
            entry_type: "episodic".to_string(),
            content: "Devis envoyé à Dupont SA".to_string(),
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
