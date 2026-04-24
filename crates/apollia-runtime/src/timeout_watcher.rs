//! TimeoutWatcher — annulation automatique des tâches `input_required` expirées.
//!
//! Scanne périodiquement la DB à la recherche de tâches suspendues depuis trop longtemps
//! et les annule en émettant [`RuntimeEvent::TaskApprovalTimeout`] puis
//! [`RuntimeEvent::TaskCanceled`] sur l'EventBus.
//!
//! Démarré par le [`Supervisor`](crate::supervisor::Supervisor) après l'APIServer.
//! Principes respectés : #4 (Fail fast) et #7 (Garde-fous non-négociables).

use std::sync::Arc;
use std::time::Duration;

use apollia_core::{RuntimeEvent, TaskId};
use apollia_tools::TaskRepository;

use crate::eventbus::EventBusSender;

// ─────────────────────────────────────────────────────────────────────────────
// Configuration
// ─────────────────────────────────────────────────────────────────────────────

/// Configuration du watcher de timeout d'approbation HITL.
///
/// Valeurs par défaut : aucun timeout global (pause indéfinie), 60 s d'intervalle de scan.
#[derive(Debug, Clone)]
pub struct TimeoutWatcherConfig {
    /// Durée après laquelle une tâche `input_required` est annulée automatiquement.
    ///
    /// `None` (défaut) : aucune annulation automatique — la tâche reste suspendue
    /// indéfiniment jusqu'à réponse explicite de l'opérateur.
    /// `Some(d)` : annulation après `d` (opt-in via `[hitl] timeout_hours` dans `apollia.toml`).
    pub input_required_timeout: Option<Duration>,
    /// Intervalle entre deux scans consécutifs.
    ///
    /// Utilise `tokio::time::interval` pour éviter la dérive temporelle.
    /// Défaut : 60 secondes. Sans effet si `input_required_timeout` est `None`.
    pub scan_interval: Duration,
}

impl Default for TimeoutWatcherConfig {
    fn default() -> Self {
        Self {
            input_required_timeout: None,
            scan_interval: Duration::from_secs(60),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Erreurs
// ─────────────────────────────────────────────────────────────────────────────

/// Erreurs internes du [`TimeoutWatcher`].
///
/// Retournées par [`TimeoutWatcher::scan_and_cancel`]. La boucle principale
/// [`TimeoutWatcher::run`] logue l'erreur et continue sans propager ni paniquer.
#[derive(Debug, thiserror::Error)]
pub enum TimeoutWatcherError {
    /// Erreur SQLite lors de l'accès au [`TaskRepository`].
    #[error("erreur DB : {0}")]
    Database(#[from] apollia_tools::TaskRepoError),
}

// ─────────────────────────────────────────────────────────────────────────────
// TimeoutWatcher
// ─────────────────────────────────────────────────────────────────────────────

/// Tâche Tokio qui annule automatiquement les tâches `input_required` expirées.
///
/// Scanne toutes les [`scan_interval`] secondes les tâches dont `input_required_at`
/// dépasse [`input_required_timeout`] (si configuré). Comportement selon config :
///
/// - `input_required_timeout = None` : aucune annulation automatique — no-op à chaque tick.
/// - `input_required_timeout = Some(d)` : pour chaque tâche expirée :
///   1. Met à jour son statut en `cancelled` via [`TaskRepository::cancel_task`].
///   2. Émet [`RuntimeEvent::TaskApprovalTimeout`] sur l'EventBus.
///   3. Émet [`RuntimeEvent::TaskCanceled`] sur l'EventBus.
///
/// En cas d'erreur SQLite, logue un `tracing::warn!` et continue la boucle sans crash.
///
/// [`scan_interval`]: TimeoutWatcherConfig::scan_interval
/// [`input_required_timeout`]: TimeoutWatcherConfig::input_required_timeout
pub struct TimeoutWatcher {
    config: TimeoutWatcherConfig,
    db: Arc<TaskRepository>,
    event_bus: EventBusSender,
}

impl TimeoutWatcher {
    /// Crée un nouveau `TimeoutWatcher`.
    ///
    /// # Arguments
    ///
    /// - `config` : intervalles et seuil de timeout.
    /// - `db` : accès au `TaskRepository` SQLite.
    /// - `event_bus` : canal d'émission des événements runtime.
    pub fn new(
        config: TimeoutWatcherConfig,
        db: Arc<TaskRepository>,
        event_bus: EventBusSender,
    ) -> Self {
        Self {
            config,
            db,
            event_bus,
        }
    }

    /// Lance la boucle de surveillance — ne retourne jamais sauf arrêt du runtime.
    ///
    /// Utilise `tokio::time::interval` (pas `sleep`) pour éviter la dérive temporelle.
    /// En cas d'erreur SQLite lors d'un scan, logue un `tracing::warn!` et continue.
    pub async fn run(self) {
        let mut interval = tokio::time::interval(self.config.scan_interval);
        loop {
            interval.tick().await;
            match self.scan_and_cancel().await {
                Ok(n) if n > 0 => {
                    tracing::info!(cancelled = n, "tâches HITL expirées annulées");
                }
                Ok(_) => {}
                Err(e) => {
                    tracing::warn!(error = %e, "erreur scan timeout watcher");
                }
            }
        }
    }

    /// Scanne les tâches `input_required` expirées et les annule.
    ///
    /// Retourne `Ok(0)` immédiatement si `input_required_timeout` est `None` (no-op).
    /// Retourne le nombre de tâches effectivement annulées sinon.
    /// Retourne une erreur uniquement si le scan DB initial échoue.
    /// Les erreurs d'annulation individuelle sont loguées en `warn!` mais ne font pas échouer.
    async fn scan_and_cancel(&self) -> Result<usize, TimeoutWatcherError> {
        let timeout = match self.config.input_required_timeout {
            Some(t) => t,
            None => return Ok(0),
        };

        let expired_ids = self
            .db
            .find_input_required_older_than(timeout)
            .await?;

        let mut cancelled = 0usize;

        for task_id_str in &expired_ids {
            if let Err(e) = self
                .db
                .cancel_task(task_id_str, "input_required_timeout")
                .await
            {
                tracing::warn!(
                    task_id = %task_id_str,
                    error = %e,
                    "erreur annulation tâche HITL expirée"
                );
                continue;
            }

            tracing::warn!(
                task_id = %task_id_str,
                "tâche annulée — timeout input_required"
            );

            let task_id: TaskId = task_id_str.as_str().into();

            let after_secs = timeout.as_secs();
            let _ = self.event_bus.send(RuntimeEvent::TaskApprovalTimeout {
                task_id: task_id.clone(),
                after_secs,
            });

            let _ = self.event_bus.send(RuntimeEvent::TaskCanceled { task_id });

            cancelled += 1;
        }

        Ok(cancelled)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_tools::TaskRepository;
    use rusqlite::params;
    use std::path::PathBuf;

    /// Ouvre un `TaskRepository` sur un fichier temporaire unique.
    async fn open_test_repo() -> (TaskRepository, PathBuf) {
        let path = std::env::temp_dir().join(format!("apollia_tw_{}.db", uuid::Uuid::new_v4()));
        let repo = TaskRepository::open(&path)
            .await
            .expect("TaskRepository::open failed");
        (repo, path)
    }

    /// Insère une tâche `input_required` en DB et recule `input_required_at` de `hours_ago`.
    async fn insert_expired_task(db_path: &PathBuf, task_id: &str, hours_ago: i64) {
        let task_id = task_id.to_string();
        let path = db_path.clone();
        tokio::task::spawn_blocking(move || {
            let conn = rusqlite::Connection::open(&path).unwrap();
            conn.execute_batch("PRAGMA journal_mode=WAL;").unwrap();
            conn.execute(
                "INSERT INTO tasks (task_id, status, input_required_prompt, input_required_at) \
                 VALUES (?1, 'input_required', 'confirmer ?', \
                         datetime('now', ?2))",
                params![&task_id, format!("-{} hours", hours_ago)],
            )
            .unwrap();
        })
        .await
        .unwrap();
    }

    /// Lit le statut d'une tâche depuis la DB.
    async fn get_task_status(db_path: &PathBuf, task_id: &str) -> Option<String> {
        let path = db_path.clone();
        let task_id = task_id.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = rusqlite::Connection::open(&path).unwrap();
            conn.query_row(
                "SELECT status FROM tasks WHERE task_id = ?1",
                params![&task_id],
                |row| row.get::<_, String>(0),
            )
            .ok()
        })
        .await
        .unwrap()
    }

    // Tâche expirée annulée + 2 events émis

    #[tokio::test]
    async fn test_ac1_expired_task_is_cancelled() {
        // GIVEN une tâche input_required depuis 25h
        let (repo, db_path) = open_test_repo().await;
        let task_id = "t-tw-001";
        insert_expired_task(&db_path, task_id, 25).await;

        let (tx, mut rx) = tokio::sync::broadcast::channel::<RuntimeEvent>(16);
        let watcher = TimeoutWatcher::new(
            TimeoutWatcherConfig {
                input_required_timeout: Some(Duration::from_secs(24 * 3600)),
                scan_interval: Duration::from_secs(3600),
            },
            Arc::new(repo),
            tx,
        );

        // WHEN scan_and_cancel() avec timeout=24h
        let result = watcher.scan_and_cancel().await;

        // THEN 1 tâche annulée, statut DB = 'cancelled'
        assert!(result.is_ok(), "scan_and_cancel doit réussir");
        assert_eq!(result.unwrap(), 1, "1 tâche doit être annulée");

        let status = get_task_status(&db_path, task_id).await;
        assert_eq!(
            status.as_deref(),
            Some("cancelled"),
            "statut DB doit être 'cancelled'"
        );

        // ET 2 events emis : TaskApprovalTimeout + TaskCanceled
        let ev1 = rx.try_recv().expect("TaskApprovalTimeout doit être émis");
        let ev2 = rx.try_recv().expect("TaskCanceled doit être émis");

        assert!(
            matches!(
                ev1,
                RuntimeEvent::TaskApprovalTimeout {
                    after_secs: 86400,
                    ..
                }
            ),
            "premier event doit être TaskApprovalTimeout ; got={ev1:?}"
        );
        assert!(
            matches!(ev2, RuntimeEvent::TaskCanceled { .. }),
            "second event doit être TaskCanceled ; got={ev2:?}"
        );
    }

    // Tâche récente (30 min) non annulée

    #[tokio::test]
    async fn test_ac2_recent_task_not_cancelled() {
        // GIVEN une tâche input_required depuis seulement 30 min
        let (repo, db_path) = open_test_repo().await;
        let task_id = "t-tw-002";

        // On insère avec input_required_at = "now" (0 heures)
        insert_expired_task(&db_path, task_id, 0).await;

        let (tx, _rx) = tokio::sync::broadcast::channel::<RuntimeEvent>(16);
        let watcher = TimeoutWatcher::new(
            TimeoutWatcherConfig {
                input_required_timeout: Some(Duration::from_secs(24 * 3600)),
                scan_interval: Duration::from_secs(3600),
            },
            Arc::new(repo),
            tx,
        );

        // WHEN scan_and_cancel() avec timeout=24h
        let result = watcher.scan_and_cancel().await;

        // THEN 0 tâches annulées, statut DB inchangé
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0, "aucune tâche ne doit être annulée");

        let status = get_task_status(&db_path, task_id).await;
        assert_eq!(
            status.as_deref(),
            Some("input_required"),
            "statut doit rester 'input_required'"
        );
    }

    // Timeout configurable : 2h et tâche depuis 3h → annulée

    #[tokio::test]
    async fn test_ac3_custom_timeout_2h() {
        // GIVEN une tâche input_required depuis 3h + config timeout=2h
        let (repo, db_path) = open_test_repo().await;
        let task_id = "t-tw-003";
        insert_expired_task(&db_path, task_id, 3).await;

        let (tx, _rx) = tokio::sync::broadcast::channel::<RuntimeEvent>(16);
        let watcher = TimeoutWatcher::new(
            TimeoutWatcherConfig {
                input_required_timeout: Some(Duration::from_secs(2 * 3600)),
                scan_interval: Duration::from_secs(3600),
            },
            Arc::new(repo),
            tx,
        );

        // WHEN scan_and_cancel() avec timeout=2h
        let result = watcher.scan_and_cancel().await;

        // THEN tâche annulée (3h > 2h)
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap(),
            1,
            "1 tâche doit être annulée (3h > timeout 2h)"
        );

        let status = get_task_status(&db_path, task_id).await;
        assert_eq!(status.as_deref(), Some("cancelled"));
    }

    // Erreur DB → Err retourné, pas de panic

    #[tokio::test]
    async fn test_ac5_db_error_does_not_crash() {
        // GIVEN un TaskRepository dont le fichier DB est corrompu
        let (repo, db_path) = open_test_repo().await;

        // Corruption : on écrase le fichier SQLite avec des octets invalides
        std::fs::write(&db_path, b"not a valid sqlite database").unwrap();

        let (tx, _rx) = tokio::sync::broadcast::channel::<RuntimeEvent>(16);
        let watcher = TimeoutWatcher::new(
            TimeoutWatcherConfig {
                input_required_timeout: Some(Duration::from_secs(24 * 3600)),
                scan_interval: Duration::from_secs(3600),
            },
            Arc::new(repo),
            tx,
        );

        // WHEN scan_and_cancel() est appelé sur une DB corrompue
        let result = watcher.scan_and_cancel().await;

        // THEN Err retourné (pas de panic) — la boucle run() peut continuer
        assert!(
            result.is_err(),
            "scan_and_cancel doit retourner Err sur DB corrompue"
        );
    }

    // Aucun timeout configuré → no-op, tâche expirée non annulée

    #[tokio::test]
    async fn test_ac6_no_global_timeout_is_noop() {
        // GIVEN une tâche input_required depuis 100h + config sans timeout global
        let (repo, db_path) = open_test_repo().await;
        let task_id = "t-tw-006";
        insert_expired_task(&db_path, task_id, 100).await;

        let (tx, _rx) = tokio::sync::broadcast::channel::<RuntimeEvent>(16);
        let watcher = TimeoutWatcher::new(
            TimeoutWatcherConfig {
                input_required_timeout: None,
                scan_interval: Duration::from_secs(60),
            },
            Arc::new(repo),
            tx,
        );

        // WHEN scan_and_cancel() sans timeout global
        let result = watcher.scan_and_cancel().await;

        // THEN 0 tâches annulées — la tâche reste en pause indéfinie
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0, "aucune annulation sans timeout global");

        let status = get_task_status(&db_path, task_id).await;
        assert_eq!(
            status.as_deref(),
            Some("input_required"),
            "statut doit rester 'input_required' — pause indéfinie"
        );
    }
}
