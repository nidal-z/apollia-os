//! PlanChoiceStore: SQLite persistence of RLHF plan choices.
//!
//! Each time an operator chooses between two alternative plans (binary feedback),
//! the choice is logged in the `plan_choices` table. This data stays local
//! (local-first) and serves as an RLHF signal to improve future plans.

use apollia_core::plan_alternatives::{PlanAlternatives, PlanChoice};

use crate::store::MemoryStore;

// ─────────────────────────────────────────────
// Errors
// ─────────────────────────────────────────────

/// Errors of the [`PlanChoiceStore`].
#[derive(Debug, thiserror::Error)]
pub enum PlanChoiceStoreError {
    /// Writing a choice to the database failed.
    #[error("failed to log plan choice: {0}")]
    LogFailed(String),

    /// Reading a choice from the database failed.
    #[error("failed to query plan choice: {0}")]
    QueryFailed(String),

    /// Raw SQLite error propagated from rusqlite.
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

// ─────────────────────────────────────────────
// PlanChoiceStore
// ─────────────────────────────────────────────

/// Persistence backend for binary plan choices (RLHF).
///
/// Persists each operator choice in the `plan_choices` table of the [`MemoryStore`].
/// The data never leaves the local machine.
pub struct PlanChoiceStore<'a> {
    store: &'a MemoryStore,
}

/// Entry returned by [`PlanChoiceStore::get_choice`].
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PlanChoiceEntry {
    /// Auto-incremented identifier.
    pub id: i64,
    /// Session identifier correlating with [`PlanAlternatives::session_id`].
    pub session_id: String,
    /// Chosen plan (`"plan_a"` or `"plan_b"`).
    pub chosen: String,
    /// Unix timestamp of the choice (seconds).
    pub chosen_at: i64,
}

impl<'a> PlanChoiceStore<'a> {
    /// Creates a store bound to an existing [`MemoryStore`].
    pub fn new(store: &'a MemoryStore) -> Self {
        Self { store }
    }

    /// Persists a plan choice in SQLite.
    ///
    /// Records the `session_id`, the chosen plan (`"plan_a"` / `"plan_b"`),
    /// both full plans as JSON, and the Unix timestamp of the choice.
    ///
    /// The `UNIQUE` constraint on `session_id` guarantees that only one choice
    /// can be recorded per session (idempotence).
    pub fn log_plan_choice(
        &self,
        choice: &PlanChoice,
        alternatives: &PlanAlternatives,
    ) -> Result<i64, PlanChoiceStoreError> {
        let plan_a_json = serde_json::to_string(&alternatives.plan_a).map_err(|e| {
            PlanChoiceStoreError::LogFailed(format!("failed to serialize plan_a: {e}"))
        })?;
        let plan_b_json = serde_json::to_string(&alternatives.plan_b).map_err(|e| {
            PlanChoiceStoreError::LogFailed(format!("failed to serialize plan_b: {e}"))
        })?;

        let conn = self.store.conn();
        conn.execute(
            "INSERT INTO plan_choices (session_id, chosen, plan_a_json, plan_b_json, chosen_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                choice.session_id,
                choice.chosen.as_db_str(),
                plan_a_json,
                plan_b_json,
                choice.chosen_at,
            ],
        )
        .map_err(|e| PlanChoiceStoreError::LogFailed(e.to_string()))?;

        let row_id = conn.last_insert_rowid();

        tracing::info!(
            session_id = %choice.session_id,
            chosen = choice.chosen.as_db_str(),
            row_id = row_id,
            "plan choice logged"
        );

        Ok(row_id)
    }

    /// Returns the choice recorded for a given session, or `None` if absent.
    pub fn get_choice(
        &self,
        session_id: &str,
    ) -> Result<Option<PlanChoiceEntry>, PlanChoiceStoreError> {
        let conn = self.store.conn();
        let mut stmt = conn
            .prepare(
                "SELECT id, session_id, chosen, chosen_at
                 FROM plan_choices WHERE session_id = ?1 LIMIT 1",
            )
            .map_err(|e| PlanChoiceStoreError::QueryFailed(e.to_string()))?;

        let mut rows = stmt
            .query_map(rusqlite::params![session_id], |row| {
                Ok(PlanChoiceEntry {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    chosen: row.get(2)?,
                    chosen_at: row.get(3)?,
                })
            })
            .map_err(|e| PlanChoiceStoreError::QueryFailed(e.to_string()))?;

        match rows.next() {
            Some(row) => {
                Ok(Some(row.map_err(|e| {
                    PlanChoiceStoreError::QueryFailed(e.to_string())
                })?))
            }
            None => Ok(None),
        }
    }

    /// Returns the total number of recorded choices.
    pub fn count(&self) -> Result<i64, PlanChoiceStoreError> {
        let conn = self.store.conn();
        conn.query_row("SELECT COUNT(*) FROM plan_choices", [], |row| row.get(0))
            .map_err(|e| PlanChoiceStoreError::QueryFailed(e.to_string()))
    }
}

// ─────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::MemoryStore;
    use apollia_core::plan_alternatives::{ChosenPlan, PlanAlternatives, TaskPlan};

    fn setup() -> (MemoryStore, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("apollia_pc_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.db");
        let store = MemoryStore::open(&path).unwrap();
        (store, path)
    }

    fn make_alternatives(session_id: &str) -> PlanAlternatives {
        let empty_plan = TaskPlan {
            plan_id: uuid::Uuid::new_v4().to_string(),
            task_id: "task-test".into(),
            steps: vec![],
        };
        PlanAlternatives {
            plan_a: empty_plan.clone(),
            plan_b: empty_plan,
            session_id: session_id.to_string(),
            generated_at: 1_700_000_000,
        }
    }

    // GIVEN a PlanChoiceStore with a plan_choices table
    // WHEN log_plan_choice(PlanChoice { chosen: PlanA, session_id: "test-123" })
    // THEN the table contains an entry with chosen = "plan_a"
    #[test]
    fn test_log_plan_choice_plan_a() {
        // GIVEN
        let (store, _) = setup();
        let choice_store = PlanChoiceStore::new(&store);
        let session_id = "test-123";
        let alternatives = make_alternatives(session_id);
        let choice = PlanChoice {
            session_id: session_id.to_string(),
            chosen: ChosenPlan::PlanA,
            chosen_at: 1_700_000_100,
        };

        // WHEN
        let row_id = choice_store
            .log_plan_choice(&choice, &alternatives)
            .unwrap();

        // THEN
        assert!(row_id > 0);
        let entry = choice_store.get_choice(session_id).unwrap().unwrap();
        assert_eq!(entry.chosen, "plan_a");
        assert_eq!(entry.session_id, session_id);
        assert_eq!(entry.chosen_at, 1_700_000_100);
    }

    // Plan B is logged correctly
    #[test]
    fn test_log_plan_choice_plan_b() {
        // GIVEN
        let (store, _) = setup();
        let choice_store = PlanChoiceStore::new(&store);
        let session_id = "test-456";
        let alternatives = make_alternatives(session_id);
        let choice = PlanChoice {
            session_id: session_id.to_string(),
            chosen: ChosenPlan::PlanB,
            chosen_at: 1_700_000_200,
        };

        // WHEN
        choice_store
            .log_plan_choice(&choice, &alternatives)
            .unwrap();

        // THEN
        let entry = choice_store.get_choice(session_id).unwrap().unwrap();
        assert_eq!(entry.chosen, "plan_b");
    }

    // UNIQUE constraint on session_id prevents duplicate entries
    #[test]
    fn test_duplicate_session_id_rejected() {
        // GIVEN
        let (store, _) = setup();
        let choice_store = PlanChoiceStore::new(&store);
        let session_id = "test-789";
        let alternatives = make_alternatives(session_id);
        let choice = PlanChoice {
            session_id: session_id.to_string(),
            chosen: ChosenPlan::PlanA,
            chosen_at: 1_700_000_300,
        };

        // WHEN
        choice_store
            .log_plan_choice(&choice, &alternatives)
            .unwrap();
        let result = choice_store.log_plan_choice(&choice, &alternatives);

        // THEN: second insert should fail (UNIQUE constraint)
        assert!(result.is_err(), "duplicate session_id should be rejected");
    }

    // get_choice returns None for unknown session
    #[test]
    fn test_get_choice_unknown_session() {
        // GIVEN
        let (store, _) = setup();
        let choice_store = PlanChoiceStore::new(&store);

        // WHEN
        let result = choice_store.get_choice("nonexistent-session").unwrap();

        // THEN
        assert!(result.is_none());
    }

    // count returns the number of logged choices
    #[test]
    fn test_count_returns_number_of_choices() {
        // GIVEN
        let (store, _) = setup();
        let choice_store = PlanChoiceStore::new(&store);

        // WHEN: log 3 choices
        for i in 0..3 {
            let session_id = format!("session-{i}");
            let alternatives = make_alternatives(&session_id);
            let choice = PlanChoice {
                session_id: session_id.clone(),
                chosen: ChosenPlan::PlanA,
                chosen_at: 1_700_000_000 + i,
            };
            choice_store
                .log_plan_choice(&choice, &alternatives)
                .unwrap();
        }

        // THEN
        assert_eq!(choice_store.count().unwrap(), 3);
    }
}
