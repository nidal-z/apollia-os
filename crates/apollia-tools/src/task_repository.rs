//! Repository SQLite pour la persistance HITL des tâches.
//!
//! Fournit [`TaskRepository`] qui stocke et restitue les données HITL
//! (prompt, contexte, réponse humaine) dans une base SQLite locale.
//!
//! Toutes les méthodes publiques sont `async` et délèguent aux opérations
//! SQLite bloquantes via `tokio::task::spawn_blocking` — pattern identique
//! à [`crate::audit::AuditTrail`] (ADR-014).
//!
//! La migration `005_hitl_tables.sql` est appliquée idempotentiellement
//! à l'appel de [`TaskRepository::open`].

use std::path::{Path, PathBuf};
use std::time::Duration;

use apollia_core::{truncate_with_marker, AIPTask, InputResponseData, ObservabilityConfig};
use rusqlite::params;

/// SQL de migration embarqué — appliqué idempotentiellement à chaque ouverture.
const MIGRATION_SQL: &str = include_str!("../migrations/005_hitl_tables.sql");

/// Colonnes à ajouter par la migration observabilité.
const OBSERVABILITY_COLUMNS: &[(&str, &str)] = &[
    ("input_text", "TEXT"),
    ("input_truncated", "INTEGER NOT NULL DEFAULT 0"),
    ("output_text", "TEXT"),
    ("output_truncated", "INTEGER NOT NULL DEFAULT 0"),
    ("duration_ms", "INTEGER"),
    ("transitions_json", "TEXT"),
];

/// Colonnes HITL timing à ajouter sur `task_approvals`.
const HITL_TIMING_COLUMNS: &[(&str, &str)] =
    &[("suspended_at", "TEXT"), ("wait_duration_ms", "INTEGER")];

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Ajoute des colonnes à une table si elles n'existent pas encore.
///
/// Utilise `PRAGMA table_info` pour lister les colonnes existantes,
/// puis exécute `ALTER TABLE ADD COLUMN` uniquement pour les manquantes.
/// Compatible avec toutes les versions de SQLite (pas de `IF NOT EXISTS`).
fn add_columns_if_missing(
    conn: &rusqlite::Connection,
    table: &str,
    columns: &[(&str, &str)],
) -> Result<(), rusqlite::Error> {
    let mut stmt = conn.prepare(&format!("SELECT name FROM pragma_table_info('{table}')"))?;
    let existing: Vec<String> = stmt
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    for (col_name, col_type) in columns {
        if !existing.iter().any(|c| c == col_name) {
            conn.execute_batch(&format!(
                "ALTER TABLE {table} ADD COLUMN {col_name} {col_type};"
            ))?;
        }
    }
    Ok(())
}

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
// Types de retour
// ─────────────────────────────────────────────────────────────────────────────

/// Détail observabilité d'une tâche retourné par [`TaskRepository::get_task_detail`].
#[derive(Debug, Clone)]
pub struct TaskDetail {
    /// Texte d'entrée de la tâche (possiblement tronqué).
    pub input_text: Option<String>,
    /// Texte de sortie de la tâche (possiblement tronqué).
    pub output_text: Option<String>,
    /// Durée d'exécution en millisecondes.
    pub duration_ms: Option<i64>,
    /// Horodatage de création ISO8601.
    pub created_at: String,
}

/// Ouvre une connexion SQLite et applique la migration HITL idempotentiellement.
///
/// Appelé par toutes les méthodes du [`TaskRepository`] pour garantir l'intégrité
/// du schéma même si la base a été supprimée et recréée après le démarrage du runtime.
/// La migration utilise `CREATE TABLE IF NOT EXISTS` — sans effet sur une base valide.
///
/// Si le fichier principal n'existe pas, les fichiers WAL/SHM résiduels éventuels
/// sont supprimés avant l'ouverture pour éviter une récupération WAL orpheline
/// qui laisserait la base sans tables.
fn open_conn(path: &std::path::Path) -> Result<rusqlite::Connection, rusqlite::Error> {
    if !path.exists() {
        let _ = std::fs::remove_file(path.with_extension("db-wal"));
        let _ = std::fs::remove_file(path.with_extension("db-shm"));
    }
    let conn = rusqlite::Connection::open(path)?;
    conn.execute_batch("PRAGMA journal_mode=WAL;")?;
    conn.execute_batch(MIGRATION_SQL)?;
    Ok(conn)
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
#[derive(Clone)]
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
            let conn = open_conn(&path)?;
            add_columns_if_missing(&conn, "tasks", OBSERVABILITY_COLUMNS)?;
            add_columns_if_missing(&conn, "task_approvals", HITL_TIMING_COLUMNS)?;
            conn.execute_batch(
                "CREATE INDEX IF NOT EXISTS idx_task_approvals_pending \
                 ON task_approvals(task_id) WHERE approved IS NULL;",
            )?;
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
    /// `RuntimeEvent::TaskInputRequired` sur l'EventBus.
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
            let conn = open_conn(&path)?;
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

    // ─── Méthodes HITL timing ──────────────────────────────────────────

    /// Enregistre le timestamp de suspension lors d'un `input_required`.
    ///
    /// Insère une ligne préliminaire dans `task_approvals` avec `suspended_at`
    /// renseigné et `approved IS NULL`. Le `prompt` et `context_json` sont
    /// lus depuis la table `tasks` (peuplée par [`save_input_required`]).
    ///
    /// Appelé par ORIA au moment de l'émission de `AIPResult.input_required()`.
    ///
    /// # Errors
    ///
    /// - [`TaskRepoError::Sqlite`] en cas d'erreur SQLite
    pub async fn save_suspended_at(
        &self,
        task_id: &str,
        step_id: Option<&str>,
        suspended_at: &str,
    ) -> Result<(), TaskRepoError> {
        let path = self.db_path.clone();
        let task_id = task_id.to_string();
        let step_id = step_id.map(|s| s.to_string());
        let suspended_at = suspended_at.to_string();

        tokio::task::spawn_blocking(move || -> Result<(), TaskRepoError> {
            let conn = open_conn(&path)?;
            conn.execute(
                "INSERT INTO task_approvals \
                     (task_id, step_id, prompt, context_json, suspended_at) \
                 SELECT ?1, ?2, \
                        COALESCE(input_required_prompt, ''), \
                        COALESCE(input_required_context, '{}'), \
                        ?3 \
                 FROM tasks WHERE task_id = ?1",
                params![&task_id, &step_id, &suspended_at],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| TaskRepoError::Internal(e.to_string()))??;

        Ok(())
    }

    /// Met à jour `responded_at` et calcule `wait_duration_ms` en SQL.
    ///
    /// La durée d'attente est calculée atomiquement en SQL comme la différence
    /// entre `responded_at` et `suspended_at` en millisecondes, via
    /// `julianday()`. Cible la ligne en attente (`approved IS NULL`).
    ///
    /// Appelé par le `ResumeHandler` au moment de la réponse humaine.
    ///
    /// # Errors
    ///
    /// - [`TaskRepoError::Sqlite`] en cas d'erreur SQLite
    pub async fn save_response_timing(
        &self,
        task_id: &str,
        responded_at: &str,
    ) -> Result<(), TaskRepoError> {
        let path = self.db_path.clone();
        let task_id = task_id.to_string();
        let responded_at = responded_at.to_string();

        tokio::task::spawn_blocking(move || -> Result<(), TaskRepoError> {
            let conn = open_conn(&path)?;
            conn.execute(
                "UPDATE task_approvals \
                 SET responded_at = ?1, \
                     wait_duration_ms = CAST( \
                         (julianday(?1) - julianday(suspended_at)) * 86400000 AS INTEGER \
                     ) \
                 WHERE task_id = ?2 AND approved IS NULL",
                params![&responded_at, &task_id],
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
    /// Appelé par le `ResumeHandler` après validation de la réponse.
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
            let conn = open_conn(&path)?;

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

            // Update the pending row created by save_suspended_at.
            // If no pending row exists (backward compat), fallback to INSERT.
            let updated = conn.execute(
                "UPDATE task_approvals \
                 SET approved = ?2, \
                     reason = ?3, \
                     responded_at = ?4, \
                     wait_duration_ms = CAST( \
                         (julianday(?4) - julianday(suspended_at)) * 86400000 AS INTEGER \
                     ) \
                 WHERE task_id = ?1 AND approved IS NULL",
                params![&task_id, approved as i32, &reason, &responded_at],
            )?;

            if updated == 0 {
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
            }

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
    /// et le contexte JSON original. Appelé par le `ResumeHandler` avant
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
            let conn = open_conn(&path)?;

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

    /// Retourne le statut SQLite d'une tâche, ou `None` si absente de la table `tasks`.
    ///
    /// Utilisé par le `ResumeHandler` pour vérifier qu'une tâche
    /// est bien en status `input_required` avant de traiter la reprise.
    ///
    /// # Errors
    ///
    /// Retourne [`TaskRepoError::Sqlite`] en cas d'erreur SQLite.
    pub async fn get_task_status(&self, task_id: &str) -> Result<Option<String>, TaskRepoError> {
        let path = self.db_path.clone();
        let task_id = task_id.to_string();

        tokio::task::spawn_blocking(move || -> Result<Option<String>, TaskRepoError> {
            let conn = open_conn(&path)?;
            let result = conn.query_row(
                "SELECT status FROM tasks WHERE task_id = ?1",
                params![&task_id],
                |row| row.get::<_, String>(0),
            );
            match result {
                Ok(status) => Ok(Some(status)),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(TaskRepoError::Sqlite(e)),
            }
        })
        .await
        .map_err(|e| TaskRepoError::Internal(e.to_string()))?
    }

    /// Retourne le détail observabilité d'une tâche : input, output, durée, date de création.
    ///
    /// Retourne `None` si la tâche n'existe pas dans la DB (tâche récente non encore
    /// persistée, ou tâche soumise avant la migration observabilité).
    ///
    /// # Errors
    ///
    /// Retourne [`TaskRepoError::Sqlite`] en cas d'erreur SQLite.
    pub async fn get_task_detail(
        &self,
        task_id: &str,
    ) -> Result<Option<TaskDetail>, TaskRepoError> {
        let path = self.db_path.clone();
        let task_id = task_id.to_string();

        tokio::task::spawn_blocking(move || -> Result<Option<TaskDetail>, TaskRepoError> {
            let conn = open_conn(&path)?;
            let result = conn.query_row(
                "SELECT input_text, output_text, duration_ms, created_at \
                 FROM tasks WHERE task_id = ?1",
                params![&task_id],
                |row| {
                    Ok(TaskDetail {
                        input_text: row.get::<_, Option<String>>(0)?,
                        output_text: row.get::<_, Option<String>>(1)?,
                        duration_ms: row.get::<_, Option<i64>>(2)?,
                        created_at: row.get::<_, Option<String>>(3)?.unwrap_or_default(),
                    })
                },
            );
            match result {
                Ok(detail) => Ok(Some(detail)),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(TaskRepoError::Sqlite(e)),
            }
        })
        .await
        .map_err(|e| TaskRepoError::Internal(e.to_string()))?
    }

    // ─── Méthodes observabilité ──────────────────────────────────────────

    /// Persiste l'input texte d'une tâche avec troncature éventuelle.
    ///
    /// Utilise [`truncate_with_marker`] pour couper l'input si sa taille
    /// dépasse `config.max_input_bytes`. La colonne `input_truncated` est
    /// mise à 1 si le texte a été tronqué, 0 sinon.
    ///
    /// Crée la ligne dans `tasks` via `INSERT ... ON CONFLICT DO UPDATE`
    /// pour supporter l'appel avant ou après `save_input_required`.
    ///
    /// # Errors
    ///
    /// Retourne [`TaskRepoError::Sqlite`] en cas d'erreur SQLite.
    pub async fn save_input(
        &self,
        task_id: &str,
        text: &str,
        config: &ObservabilityConfig,
    ) -> Result<(), TaskRepoError> {
        let path = self.db_path.clone();
        let task_id = task_id.to_string();
        let (truncated_text, was_truncated) = truncate_with_marker(text, config.max_input_bytes);

        tokio::task::spawn_blocking(move || -> Result<(), TaskRepoError> {
            let conn = open_conn(&path)?;
            conn.execute(
                "INSERT INTO tasks (task_id, input_text, input_truncated) \
                 VALUES (?1, ?2, ?3) \
                 ON CONFLICT(task_id) DO UPDATE SET \
                     input_text      = excluded.input_text, \
                     input_truncated = excluded.input_truncated, \
                     updated_at      = CURRENT_TIMESTAMP",
                params![&task_id, &truncated_text, was_truncated as i32],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| TaskRepoError::Internal(e.to_string()))??;

        Ok(())
    }

    /// Met à jour le nom de l'agent pour une tâche.
    ///
    /// Appelé par le coordinateur juste après `save_input` pour renseigner
    /// le champ `agent_name` (non disponible lors du `INSERT` initial).
    ///
    /// # Errors
    ///
    /// Retourne [`TaskRepoError::Sqlite`] en cas d'erreur SQLite.
    pub async fn set_agent_name(
        &self,
        task_id: &str,
        agent_name: &str,
    ) -> Result<(), TaskRepoError> {
        let path = self.db_path.clone();
        let task_id = task_id.to_string();
        let agent_name = agent_name.to_string();

        tokio::task::spawn_blocking(move || -> Result<(), TaskRepoError> {
            let conn = open_conn(&path)?;
            conn.execute(
                "UPDATE tasks SET agent_name = ?2, updated_at = CURRENT_TIMESTAMP \
                 WHERE task_id = ?1",
                params![&task_id, &agent_name],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| TaskRepoError::Internal(e.to_string()))??;

        Ok(())
    }

    /// Persiste l'output texte d'une tâche avec troncature éventuelle.
    ///
    /// Utilise [`truncate_with_marker`] pour couper l'output si sa taille
    /// dépasse `config.max_output_bytes`. La colonne `output_truncated` est
    /// mise à 1 si le texte a été tronqué, 0 sinon.
    ///
    /// # Errors
    ///
    /// - [`TaskRepoError::Sqlite`] en cas d'erreur SQLite
    pub async fn save_output(
        &self,
        task_id: &str,
        text: &str,
        config: &ObservabilityConfig,
    ) -> Result<(), TaskRepoError> {
        let path = self.db_path.clone();
        let task_id = task_id.to_string();
        let (truncated_text, was_truncated) = truncate_with_marker(text, config.max_output_bytes);

        tokio::task::spawn_blocking(move || -> Result<(), TaskRepoError> {
            let conn = open_conn(&path)?;
            conn.execute(
                "UPDATE tasks SET \
                     output_text      = ?2, \
                     output_truncated = ?3, \
                     updated_at       = CURRENT_TIMESTAMP \
                 WHERE task_id = ?1",
                params![&task_id, &truncated_text, was_truncated as i32],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| TaskRepoError::Internal(e.to_string()))??;

        Ok(())
    }

    /// Ajoute une transition d'état au JSON array `transitions_json`.
    ///
    /// Lit le JSON existant (ou `[]` si absent), pousse
    /// `{"status": "<status>", "ts": "<timestamp>"}`, et réécrit.
    /// Les transitions sont ordonnées chronologiquement par ordre d'insertion.
    ///
    /// # Errors
    ///
    /// - [`TaskRepoError::Sqlite`] en cas d'erreur SQLite
    /// - [`TaskRepoError::Json`] si le JSON existant est invalide
    pub async fn append_transition(
        &self,
        task_id: &str,
        status: &str,
        timestamp: &str,
    ) -> Result<(), TaskRepoError> {
        let path = self.db_path.clone();
        let task_id = task_id.to_string();
        let status = status.to_string();
        let timestamp = timestamp.to_string();

        tokio::task::spawn_blocking(move || -> Result<(), TaskRepoError> {
            let conn = open_conn(&path)?;

            let existing: Option<String> = conn
                .query_row(
                    "SELECT transitions_json FROM tasks WHERE task_id = ?1",
                    params![&task_id],
                    |row| row.get::<_, Option<String>>(0),
                )
                .unwrap_or(None);

            let mut transitions: Vec<serde_json::Value> = match existing {
                Some(ref json) if !json.is_empty() => serde_json::from_str(json)?,
                _ => Vec::new(),
            };

            transitions.push(serde_json::json!({
                "status": status,
                "ts": timestamp,
            }));

            let new_json = serde_json::to_string(&transitions)?;

            conn.execute(
                "UPDATE tasks SET \
                     transitions_json = ?2, \
                     updated_at       = CURRENT_TIMESTAMP \
                 WHERE task_id = ?1",
                params![&task_id, &new_json],
            )?;

            Ok(())
        })
        .await
        .map_err(|e| TaskRepoError::Internal(e.to_string()))?
    }

    /// Enregistre la durée d'exécution en millisecondes.
    ///
    /// Appelé par le coordinateur à la completion de la tâche.
    ///
    /// # Errors
    ///
    /// Retourne [`TaskRepoError::Sqlite`] en cas d'erreur SQLite.
    pub async fn set_duration(&self, task_id: &str, duration_ms: i64) -> Result<(), TaskRepoError> {
        let path = self.db_path.clone();
        let task_id = task_id.to_string();

        tokio::task::spawn_blocking(move || -> Result<(), TaskRepoError> {
            let conn = open_conn(&path)?;
            conn.execute(
                "UPDATE tasks SET \
                     duration_ms = ?2, \
                     updated_at  = CURRENT_TIMESTAMP \
                 WHERE task_id = ?1",
                params![&task_id, duration_ms],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| TaskRepoError::Internal(e.to_string()))??;

        Ok(())
    }

    /// Annule une tâche en mettant son statut à `cancelled` dans la DB.
    ///
    /// Persiste `reason` dans la colonne `input_response_reason` pour la traçabilité.
    /// Appelé par le `TimeoutWatcher` lors de l'expiration d'une
    /// suspension `input_required`.
    ///
    /// # Errors
    ///
    /// Retourne [`TaskRepoError::Sqlite`] en cas d'erreur SQLite.
    pub async fn cancel_task(&self, task_id: &str, reason: &str) -> Result<(), TaskRepoError> {
        let path = self.db_path.clone();
        let task_id = task_id.to_string();
        let reason = reason.to_string();

        tokio::task::spawn_blocking(move || -> Result<(), TaskRepoError> {
            let conn = open_conn(&path)?;
            conn.execute(
                "UPDATE tasks SET \
                     status                = 'cancelled', \
                     input_response_reason = ?2, \
                     updated_at            = CURRENT_TIMESTAMP \
                 WHERE task_id = ?1",
                params![&task_id, &reason],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| TaskRepoError::Internal(e.to_string()))??;

        Ok(())
    }

    /// Retourne les `task_id` en status `input_required` depuis plus longtemps que `older_than`.
    ///
    /// Utilise `strftime('%s', 'now') - strftime('%s', input_required_at)` pour calculer
    /// les secondes écoulées et les comparer au seuil `older_than.as_secs()`.
    /// Utilisé par le `TimeoutWatcher` pour annuler les tâches expirées.
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
            let conn = open_conn(&path)?;

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
    /// Retourne les informations d'approbation pour une tâche en status `input_required`.
    ///
    /// Lit le prompt, le contexte JSON et le `suspended_at` depuis les tables `tasks` et
    /// `task_approvals`. Retourne `None` si la tâche n'existe pas ou n'est pas en
    /// `input_required`.
    pub async fn get_approval_info(
        &self,
        task_id: &str,
    ) -> Result<Option<ApprovalInfo>, TaskRepoError> {
        let path = self.db_path.clone();
        let task_id = task_id.to_string();

        tokio::task::spawn_blocking(move || -> Result<Option<ApprovalInfo>, TaskRepoError> {
            let conn = open_conn(&path)?;
            let mut stmt = conn.prepare(
                "SELECT t.agent_name, \
                        COALESCE(t.input_required_prompt, ''), \
                        COALESCE(t.input_required_context, '{}'), \
                        ta.suspended_at \
                 FROM tasks t \
                 LEFT JOIN task_approvals ta ON t.task_id = ta.task_id AND ta.approved IS NULL \
                 WHERE t.task_id = ?1 AND t.status = 'input_required' \
                 LIMIT 1",
            )?;

            let result = match stmt.query_row(params![task_id], |row| {
                let agent_name: String = row.get(0)?;
                let prompt: String = row.get(1)?;
                let context_str: String = row.get(2)?;
                let suspended_at: Option<String> = row.get(3)?;
                Ok((agent_name, prompt, context_str, suspended_at))
            }) {
                Ok(row) => Some(row),
                Err(rusqlite::Error::QueryReturnedNoRows) => None,
                Err(e) => return Err(TaskRepoError::Sqlite(e)),
            };

            match result {
                None => Ok(None),
                Some((agent_name, prompt, context_str, suspended_at)) => {
                    let context = serde_json::from_str(&context_str).unwrap_or_default();
                    Ok(Some(ApprovalInfo {
                        agent_name,
                        prompt,
                        context,
                        suspended_at: suspended_at.unwrap_or_default(),
                    }))
                }
            }
        })
        .await
        .map_err(|e| TaskRepoError::Internal(e.to_string()))?
    }

    /// Liste les approbations résolues (approuvées ou rejetées) des `days` derniers jours.
    ///
    /// Retourne au plus `limit` lignes triées par `responded_at` décroissant.
    /// Lit la table `task_approvals` joinée à `tasks` pour récupérer le `agent_name`.
    ///
    /// # Errors
    ///
    /// - [`TaskRepoError::Sqlite`] en cas d'erreur SQLite
    /// - [`TaskRepoError::Internal`] si le `spawn_blocking` échoue
    pub async fn list_resolved_approvals(
        &self,
        limit: u32,
        days: u32,
    ) -> Result<Vec<ResolvedApprovalRow>, TaskRepoError> {
        let path = self.db_path.clone();

        tokio::task::spawn_blocking(move || -> Result<Vec<ResolvedApprovalRow>, TaskRepoError> {
            let conn = open_conn(&path)?;

            let mut stmt = conn.prepare(
                "SELECT ta.task_id, \
                        COALESCE(t.agent_name, ''), \
                        ta.approved, \
                        ta.reason, \
                        ta.suspended_at, \
                        ta.responded_at, \
                        ta.wait_duration_ms \
                 FROM task_approvals ta \
                 LEFT JOIN tasks t ON ta.task_id = t.task_id \
                 WHERE ta.approved IS NOT NULL \
                   AND ta.responded_at IS NOT NULL \
                   AND ta.responded_at >= datetime('now', ?1) \
                 ORDER BY ta.responded_at DESC \
                 LIMIT ?2",
            )?;

            let days_param = format!("-{days} days");
            let rows = stmt
                .query_map(params![&days_param, limit], |row| {
                    let approved_int: i32 = row.get(2)?;
                    Ok(ResolvedApprovalRow {
                        task_id: row.get(0)?,
                        agent_name: row.get(1)?,
                        approved: approved_int != 0,
                        reason: row.get(3)?,
                        suspended_at: row.get(4)?,
                        responded_at: row.get(5)?,
                        wait_duration_ms: row.get(6)?,
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;

            Ok(rows)
        })
        .await
        .map_err(|e| TaskRepoError::Internal(e.to_string()))?
    }

    /// Retourne les tâches récentes persistées dans SQLite.
    ///
    /// Ordonnées par `created_at DESC`, limitées à `limit` entrées.
    /// Le statut est déduit de `transitions_json` (dernière transition).
    /// Utilisé pour afficher l'historique des tâches après un redémarrage
    /// du runtime (quand le `TaskRouter` en mémoire est vide).
    ///
    /// # Errors
    ///
    /// Retourne [`TaskRepoError::Sqlite`] en cas d'erreur SQLite.
    pub async fn list_recent_tasks(
        &self,
        limit: usize,
    ) -> Result<Vec<PersistedTaskSummary>, TaskRepoError> {
        let path = self.db_path.clone();

        tokio::task::spawn_blocking(
            move || -> Result<Vec<PersistedTaskSummary>, TaskRepoError> {
                let conn = open_conn(&path)?;

                let mut stmt = conn.prepare(
                    "SELECT task_id, agent_name, input_text, output_text, \
                        duration_ms, transitions_json, created_at \
                 FROM tasks \
                 ORDER BY created_at DESC \
                 LIMIT ?1",
                )?;

                let rows = stmt
                    .query_map(params![limit as i64], |row| {
                        let task_id: String = row.get(0)?;
                        let agent_name: String = row.get(1)?;
                        let input_text: Option<String> = row.get(2)?;
                        let output_text: Option<String> = row.get(3)?;
                        let duration_ms: Option<i64> = row.get(4)?;
                        let transitions_json: Option<String> = row.get(5)?;
                        let created_at: String =
                            row.get::<_, Option<String>>(6)?.unwrap_or_default();

                        let status = derive_status(&transitions_json, duration_ms);
                        let input_preview = input_text
                            .as_deref()
                            .unwrap_or("")
                            .chars()
                            .take(120)
                            .collect::<String>();

                        Ok(PersistedTaskSummary {
                            task_id,
                            agent_name,
                            status,
                            input_preview,
                            output_text,
                            duration_ms,
                            created_at,
                        })
                    })?
                    .collect::<rusqlite::Result<Vec<_>>>()?;

                Ok(rows)
            },
        )
        .await
        .map_err(|e| TaskRepoError::Internal(e.to_string()))?
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers internes
// ─────────────────────────────────────────────────────────────────────────────

/// Déduit le statut final d'une tâche depuis `transitions_json`.
///
/// Lit la dernière entrée `{"status": "<status>", "ts": "<timestamp>"}` et
/// retourne le statut. Retourne `"completed"` si la durée est renseignée et
/// qu'aucune transition n'est disponible (cas des anciennes tâches).
fn derive_status(transitions_json: &Option<String>, duration_ms: Option<i64>) -> String {
    if let Some(status) = last_transition_status(transitions_json) {
        return status;
    }
    // Fallback: if we have duration, it was completed; otherwise unknown
    if duration_ms.is_some() {
        "completed".to_string()
    } else {
        "unknown".to_string()
    }
}

/// Extrait le statut de la dernière transition de `transitions_json`, ou `None`
/// si le JSON est absent, vide, invalide, ou sans champ `status` exploitable.
fn last_transition_status(transitions_json: &Option<String>) -> Option<String> {
    let json = transitions_json.as_ref()?;
    if json.is_empty() {
        return None;
    }
    let transitions = serde_json::from_str::<Vec<serde_json::Value>>(json).ok()?;
    let last = transitions.last()?;
    let status = last.get("status").and_then(|s| s.as_str())?;
    Some(status.to_string())
}

/// Résumé d'une tâche persistée, lue depuis SQLite.
///
/// Utilisé par `list_recent_tasks` pour fournir l'historique des tâches
/// après un redémarrage du runtime (les tâches terminées ne sont plus en mémoire).
#[derive(Debug, Clone)]
pub struct PersistedTaskSummary {
    /// Identifiant unique de la tâche.
    pub task_id: String,
    /// Nom de l'agent.
    pub agent_name: String,
    /// Statut déduit de `transitions_json` (dernière transition).
    pub status: String,
    /// Aperçu du texte d'entrée (tronqué à 120 chars).
    pub input_preview: String,
    /// Texte de sortie.
    pub output_text: Option<String>,
    /// Durée d'exécution en millisecondes.
    pub duration_ms: Option<i64>,
    /// Date de création ISO 8601.
    pub created_at: String,
}

/// Informations d'une approbation en attente, lues depuis SQLite.
#[derive(Debug, Clone)]
pub struct ApprovalInfo {
    /// Nom de l'agent (depuis le manifest).
    pub agent_name: String,
    /// Prompt affiché à l'utilisateur.
    pub prompt: String,
    /// Contexte JSON sérialisé par l'agent.
    pub context: serde_json::Value,
    /// Timestamp ISO 8601 de la suspension.
    pub suspended_at: String,
}

/// Ligne d'une approbation résolue, lue depuis `task_approvals`.
#[derive(Debug, Clone)]
pub struct ResolvedApprovalRow {
    /// Identifiant de la tâche.
    pub task_id: String,
    /// Nom de l'agent.
    pub agent_name: String,
    /// `true` si approuvée, `false` si rejetée.
    pub approved: bool,
    /// Raison du rejet (si applicable).
    pub reason: Option<String>,
    /// Timestamp ISO 8601 de la suspension.
    pub suspended_at: Option<String>,
    /// Timestamp ISO 8601 de la réponse.
    pub responded_at: Option<String>,
    /// Durée d'attente en millisecondes.
    pub wait_duration_ms: Option<i64>,
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

    // ─── Tests observabilité tasks ────────────────────────────────────

    // Input persisté à la soumission (non tronqué)

    #[tokio::test]
    async fn test_story126_ac1_input_persisted() {
        // GIVEN un TaskRepository en mémoire + un task_id créé via save_input
        let (repo, db_path) = open_test_repo().await;
        let config = ObservabilityConfig::default();

        // WHEN save_input avec un texte court
        repo.save_input("t-126-1", "hello world", &config)
            .await
            .expect("save_input failed");

        // THEN input_text == "hello world", input_truncated == 0
        let (text, truncated): (String, i32) = tokio::task::spawn_blocking(move || {
            let conn = rusqlite::Connection::open(&db_path).expect("open db");
            conn.query_row(
                "SELECT input_text, input_truncated FROM tasks WHERE task_id = ?1",
                params!["t-126-1"],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, i32>(1)?)),
            )
            .expect("query failed")
        })
        .await
        .expect("join failed");

        assert_eq!(text, "hello world");
        assert_eq!(truncated, 0);
    }

    // Input tronqué si supérieur à la limite

    #[tokio::test]
    async fn test_story126_ac2_input_truncated_at_limit() {
        // GIVEN config avec max_input_bytes = 100
        let (repo, db_path) = open_test_repo().await;
        let config = ObservabilityConfig {
            max_input_bytes: 100,
            ..ObservabilityConfig::default()
        };
        let big_text = "x".repeat(500);

        // WHEN save_input avec un texte de 500 octets
        repo.save_input("t-126-2", &big_text, &config)
            .await
            .expect("save_input failed");

        // THEN input_truncated == 1, input_text contient le marqueur
        let (text, truncated): (String, i32) = tokio::task::spawn_blocking(move || {
            let conn = rusqlite::Connection::open(&db_path).expect("open db");
            conn.query_row(
                "SELECT input_text, input_truncated FROM tasks WHERE task_id = ?1",
                params!["t-126-2"],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, i32>(1)?)),
            )
            .expect("query failed")
        })
        .await
        .expect("join failed");

        assert_eq!(truncated, 1);
        assert!(text.ends_with("octets total]"), "got: {text}");
        assert!(text.contains("500"), "marker should mention 500 bytes");
    }

    // Output persisté à la completion

    #[tokio::test]
    async fn test_story126_ac3_output_persisted() {
        // GIVEN une tâche existante
        let (repo, db_path) = open_test_repo().await;
        let config = ObservabilityConfig::default();
        repo.save_input("t-126-3", "input", &config)
            .await
            .expect("save_input failed");

        // WHEN save_output avec un texte court
        repo.save_output("t-126-3", "result output", &config)
            .await
            .expect("save_output failed");

        // THEN output_text == "result output", output_truncated == 0
        let (text, truncated): (String, i32) = tokio::task::spawn_blocking(move || {
            let conn = rusqlite::Connection::open(&db_path).expect("open db");
            conn.query_row(
                "SELECT output_text, output_truncated FROM tasks WHERE task_id = ?1",
                params!["t-126-3"],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, i32>(1)?)),
            )
            .expect("query failed")
        })
        .await
        .expect("join failed");

        assert_eq!(text, "result output");
        assert_eq!(truncated, 0);
    }

    // Output tronqué si supérieur à la limite

    #[tokio::test]
    async fn test_story126_ac3_output_truncated_at_limit() {
        // GIVEN config avec max_output_bytes = 100
        let (repo, db_path) = open_test_repo().await;
        let config = ObservabilityConfig {
            max_output_bytes: 100,
            ..ObservabilityConfig::default()
        };
        repo.save_input("t-126-3b", "input", &config)
            .await
            .expect("save_input failed");
        let big_output = "y".repeat(500);

        // WHEN save_output avec un texte de 500 octets
        repo.save_output("t-126-3b", &big_output, &config)
            .await
            .expect("save_output failed");

        // THEN output_truncated == 1
        let truncated: i32 = tokio::task::spawn_blocking(move || {
            let conn = rusqlite::Connection::open(&db_path).expect("open db");
            conn.query_row(
                "SELECT output_truncated FROM tasks WHERE task_id = ?1",
                params!["t-126-3b"],
                |row| row.get::<_, i32>(0),
            )
            .expect("query failed")
        })
        .await
        .expect("join failed");

        assert_eq!(truncated, 1);
    }

    // Transitions ordonnées chronologiquement

    #[tokio::test]
    async fn test_story126_ac4_transitions_ordered() {
        // GIVEN une tâche existante
        let (repo, db_path) = open_test_repo().await;
        let config = ObservabilityConfig::default();
        repo.save_input("t-126-4", "input", &config)
            .await
            .expect("save_input failed");

        // WHEN 3 transitions ajoutées dans l'ordre
        repo.append_transition("t-126-4", "submitted", "2026-03-13T10:00:00Z")
            .await
            .expect("append 1 failed");
        repo.append_transition("t-126-4", "running", "2026-03-13T10:00:01Z")
            .await
            .expect("append 2 failed");
        repo.append_transition("t-126-4", "completed", "2026-03-13T10:00:02Z")
            .await
            .expect("append 3 failed");

        // THEN transitions_json contient 3 éléments dans l'ordre
        let json_str: String = tokio::task::spawn_blocking(move || {
            let conn = rusqlite::Connection::open(&db_path).expect("open db");
            conn.query_row(
                "SELECT transitions_json FROM tasks WHERE task_id = ?1",
                params!["t-126-4"],
                |row| row.get::<_, String>(0),
            )
            .expect("query failed")
        })
        .await
        .expect("join failed");

        let transitions: Vec<serde_json::Value> =
            serde_json::from_str(&json_str).expect("parse json");
        assert_eq!(transitions.len(), 3);
        assert_eq!(transitions[0]["status"], "submitted");
        assert_eq!(transitions[1]["status"], "running");
        assert_eq!(transitions[2]["status"], "completed");
        assert_eq!(transitions[0]["ts"], "2026-03-13T10:00:00Z");
        assert_eq!(transitions[2]["ts"], "2026-03-13T10:00:02Z");
    }

    // Durée mesurée

    #[tokio::test]
    async fn test_story126_ac5_duration_recorded() {
        // GIVEN une tâche existante
        let (repo, db_path) = open_test_repo().await;
        let config = ObservabilityConfig::default();
        repo.save_input("t-126-5", "input", &config)
            .await
            .expect("save_input failed");

        // WHEN set_duration(250ms)
        repo.set_duration("t-126-5", 250)
            .await
            .expect("set_duration failed");

        // THEN duration_ms == 250
        let duration: i64 = tokio::task::spawn_blocking(move || {
            let conn = rusqlite::Connection::open(&db_path).expect("open db");
            conn.query_row(
                "SELECT duration_ms FROM tasks WHERE task_id = ?1",
                params!["t-126-5"],
                |row| row.get::<_, i64>(0),
            )
            .expect("query failed")
        })
        .await
        .expect("join failed");

        assert_eq!(duration, 250);
    }

    // Colonnes observabilité présentes après migration

    #[tokio::test]
    async fn test_story126_migration_columns_present() {
        // GIVEN une DB fraîche
        let (_, db_path) = open_test_repo().await;

        // WHEN on inspecte les colonnes
        let cols: Vec<String> = tokio::task::spawn_blocking(move || {
            let conn = rusqlite::Connection::open(&db_path).expect("open");
            let mut stmt = conn
                .prepare("SELECT name FROM pragma_table_info('tasks')")
                .expect("prepare");
            stmt.query_map([], |row| row.get::<_, String>(0))
                .expect("query")
                .map(|r| r.expect("get"))
                .collect()
        })
        .await
        .expect("join");

        // THEN les 6 colonnes observabilité sont présentes
        for expected in &[
            "input_text",
            "input_truncated",
            "output_text",
            "output_truncated",
            "duration_ms",
            "transitions_json",
        ] {
            assert!(
                cols.contains(&expected.to_string()),
                "colonne manquante : {expected} ; colonnes trouvées = {cols:?}"
            );
        }
    }

    // ─── Tests HITL existants ────────────────────────────────────────

    // Migration appliquée au démarrage : colonnes HITL présentes dans tasks

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

    // save_input_response() persiste la réponse ET insère dans task_approvals

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

    // rebuild_for_resume() reconstitue l'AIPTask avec is_resumed=true

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

    // find_input_required_older_than() retourne les tâches expirées

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

    // Tâche récente absente de la liste des expirées

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

    // ─── Tests HITL timing ───────────────────────────────────────────

    // suspended_at enregistré à la suspension

    #[tokio::test]
    async fn test_story131_ac1_suspended_at_recorded() {
        // GIVEN un TaskRepository avec une approbation en attente
        let (repo, db_path) = open_test_repo().await;
        let task_id = "t-131-1";
        repo.save_input_required(task_id, None, "Confirmer ?", &serde_json::json!({}))
            .await
            .expect("save_input_required failed");

        // WHEN save_suspended_at est appelé
        repo.save_suspended_at(task_id, None, "2026-03-13T14:30:00.000Z")
            .await
            .expect("save_suspended_at failed");

        // THEN suspended_at est renseigné dans task_approvals
        let suspended: String = tokio::task::spawn_blocking(move || {
            let conn = rusqlite::Connection::open(&db_path).expect("open db");
            conn.query_row(
                "SELECT suspended_at FROM task_approvals WHERE task_id = ?1",
                params!["t-131-1"],
                |row| row.get::<_, String>(0),
            )
            .expect("query failed")
        })
        .await
        .expect("join failed");

        assert_eq!(suspended, "2026-03-13T14:30:00.000Z");
    }

    // responded_at enregistré à la réponse (via save_input_response)

    #[tokio::test]
    async fn test_story131_ac2_responded_at_recorded() {
        // GIVEN une approbation avec suspended_at renseigné
        let (repo, db_path) = open_test_repo().await;
        let task_id = "t-131-2";
        repo.save_input_required(task_id, None, "Budget OK ?", &serde_json::json!({}))
            .await
            .expect("save_input_required failed");
        repo.save_suspended_at(task_id, None, "2026-03-13T14:30:00.000Z")
            .await
            .expect("save_suspended_at failed");

        // WHEN save_input_response est appelé
        let response = InputResponseData {
            approved: true,
            reason: None,
            context: serde_json::json!({}),
            responded_at: "2026-03-13T14:35:00.000Z".into(),
        };
        repo.save_input_response(task_id, &response)
            .await
            .expect("save_input_response failed");

        // THEN responded_at est bien renseigné dans task_approvals
        let responded: String = tokio::task::spawn_blocking(move || {
            let conn = rusqlite::Connection::open(&db_path).expect("open db");
            conn.query_row(
                "SELECT responded_at FROM task_approvals WHERE task_id = ?1",
                params!["t-131-2"],
                |row| row.get::<_, String>(0),
            )
            .expect("query failed")
        })
        .await
        .expect("join failed");

        assert_eq!(responded, "2026-03-13T14:35:00.000Z");
    }

    // wait_duration_ms calculé automatiquement (5 min = 300000ms)

    #[tokio::test]
    async fn test_story131_ac3_wait_duration_calculated() {
        // GIVEN suspended_at = 14:30:00, responded_at = 14:35:00
        let (repo, db_path) = open_test_repo().await;
        let task_id = "t-131-3";
        repo.save_input_required(task_id, None, "Valider ?", &serde_json::json!({}))
            .await
            .expect("save_input_required failed");
        repo.save_suspended_at(task_id, None, "2026-03-13T14:30:00.000Z")
            .await
            .expect("save_suspended_at failed");

        // WHEN save_input_response est appelé 5 min plus tard
        let response = InputResponseData {
            approved: true,
            reason: None,
            context: serde_json::json!({}),
            responded_at: "2026-03-13T14:35:00.000Z".into(),
        };
        repo.save_input_response(task_id, &response)
            .await
            .expect("save_input_response failed");

        // THEN wait_duration_ms ≈ 300000 (5 min en ms, tolérance ±1000 pour arrondi julianday)
        let wait_ms: i64 = tokio::task::spawn_blocking(move || {
            let conn = rusqlite::Connection::open(&db_path).expect("open db");
            conn.query_row(
                "SELECT wait_duration_ms FROM task_approvals WHERE task_id = ?1",
                params!["t-131-3"],
                |row| row.get::<_, i64>(0),
            )
            .expect("query failed")
        })
        .await
        .expect("join failed");

        assert!(
            (299_000..=301_000).contains(&wait_ms),
            "wait_duration_ms should be ~300000 (5 min), got {wait_ms}"
        );
    }

    // Index pending créé

    #[tokio::test]
    async fn test_story131_ac4_pending_index_exists() {
        // GIVEN une DB fraîche
        let (_, db_path) = open_test_repo().await;

        // WHEN on vérifie les index
        let has_index: bool = tokio::task::spawn_blocking(move || {
            let conn = rusqlite::Connection::open(&db_path).expect("open");
            let count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master \
                     WHERE type = 'index' AND name = 'idx_task_approvals_pending'",
                    [],
                    |row| row.get(0),
                )
                .expect("query failed");
            count > 0
        })
        .await
        .expect("join failed");

        // THEN l'index existe
        assert!(has_index, "idx_task_approvals_pending doit exister");
    }

    // ─── Test list_resolved_approvals ─────────────────────────────────

    #[tokio::test]
    async fn test_story141_list_resolved_approvals() {
        // GIVEN un repo avec une approbation résolue
        let (repo, _db_path) = open_test_repo().await;
        let task_id = "t-141-resolved";
        let context = serde_json::json!({"montant": 5000});

        repo.save_input_required(task_id, None, "Valider le paiement ?", &context)
            .await
            .expect("save_input_required failed");

        repo.save_suspended_at(task_id, None, "2099-01-01T10:00:00.000Z")
            .await
            .expect("save_suspended_at failed");

        let response = InputResponseData {
            approved: true,
            reason: None,
            context,
            responded_at: "2099-01-01T10:05:00.000Z".to_string(),
        };
        repo.save_input_response(task_id, &response)
            .await
            .expect("save_input_response failed");

        // WHEN list_resolved_approvals is called
        let rows = repo
            .list_resolved_approvals(20, 30)
            .await
            .expect("list_resolved_approvals failed");

        // THEN one resolved approval is returned
        assert_eq!(rows.len(), 1, "expected 1 resolved approval");
        assert_eq!(rows[0].task_id, task_id);
        assert!(rows[0].approved);
        assert!(rows[0].reason.is_none());
    }

    #[tokio::test]
    async fn test_story141_list_resolved_excludes_pending() {
        // GIVEN un repo avec une approbation encore en attente (pas de response)
        let (repo, _db_path) = open_test_repo().await;
        let task_id = "t-141-pending";

        repo.save_input_required(task_id, None, "En attente", &serde_json::json!({}))
            .await
            .expect("save_input_required failed");

        repo.save_suspended_at(task_id, None, "2026-03-13T10:00:00.000Z")
            .await
            .expect("save_suspended_at failed");

        // WHEN list_resolved_approvals is called
        let rows = repo
            .list_resolved_approvals(20, 30)
            .await
            .expect("list_resolved_approvals failed");

        // THEN no rows returned (pending is excluded)
        assert!(
            rows.is_empty(),
            "pending approval should not appear in resolved list"
        );
    }
}
