//! `LlmCallRepository` — persistance SQLite des appels LLM.
//!
//! Chaque [`RuntimeEvent::LlmCallCompleted`] reçu sur
//! l'EventBus est persisté dans la table `llm_calls` via [`spawn_subscriber`].
//!
//! Le `prompt_text` n'est persisté que si `debug_log_prompt = true` dans la
//! configuration d'observabilité (respect vie privée — ADR-026 décision 4).

use std::path::Path;
use std::sync::{Arc, Mutex};

use rusqlite::{params, Connection};
use tracing::{error, info};

use apollia_core::events::{EventBusSender, RuntimeEvent};
use apollia_core::ObservabilityConfig;

// ─────────────────────────────────────────────
// Schema
// ─────────────────────────────────────────────

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS llm_calls (
    id                TEXT PRIMARY KEY,
    task_id           TEXT,
    step_id           TEXT,
    backend           TEXT NOT NULL,
    model             TEXT NOT NULL,
    prompt_tokens     INTEGER,
    completion_tokens INTEGER,
    cost_usd          REAL,
    latency_ms        INTEGER,
    prompt_text       TEXT,
    completion_text   TEXT,
    created_at        TEXT NOT NULL
                      DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
CREATE INDEX IF NOT EXISTS idx_llm_calls_task    ON llm_calls(task_id);
CREATE INDEX IF NOT EXISTS idx_llm_calls_created ON llm_calls(created_at);
"#;

// ─────────────────────────────────────────────
// Types
// ─────────────────────────────────────────────

/// Enregistrement d'un appel LLM persisté en SQLite.
#[derive(Debug, Clone)]
pub struct LlmCallRecord {
    /// Identifiant unique (UUID v4).
    pub id: String,
    /// Identifiant de la tâche ayant déclenché l'appel.
    pub task_id: Option<String>,
    /// Identifiant du step ORIA (mode orchestré uniquement).
    pub step_id: Option<String>,
    /// Nom logique du backend (e.g. `"anthropic"`, `"local"`).
    pub backend: String,
    /// Identifiant du modèle (e.g. `"claude-sonnet-4-20250514"`).
    pub model: String,
    /// Nombre de tokens dans le prompt.
    pub prompt_tokens: Option<u32>,
    /// Nombre de tokens générés.
    pub completion_tokens: Option<u32>,
    /// Coût estimé en USD.
    pub cost_usd: Option<f64>,
    /// Latence totale de l'appel en millisecondes.
    pub latency_ms: Option<u64>,
    /// Prompt envoyé au LLM (persisté uniquement si `debug_log_prompt = true`).
    pub prompt_text: Option<String>,
    /// Texte de la complétion retournée par le LLM.
    pub completion_text: Option<String>,
}

/// Résumé coût/tokens agrégé par backend+modèle (dashboard LLM Costs).
#[derive(Debug, Clone)]
pub struct LlmCostSummary {
    /// Nom logique du backend.
    pub backend: String,
    /// Identifiant du modèle.
    pub model: String,
    /// Nombre d'appels.
    pub call_count: u64,
    /// Total de tokens (prompt + completion).
    pub total_tokens: u64,
    /// Coût total estimé en USD.
    pub total_cost_usd: f64,
}

/// Résumé coût journalier agrégé par backend (dashboard LLM Costs daily chart).
#[derive(Debug, Clone)]
pub struct LlmDailyCostSummary {
    /// Date au format `YYYY-MM-DD`.
    pub date: String,
    /// Nom logique du backend.
    pub backend: String,
    /// Coût total estimé en USD pour ce jour et ce backend.
    pub cost_usd: f64,
}

/// Erreurs du repository LLM.
#[derive(Debug, thiserror::Error)]
pub enum LlmRepositoryError {
    /// Erreur SQLite sous-jacente.
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

// ─────────────────────────────────────────────
// Repository
// ─────────────────────────────────────────────

/// Repository SQLite pour persister les appels LLM.
///
/// Créé au démarrage du Supervisor via [`LlmCallRepository::open`].
/// Le subscriber EventBus est lancé via [`spawn_subscriber`].
pub struct LlmCallRepository {
    conn: Connection,
}

impl LlmCallRepository {
    /// Ouvre la base SQLite et applique le schéma (CREATE TABLE IF NOT EXISTS).
    ///
    /// Le fichier est créé s'il n'existe pas. WAL est activé pour les
    /// performances en écriture concurrente.
    ///
    /// # Erreurs
    ///
    /// Retourne [`LlmRepositoryError::Sqlite`] si l'ouverture ou la migration échoue.
    pub fn open(path: &Path) -> Result<Self, LlmRepositoryError> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self { conn })
    }

    /// Ouvre une base in-memory pour les tests.
    #[cfg(test)]
    fn open_in_memory() -> Result<Self, LlmRepositoryError> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self { conn })
    }

    /// Persiste un enregistrement d'appel LLM.
    pub fn save(&self, record: &LlmCallRecord) -> Result<(), LlmRepositoryError> {
        self.conn.execute(
            "INSERT INTO llm_calls (
                id, task_id, step_id, backend, model,
                prompt_tokens, completion_tokens, cost_usd,
                latency_ms, prompt_text, completion_text
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                record.id,
                record.task_id,
                record.step_id,
                record.backend,
                record.model,
                record.prompt_tokens,
                record.completion_tokens,
                record.cost_usd,
                record.latency_ms.map(|v| v as i64),
                record.prompt_text,
                record.completion_text,
            ],
        )?;
        Ok(())
    }

    /// Retourne tous les appels LLM pour une tâche, triés par date.
    pub fn query_by_task(&self, task_id: &str) -> Result<Vec<LlmCallRecord>, LlmRepositoryError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, task_id, step_id, backend, model,
                    prompt_tokens, completion_tokens, cost_usd,
                    latency_ms, prompt_text, completion_text
             FROM llm_calls
             WHERE task_id = ?1
             ORDER BY created_at",
        )?;
        let rows = stmt.query_map(params![task_id], |row| {
            Ok(LlmCallRecord {
                id: row.get(0)?,
                task_id: row.get(1)?,
                step_id: row.get(2)?,
                backend: row.get(3)?,
                model: row.get(4)?,
                prompt_tokens: row.get(5)?,
                completion_tokens: row.get(6)?,
                cost_usd: row.get(7)?,
                latency_ms: row.get::<_, Option<i64>>(8)?.map(|v| v as u64),
                prompt_text: row.get(9)?,
                completion_text: row.get(10)?,
            })
        })?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// Agrégation coût/tokens par backend+modèle depuis `since` (format ISO 8601).
    ///
    /// Utilisé par le dashboard LLM Costs.
    pub fn costs_by_backend_model_since(
        &self,
        since: &str,
    ) -> Result<Vec<LlmCostSummary>, LlmRepositoryError> {
        let mut stmt = self.conn.prepare(
            "SELECT backend, model,
                    COUNT(*) AS call_count,
                    COALESCE(SUM(prompt_tokens), 0) + COALESCE(SUM(completion_tokens), 0) AS total_tokens,
                    COALESCE(SUM(cost_usd), 0.0) AS total_cost_usd
             FROM llm_calls
             WHERE created_at >= ?1
             GROUP BY backend, model
             ORDER BY total_cost_usd DESC",
        )?;
        let rows = stmt.query_map(params![since], |row| {
            Ok(LlmCostSummary {
                backend: row.get(0)?,
                model: row.get(1)?,
                call_count: row.get::<_, i64>(2)? as u64,
                total_tokens: row.get::<_, i64>(3)? as u64,
                total_cost_usd: row.get(4)?,
            })
        })?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// Agrégation coût journalier par backend depuis `since` (format ISO 8601).
    ///
    /// Retourne un vecteur de `LlmDailyCostSummary` trié par date ASC puis backend.
    /// Utilisé par le dashboard Observability.
    pub fn costs_by_day_backend_since(
        &self,
        since: &str,
    ) -> Result<Vec<LlmDailyCostSummary>, LlmRepositoryError> {
        let mut stmt = self.conn.prepare(
            "SELECT DATE(created_at) AS day, backend,
                    COALESCE(SUM(cost_usd), 0.0) AS cost_usd
             FROM llm_calls
             WHERE created_at >= ?1
             GROUP BY day, backend
             ORDER BY day ASC, backend ASC",
        )?;
        let rows = stmt.query_map(params![since], |row| {
            Ok(LlmDailyCostSummary {
                date: row.get(0)?,
                backend: row.get(1)?,
                cost_usd: row.get(2)?,
            })
        })?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }
}

// ─────────────────────────────────────────────
// EventBus subscriber
// ─────────────────────────────────────────────

/// Lance un subscriber EventBus qui persiste chaque `LlmCallCompleted`.
///
/// Le subscriber tourne dans un `tokio::spawn` dédié. Chaque persistance
/// passe par `spawn_blocking` (rusqlite est sync). Le prompt n'est persisté
/// que si `obs_config.debug_log_prompt` est `true`.
///
/// Le `JoinHandle` retourné peut être utilisé pour attendre l'arrêt
/// (le subscriber s'arrête quand l'EventBus est fermé — `RecvError`).
pub fn spawn_subscriber(
    repo: Arc<Mutex<LlmCallRepository>>,
    event_bus: &EventBusSender,
    obs_config: ObservabilityConfig,
) -> tokio::task::JoinHandle<()> {
    let mut rx = event_bus.subscribe();
    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(RuntimeEvent::LlmCallCompleted {
                    backend,
                    model,
                    task_id,
                    step_id,
                    prompt_tokens,
                    completion_tokens,
                    latency_ms,
                    cost_usd,
                }) => {
                    let record = LlmCallRecord {
                        id: uuid::Uuid::new_v4().to_string(),
                        task_id,
                        step_id,
                        backend,
                        model,
                        prompt_tokens: Some(prompt_tokens),
                        completion_tokens: Some(completion_tokens),
                        cost_usd,
                        latency_ms: Some(latency_ms),
                        // prompt_text omis ici car l'event ne le transporte pas ;
                        // seul le flag debug_log_prompt contrôle la persistance future.
                        prompt_text: None,
                        completion_text: None,
                    };
                    let _ = obs_config; // config conservée pour extensions futures
                    let repo = Arc::clone(&repo);
                    tokio::task::spawn_blocking(move || {
                        if let Ok(guard) = repo.lock() {
                            if let Err(e) = guard.save(&record) {
                                error!(error = %e, "failed to persist LLM call");
                            }
                        }
                    });
                }
                Ok(_) => {
                    // Ignorer les autres événements.
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(lagged = n, "LlmCallRepository subscriber lagged");
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    info!("LlmCallRepository subscriber: EventBus closed, stopping");
                    break;
                }
            }
        }
    })
}

// ─────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_llm_call_persisted_on_save() {
        // GIVEN un LlmCallRepository in-memory
        let repo = LlmCallRepository::open_in_memory().expect("open in-memory");

        // WHEN save(record avec backend="anthropic", tokens=150/50)
        let record = LlmCallRecord {
            id: "call-001".into(),
            task_id: Some("task-42".into()),
            step_id: None,
            backend: "anthropic".into(),
            model: "claude-sonnet-4-20250514".into(),
            prompt_tokens: Some(150),
            completion_tokens: Some(50),
            cost_usd: Some(0.003),
            latency_ms: Some(800),
            prompt_text: None,
            completion_text: Some("Hello!".into()),
        };
        repo.save(&record).expect("save should succeed");

        // THEN SELECT * FROM llm_calls → 1 row avec les bonnes valeurs
        let rows = repo.query_by_task("task-42").expect("query should succeed");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "call-001");
        assert_eq!(rows[0].backend, "anthropic");
        assert_eq!(rows[0].model, "claude-sonnet-4-20250514");
        assert_eq!(rows[0].prompt_tokens, Some(150));
        assert_eq!(rows[0].completion_tokens, Some(50));
        assert_eq!(rows[0].latency_ms, Some(800));
    }

    #[test]
    fn test_llm_call_prompt_null_when_not_provided() {
        // GIVEN debug_log_prompt = false → prompt_text not passed
        let repo = LlmCallRepository::open_in_memory().expect("open in-memory");

        // WHEN save(record avec prompt_text = None)
        let record = LlmCallRecord {
            id: "call-002".into(),
            task_id: Some("task-43".into()),
            step_id: None,
            backend: "local".into(),
            model: "llama3.2-q4".into(),
            prompt_tokens: Some(100),
            completion_tokens: Some(30),
            cost_usd: None,
            latency_ms: Some(200),
            prompt_text: None,
            completion_text: None,
        };
        repo.save(&record).expect("save should succeed");

        // THEN prompt_text IS NULL
        let rows = repo.query_by_task("task-43").expect("query should succeed");
        assert_eq!(rows.len(), 1);
        assert!(rows[0].prompt_text.is_none());
    }

    #[test]
    fn test_llm_call_cost_and_tokens_stored_correctly() {
        // GIVEN un record avec prompt_tokens=1000, completion_tokens=500, cost_usd=0.0125
        let repo = LlmCallRepository::open_in_memory().expect("open in-memory");

        let record = LlmCallRecord {
            id: "call-003".into(),
            task_id: Some("task-44".into()),
            step_id: Some("step-1".into()),
            backend: "anthropic".into(),
            model: "claude-opus-4-20250514".into(),
            prompt_tokens: Some(1000),
            completion_tokens: Some(500),
            cost_usd: Some(0.0125),
            latency_ms: Some(1500),
            prompt_text: None,
            completion_text: None,
        };
        repo.save(&record).expect("save should succeed");

        // THEN SELECT prompt_tokens, completion_tokens, cost_usd → (1000, 500, 0.0125)
        let rows = repo.query_by_task("task-44").expect("query should succeed");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].prompt_tokens, Some(1000));
        assert_eq!(rows[0].completion_tokens, Some(500));
        let cost = rows[0].cost_usd.expect("cost_usd should be Some");
        assert!((cost - 0.0125).abs() < f64::EPSILON);
    }

    #[test]
    fn test_costs_by_backend_model_aggregation() {
        // GIVEN plusieurs appels sur 2 backends
        let repo = LlmCallRepository::open_in_memory().expect("open in-memory");

        for i in 0..3 {
            let record = LlmCallRecord {
                id: format!("call-a{i}"),
                task_id: Some("task-50".into()),
                step_id: None,
                backend: "anthropic".into(),
                model: "sonnet".into(),
                prompt_tokens: Some(100),
                completion_tokens: Some(50),
                cost_usd: Some(0.001),
                latency_ms: Some(100),
                prompt_text: None,
                completion_text: None,
            };
            repo.save(&record).expect("save should succeed");
        }
        let record = LlmCallRecord {
            id: "call-b0".into(),
            task_id: Some("task-50".into()),
            step_id: None,
            backend: "local".into(),
            model: "llama".into(),
            prompt_tokens: Some(200),
            completion_tokens: Some(100),
            cost_usd: None,
            latency_ms: Some(50),
            prompt_text: None,
            completion_text: None,
        };
        repo.save(&record).expect("save should succeed");

        // WHEN agrégation depuis epoch
        let summaries = repo
            .costs_by_backend_model_since("2000-01-01T00:00:00Z")
            .expect("query should succeed");

        // THEN 2 groupes
        assert_eq!(summaries.len(), 2);

        let anthropic = summaries
            .iter()
            .find(|s| s.backend == "anthropic")
            .expect("anthropic group");
        assert_eq!(anthropic.call_count, 3);
        assert_eq!(anthropic.total_tokens, 450); // (100+50)*3
        assert!((anthropic.total_cost_usd - 0.003).abs() < f64::EPSILON);

        let local = summaries
            .iter()
            .find(|s| s.backend == "local")
            .expect("local group");
        assert_eq!(local.call_count, 1);
        assert_eq!(local.total_tokens, 300); // 200+100
        assert!(local.total_cost_usd.abs() < f64::EPSILON); // NULL → 0.0
    }

    #[test]
    fn test_query_by_task_returns_empty_for_unknown() {
        // GIVEN un repo vide
        let repo = LlmCallRepository::open_in_memory().expect("open in-memory");

        // WHEN query pour un task_id inconnu
        let rows = repo.query_by_task("unknown").expect("query should succeed");

        // THEN vide
        assert!(rows.is_empty());
    }

    #[test]
    fn test_llm_call_with_no_task_id() {
        // GIVEN un appel sans task_id (e.g. depuis /llm/chat API)
        let repo = LlmCallRepository::open_in_memory().expect("open in-memory");

        let record = LlmCallRecord {
            id: "call-no-task".into(),
            task_id: None,
            step_id: None,
            backend: "anthropic".into(),
            model: "sonnet".into(),
            prompt_tokens: Some(50),
            completion_tokens: Some(20),
            cost_usd: Some(0.001),
            latency_ms: Some(100),
            prompt_text: None,
            completion_text: None,
        };
        repo.save(&record).expect("save should succeed");

        // THEN l'enregistrement existe (query directe SQL)
        let count: i64 = repo
            .conn
            .query_row(
                "SELECT COUNT(*) FROM llm_calls WHERE id = ?1",
                params!["call-no-task"],
                |row| row.get(0),
            )
            .expect("count query");
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn test_subscriber_persists_llm_call_completed() {
        // GIVEN un repo in-memory + EventBus + subscriber
        let repo = LlmCallRepository::open_in_memory().expect("open in-memory");
        let repo = Arc::new(Mutex::new(repo));
        let (tx, _rx) = tokio::sync::broadcast::channel::<RuntimeEvent>(16);
        let obs = ObservabilityConfig::default();

        let _handle = spawn_subscriber(Arc::clone(&repo), &tx, obs);

        // WHEN un LlmCallCompleted est émis
        let _ = tx.send(RuntimeEvent::LlmCallCompleted {
            backend: "anthropic".into(),
            model: "sonnet".into(),
            task_id: Some("task-99".into()),
            step_id: None,
            prompt_tokens: 150,
            completion_tokens: 50,
            latency_ms: 800,
            cost_usd: Some(0.003),
        });

        // Attendre que le spawn_blocking ait le temps de s'exécuter
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // THEN l'enregistrement est persisté
        let guard = repo.lock().expect("lock");
        let rows = guard.query_by_task("task-99").expect("query");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].backend, "anthropic");
        assert_eq!(rows[0].model, "sonnet");
        assert_eq!(rows[0].prompt_tokens, Some(150));
        assert_eq!(rows[0].completion_tokens, Some(50));
    }
}
