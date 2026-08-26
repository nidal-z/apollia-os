//! `LlmCallRepository`, SQLite persistence of LLM calls.
//!
//! Each [`RuntimeEvent::LlmCallCompleted`] received on the EventBus is
//! persisted into the `llm_calls` table via [`spawn_subscriber`].
//!
//! `prompt_text` and `completion_text` are schema columns that the EventBus
//! subscriber never fills: `LlmCallCompleted` carries token counts and latency,
//! not text. They stay writable through [`LlmCallRepository::save`] for an
//! embedder that has the text in hand, and are always NULL for the calls the
//! runtime records itself.
//!
//! This module used to state that `prompt_text` was persisted when
//! `debug_log_prompt = true`. That was never implemented. The only setting that
//! can surface prompt content is `[llm.observability] debug_log_prompt`, which
//! logs at `TRACE` and persists nothing.

use std::path::Path;
use std::sync::{Arc, Mutex};

use rusqlite::{params, Connection};
use tracing::error;

use apollia_core::events::{subscribe_resilient, EventBusSender, RuntimeEvent};

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

/// Current schema version of `llm_calls.db`.
const LLM_CALLS_SCHEMA_VERSION: u32 = 1;

/// Numbered migration steps of `llm_calls.db`.
///
/// Step `k` migrates the file from version `k` to `k + 1`; the list length
/// always equals [`LLM_CALLS_SCHEMA_VERSION`].
const LLM_CALLS_MIGRATIONS: &[apollia_core::schema::Migration] = &[migrate_v1];

/// v1: the pre-versioning schema of the file, replayed idempotently.
fn migrate_v1(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(SCHEMA)
}

/// A persisted LLM call record in SQLite.
#[derive(Debug, Clone)]
pub struct LlmCallRecord {
    /// Unique identifier (UUID v4).
    pub id: String,
    /// Identifier of the task that triggered the call.
    pub task_id: Option<String>,
    /// ORIA step identifier (orchestrated mode only).
    pub step_id: Option<String>,
    /// Logical backend name (e.g. `"anthropic"`, `"local"`).
    pub backend: String,
    /// Model identifier (e.g. `"claude-sonnet-4-20250514"`).
    pub model: String,
    /// Number of tokens in the prompt.
    pub prompt_tokens: Option<u32>,
    /// Number of generated tokens.
    pub completion_tokens: Option<u32>,
    /// Estimated cost in USD.
    pub cost_usd: Option<f64>,
    /// Total call latency in milliseconds.
    pub latency_ms: Option<u64>,
    /// Prompt sent to the LLM. Always `None` for calls recorded by the
    /// runtime: the event carries no text. Set only by an embedder calling
    /// [`LlmCallRepository::save`] directly.
    pub prompt_text: Option<String>,
    /// Text of the completion returned by the LLM.
    pub completion_text: Option<String>,
}

/// Cost/token summary aggregated by backend+model (LLM Costs dashboard).
#[derive(Debug, Clone)]
pub struct LlmCostSummary {
    /// Logical backend name.
    pub backend: String,
    /// Model identifier.
    pub model: String,
    /// Number of calls.
    pub call_count: u64,
    /// Total tokens (prompt + completion).
    pub total_tokens: u64,
    /// Total estimated cost in USD.
    pub total_cost_usd: f64,
}

/// Daily cost summary aggregated by backend (LLM Costs daily chart).
#[derive(Debug, Clone)]
pub struct LlmDailyCostSummary {
    /// Local calendar day of the host, in `YYYY-MM-DD` format.
    pub date: String,
    /// Logical backend name.
    pub backend: String,
    /// Total estimated cost in USD for this day and backend.
    pub cost_usd: f64,
}

/// Errors of the LLM repository.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum LlmRepositoryError {
    /// Underlying SQLite error.
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// The database schema could not be brought to the supported version.
    #[error(transparent)]
    Schema(#[from] apollia_core::schema::SchemaError),
}

/// SQLite repository to persist LLM calls.
///
/// Created at Supervisor startup via [`LlmCallRepository::open`]. The EventBus
/// subscriber is started via [`spawn_subscriber`].
pub struct LlmCallRepository {
    conn: Connection,
}

impl LlmCallRepository {
    /// Open the SQLite database and apply the schema (CREATE TABLE IF NOT EXISTS).
    ///
    /// The file is created if it does not exist. WAL is enabled for concurrent
    /// write performance.
    ///
    /// # Errors
    ///
    /// Returns [`LlmRepositoryError::Sqlite`] if opening or migrating fails.
    pub fn open(path: &Path) -> Result<Self, LlmRepositoryError> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        apollia_core::schema::open_versioned(
            &conn,
            apollia_core::paths::DataFile::LlmCalls.file_name(),
            LLM_CALLS_SCHEMA_VERSION,
            LLM_CALLS_MIGRATIONS,
        )?;
        Ok(Self { conn })
    }

    /// Open an in-memory database for tests.
    #[cfg(test)]
    fn open_in_memory() -> Result<Self, LlmRepositoryError> {
        let conn = Connection::open_in_memory()?;
        apollia_core::schema::open_versioned(
            &conn,
            apollia_core::paths::DataFile::LlmCalls.file_name(),
            LLM_CALLS_SCHEMA_VERSION,
            LLM_CALLS_MIGRATIONS,
        )?;
        Ok(Self { conn })
    }

    /// Persist an LLM call record.
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

    /// Return all LLM calls for a task, sorted by date.
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

    /// Cost/token aggregation by backend+model since `since` (ISO 8601 format).
    ///
    /// Used by the LLM Costs dashboard.
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

    /// Daily cost aggregation by backend since `since` (ISO 8601 format).
    ///
    /// Returns a vector of `LlmDailyCostSummary` sorted by date ASC then backend.
    /// Used by the Observability dashboard.
    ///
    /// The day is the **local calendar day of the host**, not the UTC day.
    /// `created_at` is stored in UTC, so `DATE(created_at)` alone buckets a
    /// call by a day the operator never lived: on a host at UTC-7 an 18:00
    /// call falls in the next UTC day, and the chart, whose axis is the local
    /// calendar, then draws it on no bar at all. The `localtime` modifier is
    /// timezone-database driven, so it stays exact across a DST change.
    pub fn costs_by_day_backend_since(
        &self,
        since: &str,
    ) -> Result<Vec<LlmDailyCostSummary>, LlmRepositoryError> {
        let mut stmt = self.conn.prepare(
            "SELECT DATE(created_at, 'localtime') AS day, backend,
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

// ── EventBus subscriber ──────────────────────────────────────────────────

/// Start an EventBus subscriber that persists each `LlmCallCompleted`.
///
/// The subscriber runs in a dedicated `tokio::spawn`. Each persistence goes
/// through `spawn_blocking` (rusqlite is sync). No prompt or completion text is
/// written: the event does not carry any.
///
/// The returned `JoinHandle` can be used to await shutdown (the subscriber
/// stops when the EventBus is closed, a `RecvError`).
pub fn spawn_subscriber(
    repo: Arc<Mutex<LlmCallRepository>>,
    event_bus: &EventBusSender,
) -> tokio::task::JoinHandle<()> {
    let mut rx = subscribe_resilient(event_bus, "llm.call_repository");
    tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            match event {
                RuntimeEvent::LlmCallCompleted {
                    backend,
                    model,
                    task_id,
                    step_id,
                    prompt_tokens,
                    completion_tokens,
                    latency_ms,
                    cost_usd,
                    ..
                } => {
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
                        // The event carries no text, so these stay NULL. See the
                        // module header: there is no conditional persistence.
                        prompt_text: None,
                        completion_text: None,
                    };
                    let repo = Arc::clone(&repo);
                    tokio::task::spawn_blocking(move || {
                        if let Ok(guard) = repo.lock() {
                            if let Err(e) = guard.save(&record) {
                                error!(error = %e, "failed to persist LLM call");
                            }
                        }
                    });
                }
                _ => {
                    // Ignore other events.
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_llm_call_persisted_on_save() {
        // GIVEN an in-memory LlmCallRepository
        let repo = LlmCallRepository::open_in_memory().expect("open in-memory");

        // WHEN save(record with backend="anthropic", tokens=150/50)
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

        // THEN SELECT * FROM llm_calls returns 1 row with the right values
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
        // GIVEN a record saved without prompt text
        let repo = LlmCallRepository::open_in_memory().expect("open in-memory");

        // WHEN save(record with prompt_text = None)
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
        // GIVEN a record with prompt_tokens=1000, completion_tokens=500, cost_usd=0.0125
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

        // THEN SELECT prompt_tokens, completion_tokens, cost_usd returns (1000, 500, 0.0125)
        let rows = repo.query_by_task("task-44").expect("query should succeed");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].prompt_tokens, Some(1000));
        assert_eq!(rows[0].completion_tokens, Some(500));
        let cost = rows[0].cost_usd.expect("cost_usd should be Some");
        assert!((cost - 0.0125).abs() < f64::EPSILON);
    }

    #[test]
    fn test_costs_by_backend_model_aggregation() {
        // GIVEN several calls across 2 backends
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

        // WHEN aggregating since epoch
        let summaries = repo
            .costs_by_backend_model_since("2000-01-01T00:00:00Z")
            .expect("query should succeed");

        // THEN 2 groups
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

    /// Insert a call at an exact UTC instant. `save` lets SQLite stamp
    /// `created_at` with `now`, which no test can pin.
    fn insert_call_at(repo: &LlmCallRepository, id: &str, created_at: &str, cost: f64) {
        repo.conn
            .execute(
                "INSERT INTO llm_calls (id, backend, model, cost_usd, created_at)
                 VALUES (?1, 'anthropic', 'sonnet', ?2, ?3)",
                params![id, cost, created_at],
            )
            .expect("insert call");
    }

    #[test]
    fn test_daily_costs_bucket_on_the_hosts_local_calendar_day() {
        // GIVEN two calls at the instants that straddle local midnight on a
        // host west of UTC and on a host east of UTC
        let repo = LlmCallRepository::open_in_memory().expect("open in-memory");
        insert_call_at(&repo, "call-west", "2026-08-16T01:00:00.000Z", 0.50);
        insert_call_at(&repo, "call-east", "2026-08-14T23:00:00.000Z", 0.25);

        // WHEN aggregating the window that contains both
        let summaries = repo
            .costs_by_day_backend_since("2026-08-01T00:00:00Z")
            .expect("daily costs query");

        // THEN each call is filed under the day the host's calendar shows for
        // its instant, which is what the chart's axis is built from
        for (id, instant, cost) in [
            ("call-west", "2026-08-16T01:00:00Z", 0.50_f64),
            ("call-east", "2026-08-14T23:00:00Z", 0.25_f64),
        ] {
            let expected_day = instant
                .parse::<chrono::DateTime<chrono::Utc>>()
                .expect("parse instant")
                .with_timezone(&chrono::Local)
                .format("%Y-%m-%d")
                .to_string();
            let found = summaries
                .iter()
                .find(|s| (s.cost_usd - cost).abs() < 1e-9)
                .unwrap_or_else(|| panic!("no bucket carries the cost of {id}"));
            assert_eq!(
                found.date, expected_day,
                "{id} must be filed under its local calendar day"
            );
        }
    }

    #[test]
    fn test_query_by_task_returns_empty_for_unknown() {
        // GIVEN an empty repo
        let repo = LlmCallRepository::open_in_memory().expect("open in-memory");

        // WHEN querying for an unknown task_id
        let rows = repo.query_by_task("unknown").expect("query should succeed");

        // THEN empty
        assert!(rows.is_empty());
    }

    #[test]
    fn test_llm_call_with_no_task_id() {
        // GIVEN a call with no task_id (e.g. from the /llm/chat API)
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

        // THEN the record exists (direct SQL query)
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
        // GIVEN an in-memory repo + EventBus + subscriber
        let repo = LlmCallRepository::open_in_memory().expect("open in-memory");
        let repo = Arc::new(Mutex::new(repo));
        let (tx, _rx) = tokio::sync::broadcast::channel::<RuntimeEvent>(16);

        let _handle = spawn_subscriber(Arc::clone(&repo), &tx);

        // WHEN an LlmCallCompleted is emitted
        let _ = tx.send(RuntimeEvent::LlmCallCompleted {
            backend: "anthropic".into(),
            model: "sonnet".into(),
            task_id: Some("task-99".into()),
            step_id: None,
            prompt_tokens: 150,
            completion_tokens: 50,
            latency_ms: 800,
            cost_usd: Some(0.003),
            run_id: None,
        });

        // Wait for the spawn_blocking to have time to run
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // THEN the record is persisted
        let guard = repo.lock().expect("lock");
        let rows = guard.query_by_task("task-99").expect("query");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].backend, "anthropic");
        assert_eq!(rows[0].model, "sonnet");
        assert_eq!(rows[0].prompt_tokens, Some(150));
        assert_eq!(rows[0].completion_tokens, Some(50));
    }
}
