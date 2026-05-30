//! Repository en lecture pour `runtime_events.db`.
//!
//! L'écriture passe exclusivement par [`super::persistor::EventPersistorHandle`].
//! Ce module fournit les requêtes paginées que consomment l'API
//! `GET /api/v1/tasks/{id}/trace` et la routine de purge.

use std::path::{Path, PathBuf};

use serde::Serialize;
use thiserror::Error;

/// Représentation in-memory d'une ligne de `runtime_events`.
///
/// Sérialisable en JSON pour l'API. Le champ `payload_json` reste opaque à
/// ce niveau ; chaque `kind` définit son schéma propre côté UI / consumers.
#[derive(Debug, Clone, Serialize)]
pub struct RuntimeEventRecord {
    /// UUID v7, ordonnable lexicographiquement.
    pub event_id: String,
    /// Tâche concernée.
    pub task_id: String,
    /// Agent émetteur.
    pub agent_id: String,
    /// Self-FK pour nesting (tool_call_completed → tool_call_started, A2A).
    pub parent_event_id: Option<String>,
    /// ID partagé sur une chaîne A2A (NULL pour les exécutions racine).
    pub correlation_id: Option<String>,
    /// Tour ReAct (NULL hors loop).
    pub step_num: Option<i64>,
    /// Discriminant, voir `EventKind` côté UI.
    pub kind: String,
    /// Payload typé par kind, JSON brut.
    pub payload_json: String,
    /// ISO 8601 RFC 3339, millisecondes incluses.
    pub ts: String,
    /// Unix seconds, utilisé par la purge.
    pub created_at_unix: i64,
}

/// Erreurs de lecture.
#[derive(Debug, Error)]
pub enum RepositoryError {
    /// Échec d'ouverture du fichier SQLite.
    #[error("failed to open runtime_events database at {path}: {source}")]
    OpenFailed {
        /// Chemin tenté.
        path: PathBuf,
        /// Cause SQLite.
        source: rusqlite::Error,
    },
    /// Erreur SQLite à l'exécution d'une requête.
    #[error("query failed: {0}")]
    Query(#[from] rusqlite::Error),
}

/// Repository synchrone (les appels SQLite blocants sont attendus dans
/// `tokio::task::spawn_blocking` côté handler).
pub struct RuntimeEventsRepository {
    conn: rusqlite::Connection,
}

impl RuntimeEventsRepository {
    /// Ouvre la base en lecture seule logique (pas d'écriture via ce handle).
    pub fn open(db_path: &Path) -> Result<Self, RepositoryError> {
        let conn =
            rusqlite::Connection::open(db_path).map_err(|source| RepositoryError::OpenFailed {
                path: db_path.to_path_buf(),
                source,
            })?;
        // Mode WAL côté lecteur aussi : permet la lecture concurrente sans
        // blocage par le persistor.
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        Ok(Self { conn })
    }

    /// Liste les événements d'une tâche, ordonnés chronologiquement (UUIDv7
    /// → ordre lex == ordre de création).
    ///
    /// `since` : si présent, ne retourne que les événements *strictement*
    /// après ce `event_id`. Permet la pagination par curseur sans `OFFSET`.
    /// `limit` : borne supérieure sur le nombre de lignes retournées.
    pub fn list_for_task(
        &self,
        task_id: &str,
        since: Option<&str>,
        limit: usize,
    ) -> Result<Vec<RuntimeEventRecord>, RepositoryError> {
        let limit_i = limit as i64;
        let mut stmt = match since {
            Some(_) => self.conn.prepare(
                "SELECT event_id, task_id, agent_id, parent_event_id, correlation_id, \
                 step_num, kind, payload_json, ts, created_at_unix \
                 FROM runtime_events \
                 WHERE task_id = ?1 AND event_id > ?2 \
                 ORDER BY event_id ASC \
                 LIMIT ?3",
            )?,
            None => self.conn.prepare(
                "SELECT event_id, task_id, agent_id, parent_event_id, correlation_id, \
                 step_num, kind, payload_json, ts, created_at_unix \
                 FROM runtime_events \
                 WHERE task_id = ?1 \
                 ORDER BY event_id ASC \
                 LIMIT ?2",
            )?,
        };

        let map_row = |row: &rusqlite::Row<'_>| {
            Ok(RuntimeEventRecord {
                event_id: row.get(0)?,
                task_id: row.get(1)?,
                agent_id: row.get(2)?,
                parent_event_id: row.get(3)?,
                correlation_id: row.get(4)?,
                step_num: row.get(5)?,
                kind: row.get(6)?,
                payload_json: row.get(7)?,
                ts: row.get(8)?,
                created_at_unix: row.get(9)?,
            })
        };

        let rows: Vec<RuntimeEventRecord> = match since {
            Some(s) => stmt
                .query_map(rusqlite::params![task_id, s, limit_i], map_row)?
                .collect::<rusqlite::Result<Vec<_>>>()?,
            None => stmt
                .query_map(rusqlite::params![task_id, limit_i], map_row)?
                .collect::<rusqlite::Result<Vec<_>>>()?,
        };

        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::observability::persistor::EventPersistorHandle;
    use tempfile::tempdir;

    #[tokio::test]
    async fn list_for_task_returns_chronological() {
        // GIVEN a persistor with three events for the same task, written in
        // the right order (UUID v7 generated by chrono::Utc::now would be
        // ordered, but we'll forge stable UUIDv7-ish ids manually).
        let dir = tempdir().expect("tempdir");
        let db = dir.path().join("rt.db");
        let handle = EventPersistorHandle::open(&db).await.expect("open");

        let now_unix = chrono::Utc::now().timestamp();
        let now_iso = chrono::Utc::now().to_rfc3339();

        for (i, kind) in [(0, "agent_log"), (1, "agent_log"), (2, "agent_log")] {
            handle.append(RuntimeEventRecord {
                event_id: format!("01900000-0000-7000-8000-00000000000{i}"),
                task_id: "T1".into(),
                agent_id: "A1".into(),
                parent_event_id: None,
                correlation_id: None,
                step_num: None,
                kind: kind.into(),
                payload_json: format!("{{\"i\":{i}}}"),
                ts: now_iso.clone(),
                created_at_unix: now_unix,
            });
        }

        handle.shutdown().await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // WHEN we query the repository
        let repo = RuntimeEventsRepository::open(&db).expect("open repo");
        let rows = repo.list_for_task("T1", None, 10).expect("list");

        // THEN all three rows are returned, in insertion order
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].payload_json, "{\"i\":0}");
        assert_eq!(rows[2].payload_json, "{\"i\":2}");
    }

    #[tokio::test]
    async fn list_for_task_supports_cursor_pagination() {
        // GIVEN a persistor with three events
        let dir = tempdir().expect("tempdir");
        let db = dir.path().join("rt.db");
        let handle = EventPersistorHandle::open(&db).await.expect("open");

        let now_unix = chrono::Utc::now().timestamp();
        let now_iso = chrono::Utc::now().to_rfc3339();
        for i in 0..3 {
            handle.append(RuntimeEventRecord {
                event_id: format!("01900000-0000-7000-8000-00000000000{i}"),
                task_id: "T1".into(),
                agent_id: "A1".into(),
                parent_event_id: None,
                correlation_id: None,
                step_num: None,
                kind: "agent_log".into(),
                payload_json: "{}".into(),
                ts: now_iso.clone(),
                created_at_unix: now_unix,
            });
        }
        handle.shutdown().await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let repo = RuntimeEventsRepository::open(&db).expect("open repo");

        // WHEN we paginate after the first row
        let cursor = "01900000-0000-7000-8000-000000000000";
        let rows = repo.list_for_task("T1", Some(cursor), 10).expect("list");

        // THEN only events 1 and 2 are returned (strictly after the cursor)
        assert_eq!(rows.len(), 2);
        assert!(rows[0].event_id.ends_with("000000000001"));
        assert!(rows[1].event_id.ends_with("000000000002"));
    }
}
