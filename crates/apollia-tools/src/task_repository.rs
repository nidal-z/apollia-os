//! Repository SQLite pour la persistance HITL des tâches.
//!
//! Fournit [`TaskRepository`] qui stocke et restitue les données HITL
//! (prompt, contexte, réponse humaine) dans une base SQLite locale.
//!
//! Toutes les méthodes publiques sont `async` et délèguent aux opérations
//! SQLite bloquantes via `tokio::task::spawn_blocking` — pattern identique
//! à [`crate::audit::AuditTrail`] (STORY-016, ADR-014).
//!
//! La migration `005_hitl_tables.sql` est appliquée idempotentiellement
//! à l'appel de [`TaskRepository::open`].

use std::path::{Path, PathBuf};
use std::time::Duration;

use apollia_core::{AIPTask, InputResponseData};
use rusqlite::params;

/// SQL de migration embarqué — appliqué idempotentiellement à chaque ouverture.
const MIGRATION_SQL: &str = include_str!("../migrations/005_hitl_tables.sql");

// ─────────────────────────────────────────────────────────────────────────────
// Erreurs
// ─────────────────────────────────────────────────────────────────────────────

/// Erreurs spécifiques au [`TaskRepository`].
#[derive(Debug, thiserror::Error)]
pub enum TaskRepoError {
    /// Aucune tâche trouvée pour le `task_id` donné.
    #[error("tâche introuvable : {0}")]
    NotFound(String),

    /// Erreur SQLite sous-jacente.
    #[error("erreur SQLite : {0}")]
    Sqlite(#[from] rusqlite::Error),

    /// Erreur de désérialisation JSON.
    #[error("erreur de désérialisation JSON : {0}")]
    Json(#[from] serde_json::Error),

    /// Erreur d'infrastructure (join error spawn_blocking, etc.).
    #[error("erreur interne : {0}")]
    Internal(String),
}

// ─────────────────────────────────────────────────────────────────────────────
// Repository
// ─────────────────────────────────────────────────────────────────────────────

/// Repository SQLite pour la persistance HITL des tâches.
///
/// Encapsule le chemin de la base SQLite. Chaque méthode ouvre une connexion
/// dédiée dans `spawn_blocking`, ce qui garantit la compatibilité avec le runtime
/// Tokio sans partage de connexion entre threads.
///
/// La migration `005_hitl_tables.sql` est appliquée à [`open`](Self::open).
pub struct TaskRepository {
    db_path: PathBuf,
}

impl TaskRepository {
    /// Ouvre (ou crée) la base SQLite et applique la migration 005.
    ///
    /// La migration est idempotente (`CREATE TABLE IF NOT EXISTS`), donc sûre
    /// à rejouer sur une base existante. Active le mode WAL pour la concurrence
    /// lecture/écriture.
    ///
    /// # Errors
    ///
    /// Retourne une erreur si le fichier SQLite ne peut pas être ouvert ou si
    /// la migration échoue (schéma incompatible, permissions, etc.).
    pub async fn open(db_path: &Path) -> Result<Self, TaskRepoError> {
        let path = db_path.to_path_buf();

        tokio::task::spawn_blocking(move || -> Result<(), TaskRepoError> {
            let conn = rusqlite::Connection::open(&path)?;
            conn.execute_batch("PRAGMA journal_mode=WAL;")?;
            conn.execute_batch(MIGRATION_SQL)?;
            Ok(())
        })
        .await
        .map_err(|e| TaskRepoError::Internal(e.to_string()))??;

        Ok(Self {
            db_path: db_path.to_path_buf(),
        })
    }

    /// Persiste une suspension `input_required` avec son prompt et son contexte JSON.
    ///
    /// Insère ou met à jour la ligne dans `tasks` avec `status = 'input_required'`
    /// et `input_required_at = CURRENT_TIMESTAMP`. Appelé par ORIA avant d'émettre
    /// `RuntimeEvent::TaskInputRequired` sur l'EventBus (STORY-096).
    ///
    /// # Errors
    ///
    /// - [`TaskRepoError::Json`] si `context` ne peut pas être sérialisé
    /// - [`TaskRepoError::Sqlite`] en cas d'erreur SQLite
    pub async fn save_input_required(
        &self,
        task_id: &str,
        step_id: Option<&str>,
        prompt: &str,
        context: &serde_json::Value,
    ) -> Result<(), TaskRepoError> {
        let path = self.db_path.clone();
        let task_id = task_id.to_string();
        let step_id = step_id.map(|s| s.to_string());
        let prompt = prompt.to_string();
        let context_json = serde_json::to_string(context)?;

        tokio::task::spawn_blocking(move || -> Result<(), TaskRepoError> {
            let conn = rusqlite::Connection::open(&path)?;
            conn.execute_batch("PRAGMA journal_mode=WAL;")?;
            conn.execute(
                "INSERT INTO tasks \
                     (task_id, step_id, status, \
                      input_required_prompt, input_required_context, input_required_at) \
                 VALUES (?1, ?2, 'input_required', ?3, ?4, CURRENT_TIMESTAMP) \
                 ON CONFLICT(task_id) DO UPDATE SET \
                     step_id                = excluded.step_id, \
                     status                 = 'input_required', \
                     input_required_prompt  = excluded.input_required_prompt, \
                     input_required_context = excluded.input_required_context, \
                     input_required_at      = CURRENT_TIMESTAMP, \
                     updated_at             = CURRENT_TIMESTAMP",
                params![&task_id, &step_id, &prompt, &context_json],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| TaskRepoError::Internal(e.to_string()))??;

        Ok(())
    }

    /// Persiste la réponse humaine et insère une ligne dans `task_approvals`.
    ///
    /// Met à jour `input_response_approved`, `input_response_reason`,
    /// `input_response_at` et `status` dans `tasks`, puis insère une ligne
    /// dans `task_approvals` pour l'historique multi-approbation.
    /// Appelé par le `ResumeHandler` (STORY-095) après validation de la réponse.
    ///
    /// # Errors
    ///
    /// - [`TaskRepoError::NotFound`] si `task_id` est absent de la table `tasks`
    /// - [`TaskRepoError::Json`] si le contexte ne peut pas être sérialisé
    /// - [`TaskRepoError::Sqlite`] en cas d'erreur SQLite
    pub async fn save_input_response(
        &self,
        task_id: &str,
        response: &InputResponseData,
    ) -> Result<(), TaskRepoError> {
        let path = self.db_path.clone();
        let task_id = task_id.to_string();
        let approved = response.approved;
        let reason = response.reason.clone();
        let responded_at = response.responded_at.clone();
        let context_json = serde_json::to_string(&response.context)?;

        tokio::task::spawn_blocking(move || -> Result<(), TaskRepoError> {
            let conn = rusqlite::Connection::open(&path)?;
            conn.execute_batch("PRAGMA journal_mode=WAL;")?;

            // Récupère prompt et step_id pour l'insertion dans task_approvals.
            let (prompt, step_id): (String, Option<String>) = conn
                .query_row(
                    "SELECT COALESCE(input_required_prompt, ''), step_id \
                     FROM tasks WHERE task_id = ?1",
                    params![&task_id],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
                )
                .map_err(|_| TaskRepoError::NotFound(task_id.clone()))?;

            // Met à jour la ligne tasks avec la décision humaine.
            conn.execute(
                "UPDATE tasks SET \
                     input_response_approved = ?2, \
                     input_response_reason   = ?3, \
                     input_response_at       = ?4, \
                     status                  = 'working', \
                     updated_at              = CURRENT_TIMESTAMP \
                 WHERE task_id = ?1",
                params![&task_id, approved as i32, &reason, &responded_at],
            )?;

            // Insère dans l'historique task_approvals.
            conn.execute(
                "INSERT INTO task_approvals \
                     (task_id, step_id, prompt, context_json, approved, reason, responded_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    &task_id,
                    &step_id,
                    &prompt,
                    &context_json,
                    approved as i32,
                    &reason,
                    &responded_at,
                ],
            )?;

            Ok(())
        })
        .await
        .map_err(|e| TaskRepoError::Internal(e.to_string()))??;

        Ok(())
    }

    /// Reconstitue un [`AIPTask`] enrichi pour la reprise après `input_required`.
    ///
    /// Lit les colonnes `input_response_*` depuis `tasks` et construit un `AIPTask`
    /// avec `is_resumed = true` et `input_response` peuplé avec la décision humaine
    /// et le contexte JSON original. Appelé par le `ResumeHandler` (STORY-095) avant
    /// de relancer l'agent via ORIA.
    ///
    /// # Errors
    ///
    /// - [`TaskRepoError::NotFound`] si `task_id` est absent de la table `tasks`
    /// - [`TaskRepoError::Json`] si le contexte JSON stocké est invalide
    /// - [`TaskRepoError::Sqlite`] en cas d'erreur SQLite
    pub async fn rebuild_for_resume(&self, task_id: &str) -> Result<AIPTask, TaskRepoError> {
        let path = self.db_path.clone();
        let task_id = task_id.to_string();

        tokio::task::spawn_blocking(move || -> Result<AIPTask, TaskRepoError> {
            let conn = rusqlite::Connection::open(&path)?;

            // Lit les colonnes HITL nécessaires à la reconstruction.
            let (tid, approved_raw, reason, context_json_opt, responded_at_opt): (
                String,
                Option<i32>,
                Option<String>,
                Option<String>,
                Option<String>,
            ) = conn
                .query_row(
                    "SELECT task_id, \
                            input_response_approved, \
                            input_response_reason, \
                            input_required_context, \
                            input_response_at \
                     FROM tasks WHERE task_id = ?1",
                    params![&task_id],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, Option<i32>>(1)?,
                            row.get::<_, Option<String>>(2)?,
                            row.get::<_, Option<String>>(3)?,
                            row.get::<_, Option<String>>(4)?,
                        ))
                    },
                )
                .map_err(|_| TaskRepoError::NotFound(task_id.clone()))?;

            let input_response = match approved_raw {
                Some(raw) => {
                    let ctx_str = context_json_opt.as_deref().unwrap_or("{}");
                    let context: serde_json::Value = serde_json::from_str(ctx_str)?;
                    Some(InputResponseData {
                        approved: raw != 0,
                        reason,
                        context,
                        responded_at: responded_at_opt.unwrap_or_default(),
                    })
                }
                None => None,
            };

            Ok(AIPTask {
                task_id: tid,
                is_resumed: true,
                input_response,
                ..AIPTask::default()
            })
        })
        .await
        .map_err(|e| TaskRepoError::Internal(e.to_string()))?
    }

    /// Retourne les `task_id` en status `input_required` depuis plus longtemps que `older_than`.
    ///
    /// Utilise `strftime('%s', 'now') - strftime('%s', input_required_at)` pour calculer
    /// les secondes écoulées et les comparer au seuil `older_than.as_secs()`.
    /// Utilisé par le `TimeoutWatcher` (STORY-098) pour annuler les tâches expirées.
    ///
    /// # Errors
    ///
    /// Retourne [`TaskRepoError::Sqlite`] en cas d'erreur SQLite.
    pub async fn find_input_required_older_than(
        &self,
        older_than: Duration,
    ) -> Result<Vec<String>, TaskRepoError> {
        let path = self.db_path.clone();
        let threshold_secs = older_than.as_secs() as i64;

        tokio::task::spawn_blocking(move || -> Result<Vec<String>, TaskRepoError> {
            let conn = rusqlite::Connection::open(&path)?;

            let mut stmt = conn.prepare(
                "SELECT task_id FROM tasks \
                 WHERE status = 'input_required' \
                   AND input_required_at IS NOT NULL \
                   AND (strftime('%s', 'now') - strftime('%s', input_required_at)) > ?1",
            )?;

            let ids = stmt
                .query_map(params![threshold_secs], |row| row.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;

            Ok(ids)
        })
        .await
        .map_err(|e| TaskRepoError::Internal(e.to_string()))?
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Ouvre un `TaskRepository` sur un fichier temporaire unique.
    async fn open_test_repo() -> (TaskRepository, PathBuf) {
        let path = std::env::temp_dir().join(format!("apollia_hitl_{}.db", uuid::Uuid::new_v4()));
        let repo = TaskRepository::open(&path).await.expect("open failed");
        (repo, path)
    }

    // AC-1 — Migration appliquée au démarrage : colonnes HITL présentes dans tasks

    #[tokio::test]
    async fn test_ac1_migration_005_colonnes_existantes() {
        // GIVEN une DB temporaire fraîche
        let (_, db_path) = open_test_repo().await;

        // WHEN on inspecte les colonnes via PRAGMA table_info
        let cols: Vec<String> = tokio::task::spawn_blocking(move || {
            let conn = rusqlite::Connection::open(&db_path).unwrap();
            let mut stmt = conn
                .prepare("SELECT name FROM pragma_table_info('tasks')")
                .unwrap();
            stmt.query_map([], |row| row.get::<_, String>(0))
                .unwrap()
                .map(|r| r.unwrap())
                .collect()
        })
        .await
        .unwrap();

        // THEN les colonnes HITL sont toutes présentes
        for expected in &[
            "input_required_prompt",
            "input_required_context",
            "input_required_at",
            "input_response_approved",
            "input_response_reason",
            "input_response_at",
        ] {
            assert!(
                cols.contains(&expected.to_string()),
                "colonne manquante : {expected} ; colonnes trouvées = {cols:?}"
            );
        }

        // ET la table task_approvals est créée
        let tables: Vec<String> = tokio::task::spawn_blocking(|| {
            let conn = rusqlite::Connection::open_in_memory().unwrap();
            conn.execute_batch(MIGRATION_SQL).unwrap();
            let mut stmt = conn
                .prepare(
                    "SELECT name FROM sqlite_master \
                     WHERE type='table' AND name='task_approvals'",
                )
                .unwrap();
            stmt.query_map([], |row| row.get::<_, String>(0))
                .unwrap()
                .map(|r| r.unwrap())
                .collect()
        })
        .await
        .unwrap();

        assert!(
            !tables.is_empty(),
            "la table task_approvals doit exister après la migration"
        );
    }

    // AC-2 — save_input_response() persiste la réponse ET insère dans task_approvals

    #[tokio::test]
    async fn test_ac2_save_input_response_persiste() {
        // GIVEN une tâche créée en status input_required
        let (repo, db_path) = open_test_repo().await;
        let task_id = "t-hitl-002";
        let context = serde_json::json!({"montant": 12_500});

        repo.save_input_required(task_id, None, "Confirmer l'envoi ?", &context)
            .await
            .expect("save_input_required failed");

        // WHEN save_input_response() est appelé avec approved=true
        let response = InputResponseData {
            approved: true,
            reason: None,
            context: context.clone(),
            responded_at: "2026-03-09T10:00:00Z".into(),
        };
        repo.save_input_response(task_id, &response)
            .await
            .expect("save_input_response failed");

        // THEN la ligne tasks est mise à jour avec les valeurs correctes
        let db_path_a = db_path.clone();
        let (approved, at, status): (i32, String, String) =
            tokio::task::spawn_blocking(move || {
                let conn = rusqlite::Connection::open(&db_path_a).unwrap();
                conn.query_row(
                    "SELECT input_response_approved, input_response_at, status \
                     FROM tasks WHERE task_id = ?1",
                    params![task_id],
                    |row| {
                        Ok((
                            row.get::<_, i32>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                        ))
                    },
                )
                .unwrap()
            })
            .await
            .unwrap();

        assert_eq!(approved, 1, "input_response_approved doit être 1 (true)");
        assert_eq!(at, "2026-03-09T10:00:00Z");
        assert_eq!(status, "working");

        // ET task_approvals contient exactement une ligne pour ce task_id
        let count: i64 = tokio::task::spawn_blocking(move || {
            let conn = rusqlite::Connection::open(&db_path).unwrap();
            conn.query_row(
                "SELECT COUNT(*) FROM task_approvals WHERE task_id = ?1",
                params![task_id],
                |row| row.get::<_, i64>(0),
            )
            .unwrap()
        })
        .await
        .unwrap();

        assert_eq!(
            count, 1,
            "task_approvals doit contenir exactement une ligne"
        );
    }

    // AC-3 — rebuild_for_resume() reconstitue l'AIPTask avec is_resumed=true

    #[tokio::test]
    async fn test_ac3_rebuild_for_resume_is_resumed_true() {
        // GIVEN une tâche avec response persistée (approved=true)
        let (repo, _db_path) = open_test_repo().await;
        let task_id = "t-resume-003";
        let context = serde_json::json!({"devis": 42});

        repo.save_input_required(task_id, None, "Confirmer ?", &context)
            .await
            .unwrap();

        let response = InputResponseData {
            approved: true,
            reason: None,
            context: context.clone(),
            responded_at: "2026-03-09T11:00:00Z".into(),
        };
        repo.save_input_response(task_id, &response).await.unwrap();

        // WHEN rebuild_for_resume() est appelé
        let task = repo.rebuild_for_resume(task_id).await.unwrap();

        // THEN is_resumed=true, input_response.approved=true, context == original
        assert!(task.is_resumed, "is_resumed doit être true");
        assert_eq!(task.task_id, task_id);

        let ir = task
            .input_response
            .expect("input_response doit être Some après rebuild");
        assert!(ir.approved, "approved doit être true");
        assert_eq!(
            ir.context, context,
            "context doit correspondre à l'original"
        );
        assert!(
            !ir.responded_at.is_empty(),
            "responded_at ne doit pas être vide"
        );
    }

    // AC-4 — find_input_required_older_than() retourne les tâches expirées

    #[tokio::test]
    async fn test_ac4_find_expired_input_required() {
        // GIVEN une tâche input_required depuis 25h (manipulée directement en DB)
        let (repo, db_path) = open_test_repo().await;
        let task_id = "t-expired-004";

        repo.save_input_required(task_id, None, "check", &serde_json::json!({}))
            .await
            .unwrap();

        // Manipulation directe : on recule input_required_at de 25h
        tokio::task::spawn_blocking({
            let path = db_path.clone();
            move || {
                let conn = rusqlite::Connection::open(&path).unwrap();
                conn.execute(
                    "UPDATE tasks \
                     SET input_required_at = datetime('now', '-25 hours') \
                     WHERE task_id = ?1",
                    params![task_id],
                )
                .unwrap();
            }
        })
        .await
        .unwrap();

        // WHEN find_input_required_older_than(24h)
        let expired = repo
            .find_input_required_older_than(Duration::from_secs(24 * 3600))
            .await
            .unwrap();

        // THEN le task_id est présent dans la liste des expirées
        assert!(
            expired.contains(&task_id.to_string()),
            "task_id doit être dans la liste des expirées ; got={expired:?}"
        );
    }

    // AC-5 — Tâche récente absente de la liste des expirées

    #[tokio::test]
    async fn test_ac5_find_recent_not_expired() {
        // GIVEN une tâche input_required créée maintenant (~0s)
        let (repo, _db_path) = open_test_repo().await;
        let task_id = "t-recent-005";

        repo.save_input_required(task_id, None, "check", &serde_json::json!({}))
            .await
            .unwrap();

        // WHEN find_input_required_older_than(24h)
        let expired = repo
            .find_input_required_older_than(Duration::from_secs(24 * 3600))
            .await
            .unwrap();

        // THEN le task_id n'est PAS dans la liste
        assert!(
            !expired.contains(&task_id.to_string()),
            "tâche récente ne doit PAS être dans les expirées ; got={expired:?}"
        );
    }
}
