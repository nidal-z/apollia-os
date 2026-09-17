//! The human-in-the-loop half of the task repository.
//!
//! Split out of `task_repository.rs`: opening the store and reading a task
//! stay in the parent, the rows that park a task on a human answer, resume it,
//! and report the approvals live here.

use std::time::Duration;

use apollia_core::{truncate_with_marker, AIPTask, InputResponseData, ObservabilityConfig};
use rusqlite::params;

use crate::task_repository::{
    open_conn, ApprovalInfo, PauseRecord, PausedTaskRow, ResolvedApprovalRow, TaskRepoError,
    TaskRepository,
};

impl TaskRepository {
    /// Persists a pause with its typed payload, agent and skill.
    ///
    /// The payload must already be parsed and checked: this layer stores what
    /// it is given and is not where an invalid payload is refused. `agent_name`
    /// and `skill_id` are written only when given, so a later pause of the same
    /// task keeps what an earlier one recorded.
    ///
    /// # Errors
    ///
    /// - [`TaskRepoError::Json`] if `context` or `payload` cannot be serialized
    /// - [`TaskRepoError::Sqlite`] on a SQLite error
    pub async fn save_pause(&self, record: PauseRecord<'_>) -> Result<(), TaskRepoError> {
        let path = self.db_path.clone();
        let task_id = record.task_id.to_string();
        let step_id = record.step_id.map(str::to_string);
        let prompt = record.prompt.to_string();
        let context_json = serde_json::to_string(record.context)?;
        let payload_json = record.payload.map(serde_json::to_string).transpose()?;
        let agent_name = record.agent_name.map(str::to_string);
        let skill_id = record.skill_id.map(str::to_string);

        tokio::task::spawn_blocking(move || -> Result<(), TaskRepoError> {
            let conn = open_conn(&path)?;
            conn.execute(
                "INSERT INTO tasks \
                     (task_id, step_id, status, agent_name, skill_id, \
                      input_required_prompt, input_required_context, input_required_payload, \
                      input_required_at) \
                 VALUES (?1, ?2, 'input_required', COALESCE(?5, ''), ?6, ?3, ?4, ?7, \
                         CURRENT_TIMESTAMP) \
                 ON CONFLICT(task_id) DO UPDATE SET \
                     step_id                = excluded.step_id, \
                     status                 = 'input_required', \
                     agent_name             = COALESCE(?5, tasks.agent_name), \
                     skill_id               = COALESCE(?6, tasks.skill_id), \
                     input_required_prompt  = excluded.input_required_prompt, \
                     input_required_context = excluded.input_required_context, \
                     input_required_payload = excluded.input_required_payload, \
                     input_response_approved = NULL, \
                     input_response_reason  = NULL, \
                     input_response_answer  = NULL, \
                     input_response_at      = NULL, \
                     input_required_at      = CURRENT_TIMESTAMP, \
                     updated_at             = CURRENT_TIMESTAMP",
                params![
                    &task_id,
                    &step_id,
                    &prompt,
                    &context_json,
                    &agent_name,
                    &skill_id,
                    &payload_json
                ],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| TaskRepoError::Internal(e.to_string()))??;

        Ok(())
    }

    /// The pause a task is waiting on, as the listing and the resume route read it.
    ///
    /// `None` when the task is not paused.
    ///
    /// # Errors
    ///
    /// - [`TaskRepoError::Json`] if a stored JSON column is invalid
    /// - [`TaskRepoError::Sqlite`] on a SQLite error
    pub async fn pending_pause(
        &self,
        task_id: &str,
    ) -> Result<Option<PausedTaskRow>, TaskRepoError> {
        let path = self.db_path.clone();
        let task_id = task_id.to_string();

        tokio::task::spawn_blocking(move || -> Result<Option<PausedTaskRow>, TaskRepoError> {
            let conn = open_conn(&path)?;
            let row = conn.query_row(
                "SELECT COALESCE(agent_name, ''), skill_id, created_at, \
                        COALESCE(input_required_prompt, ''), input_required_payload, \
                        COALESCE(input_required_context, '{}') \
                 FROM tasks WHERE task_id = ?1 AND status = 'input_required'",
                params![&task_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, Option<String>>(4)?,
                        row.get::<_, String>(5)?,
                    ))
                },
            );
            let (agent_name, skill_id, created_at, prompt, payload_json, context_json) = match row {
                Ok(r) => r,
                Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(None),
                Err(e) => return Err(e.into()),
            };
            let payload = payload_json
                .as_deref()
                .map(serde_json::from_str::<serde_json::Value>)
                .transpose()?;
            let context = serde_json::from_str(&context_json)?;
            Ok(Some(PausedTaskRow {
                agent_name,
                skill_id,
                created_at: apollia_core::utils::sqlite_to_rfc3339(&created_at),
                prompt,
                payload,
                context,
            }))
        })
        .await
        .map_err(|e| TaskRepoError::Internal(e.to_string()))?
    }
    /// Records the suspension timestamp for an `input_required` event.
    ///
    /// Inserts a preliminary row into `task_approvals` with `suspended_at` set
    /// and `approved IS NULL`. The `prompt` and `context_json` are read from the
    /// `tasks` table (populated by [`save_input_required`]).
    ///
    /// Called by ORIA when emitting `AIPResult.input_required()`.
    ///
    /// # Errors
    ///
    /// - [`TaskRepoError::Sqlite`] on a SQLite error
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
    /// Persists the human response and inserts a row into `task_approvals`.
    ///
    /// Updates `input_response_approved`, `input_response_reason`,
    /// `input_response_at` and `status` in `tasks`, then inserts a row into
    /// `task_approvals` for the multi-approval history. Called by the
    /// `ResumeHandler` after the response is validated.
    ///
    /// # Errors
    ///
    /// - [`TaskRepoError::NotFound`] if `task_id` is absent from the `tasks` table
    /// - [`TaskRepoError::Json`] if the context cannot be serialized
    /// - [`TaskRepoError::Sqlite`] on a SQLite error
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
        let answer_json = response
            .answer
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;
        let payload_json = response
            .payload
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;

        tokio::task::spawn_blocking(move || -> Result<(), TaskRepoError> {
            let conn = open_conn(&path)?;

            // Fetch prompt and step_id for the task_approvals insert.
            let (prompt, step_id): (String, Option<String>) = conn
                .query_row(
                    "SELECT COALESCE(input_required_prompt, ''), step_id \
                     FROM tasks WHERE task_id = ?1",
                    params![&task_id],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
                )
                .map_err(|_| TaskRepoError::NotFound(task_id.clone()))?;

            // Update the tasks row with the human decision.
            conn.execute(
                "UPDATE tasks SET \
                     input_response_approved = ?2, \
                     input_response_reason   = ?3, \
                     input_response_at       = ?4, \
                     input_response_answer   = ?5, \
                     status                  = 'working', \
                     updated_at              = CURRENT_TIMESTAMP \
                 WHERE task_id = ?1",
                params![
                    &task_id,
                    approved as i32,
                    &reason,
                    &responded_at,
                    &answer_json
                ],
            )?;

            // Update the pending row created by save_suspended_at.
            // If no pending row exists (backward compat), fallback to INSERT.
            let updated = conn.execute(
                "UPDATE task_approvals \
                 SET approved = ?2, \
                     reason = ?3, \
                     responded_at = ?4, \
                     answer_json = ?5, \
                     payload_json = COALESCE(payload_json, ?6), \
                     wait_duration_ms = CAST( \
                         (julianday(?4) - julianday(suspended_at)) * 86400000 AS INTEGER \
                     ) \
                 WHERE task_id = ?1 AND approved IS NULL",
                params![
                    &task_id,
                    approved as i32,
                    &reason,
                    &responded_at,
                    &answer_json,
                    &payload_json
                ],
            )?;

            if updated == 0 {
                conn.execute(
                    "INSERT INTO task_approvals \
                         (task_id, step_id, prompt, context_json, approved, reason, responded_at, \
                      answer_json, payload_json) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    params![
                        &task_id,
                        &step_id,
                        &prompt,
                        &context_json,
                        approved as i32,
                        &reason,
                        &responded_at,
                        &answer_json,
                        &payload_json,
                    ],
                )?;
            }

            Ok(())
        })
        .await
        .map_err(|e| TaskRepoError::Internal(e.to_string()))??;

        Ok(())
    }
    /// Rebuilds an enriched [`AIPTask`] for resuming after `input_required`.
    ///
    /// Reads the `input_response_*` columns from `tasks` and builds an `AIPTask`
    /// with `is_resumed = true` and `input_response` populated with the human
    /// decision and the original JSON context. Called by the `ResumeHandler`
    /// before relaunching the agent through ORIA.
    ///
    /// # Errors
    ///
    /// - [`TaskRepoError::NotFound`] if `task_id` is absent from the `tasks` table
    /// - [`TaskRepoError::Json`] if the stored JSON context is invalid
    /// - [`TaskRepoError::Sqlite`] on a SQLite error
    pub async fn rebuild_for_resume(&self, task_id: &str) -> Result<AIPTask, TaskRepoError> {
        let path = self.db_path.clone();
        let task_id = task_id.to_string();

        tokio::task::spawn_blocking(move || -> Result<AIPTask, TaskRepoError> {
            let conn = open_conn(&path)?;

            // Read the HITL columns needed for reconstruction.
            #[allow(clippy::type_complexity)]
            let (
                tid,
                approved_raw,
                reason,
                context_json_opt,
                responded_at_opt,
                answer_json,
                payload_json,
            ): (
                String,
                Option<i32>,
                Option<String>,
                Option<String>,
                Option<String>,
                Option<String>,
                Option<String>,
            ) = conn
                .query_row(
                    "SELECT task_id, \
                            input_response_approved, \
                            input_response_reason, \
                            input_required_context, \
                            input_response_at, \
                            input_response_answer, \
                            input_required_payload \
                     FROM tasks WHERE task_id = ?1",
                    params![&task_id],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, Option<i32>>(1)?,
                            row.get::<_, Option<String>>(2)?,
                            row.get::<_, Option<String>>(3)?,
                            row.get::<_, Option<String>>(4)?,
                            row.get::<_, Option<String>>(5)?,
                            row.get::<_, Option<String>>(6)?,
                        ))
                    },
                )
                .map_err(|_| TaskRepoError::NotFound(task_id.clone()))?;

            let input_response = match approved_raw {
                Some(raw) => {
                    let ctx_str = context_json_opt.as_deref().unwrap_or("{}");
                    let context: serde_json::Value = serde_json::from_str(ctx_str)?;
                    let answer = answer_json
                        .as_deref()
                        .map(serde_json::from_str::<serde_json::Value>)
                        .transpose()?;
                    let payload = payload_json
                        .as_deref()
                        .map(serde_json::from_str::<serde_json::Value>)
                        .transpose()?;
                    Some(InputResponseData {
                        approved: raw != 0,
                        reason,
                        context,
                        responded_at: responded_at_opt.unwrap_or_default(),
                        answer,
                        payload,
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
    /// Persists a task's input text, truncating if necessary.
    ///
    /// Uses [`truncate_with_marker`] to cut the input if its size exceeds
    /// `config.max_input_bytes`. The `input_truncated` column is set to 1 if the
    /// text was truncated, 0 otherwise.
    ///
    /// Creates the `tasks` row via `INSERT ... ON CONFLICT DO UPDATE` so it can
    /// be called before or after `save_input_required`.
    ///
    /// # Errors
    ///
    /// Returns [`TaskRepoError::Sqlite`] on a SQLite error.
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
    /// Returns the `task_id`s in `input_required` status for longer than `older_than`.
    ///
    /// Uses `strftime('%s', 'now') - strftime('%s', input_required_at)` to
    /// compute the elapsed seconds and compare them to the `older_than.as_secs()`
    /// threshold. Used by the `TimeoutWatcher` to cancel expired tasks.
    ///
    /// # Errors
    ///
    /// Returns [`TaskRepoError::Sqlite`] on a SQLite error.
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
    /// Returns the approval info for a task in `input_required` status.
    ///
    /// Reads the prompt, JSON context, and `suspended_at` from the `tasks` and
    /// `task_approvals` tables. Returns `None` if the task does not exist or is
    /// not in `input_required`.
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
                        ta.suspended_at, \
                        t.input_required_payload, \
                        t.skill_id \
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
                let payload_str: Option<String> = row.get(4)?;
                let skill_id: Option<String> = row.get(5)?;
                Ok((
                    agent_name,
                    prompt,
                    context_str,
                    suspended_at,
                    payload_str,
                    skill_id,
                ))
            }) {
                Ok(row) => Some(row),
                Err(rusqlite::Error::QueryReturnedNoRows) => None,
                Err(e) => return Err(TaskRepoError::Sqlite(e)),
            };

            match result {
                None => Ok(None),
                Some((agent_name, prompt, context_str, suspended_at, payload_str, skill_id)) => {
                    let context = serde_json::from_str(&context_str).unwrap_or_default();
                    // Stored only after it parsed at the pause, so an unreadable
                    // column is a defect of the row, not a prompt-only pause.
                    let payload = payload_str
                        .as_deref()
                        .map(serde_json::from_str::<serde_json::Value>)
                        .transpose()?;
                    Ok(Some(ApprovalInfo {
                        agent_name,
                        prompt,
                        context,
                        suspended_at: suspended_at.unwrap_or_default(),
                        payload,
                        skill_id,
                    }))
                }
            }
        })
        .await
        .map_err(|e| TaskRepoError::Internal(e.to_string()))?
    }
    /// Lists resolved approvals (approved or rejected) from the last `days` days.
    ///
    /// Returns at most `limit` rows sorted by `responded_at` descending. Reads
    /// the `task_approvals` table joined to `tasks` to fetch the `agent_name`.
    ///
    /// # Errors
    ///
    /// - [`TaskRepoError::Sqlite`] on a SQLite error
    /// - [`TaskRepoError::Internal`] if the `spawn_blocking` fails
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
}

#[cfg(test)]
mod typed_pause_tests {
    use super::*;
    use serde_json::json;

    async fn open_test_repo() -> TaskRepository {
        let path = std::env::temp_dir().join(format!("apollia_pause_{}.db", uuid::Uuid::new_v4()));
        TaskRepository::open(&path).await.expect("open failed")
    }

    fn approbation() -> serde_json::Value {
        json!({"genre": "approbation", "geste": "crm/delete", "risque": "critical", "detail": ["id: 42"]})
    }

    #[tokio::test]
    async fn a_typed_pause_reads_back_with_its_payload_agent_and_skill() {
        // GIVEN a repository and a pause carrying a payload
        let repo = open_test_repo().await;
        let payload = approbation();
        repo.save_pause(PauseRecord {
            task_id: "t-1",
            step_id: None,
            prompt: "Delete contact 42?",
            context: &json!({"step": 2}),
            payload: Some(&payload),
            agent_name: Some("crm-agent"),
            skill_id: Some("cleanup"),
        })
        .await
        .expect("save");

        // WHEN the pending pause is read
        let row = repo
            .pending_pause("t-1")
            .await
            .expect("read")
            .expect("paused");

        // THEN everything the listing needs comes back as it was written
        assert_eq!(row.agent_name, "crm-agent");
        assert_eq!(row.skill_id.as_deref(), Some("cleanup"));
        assert_eq!(row.prompt, "Delete contact 42?");
        assert_eq!(row.payload, Some(payload));
        assert_eq!(row.context, json!({"step": 2}));
        assert!(!row.created_at.is_empty());
    }

    #[tokio::test]
    async fn the_pending_approval_info_carries_the_payload_and_skill() {
        // GIVEN a typed pause from a skill
        let repo = open_test_repo().await;
        let payload = approbation();
        repo.save_pause(PauseRecord {
            task_id: "t-3",
            step_id: None,
            prompt: "Delete contact 42?",
            context: &json!({"step": 1}),
            payload: Some(&payload),
            agent_name: Some("crm-agent"),
            skill_id: Some("cleanup"),
        })
        .await
        .expect("save");

        // WHEN the approvals listing reads it
        let info = repo
            .get_approval_info("t-3")
            .await
            .expect("read")
            .expect("pending");

        // THEN the card can be drawn from it, as from the task listing
        assert_eq!(info.payload, Some(payload));
        assert_eq!(info.skill_id.as_deref(), Some("cleanup"));
        assert_eq!(info.context, json!({"step": 1}));
    }

    #[tokio::test]
    async fn a_resumed_task_carries_the_answer_and_the_payload_it_answers() {
        // GIVEN a task paused on a question
        let repo = open_test_repo().await;
        let question = json!({"genre": "choix", "question": "Which?",
                              "propositions": [{"id": "a", "libelle": "A"}]});
        repo.save_pause(PauseRecord {
            task_id: "t-2",
            step_id: None,
            prompt: "Which?",
            context: &json!({"kept": true}),
            payload: Some(&question),
            agent_name: None,
            skill_id: None,
        })
        .await
        .expect("save");

        // WHEN the operator answers and the task is rebuilt for resume
        repo.save_input_response(
            "t-2",
            &InputResponseData {
                approved: true,
                reason: None,
                context: json!({}),
                responded_at: "2026-09-16T10:00:00Z".into(),
                answer: Some(json!("a")),
                payload: Some(question.clone()),
            },
        )
        .await
        .expect("respond");
        let task = repo.rebuild_for_resume("t-2").await.expect("rebuild");

        // THEN the resumed agent receives the answer, the payload it answers,
        // and its own context rather than an empty one
        let response = task.input_response.expect("a response");
        assert_eq!(response.answer, Some(json!("a")));
        assert_eq!(response.payload, Some(question));
        assert_eq!(response.context, json!({"kept": true}));
        // AND the task is no longer listed as paused
        assert!(repo.pending_pause("t-2").await.expect("read").is_none());
    }

    #[tokio::test]
    async fn a_second_pause_does_not_inherit_the_first_answer() {
        // GIVEN a task that paused, was answered, then paused again
        let repo = open_test_repo().await;
        let first = json!({"genre": "confirmation", "question": "Go?"});
        repo.save_pause(PauseRecord {
            task_id: "t-3",
            step_id: None,
            prompt: "Go?",
            context: &json!({}),
            payload: Some(&first),
            agent_name: Some("agent"),
            skill_id: Some("skill"),
        })
        .await
        .expect("save first");
        repo.save_input_response(
            "t-3",
            &InputResponseData {
                approved: true,
                reason: None,
                context: json!({}),
                responded_at: "2026-09-16T10:00:00Z".into(),
                answer: Some(json!(true)),
                payload: Some(first),
            },
        )
        .await
        .expect("answer first");
        let second = approbation();
        repo.save_pause(PauseRecord {
            task_id: "t-3",
            step_id: None,
            prompt: "Delete?",
            context: &json!({"first": true}),
            payload: Some(&second),
            agent_name: None,
            skill_id: None,
        })
        .await
        .expect("save second");

        // WHEN the pending pause is read
        let row = repo
            .pending_pause("t-3")
            .await
            .expect("read")
            .expect("paused");

        // THEN it is the second pause, and the agent and skill recorded by the
        // first are kept rather than blanked
        assert_eq!(row.payload, Some(second));
        assert_eq!(row.agent_name, "agent");
        assert_eq!(row.skill_id.as_deref(), Some("skill"));
        // AND the first answer is gone, so a rebuild cannot hand it back as
        // the answer to the second
        let task = repo.rebuild_for_resume("t-3").await.expect("rebuild");
        assert!(task.input_response.is_none());
    }
}
