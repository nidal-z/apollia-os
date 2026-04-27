//! Export et import de la mémoire d'un agent par namespace.
//!
//! Format d'export `.apollia-memory` : JSON avec format_version = 1.
//! Utilisé par la CLI (`memory export` / `memory import`) et le Desktop IPC.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::store::{MemoryStore, MemoryStoreError};

/// Format d'export de la mémoire d'un namespace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryExport {
    /// Version du format (toujours 1 pour cette implémentation).
    pub format_version: u32,
    /// Namespace exporté.
    pub namespace: String,
    /// Horodatage UTC ISO 8601 de l'export.
    pub exported_at: String,
    /// Entrées épisodiques sérialisées.
    pub episodic: Vec<serde_json::Value>,
    /// Entrées sémantiques sérialisées.
    pub semantic: Vec<serde_json::Value>,
    /// Procédures sérialisées.
    pub procedural: Vec<serde_json::Value>,
}

/// Mode d'import : remplace ou fusionne les entrées existantes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportMode {
    /// Vide le namespace puis importe toutes les entrées.
    Replace,
    /// Insère les entrées absentes (ignore les doublons par id).
    Merge,
}

/// Erreur d'export/import.
#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    /// Erreur SQLite.
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    /// Erreur du MemoryStore.
    #[error("memory store error: {0}")]
    Store(#[from] MemoryStoreError),

    /// Erreur de sérialisation/désérialisation JSON.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// Format non reconnu.
    #[error("unsupported format version: {0}")]
    UnsupportedVersion(u32),

    /// Erreur I/O.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Exporte toutes les entrées d'un namespace dans un [`MemoryExport`].
///
/// Ouvre le fichier `.db` existant pour `namespace` dans `data_dir` et
/// sérialise toutes les entrées épisodiques, sémantiques et procédurales.
pub fn export_namespace(data_dir: &Path, namespace: &str) -> Result<MemoryExport, ExportError> {
    let db_path = data_dir.join(format!("{namespace}.db"));
    let store = MemoryStore::open(&db_path)?;
    let conn = store.conn();

    // Episodic
    let mut stmt = conn.prepare(
        "SELECT id, namespace, agent_id, task_id, content, importance, created_at, expires_at, metadata
         FROM episodic_memories WHERE namespace = ?1",
    )?;
    let episodic: Vec<serde_json::Value> = stmt
        .query_map(rusqlite::params![namespace], |row| {
            Ok(serde_json::json!({
                "id": row.get::<_, String>(0)?,
                "namespace": row.get::<_, String>(1)?,
                "agent_id": row.get::<_, String>(2)?,
                "task_id": row.get::<_, Option<String>>(3)?,
                "content": row.get::<_, String>(4)?,
                "importance": row.get::<_, f64>(5)?,
                "created_at": row.get::<_, String>(6)?,
                "expires_at": row.get::<_, Option<String>>(7)?,
                "metadata": row.get::<_, String>(8)?,
            }))
        })?
        .collect::<Result<_, _>>()?;

    // Semantic
    let mut stmt = conn.prepare(
        "SELECT id, namespace, key, value, source, confidence, created_at, updated_at, expires_at
         FROM semantic_memories WHERE namespace = ?1",
    )?;
    let semantic: Vec<serde_json::Value> = stmt
        .query_map(rusqlite::params![namespace], |row| {
            Ok(serde_json::json!({
                "id": row.get::<_, String>(0)?,
                "namespace": row.get::<_, String>(1)?,
                "key": row.get::<_, String>(2)?,
                "value": row.get::<_, String>(3)?,
                "source": row.get::<_, Option<String>>(4)?,
                "confidence": row.get::<_, f64>(5)?,
                "created_at": row.get::<_, String>(6)?,
                "updated_at": row.get::<_, String>(7)?,
                "expires_at": row.get::<_, Option<String>>(8)?,
            }))
        })?
        .collect::<Result<_, _>>()?;

    // Procedural
    let mut stmt = conn.prepare(
        "SELECT id, namespace, trigger_text, steps, success_count, last_used_at, created_at
         FROM procedural_memories WHERE namespace = ?1",
    )?;
    let procedural: Vec<serde_json::Value> = stmt
        .query_map(rusqlite::params![namespace], |row| {
            let steps_str: String = row.get(3)?;
            let steps: Vec<String> = serde_json::from_str(&steps_str).unwrap_or_default();
            Ok(serde_json::json!({
                "id": row.get::<_, String>(0)?,
                "namespace": row.get::<_, String>(1)?,
                "trigger": row.get::<_, String>(2)?,
                "steps": steps,
                "success_count": row.get::<_, u32>(4)?,
                "last_used_at": row.get::<_, String>(5)?,
                "created_at": row.get::<_, String>(6)?,
            }))
        })?
        .collect::<Result<_, _>>()?;

    let exported_at = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    Ok(MemoryExport {
        format_version: 1,
        namespace: namespace.to_string(),
        exported_at,
        episodic,
        semantic,
        procedural,
    })
}

/// Importe les entrées d'un [`MemoryExport`] dans un namespace.
///
/// - `ImportMode::Replace` : vide le namespace avant d'insérer.
/// - `ImportMode::Merge` : insère uniquement les entrées absentes (par `id`).
///
/// Crée le fichier `.db` si absent. Retourne le nombre total d'entrées importées.
pub fn import_namespace(
    data_dir: &Path,
    namespace: &str,
    export: &MemoryExport,
    mode: ImportMode,
) -> Result<usize, ExportError> {
    if export.format_version != 1 {
        return Err(ExportError::UnsupportedVersion(export.format_version));
    }

    std::fs::create_dir_all(data_dir)?;
    let db_path = data_dir.join(format!("{namespace}.db"));
    let store = MemoryStore::open(&db_path)?;
    let conn = store.conn();

    if mode == ImportMode::Replace {
        conn.execute(
            "DELETE FROM episodic_memories WHERE namespace = ?1",
            rusqlite::params![namespace],
        )?;
        conn.execute(
            "DELETE FROM semantic_memories WHERE namespace = ?1",
            rusqlite::params![namespace],
        )?;
        conn.execute(
            "DELETE FROM procedural_memories WHERE namespace = ?1",
            rusqlite::params![namespace],
        )?;
        // Also clear FTS index
        conn.execute(
            "DELETE FROM memory_fts WHERE namespace = ?1",
            rusqlite::params![namespace],
        )
        .ok(); // ignore if FTS table differs
    }

    let mut count = 0usize;

    // Insert episodic
    for entry in &export.episodic {
        let id = entry["id"].as_str().unwrap_or_default();
        let res = conn.execute(
            "INSERT OR IGNORE INTO episodic_memories (id, namespace, agent_id, task_id, content, importance, created_at, expires_at, metadata)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![
                id,
                namespace,
                entry["agent_id"].as_str().unwrap_or(""),
                entry["task_id"].as_str(),
                entry["content"].as_str().unwrap_or(""),
                entry["importance"].as_f64().unwrap_or(0.5),
                entry["created_at"].as_str().unwrap_or(""),
                entry["expires_at"].as_str(),
                entry["metadata"].as_str().unwrap_or("{}"),
            ],
        )?;
        count += res;
    }

    // Insert semantic
    for entry in &export.semantic {
        let res = conn.execute(
            "INSERT OR IGNORE INTO semantic_memories (id, namespace, key, value, source, confidence, created_at, updated_at, expires_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![
                entry["id"].as_str().unwrap_or_default(),
                namespace,
                entry["key"].as_str().unwrap_or(""),
                entry["value"].as_str().unwrap_or(""),
                entry["source"].as_str(),
                entry["confidence"].as_f64().unwrap_or(1.0),
                entry["created_at"].as_str().unwrap_or(""),
                entry["updated_at"].as_str().unwrap_or(""),
                entry["expires_at"].as_str(),
            ],
        )?;
        count += res;
    }

    // Insert procedural
    for entry in &export.procedural {
        let steps = serde_json::to_string(entry["steps"].as_array().unwrap_or(&vec![]))?;
        let res = conn.execute(
            "INSERT OR IGNORE INTO procedural_memories (id, namespace, trigger_text, steps, success_count, last_used_at, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                entry["id"].as_str().unwrap_or_default(),
                namespace,
                entry["trigger"].as_str().unwrap_or(""),
                steps,
                entry["success_count"].as_u64().unwrap_or(1) as u32,
                entry["last_used_at"].as_str().unwrap_or(""),
                entry["created_at"].as_str().unwrap_or(""),
            ],
        )?;
        count += res;
    }

    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup_namespace(dir: &TempDir, namespace: &str) -> MemoryStore {
        let db_path = dir.path().join(format!("{namespace}.db"));
        MemoryStore::open(&db_path).expect("open store")
    }

    fn seed_entries(store: &MemoryStore, namespace: &str) {
        let conn = store.conn();

        // Episodic
        conn.execute(
            "INSERT INTO episodic_memories (id, namespace, agent_id, content, importance, created_at, metadata)
             VALUES ('ep-1', ?1, 'agent', 'épisode test', 0.7, '2026-01-01T00:00:00Z', '{}')",
            rusqlite::params![namespace],
        )
        .unwrap();

        // Semantic
        conn.execute(
            "INSERT INTO semantic_memories (id, namespace, key, value, confidence, created_at, updated_at)
             VALUES ('sem-1', ?1, 'budget', '\"15000\"', 0.9, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            rusqlite::params![namespace],
        )
        .unwrap();

        // Procedural
        conn.execute(
            "INSERT INTO procedural_memories (id, namespace, trigger_text, steps, success_count, last_used_at, created_at)
             VALUES ('proc-1', ?1, 'analyser rapport', '[\"step1\",\"step2\"]', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            rusqlite::params![namespace],
        )
        .unwrap();
    }

    // FC22 — export serializes episodic, semantic, procedural entries
    #[test]
    fn test_export_serializes_all_types() {
        // GIVEN a namespace with entries of each type
        let dir = TempDir::new().expect("create temp dir");
        let store = setup_namespace(&dir, "agent-x");
        seed_entries(&store, "agent-x");
        drop(store);

        // WHEN export
        let export = export_namespace(dir.path(), "agent-x").expect("export");

        // THEN all entries are present
        assert_eq!(export.format_version, 1);
        assert_eq!(export.namespace, "agent-x");
        assert_eq!(export.episodic.len(), 1);
        assert_eq!(export.semantic.len(), 1);
        assert_eq!(export.procedural.len(), 1);
        assert_eq!(export.episodic[0]["content"], "épisode test");
        assert_eq!(export.semantic[0]["key"], "budget");
        assert_eq!(export.procedural[0]["trigger"], "analyser rapport");
    }

    // FC22 — import with replace clears namespace and restores entries
    #[test]
    fn test_import_replace_round_trip() {
        // GIVEN a namespace with entries
        let dir = TempDir::new().expect("create temp dir");
        let store = setup_namespace(&dir, "agent-y");
        seed_entries(&store, "agent-y");
        drop(store);

        let export = export_namespace(dir.path(), "agent-y").expect("export");
        assert_eq!(export.episodic.len(), 1);

        // Clear the namespace manually
        {
            let store = setup_namespace(&dir, "agent-y");
            let conn = store.conn();
            conn.execute("DELETE FROM episodic_memories WHERE namespace = 'agent-y'", []).unwrap();
            conn.execute("DELETE FROM semantic_memories WHERE namespace = 'agent-y'", []).unwrap();
            conn.execute("DELETE FROM procedural_memories WHERE namespace = 'agent-y'", []).unwrap();
        }

        // WHEN import with Replace mode
        let count = import_namespace(dir.path(), "agent-y", &export, ImportMode::Replace)
            .expect("import");

        // THEN 3 entries restored
        assert_eq!(count, 3);
    }

    // FC22 — import with unsupported version returns error
    #[test]
    fn test_import_unsupported_version_error() {
        // GIVEN an export with version 99
        let dir = TempDir::new().expect("create temp dir");
        let bad_export = MemoryExport {
            format_version: 99,
            namespace: "ns".to_string(),
            exported_at: "2026-01-01T00:00:00Z".to_string(),
            episodic: vec![],
            semantic: vec![],
            procedural: vec![],
        };

        // WHEN import
        let result = import_namespace(dir.path(), "ns", &bad_export, ImportMode::Replace);

        // THEN error
        assert!(matches!(result, Err(ExportError::UnsupportedVersion(99))));
    }
}
