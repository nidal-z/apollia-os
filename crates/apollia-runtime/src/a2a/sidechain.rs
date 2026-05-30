//! Sidechain logging, traçabilité structurée des délégations A2A dans SQLite.
//!
//! [`SidechainRepository`] persiste chaque délégation dans la table `task_sidechains`.
//! [`SidechainLogger`] en est le wrapper async best-effort : toute erreur de log
//! est tracée via `tracing::warn` et ne bloque jamais la délégation.

use std::path::Path;
use std::sync::{Arc, Mutex};

use rusqlite::{params, Connection};
use serde::Serialize;
use tracing::warn;

use apollia_core::TaskId;

/// Migration SQL appliquée à l'ouverture de la base.
const MIGRATION_SQL: &str = include_str!("../../migrations/002_task_sidechains.sql");

// ─────────────────────────────────────────────────────────────────────────────
// Error
// ─────────────────────────────────────────────────────────────────────────────

/// Erreurs retournées par les opérations SQLite du sidechain.
#[derive(Debug, thiserror::Error)]
pub enum SidechainError {
    /// Erreur SQLite sous-jacente.
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

// ─────────────────────────────────────────────────────────────────────────────
// Row type
// ─────────────────────────────────────────────────────────────────────────────

/// Ligne retournée par [`SidechainRepository::list_by_parent`].
#[derive(Debug, Serialize, Clone)]
pub struct SidechainRow {
    /// Numéro séquentiel de la délégation pour ce parent (1-based).
    pub sidechain_n: i64,
    /// Nom de l'agent cible (ou skill_id utilisé pour la résolution).
    pub agent_name: String,
    /// Statut courant : `"running"`, `"completed"`, ou `"failed"`.
    pub status: String,
    /// Premiers 500 caractères de l'input.
    pub input_summary: Option<String>,
    /// Premiers 500 caractères de l'output ou du message d'erreur.
    pub output_summary: Option<String>,
    /// Horodatage ISO 8601 de début de délégation.
    pub started_at: Option<String>,
    /// Horodatage ISO 8601 de fin de délégation (`None` si encore en cours).
    pub completed_at: Option<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Repository (synchrone, rusqlite)
// ─────────────────────────────────────────────────────────────────────────────

/// Repository SQLite synchrone pour les délégations A2A.
///
/// Toutes les méthodes sont synchrones et doivent être appelées depuis un
/// thread bloquant (via `tokio::task::spawn_blocking` en contexte async).
pub struct SidechainRepository {
    conn: Connection,
}

impl SidechainRepository {
    /// Ouvre ou crée la base SQLite au chemin indiqué et applique la migration.
    pub fn open(path: &Path) -> Result<Self, SidechainError> {
        let conn = Connection::open(path)?;
        conn.execute_batch(MIGRATION_SQL)?;
        Ok(Self { conn })
    }

    /// Crée une base en mémoire pour les tests.
    pub fn new_in_memory() -> Result<Self, SidechainError> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(MIGRATION_SQL)?;
        Ok(Self { conn })
    }

    /// Enregistre le début d'une délégation. Retourne `sidechain_n` (1-based).
    ///
    /// `sidechain_n` est calculé côté application : `COUNT(*) + 1` pour ce
    /// `parent_task_id`. Idempotent si appelé plusieurs fois pour le même parent.
    pub fn log_start(
        &self,
        parent_task_id: &str,
        agent_name: &str,
        input_summary: &str,
    ) -> Result<i64, SidechainError> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM task_sidechains WHERE parent_task_id = ?1",
            params![parent_task_id],
            |row| row.get(0),
        )?;
        let sidechain_n = count + 1;
        self.conn.execute(
            "INSERT INTO task_sidechains \
             (parent_task_id, sidechain_n, agent_name, input_summary, status) \
             VALUES (?1, ?2, ?3, ?4, 'running')",
            params![parent_task_id, sidechain_n, agent_name, input_summary],
        )?;
        Ok(sidechain_n)
    }

    /// Met à jour une délégation terminée avec son statut final et son résumé d'output.
    pub fn log_complete(
        &self,
        parent_task_id: &str,
        sidechain_n: i64,
        output_summary: &str,
        status: &str,
    ) -> Result<(), SidechainError> {
        self.conn.execute(
            "UPDATE task_sidechains \
             SET status = ?1, output_summary = ?2, completed_at = CURRENT_TIMESTAMP \
             WHERE parent_task_id = ?3 AND sidechain_n = ?4",
            params![status, output_summary, parent_task_id, sidechain_n],
        )?;
        Ok(())
    }

    /// Retourne toutes les délégations pour un `parent_task_id`, triées par `sidechain_n`.
    pub fn list_by_parent(
        &self,
        parent_task_id: &str,
    ) -> Result<Vec<SidechainRow>, SidechainError> {
        let mut stmt = self.conn.prepare(
            "SELECT sidechain_n, agent_name, status, input_summary, output_summary, \
             started_at, completed_at \
             FROM task_sidechains \
             WHERE parent_task_id = ?1 \
             ORDER BY sidechain_n ASC",
        )?;
        let rows = stmt
            .query_map(params![parent_task_id], |row| {
                Ok(SidechainRow {
                    sidechain_n: row.get(0)?,
                    agent_name: row.get(1)?,
                    status: row.get(2)?,
                    input_summary: row.get(3)?,
                    output_summary: row.get(4)?,
                    started_at: row.get(5)?,
                    completed_at: row.get(6)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Logger (async, best-effort)
// ─────────────────────────────────────────────────────────────────────────────

/// Wrapper async best-effort autour de [`SidechainRepository`].
///
/// Toutes les opérations utilisent `spawn_blocking`. Les erreurs sont loguées
/// via `tracing::warn` et ne sont jamais propagées, la délégation A2A continue
/// même si le logging échoue.
#[derive(Clone)]
pub struct SidechainLogger {
    repository: Arc<Mutex<SidechainRepository>>,
}

impl SidechainLogger {
    /// Construit un `SidechainLogger` depuis un repository partagé.
    pub fn new(repository: Arc<Mutex<SidechainRepository>>) -> Self {
        Self { repository }
    }

    /// Construit un `SidechainLogger` en mémoire pour les tests.
    pub fn new_in_memory() -> Result<Self, SidechainError> {
        let repo = SidechainRepository::new_in_memory()?;
        Ok(Self {
            repository: Arc::new(Mutex::new(repo)),
        })
    }

    /// Enregistre le début d'une délégation. Retourne `sidechain_n`, ou `0` en cas d'erreur.
    ///
    /// `sidechain_n == 0` signale un échec du logging et est ignoré dans [`complete`].
    pub async fn start(
        &self,
        parent_task_id: &TaskId,
        agent_name: &str,
        input: &serde_json::Value,
    ) -> i64 {
        let input_summary: String = input.to_string().chars().take(500).collect();
        let parent_id = parent_task_id.to_string();
        let agent = agent_name.to_string();
        let repo = self.repository.clone();

        match tokio::task::spawn_blocking(move || {
            repo.lock()
                .map_err(|e| rusqlite::Error::InvalidParameterName(format!("mutex poisoned: {e}")))
                .map_err(SidechainError::from)
                .and_then(|guard| guard.log_start(&parent_id, &agent, &input_summary))
        })
        .await
        {
            Ok(Ok(n)) => n,
            Ok(Err(e)) => {
                warn!(error = %e, "sidechain start failed — delegation continues untracked");
                0
            }
            Err(e) => {
                warn!(error = %e, "sidechain start task panicked — delegation continues untracked");
                0
            }
        }
    }

    /// Met à jour une délégation terminée. Best-effort, les erreurs sont loguées sans propagation.
    ///
    /// `sidechain_n == 0` indique que [`start`] a échoué ; l'appel est ignoré silencieusement.
    pub async fn complete(
        &self,
        parent_task_id: &TaskId,
        sidechain_n: i64,
        output_summary: &str,
        status: &str,
    ) {
        if sidechain_n == 0 {
            return;
        }
        let parent_id = parent_task_id.to_string();
        let output: String = output_summary.chars().take(500).collect();
        let status = status.to_string();
        let repo = self.repository.clone();

        let result = tokio::task::spawn_blocking(move || {
            repo.lock()
                .map_err(|e| rusqlite::Error::InvalidParameterName(format!("mutex poisoned: {e}")))
                .map_err(SidechainError::from)
                .and_then(|guard| guard.log_complete(&parent_id, sidechain_n, &output, &status))
        })
        .await;

        match result {
            Ok(Ok(())) => {}
            Ok(Err(e)) => warn!(error = %e, "sidechain complete failed"),
            Err(e) => warn!(error = %e, "sidechain complete task panicked"),
        }
    }

    /// Retourne toutes les délégations pour un `parent_task_id`, triées par `sidechain_n`.
    pub async fn list_by_parent(
        &self,
        parent_task_id: &str,
    ) -> Result<Vec<SidechainRow>, SidechainError> {
        let parent_id = parent_task_id.to_string();
        let repo = self.repository.clone();

        tokio::task::spawn_blocking(move || {
            repo.lock()
                .map_err(|e| rusqlite::Error::InvalidParameterName(format!("mutex poisoned: {e}")))
                .map_err(SidechainError::from)
                .and_then(|guard| guard.list_by_parent(&parent_id))
        })
        .await
        .map_err(|e| SidechainError::Sqlite(rusqlite::Error::InvalidParameterName(e.to_string())))?
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_core::TaskId;

    #[tokio::test]
    async fn test_two_delegations_get_sequential_sidechain_n() {
        // GIVEN un logger en mémoire et une tâche parent
        let logger = SidechainLogger::new_in_memory().unwrap();
        let parent = TaskId::new_v4();

        // WHEN deux délégations sont enregistrées
        let n1 = logger
            .start(&parent, "agent-a", &serde_json::json!({"task": "hello"}))
            .await;
        let n2 = logger
            .start(&parent, "agent-b", &serde_json::json!({"task": "world"}))
            .await;

        // THEN les numéros séquentiels sont 1 et 2
        assert_eq!(n1, 1);
        assert_eq!(n2, 2);
    }

    #[tokio::test]
    async fn test_completed_delegation_updates_status() {
        // GIVEN une délégation en cours
        let logger = SidechainLogger::new_in_memory().unwrap();
        let parent = TaskId::new_v4();
        let n = logger
            .start(&parent, "agent-a", &serde_json::json!({}))
            .await;
        assert_eq!(n, 1);

        // WHEN la délégation se termine avec succès
        logger
            .complete(&parent, n, r#"{"result":"ok"}"#, "completed")
            .await;

        // THEN la row a status = "completed"
        let rows = logger.list_by_parent(&parent.to_string()).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].status, "completed");
    }

    #[tokio::test]
    async fn test_failed_delegation_sets_status_failed() {
        // GIVEN une délégation en cours
        let logger = SidechainLogger::new_in_memory().unwrap();
        let parent = TaskId::new_v4();
        let n = logger
            .start(&parent, "agent-a", &serde_json::json!({}))
            .await;

        // WHEN la délégation échoue
        logger
            .complete(&parent, n, "agent not found", "failed")
            .await;

        // THEN la row a status = "failed"
        let rows = logger.list_by_parent(&parent.to_string()).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].status, "failed");
    }

    #[tokio::test]
    async fn test_complete_with_zero_sidechain_n_is_noop() {
        // GIVEN un sidechain_n == 0 (start() a échoué)
        let logger = SidechainLogger::new_in_memory().unwrap();
        let parent = TaskId::new_v4();

        // WHEN complete() est appelé avec sidechain_n = 0
        logger.complete(&parent, 0, "output", "completed").await;

        // THEN aucune row n'est insérée
        let rows = logger.list_by_parent(&parent.to_string()).await.unwrap();
        assert!(rows.is_empty());
    }

    #[tokio::test]
    async fn test_list_by_parent_returns_empty_for_unknown_task() {
        // GIVEN un logger sans aucune délégation
        let logger = SidechainLogger::new_in_memory().unwrap();

        // WHEN on liste pour un parent inconnu
        let rows = logger.list_by_parent("unknown-task-id").await.unwrap();

        // THEN la liste est vide
        assert!(rows.is_empty());
    }

    #[tokio::test]
    async fn test_input_summary_is_truncated_at_500_chars() {
        // GIVEN un input très long
        let logger = SidechainLogger::new_in_memory().unwrap();
        let parent = TaskId::new_v4();
        let long_value: String = "x".repeat(1000);
        let input = serde_json::json!({"data": long_value});

        // WHEN la délégation est enregistrée
        let n = logger.start(&parent, "agent-a", &input).await;
        assert_eq!(n, 1);

        // THEN l'input_summary est tronquée à 500 chars
        let rows = logger.list_by_parent(&parent.to_string()).await.unwrap();
        assert_eq!(rows.len(), 1);
        if let Some(summary) = &rows[0].input_summary {
            assert!(summary.chars().count() <= 500);
        }
    }
}
