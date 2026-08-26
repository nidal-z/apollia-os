//! SQLite persistence for ORIA execution plans.
//!
//! `PlanRepository` is a synchronous wrapper around `rusqlite`, same pattern as
//! `AuditTrail`. All methods are synchronous; the async `ActorLoop` calls them
//! via `tokio::task::spawn_blocking` when needed.
//!
//! The `004_execution_plans.sql` migration is included at compile time and
//! applied idempotently when the database is opened.

use std::sync::{Mutex, MutexGuard, PoisonError};

use rusqlite::{params, Connection};

use apollia_core::observability::{truncate_with_marker, ObservabilityConfig};
use apollia_core::plan::StepProvenance;

use crate::plan::{ExecutionPlan, PlanStep};

/// Embedded migration SQL, applied idempotently on every open.
const MIGRATION_SQL: &str = include_str!("../../apollia-tools/migrations/004_execution_plans.sql");

/// Observability columns to add to `plan_steps`.
///
/// Each tuple: (column name, SQL type). Applied idempotently by the v1
/// migration step; columns that already exist are silently ignored.
const OBSERVABILITY_COLUMNS: &[(&str, &str)] = &[
    ("input_rendered", "TEXT"),
    ("input_truncated", "INTEGER NOT NULL DEFAULT 0"),
    ("output_text", "TEXT"),
    ("output_truncated", "INTEGER NOT NULL DEFAULT 0"),
    ("tool_used", "TEXT"),
    ("error_detail", "TEXT"),
    ("duration_ms", "INTEGER"),
];

/// Provenance columns to add to `plan_steps`.
///
/// `rationale` holds the free-form step justification, `provenance` holds the
/// JSON-encoded [`StepProvenance`]. Both are nullable so legacy rows survive
/// the additive migration, applied idempotently by the v1 migration step.
const PROVENANCE_COLUMNS: &[(&str, &str)] = &[("rationale", "TEXT"), ("provenance", "TEXT")];

/// Argument column to add to `plan_steps`.
///
/// `args` holds the JSON-encoded structured tool arguments (the step's
/// `Option<serde_json::Value>`). Nullable so legacy rows survive the additive
/// migration (applied idempotently by the v1 migration step) and read back
/// as `None`.
const ARGS_COLUMNS: &[(&str, &str)] = &[("args", "TEXT")];

/// Current schema version of `plans.db`.
const PLANS_SCHEMA_VERSION: u32 = 1;

/// Numbered migration steps of `plans.db`.
///
/// Step `k` migrates the file from version `k` to `k + 1`; the list length
/// always equals [`PLANS_SCHEMA_VERSION`].
const PLANS_MIGRATIONS: &[apollia_core::schema::Migration] = &[migrate_v1];

/// v1: the pre-versioning lineage of the file, replayed idempotently.
///
/// Every `plans.db` written before the versioned layer is at
/// `user_version = 0` whatever columns it carries, so this step must accept
/// a fresh file, the initial `004_execution_plans` shape, and every additive
/// intermediate one, and bring each of them to the same state.
fn migrate_v1(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(MIGRATION_SQL)?;
    for columns in [OBSERVABILITY_COLUMNS, PROVENANCE_COLUMNS, ARGS_COLUMNS] {
        for (col, col_type) in columns {
            apollia_core::schema::add_column_if_missing(
                conn,
                &format!("ALTER TABLE plan_steps ADD COLUMN {col} {col_type}"),
            )?;
        }
    }
    Ok(())
}

// Errors

/// Possible errors from the [`PlanRepository`].
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PlanRepositoryError {
    /// Underlying SQLite error.
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// The database schema could not be brought to the supported version.
    #[error(transparent)]
    Schema(#[from] apollia_core::schema::SchemaError),
    /// No plan found for the given `task_id`.
    #[error("plan not found for task_id: {0}")]
    NotFound(String),
}

// Return types

/// Full representation of a plan with its steps, used by `task inspect`.
#[derive(Debug)]
pub struct PlanWithSteps {
    /// Unique plan identifier (UUID v4).
    pub plan_id: String,
    /// Identifier of the associated task.
    pub task_id: String,
    /// Name of the agent that owns the plan.
    pub agent_name: String,
    /// Current status: `running` | `completed` | `failed` | `replanning`.
    pub status: String,
    /// Number of replans performed since creation.
    pub replan_count: u32,
    /// ISO 8601 timestamp of plan creation.
    pub created_at: String,
    /// Plan steps in SQLite retrieval order.
    pub steps: Vec<StepRecord>,
}

/// Full state of a single step as persisted in SQLite.
#[derive(Debug)]
pub struct StepRecord {
    /// Unique identifier within the plan (e.g. `"s1"`).
    pub step_id: String,
    /// Natural-language description of the action to perform.
    pub description: String,
    /// Tool suggested by the LLM, if any.
    pub tool_hint: Option<String>,
    /// Identifiers of the steps this step depends on (deserialized from JSON).
    pub depends_on: Vec<String>,
    /// Current status: `pending` | `running` | `completed` | `failed` | `skipped`.
    pub status: String,
    /// Output produced by the step, present once `completed`.
    pub output: Option<String>,
    /// Error message, present when `failed` or `skipped`.
    pub error: Option<String>,
    /// Step start timestamp.
    pub started_at: Option<String>,
    /// Step completion timestamp.
    pub completed_at: Option<String>,
    /// Input rendered after template interpolation (possibly truncated).
    pub input_rendered: Option<String>,
    /// Whether `input_rendered` was truncated.
    pub input_truncated: bool,
    /// Observability output text (possibly truncated).
    pub output_text: Option<String>,
    /// Whether `output_text` was truncated.
    pub output_truncated: bool,
    /// Name of the tool actually used (vs `tool_hint` suggested by the LLM).
    pub tool_used: Option<String>,
    /// Full error detail for diagnostics (vs `error` which is the brief message).
    pub error_detail: Option<String>,
    /// Step execution duration in milliseconds.
    pub duration_ms: Option<i64>,
    /// Free-form justification of the step, `None` for legacy rows.
    pub rationale: Option<String>,
    /// Provenance of the step; a legacy `NULL` value loads as
    /// [`StepProvenance::default`] (origin Initial, `at` 0).
    pub provenance: StepProvenance,
    /// Structured tool arguments; `None` for legacy rows or steps without a
    /// resolved argument object.
    pub args: Option<serde_json::Value>,
}

// Repository

/// SQLite repository for persisting ORIA execution plans.
///
/// Wraps a `rusqlite` connection behind a [`Mutex`] to offer
/// an `&self` API on all methods while allowing the mutable borrow required for
/// atomic transactions (cf. [`Self::begin_replan`]).
///
/// **Thread safety:** `PlanRepository` is `Send + Sync` thanks to the `Mutex`.
/// Every SQLite operation acquires the lock, so operations are serialized at the
/// database level (consistent with SQLite's single-writer model).
pub struct PlanRepository {
    conn: Mutex<Connection>,
}

impl PlanRepository {
    /// Open the SQLite database and apply the `004_execution_plans.sql` migration.
    ///
    /// The migration is idempotent (`CREATE TABLE IF NOT EXISTS`), so it is safe
    /// to re-run on an existing database.
    ///
    /// # Errors
    /// Returns [`PlanRepositoryError::Sqlite`] if opening or the migration fails.
    pub fn new(db_path: &str) -> Result<Self, PlanRepositoryError> {
        let conn = Connection::open(db_path)?;
        apollia_core::schema::open_versioned(
            &conn,
            apollia_core::paths::DataFile::Plans.file_name(),
            PLANS_SCHEMA_VERSION,
            PLANS_MIGRATIONS,
        )?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// The connection guard, recovered when a panic poisoned the mutex.
    ///
    /// Poisoning records that a previous holder panicked; it says nothing about
    /// the SQLite handle, which `rusqlite` leaves usable because it finalizes
    /// statements on drop and rolls back an unfinished transaction. Recovering
    /// the guard keeps one panic from turning every later plan write into a
    /// second panic, which is what `expect` did here.
    fn locked(&self) -> MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Insert a new plan with `running` status.
    ///
    /// # Errors
    /// Returns [`PlanRepositoryError::Sqlite`] on a SQLite error (e.g. duplicate `plan_id`).
    pub fn insert_plan(
        &self,
        plan: &ExecutionPlan,
        agent_name: &str,
    ) -> Result<(), PlanRepositoryError> {
        self.locked().execute(
            "INSERT INTO execution_plans (plan_id, task_id, agent_name, status) \
             VALUES (?1, ?2, ?3, 'running')",
            params![plan.plan_id, plan.task_id, agent_name],
        )?;
        Ok(())
    }

    /// JSON encoding of the default [`StepProvenance`] (origin Initial, at 0).
    ///
    /// Used as the fallback when a step's provenance cannot be serialized, which
    /// keeps the insert infallible without an `unwrap`. The literal mirrors the
    /// `StepProvenance::default()` shape.
    fn default_provenance_json() -> String {
        serde_json::to_string(&StepProvenance::default())
            .unwrap_or_else(|_| r#"{"origin":"initial","reason":null,"at":0}"#.to_string())
    }

    /// Read a step's rationale and provenance back from SQLite.
    ///
    /// A `NULL` provenance (legacy row written before this column existed) or a
    /// corrupted JSON payload degrades to [`StepProvenance::default`] (origin
    /// [`apollia_core::plan::StepOrigin::Initial`], `at` 0) instead of failing
    /// the read, so older plans stay loadable without a backfill.
    ///
    /// The `plan_steps` primary key is composite `(step_id, plan_id)`: a
    /// `step_id` like `"s1"` is only unique within its plan, so both halves of
    /// the key are required to read the right row.
    ///
    /// # Errors
    /// Returns [`PlanRepositoryError::NotFound`] when no step matches
    /// `(plan_id, step_id)`, or [`PlanRepositoryError::Sqlite`] on any other
    /// SQLite error.
    pub fn load_step_provenance(
        &self,
        plan_id: &str,
        step_id: &str,
    ) -> Result<(Option<String>, StepProvenance), PlanRepositoryError> {
        let conn = self.locked();
        let (rationale, provenance_raw): (Option<String>, Option<String>) = conn
            .query_row(
                "SELECT rationale, provenance FROM plan_steps WHERE plan_id = ?1 AND step_id = ?2",
                params![plan_id, step_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => {
                    PlanRepositoryError::NotFound(step_id.to_string())
                }
                other => PlanRepositoryError::Sqlite(other),
            })?;

        let provenance = provenance_raw
            .map(|json| serde_json::from_str(&json).unwrap_or_default())
            .unwrap_or_default();
        Ok((rationale, provenance))
    }

    /// Insert a plan's steps, all with `pending` status.
    ///
    /// Each step's `depends_on` field is serialized to JSON before storage, and
    /// `rationale` plus the JSON-encoded `provenance` are persisted alongside.
    ///
    /// # Errors
    /// Returns [`PlanRepositoryError::Sqlite`] if a step cannot be inserted.
    pub fn insert_steps(
        &self,
        plan_id: &str,
        steps: &[PlanStep],
    ) -> Result<(), PlanRepositoryError> {
        let conn = self.locked();
        for step in steps {
            let depends_on_json =
                serde_json::to_string(&step.depends_on).unwrap_or_else(|_| "[]".to_string());
            let provenance_json = serde_json::to_string(&step.provenance)
                .unwrap_or_else(|_| Self::default_provenance_json());
            let args_json = step
                .args
                .as_ref()
                .and_then(|v| serde_json::to_string(v).ok());
            conn.execute(
                "INSERT INTO plan_steps \
                     (step_id, plan_id, description, tool_hint, depends_on, status, \
                      rationale, provenance, args) \
                     VALUES (?1, ?2, ?3, ?4, ?5, 'pending', ?6, ?7, ?8)",
                params![
                    step.step_id,
                    plan_id,
                    step.description,
                    step.tool_hint,
                    depends_on_json,
                    step.rationale,
                    provenance_json,
                    args_json,
                ],
            )?;
        }
        Ok(())
    }

    /// Mark a step as `running` with `started_at = CURRENT_TIMESTAMP`.
    ///
    /// # Errors
    /// Returns [`PlanRepositoryError::Sqlite`] on a SQLite error.
    pub fn start_step(&self, plan_id: &str, step_id: &str) -> Result<(), PlanRepositoryError> {
        self.locked().execute(
            "UPDATE plan_steps \
             SET status = 'running', started_at = CURRENT_TIMESTAMP \
             WHERE plan_id = ?1 AND step_id = ?2",
            params![plan_id, step_id],
        )?;
        Ok(())
    }

    /// Mark a step as `completed` with its `output` and `completed_at = CURRENT_TIMESTAMP`.
    ///
    /// # Errors
    /// Returns [`PlanRepositoryError::Sqlite`] on a SQLite error.
    pub fn complete_step(
        &self,
        plan_id: &str,
        step_id: &str,
        output: &str,
    ) -> Result<(), PlanRepositoryError> {
        self.locked().execute(
            "UPDATE plan_steps \
             SET status = 'completed', output = ?1, completed_at = CURRENT_TIMESTAMP \
             WHERE plan_id = ?2 AND step_id = ?3",
            params![output, plan_id, step_id],
        )?;
        Ok(())
    }

    /// Mark a step as `failed` with an error message.
    ///
    /// # Errors
    /// Returns [`PlanRepositoryError::Sqlite`] on a SQLite error.
    pub fn fail_step(
        &self,
        plan_id: &str,
        step_id: &str,
        error: &str,
    ) -> Result<(), PlanRepositoryError> {
        self.locked().execute(
            "UPDATE plan_steps \
             SET status = 'failed', error = ?1, completed_at = CURRENT_TIMESTAMP \
             WHERE plan_id = ?2 AND step_id = ?3",
            params![error, plan_id, step_id],
        )?;
        Ok(())
    }

    /// Prepare the replan atomically.
    ///
    /// In a single SQLite transaction:
    /// - Set `execution_plans.status` to `replanning`
    /// - Update `replan_count` with `new_count`
    /// - **Delete** all steps with `pending` status (`completed`/`failed` steps are kept)
    ///
    /// The new steps must then be inserted via [`Self::insert_steps`].
    ///
    /// # Errors
    /// Returns [`PlanRepositoryError::Sqlite`] if the transaction fails.
    pub fn begin_replan(&self, plan_id: &str, new_count: u32) -> Result<(), PlanRepositoryError> {
        let mut conn = self.locked();
        let tx = conn.transaction()?;
        tx.execute(
            "DELETE FROM plan_steps WHERE plan_id = ?1 AND status = 'pending'",
            params![plan_id],
        )?;
        tx.execute(
            "UPDATE execution_plans \
             SET status = 'replanning', replan_count = ?1, updated_at = CURRENT_TIMESTAMP \
             WHERE plan_id = ?2",
            params![new_count, plan_id],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Mark the plan as `failed` and move all its `pending` steps to `skipped`.
    ///
    /// The failure reason is kept in the `error` field of the skipped steps.
    ///
    /// # Errors
    /// Returns [`PlanRepositoryError::Sqlite`] on a SQLite error.
    pub fn fail_plan(&self, plan_id: &str, reason: &str) -> Result<(), PlanRepositoryError> {
        let conn = self.locked();
        conn.execute(
            "UPDATE plan_steps \
             SET status = 'skipped', error = ?1, completed_at = CURRENT_TIMESTAMP \
             WHERE plan_id = ?2 AND status = 'pending'",
            params![reason, plan_id],
        )?;
        conn.execute(
            "UPDATE execution_plans \
             SET status = 'failed', updated_at = CURRENT_TIMESTAMP \
             WHERE plan_id = ?1",
            params![plan_id],
        )?;
        Ok(())
    }

    /// Mark the plan as `completed`.
    ///
    /// # Errors
    /// Returns [`PlanRepositoryError::Sqlite`] on a SQLite error.
    pub fn complete_plan(&self, plan_id: &str) -> Result<(), PlanRepositoryError> {
        self.locked().execute(
            "UPDATE execution_plans \
             SET status = 'completed', updated_at = CURRENT_TIMESTAMP \
             WHERE plan_id = ?1",
            params![plan_id],
        )?;
        Ok(())
    }

    /// Persist a step's rendered input with truncation per [`ObservabilityConfig`].
    ///
    /// The input is truncated when its size exceeds `config.max_input_bytes`.
    /// The `input_truncated` flag is set accordingly.
    ///
    /// # Errors
    /// Returns [`PlanRepositoryError::Sqlite`] on a SQLite error.
    pub fn save_step_input(
        &self,
        step_id: &str,
        plan_id: &str,
        rendered_input: &str,
        config: &ObservabilityConfig,
    ) -> Result<(), PlanRepositoryError> {
        let (text, truncated) = truncate_with_marker(rendered_input, config.max_input_bytes);
        self.locked().execute(
            "UPDATE plan_steps \
             SET input_rendered = ?1, input_truncated = ?2 \
             WHERE plan_id = ?3 AND step_id = ?4",
            params![text, truncated as i32, plan_id, step_id],
        )?;
        Ok(())
    }

    /// Persist a step's observability output with truncation.
    ///
    /// Distinct from the `output` column (used by ORIA for chaining).
    /// The output is truncated when its size exceeds `config.max_output_bytes`.
    ///
    /// # Errors
    /// Returns [`PlanRepositoryError::Sqlite`] on a SQLite error.
    pub fn save_step_output(
        &self,
        step_id: &str,
        plan_id: &str,
        output: &str,
        config: &ObservabilityConfig,
    ) -> Result<(), PlanRepositoryError> {
        let (text, truncated) = truncate_with_marker(output, config.max_output_bytes);
        self.locked().execute(
            "UPDATE plan_steps \
             SET output_text = ?1, output_truncated = ?2 \
             WHERE plan_id = ?3 AND step_id = ?4",
            params![text, truncated as i32, plan_id, step_id],
        )?;
        Ok(())
    }

    /// Persist a step's full error detail for diagnostics.
    ///
    /// Distinct from the `error` column (brief message used by ORIA).
    ///
    /// # Errors
    /// Returns [`PlanRepositoryError::Sqlite`] on a SQLite error.
    pub fn save_step_error(
        &self,
        step_id: &str,
        plan_id: &str,
        error_detail: &str,
    ) -> Result<(), PlanRepositoryError> {
        self.locked().execute(
            "UPDATE plan_steps \
             SET error_detail = ?1 \
             WHERE plan_id = ?2 AND step_id = ?3",
            params![error_detail, plan_id, step_id],
        )?;
        Ok(())
    }

    /// Persist the name of the tool actually used by the step.
    ///
    /// Distinct from `tool_hint` (tool suggested by the LLM).
    ///
    /// # Errors
    /// Returns [`PlanRepositoryError::Sqlite`] on a SQLite error.
    pub fn save_step_tool(
        &self,
        step_id: &str,
        plan_id: &str,
        tool_name: &str,
    ) -> Result<(), PlanRepositoryError> {
        self.locked().execute(
            "UPDATE plan_steps \
             SET tool_used = ?1 \
             WHERE plan_id = ?2 AND step_id = ?3",
            params![tool_name, plan_id, step_id],
        )?;
        Ok(())
    }

    /// Persist the step execution duration in milliseconds.
    ///
    /// # Errors
    /// Returns [`PlanRepositoryError::Sqlite`] on a SQLite error.
    pub fn save_step_duration(
        &self,
        step_id: &str,
        plan_id: &str,
        duration_ms: i64,
    ) -> Result<(), PlanRepositoryError> {
        self.locked().execute(
            "UPDATE plan_steps \
             SET duration_ms = ?1 \
             WHERE plan_id = ?2 AND step_id = ?3",
            params![duration_ms, plan_id, step_id],
        )?;
        Ok(())
    }

    /// Retrieve the full plan with its steps from SQLite.
    ///
    /// Used by `apollia-os task inspect`.
    ///
    /// # Errors
    /// - [`PlanRepositoryError::NotFound`] if no plan is associated with `task_id`.
    /// - [`PlanRepositoryError::Sqlite`] for any other SQLite error.
    pub fn get_plan_with_steps(&self, task_id: &str) -> Result<PlanWithSteps, PlanRepositoryError> {
        let conn = self.locked();

        // Fetch the plan
        let (plan_id, agent_name, status, replan_count, created_at) = conn
            .query_row(
                "SELECT plan_id, agent_name, status, replan_count, created_at \
                 FROM execution_plans WHERE task_id = ?1",
                params![task_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, u32>(3)?,
                        row.get::<_, String>(4)?,
                    ))
                },
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => {
                    PlanRepositoryError::NotFound(task_id.to_string())
                }
                other => PlanRepositoryError::Sqlite(other),
            })?;

        // Fetch the steps
        let mut stmt = conn.prepare(
            "SELECT step_id, description, tool_hint, depends_on, status, \
                    output, error, started_at, completed_at, \
                    input_rendered, input_truncated, output_text, output_truncated, \
                    tool_used, error_detail, duration_ms, rationale, provenance, args \
             FROM plan_steps WHERE plan_id = ?1",
        )?;

        let steps = stmt
            .query_map(params![plan_id], |row| {
                let depends_on_str: String = row.get(3)?;
                // Data inserted by our own code: safe fallback if the JSON is malformed.
                let depends_on: Vec<String> =
                    serde_json::from_str(&depends_on_str).unwrap_or_default();
                let input_truncated_raw: i32 = row.get(10)?;
                let output_truncated_raw: i32 = row.get(12)?;
                // A legacy NULL provenance or a corrupted payload degrades to the
                // default (origin Initial, at 0) rather than failing the read.
                let provenance_raw: Option<String> = row.get(17)?;
                let provenance = provenance_raw
                    .map(|json| serde_json::from_str(&json).unwrap_or_default())
                    .unwrap_or_default();
                // A legacy NULL args or a corrupted payload reads back as None.
                let args_raw: Option<String> = row.get(18)?;
                let args = args_raw.and_then(|json| serde_json::from_str(&json).ok());
                Ok(StepRecord {
                    step_id: row.get(0)?,
                    description: row.get(1)?,
                    tool_hint: row.get(2)?,
                    depends_on,
                    status: row.get(4)?,
                    output: row.get(5)?,
                    error: row.get(6)?,
                    started_at: row.get(7)?,
                    completed_at: row.get(8)?,
                    input_rendered: row.get(9)?,
                    input_truncated: input_truncated_raw != 0,
                    output_text: row.get(11)?,
                    output_truncated: output_truncated_raw != 0,
                    tool_used: row.get(13)?,
                    error_detail: row.get(14)?,
                    duration_ms: row.get(15)?,
                    rationale: row.get(16)?,
                    provenance,
                    args,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(PlanWithSteps {
            plan_id,
            task_id: task_id.to_string(),
            agent_name,
            status,
            replan_count,
            created_at,
            steps,
        })
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Tests
// ────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    fn make_repo() -> (PlanRepository, NamedTempFile) {
        let f = NamedTempFile::new().unwrap();
        let repo = PlanRepository::new(f.path().to_str().unwrap()).unwrap();
        (repo, f)
    }

    fn make_plan(task_id: &str) -> ExecutionPlan {
        ExecutionPlan {
            plan_id: uuid::Uuid::new_v4().to_string(),
            task_id: task_id.to_string(),
            steps: vec![
                {
                    let mut s = PlanStep::new("s1", "Step 1");
                    s.tool_hint = Some("file_io".into());
                    s
                },
                {
                    let mut s = PlanStep::new("s2", "Step 2");
                    s.depends_on = vec!["s1".into()];
                    s
                },
            ],
        }
    }

    // GIVEN: PlanRepository::new() on an empty database
    // WHEN:  a plan is looked up in the freshly migrated schema
    // THEN:  the query reaches a table that exists and reports the plan absent.
    //        A migration that never ran surfaces here as Sqlite("no such
    //        table"), which `new()` returning Ok cannot tell apart.
    #[test]
    fn test_migration_appliquee() {
        let (repo, _f) = make_repo();
        let found = repo.get_plan_with_steps("no-such-task");
        assert!(
            matches!(found, Err(PlanRepositoryError::NotFound(_))),
            "{found:?}"
        );
    }

    // GIVEN: an open PlanRepository
    // WHEN:  full life cycle: insert_plan -> start/complete steps -> complete_plan
    // THEN:  get_plan_with_steps returns status=completed and correct outputs
    #[test]
    fn test_cycle_de_vie_complet() {
        let (repo, _f) = make_repo();
        let plan = make_plan("task-001");

        repo.insert_plan(&plan, "test-agent").unwrap();
        repo.insert_steps(&plan.plan_id, &plan.steps).unwrap();
        repo.start_step(&plan.plan_id, "s1").unwrap();
        repo.complete_step(&plan.plan_id, "s1", "output-1").unwrap();
        repo.start_step(&plan.plan_id, "s2").unwrap();
        repo.complete_step(&plan.plan_id, "s2", "output-2").unwrap();
        repo.complete_plan(&plan.plan_id).unwrap();

        let result = repo.get_plan_with_steps("task-001").unwrap();
        assert_eq!(result.status, "completed");
        let s1 = result.steps.iter().find(|s| s.step_id == "s1").unwrap();
        assert_eq!(s1.status, "completed");
        assert_eq!(s1.output.as_deref(), Some("output-1"));
        let s2 = result.steps.iter().find(|s| s.step_id == "s2").unwrap();
        assert_eq!(s2.output.as_deref(), Some("output-2"));
    }

    // GIVEN: a plan with s1 completed, s2 pending
    // WHEN:  begin_replan(plan_id, 1)
    // THEN:  status=replanning, replan_count=1, s2 deleted, s1 kept
    #[test]
    fn test_replan_supprime_pending_garde_completed() {
        let (repo, _f) = make_repo();
        let plan = make_plan("task-002");

        repo.insert_plan(&plan, "test-agent").unwrap();
        repo.insert_steps(&plan.plan_id, &plan.steps).unwrap();
        repo.complete_step(&plan.plan_id, "s1", "done").unwrap();

        repo.begin_replan(&plan.plan_id, 1).unwrap();

        let result = repo.get_plan_with_steps("task-002").unwrap();
        assert_eq!(result.status, "replanning");
        assert_eq!(result.replan_count, 1);

        let s1 = result.steps.iter().find(|s| s.step_id == "s1").unwrap();
        assert_eq!(s1.status, "completed"); // kept

        let s2 = result.steps.iter().find(|s| s.step_id == "s2");
        assert!(s2.is_none()); // deleted
    }

    // GIVEN: a plan with s1 and s2 pending
    // WHEN:  fail_plan(plan_id, "STEP_BUDGET_EXCEEDED")
    // THEN:  plan.status=failed, all steps skipped or failed
    #[test]
    fn test_fail_plan_skippe_pending() {
        let (repo, _f) = make_repo();
        let plan = make_plan("task-003");

        repo.insert_plan(&plan, "test-agent").unwrap();
        repo.insert_steps(&plan.plan_id, &plan.steps).unwrap();

        repo.fail_plan(&plan.plan_id, "STEP_BUDGET_EXCEEDED")
            .unwrap();

        let result = repo.get_plan_with_steps("task-003").unwrap();
        assert_eq!(result.status, "failed");
        for step in &result.steps {
            assert!(
                step.status == "skipped" || step.status == "failed",
                "unexpected step status: {}",
                step.status,
            );
        }
    }

    // GIVEN / WHEN: get_plan_with_steps on a nonexistent task_id
    // THEN:  PlanRepositoryError::NotFound returned
    #[test]
    fn test_not_found() {
        let (repo, _f) = make_repo();
        let result = repo.get_plan_with_steps("inexistant");
        assert!(matches!(result, Err(PlanRepositoryError::NotFound(_))));
    }

    // step observability

    // GIVEN a step in a plan
    // WHEN  save_step_input with a short text
    // THEN  input_rendered persisted, input_truncated = false
    #[test]
    fn test_step_input_rendered_persisted() {
        let (repo, _f) = make_repo();
        let plan = make_plan("task-obs-1");
        repo.insert_plan(&plan, "test-agent").unwrap();
        repo.insert_steps(&plan.plan_id, &plan.steps).unwrap();

        let config = ObservabilityConfig::default();
        repo.save_step_input("s1", &plan.plan_id, "lire /tmp/data.json", &config)
            .unwrap();

        let result = repo.get_plan_with_steps("task-obs-1").unwrap();
        let s1 = result.steps.iter().find(|s| s.step_id == "s1").unwrap();
        assert_eq!(s1.input_rendered.as_deref(), Some("lire /tmp/data.json"));
        assert!(!s1.input_truncated);
    }

    // GIVEN a completed step
    // WHEN  save_step_output + save_step_tool + save_step_duration
    // THEN  output_text, tool_used, duration_ms persisted
    #[test]
    fn test_step_output_on_success() {
        let (repo, _f) = make_repo();
        let plan = make_plan("task-obs-2");
        repo.insert_plan(&plan, "test-agent").unwrap();
        repo.insert_steps(&plan.plan_id, &plan.steps).unwrap();

        let config = ObservabilityConfig::default();
        repo.save_step_output("s1", &plan.plan_id, "file content", &config)
            .unwrap();
        repo.save_step_tool("s1", &plan.plan_id, "file_io").unwrap();
        repo.save_step_duration("s1", &plan.plan_id, 42).unwrap();

        let result = repo.get_plan_with_steps("task-obs-2").unwrap();
        let s1 = result.steps.iter().find(|s| s.step_id == "s1").unwrap();
        assert_eq!(s1.output_text.as_deref(), Some("file content"));
        assert!(!s1.output_truncated);
        assert_eq!(s1.tool_used.as_deref(), Some("file_io"));
        assert_eq!(s1.duration_ms, Some(42));
    }

    // GIVEN a failed step
    // WHEN  save_step_error + save_step_tool + save_step_duration
    // THEN  error_detail, tool_used, duration_ms persisted
    #[test]
    fn test_step_error_detail_on_failure() {
        let (repo, _f) = make_repo();
        let plan = make_plan("task-obs-3");
        repo.insert_plan(&plan, "test-agent").unwrap();
        repo.insert_steps(&plan.plan_id, &plan.steps).unwrap();

        repo.save_step_error("s1", &plan.plan_id, "Permission denied: /etc/shadow")
            .unwrap();
        repo.save_step_tool("s1", &plan.plan_id, "file_io").unwrap();
        repo.save_step_duration("s1", &plan.plan_id, 5).unwrap();

        let result = repo.get_plan_with_steps("task-obs-3").unwrap();
        let s1 = result.steps.iter().find(|s| s.step_id == "s1").unwrap();
        assert_eq!(
            s1.error_detail.as_deref(),
            Some("Permission denied: /etc/shadow")
        );
        assert_eq!(s1.tool_used.as_deref(), Some("file_io"));
        assert_eq!(s1.duration_ms, Some(5));
    }

    // GIVEN a step
    // WHEN  save_step_duration with a value
    // THEN  duration_ms persisted
    #[test]
    fn test_step_duration_measured() {
        let (repo, _f) = make_repo();
        let plan = make_plan("task-obs-4");
        repo.insert_plan(&plan, "test-agent").unwrap();
        repo.insert_steps(&plan.plan_id, &plan.steps).unwrap();

        repo.save_step_duration("s1", &plan.plan_id, 150).unwrap();

        let result = repo.get_plan_with_steps("task-obs-4").unwrap();
        let s1 = result.steps.iter().find(|s| s.step_id == "s1").unwrap();
        assert_eq!(s1.duration_ms, Some(150));
    }

    // GIVEN a step with an input > max_input_bytes
    // WHEN  save_step_input
    // THEN  input truncated, input_truncated = true
    #[test]
    fn test_step_input_truncated_when_over_limit() {
        let (repo, _f) = make_repo();
        let plan = make_plan("task-obs-5");
        repo.insert_plan(&plan, "test-agent").unwrap();
        repo.insert_steps(&plan.plan_id, &plan.steps).unwrap();

        let config = ObservabilityConfig {
            max_input_bytes: 50,
            ..ObservabilityConfig::default()
        };
        let long_input = "x".repeat(200);
        repo.save_step_input("s1", &plan.plan_id, &long_input, &config)
            .unwrap();

        let result = repo.get_plan_with_steps("task-obs-5").unwrap();
        let s1 = result.steps.iter().find(|s| s.step_id == "s1").unwrap();
        assert!(s1.input_truncated);
        assert!(s1
            .input_rendered
            .as_ref()
            .is_some_and(|t| t.contains("200 bytes total")));
    }

    // provenance and rationale

    // GIVEN a base plan_steps schema in a fresh in-memory database
    // WHEN  applying the v1 migration step twice
    // THEN  both provenance columns exist and neither run raised an error
    #[test]
    fn test_provenance_migration_is_idempotent() {
        // GIVEN
        let conn = Connection::open_in_memory().expect("open");
        conn.execute_batch(MIGRATION_SQL).expect("base");

        // WHEN
        migrate_v1(&conn).expect("first");
        migrate_v1(&conn).expect("second");

        // THEN
        let cols: Vec<String> = {
            let mut stmt = conn
                .prepare("PRAGMA table_info(plan_steps)")
                .expect("pragma");
            let rows = stmt
                .query_map([], |row| row.get::<_, String>(1))
                .expect("query");
            rows.collect::<Result<Vec<_>, _>>().expect("collect")
        };
        assert!(cols.iter().any(|c| c == "rationale"));
        assert!(cols.iter().any(|c| c == "provenance"));
    }

    // GIVEN a step persisted with a rationale and Replan(1) provenance
    // WHEN  reading it back via load_step_provenance and get_plan_with_steps
    // THEN  both fields round-trip verbatim
    #[test]
    fn test_round_trip_with_provenance() {
        use apollia_core::plan::{StepOrigin, StepProvenance};

        // GIVEN
        let (repo, _f) = make_repo();
        let mut plan = make_plan("task-prov-1");
        plan.steps[0].rationale = Some("decoupe".into());
        plan.steps[0].provenance = StepProvenance {
            origin: StepOrigin::Replan(1),
            reason: Some("revision".into()),
            at: 1_700_000_000,
        };
        repo.insert_plan(&plan, "test-agent").unwrap();
        repo.insert_steps(&plan.plan_id, &plan.steps).unwrap();

        // WHEN
        let (rationale, provenance) = repo
            .load_step_provenance(&plan.plan_id, "s1")
            .expect("load");

        // THEN
        assert_eq!(rationale.as_deref(), Some("decoupe"));
        assert_eq!(provenance.origin, StepOrigin::Replan(1));
        assert_eq!(provenance.reason.as_deref(), Some("revision"));
        assert_eq!(provenance.at, 1_700_000_000);

        let result = repo.get_plan_with_steps("task-prov-1").unwrap();
        let s1 = result.steps.iter().find(|s| s.step_id == "s1").unwrap();
        assert_eq!(s1.rationale.as_deref(), Some("decoupe"));
        assert_eq!(s1.provenance.origin, StepOrigin::Replan(1));

        // A step inserted with default provenance round-trips to Initial / None.
        let (rationale_s2, provenance_s2) = repo
            .load_step_provenance(&plan.plan_id, "s2")
            .expect("load s2");
        assert_eq!(rationale_s2, None);
        assert_eq!(provenance_s2.origin, StepOrigin::Initial);
    }

    // GIVEN two plans that both own a step id "s1" with distinct provenance
    // WHEN  loading "s1" for the second plan
    // THEN  the second plan's provenance is returned, not the first plan's
    //       (the composite key (step_id, plan_id) disambiguates across plans)
    #[test]
    fn test_load_step_provenance_disambiguates_across_plans() {
        use apollia_core::plan::{StepOrigin, StepProvenance};

        // GIVEN plan A with s1 = Replan(1) and plan B with s1 = Replan(2)
        let (repo, _f) = make_repo();

        let mut plan_a = make_plan("task-A");
        plan_a.steps[0].rationale = Some("from-A".into());
        plan_a.steps[0].provenance = StepProvenance {
            origin: StepOrigin::Replan(1),
            reason: Some("A".into()),
            at: 1_000,
        };
        repo.insert_plan(&plan_a, "agent-a").unwrap();
        repo.insert_steps(&plan_a.plan_id, &plan_a.steps).unwrap();

        let mut plan_b = make_plan("task-B");
        plan_b.steps[0].rationale = Some("from-B".into());
        plan_b.steps[0].provenance = StepProvenance {
            origin: StepOrigin::Replan(2),
            reason: Some("B".into()),
            at: 2_000,
        };
        repo.insert_plan(&plan_b, "agent-b").unwrap();
        repo.insert_steps(&plan_b.plan_id, &plan_b.steps).unwrap();

        // WHEN loading s1 scoped to plan B
        let (rationale, provenance) = repo
            .load_step_provenance(&plan_b.plan_id, "s1")
            .expect("load");

        // THEN plan B's row is returned, never plan A's
        assert_eq!(rationale.as_deref(), Some("from-B"));
        assert_eq!(provenance.origin, StepOrigin::Replan(2));
        assert_eq!(provenance.at, 2_000);
    }

    // GIVEN a legacy row whose provenance column is NULL
    // WHEN  reading it back via load_step_provenance and get_plan_with_steps
    // THEN  it degrades to the default (origin Initial, at 0) without panic
    #[test]
    fn test_legacy_null_provenance_defaults_to_initial() {
        use apollia_core::plan::StepOrigin;

        // GIVEN: insert a row directly, leaving rationale and provenance NULL
        let (repo, _f) = make_repo();
        let plan = make_plan("task-legacy-1");
        repo.insert_plan(&plan, "test-agent").unwrap();
        {
            let conn = repo.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO plan_steps \
                     (step_id, plan_id, description, tool_hint, depends_on, status) \
                     VALUES ('legacy', ?1, 'old step', NULL, '[]', 'pending')",
                params![plan.plan_id],
            )
            .unwrap();
        }

        // WHEN
        let (rationale, provenance) = repo
            .load_step_provenance(&plan.plan_id, "legacy")
            .expect("load");

        // THEN
        assert_eq!(rationale, None);
        assert_eq!(provenance.origin, StepOrigin::Initial);
        assert_eq!(provenance.reason, None);
        assert_eq!(provenance.at, 0);

        let result = repo.get_plan_with_steps("task-legacy-1").unwrap();
        let legacy = result.steps.iter().find(|s| s.step_id == "legacy").unwrap();
        assert_eq!(legacy.provenance.origin, StepOrigin::Initial);
        assert_eq!(legacy.rationale, None);
    }

    // GIVEN a step persisted with structured tool arguments
    // WHEN  reading it back via get_plan_with_steps
    // THEN  the args JSON round-trips verbatim
    #[test]
    fn test_round_trip_with_args() {
        // GIVEN
        let (repo, _f) = make_repo();
        let mut plan = make_plan("task-args-1");
        plan.steps[0].tool_hint = Some("file_write".into());
        plan.steps[0].args = Some(serde_json::json!({"path": "/tmp/x", "content": "hi"}));
        repo.insert_plan(&plan, "test-agent").unwrap();
        repo.insert_steps(&plan.plan_id, &plan.steps).unwrap();

        // WHEN
        let result = repo.get_plan_with_steps("task-args-1").unwrap();
        let s1 = result.steps.iter().find(|s| s.step_id == "s1").unwrap();

        // THEN
        assert_eq!(
            s1.args,
            Some(serde_json::json!({"path": "/tmp/x", "content": "hi"}))
        );
        // A step inserted without args round-trips to None.
        let s2 = result.steps.iter().find(|s| s.step_id == "s2").unwrap();
        assert_eq!(s2.args, None);
    }

    // GIVEN a legacy row whose args column is NULL
    // WHEN  reading it back via get_plan_with_steps
    // THEN  it degrades to None without panic
    #[test]
    fn test_legacy_null_args_defaults_to_none() {
        // GIVEN: insert a row directly, leaving args NULL
        let (repo, _f) = make_repo();
        let plan = make_plan("task-legacy-args");
        repo.insert_plan(&plan, "test-agent").unwrap();
        {
            let conn = repo.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO plan_steps \
                     (step_id, plan_id, description, tool_hint, depends_on, status) \
                     VALUES ('legacy', ?1, 'old step', NULL, '[]', 'pending')",
                params![plan.plan_id],
            )
            .unwrap();
        }

        // WHEN
        let result = repo.get_plan_with_steps("task-legacy-args").unwrap();
        let legacy = result.steps.iter().find(|s| s.step_id == "legacy").unwrap();

        // THEN
        assert_eq!(legacy.args, None);
    }

    // GIVEN: plan with a non-empty depends_on
    // WHEN:  get_plan_with_steps
    // THEN:  depends_on is correctly deserialized
    #[test]
    fn test_depends_on_deserialise() {
        let (repo, _f) = make_repo();
        let plan = make_plan("task-004");

        repo.insert_plan(&plan, "test-agent").unwrap();
        repo.insert_steps(&plan.plan_id, &plan.steps).unwrap();

        let result = repo.get_plan_with_steps("task-004").unwrap();
        assert_eq!(result.steps.len(), 2);

        let s1 = result.steps.iter().find(|s| s.step_id == "s1").unwrap();
        assert!(s1.depends_on.is_empty());

        let s2 = result.steps.iter().find(|s| s.step_id == "s2").unwrap();
        assert_eq!(s2.depends_on, vec!["s1".to_string()]);
    }
}
