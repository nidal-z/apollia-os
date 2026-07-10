//! Tokio actor owning the per-session conversational plan store.
//!
//! Mirrors `todo_actor.rs`: the actor is the single owner of the three plan
//! tables (`session_plans`, `session_plan_steps`, `session_plan_mutations`). It
//! serializes every read and write over a bounded `mpsc` channel and validates
//! the resulting plan with [`apollia_core::plan::validate_plan`] before any
//! write. Callers interact through a clonable [`PlanHandle`]; there is no
//! `Arc<Mutex>` shared across actors.
//!
//! A step removal never silently loses data: the removed step is preserved as a
//! tombstone in the mutation history, keyed alongside the `RemoveStep` mutation,
//! so replay and audit can reconstruct the full lineage. The default plan read
//! ([`PlanHandle::get_plan`]) excludes tombstones to keep the active plan clean;
//! the history read ([`PlanHandle::plan_with_tombstones`]) re-includes them with
//! their removal provenance (origin, reason, timestamp).

use apollia_core::plan::{
    validate_plan, Plan, PlanMutation, PlanMutationKind, PlanScope, PlanStatus, PlanStep,
    PlanValidationError, StepOrigin, StepProvenance, StepStatus,
};
use apollia_core::RuntimeEvent;
use rusqlite::{params, Connection};
use tokio::sync::{mpsc, oneshot};
use tracing::{event, warn, Level};

use crate::eventbus::EventBusSender;

/// Bounded capacity of the plan actor's command channel.
///
/// Plan mutations are infrequent (one agent turn at a time per session), so a
/// small buffer absorbs bursts without unbounded growth, matching the todo
/// actor's sizing.
const PLAN_CHANNEL_CAPACITY: usize = 64;

/// SQL applied on actor startup to create the three plan tables if absent.
const PLAN_MIGRATION_SQL: &str = "\
CREATE TABLE IF NOT EXISTS session_plans (
    session_id  TEXT PRIMARY KEY,
    plan_id     TEXT NOT NULL,
    revision    INTEGER NOT NULL DEFAULT 0,
    status      TEXT NOT NULL,
    summary     TEXT,
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE TABLE IF NOT EXISTS session_plan_steps (
    session_id  TEXT NOT NULL,
    step_id     TEXT NOT NULL,
    ordinal     INTEGER NOT NULL,
    payload     TEXT NOT NULL,
    PRIMARY KEY (session_id, step_id)
);
CREATE TABLE IF NOT EXISTS session_plan_mutations (
    session_id  TEXT NOT NULL,
    seq         INTEGER PRIMARY KEY AUTOINCREMENT,
    payload     TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_session_plan_mut ON session_plan_mutations(session_id);";

/// Errors of the per-session plan subsystem.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PlanStoreError {
    /// The resulting plan is invalid (not a DAG).
    #[error("invalid plan")]
    Validation(#[from] PlanValidationError),
    /// A SQLite operation failed.
    #[error("sqlite error in plan store")]
    Sqlite(#[source] rusqlite::Error),
    /// A payload could not be (de)serialized to or from JSON.
    #[error("plan payload serialization failed")]
    Serde(#[source] serde_json::Error),
    /// The mutation targets a step that is not in the current plan.
    #[error("unknown step: {step_id}")]
    UnknownStep {
        /// The identifier that did not resolve to a step.
        step_id: String,
    },
    /// A reorder request did not list exactly the current step set.
    #[error("reorder set does not match the current steps")]
    ReorderMismatch,
    /// No plan exists yet for the session (mutation before a proposal).
    #[error("no plan for session: {session_id}")]
    NoPlan {
        /// Session that has no plan.
        session_id: String,
    },
    /// The actor's channel is closed (actor stopped).
    #[error("plan actor channel closed")]
    ActorGone,
}

impl From<rusqlite::Error> for PlanStoreError {
    fn from(source: rusqlite::Error) -> Self {
        PlanStoreError::Sqlite(source)
    }
}

impl From<serde_json::Error> for PlanStoreError {
    fn from(source: serde_json::Error) -> Self {
        PlanStoreError::Serde(source)
    }
}

/// Internal protocol between [`PlanHandle`] and the plan actor.
pub(crate) enum PlanMessage {
    /// Return the current plan for a session, or `None` when absent.
    GetPlan {
        /// Session to read.
        session_id: String,
        /// Reply channel for the read outcome.
        reply: oneshot::Sender<Result<Option<Plan>, PlanStoreError>>,
    },
    /// Replace the draft plan of a session with a fresh step list.
    Propose {
        /// Session whose draft is replaced.
        session_id: String,
        /// Full replacement step list.
        steps: Vec<PlanStep>,
        /// Optional human-readable plan summary.
        summary: Option<String>,
        /// Reply channel carrying the resulting plan.
        reply: oneshot::Sender<Result<Plan, PlanStoreError>>,
    },
    /// Apply one atomic mutation to the session's plan.
    Mutate {
        /// Target session.
        session_id: String,
        /// Mutation to apply (boxed: it inlines a full [`PlanStep`], which would
        /// otherwise make this the dominant variant of the message enum).
        op: Box<MutationOp>,
        /// Reply channel carrying the resulting plan.
        reply: oneshot::Sender<Result<Plan, PlanStoreError>>,
    },
    /// Submit the plan for approval (status becomes `AwaitingApproval`).
    Submit {
        /// Target session.
        session_id: String,
        /// Reply channel carrying the resulting plan.
        reply: oneshot::Sender<Result<Plan, PlanStoreError>>,
    },
    /// Return the recorded mutation history for a session, oldest first.
    ReadMutations {
        /// Target session.
        session_id: String,
        /// Reply channel carrying the mutation history.
        reply: oneshot::Sender<Result<Vec<PlanMutation>, PlanStoreError>>,
    },
    /// Return the plan including tombstoned (removed) steps, for history.
    PlanWithTombstones {
        /// Target session.
        session_id: String,
        /// Reply channel carrying the live steps plus tombstones, or `None`.
        reply: oneshot::Sender<Result<Option<Plan>, PlanStoreError>>,
    },
    /// Stop the actor loop.
    Shutdown,
}

/// Atomic mutation applied to the plan of a session.
pub(crate) enum MutationOp {
    /// Append a step.
    AddStep {
        /// Step to add.
        step: PlanStep,
        /// Documented reason for the addition.
        reason: Option<String>,
    },
    /// Replace an existing step in place.
    ModifyStep {
        /// Identifier of the step to replace.
        step_id: String,
        /// New step payload.
        after: PlanStep,
        /// Documented reason for the modification.
        reason: Option<String>,
    },
    /// Remove a step, keeping a tombstone in the mutation history.
    RemoveStep {
        /// Identifier of the step to remove.
        step_id: String,
        /// Documented reason for the removal.
        reason: Option<String>,
        /// Origin of the removal (agent edit by default, replan when driven by
        /// an automatic revision). Stamped on the tombstone provenance.
        origin: StepOrigin,
    },
    /// Reorder the steps to match an explicit identifier order.
    Reorder {
        /// The full step identifier set in the desired order.
        ordered_ids: Vec<String>,
        /// Documented reason for the reorder.
        reason: Option<String>,
    },
    /// Change the execution status of a step.
    SetStepStatus {
        /// Identifier of the step.
        step_id: String,
        /// New execution status.
        status: StepStatus,
        /// Documented reason for the status change.
        reason: Option<String>,
    },
}

/// Clonable handle to the plan actor.
///
/// All methods are async and communicate via a bounded `mpsc` channel. Cloning
/// is cheap (it clones the sender); there is no `Arc<Mutex>`.
#[derive(Clone)]
pub struct PlanHandle {
    pub(crate) tx: mpsc::Sender<PlanMessage>,
}

impl PlanHandle {
    /// Returns the current plan for `session_id`, or `None` when none exists.
    ///
    /// # Errors
    ///
    /// - [`PlanStoreError::Sqlite`] / [`PlanStoreError::Serde`] on a read failure.
    /// - [`PlanStoreError::ActorGone`] when the actor has stopped.
    pub async fn get_plan(&self, session_id: &str) -> Result<Option<Plan>, PlanStoreError> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(PlanMessage::GetPlan {
                session_id: session_id.to_string(),
                reply,
            })
            .await
            .map_err(|_| PlanStoreError::ActorGone)?;
        rx.await.map_err(|_| PlanStoreError::ActorGone)?
    }

    /// Replaces the draft plan of `session_id` with `steps`.
    ///
    /// Validates the DAG before writing; on rejection nothing is mutated. The
    /// resulting plan is a fresh [`PlanStatus::Draft`] at revision `0`.
    ///
    /// # Errors
    ///
    /// - [`PlanStoreError::Validation`] when the step set is not a valid DAG.
    /// - [`PlanStoreError::Sqlite`] / [`PlanStoreError::Serde`] on a write failure.
    /// - [`PlanStoreError::ActorGone`] when the actor has stopped.
    pub async fn propose(
        &self,
        session_id: &str,
        steps: Vec<PlanStep>,
        summary: Option<String>,
    ) -> Result<Plan, PlanStoreError> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(PlanMessage::Propose {
                session_id: session_id.to_string(),
                steps,
                summary,
                reply,
            })
            .await
            .map_err(|_| PlanStoreError::ActorGone)?;
        rx.await.map_err(|_| PlanStoreError::ActorGone)?
    }

    /// Adds a step to the session plan with a documented reason.
    ///
    /// # Errors
    ///
    /// See [`PlanHandle::propose`] for the shared error variants; additionally
    /// [`PlanStoreError::NoPlan`] when the session has no plan yet.
    pub async fn add_step(
        &self,
        session_id: &str,
        step: PlanStep,
        reason: Option<String>,
    ) -> Result<Plan, PlanStoreError> {
        self.mutate(session_id, MutationOp::AddStep { step, reason })
            .await
    }

    /// Replaces an existing step in place.
    ///
    /// # Errors
    ///
    /// [`PlanStoreError::UnknownStep`] when `step_id` is not in the plan, plus
    /// the shared variants from [`PlanHandle::propose`].
    pub async fn modify_step(
        &self,
        session_id: &str,
        step_id: &str,
        after: PlanStep,
        reason: Option<String>,
    ) -> Result<Plan, PlanStoreError> {
        self.mutate(
            session_id,
            MutationOp::ModifyStep {
                step_id: step_id.to_string(),
                after,
                reason,
            },
        )
        .await
    }

    /// Removes a step, keeping it as a tombstone with the removal provenance.
    ///
    /// The removal is attributed to [`StepOrigin::AgentEdit`]. Use
    /// [`PlanHandle::remove_step_with_origin`] for a replan-driven removal. The
    /// removed step stays retrievable through
    /// [`PlanHandle::plan_with_tombstones`]; [`PlanHandle::get_plan`] excludes it.
    ///
    /// # Errors
    ///
    /// [`PlanStoreError::UnknownStep`] when `step_id` is not in the plan, plus
    /// the shared variants from [`PlanHandle::propose`].
    pub async fn remove_step(
        &self,
        session_id: &str,
        step_id: &str,
        reason: Option<String>,
    ) -> Result<Plan, PlanStoreError> {
        self.remove_step_with_origin(session_id, step_id, reason, StepOrigin::AgentEdit)
            .await
    }

    /// Removes a step, stamping the tombstone with an explicit `origin`.
    ///
    /// A replan-driven removal passes [`StepOrigin::Replan`] so the tombstone
    /// records the revision that dropped the step. Same code path as
    /// [`PlanHandle::remove_step`]: no parallel removal route.
    ///
    /// # Errors
    ///
    /// [`PlanStoreError::UnknownStep`] when `step_id` is not in the plan, plus
    /// the shared variants from [`PlanHandle::propose`].
    pub async fn remove_step_with_origin(
        &self,
        session_id: &str,
        step_id: &str,
        reason: Option<String>,
        origin: StepOrigin,
    ) -> Result<Plan, PlanStoreError> {
        self.mutate(
            session_id,
            MutationOp::RemoveStep {
                step_id: step_id.to_string(),
                reason,
                origin,
            },
        )
        .await
    }

    /// Reorders the steps to match `ordered_ids`.
    ///
    /// # Errors
    ///
    /// [`PlanStoreError::ReorderMismatch`] when `ordered_ids` is not exactly the
    /// current step set, plus the shared variants from [`PlanHandle::propose`].
    pub async fn reorder(
        &self,
        session_id: &str,
        ordered_ids: Vec<String>,
        reason: Option<String>,
    ) -> Result<Plan, PlanStoreError> {
        self.mutate(
            session_id,
            MutationOp::Reorder {
                ordered_ids,
                reason,
            },
        )
        .await
    }

    /// Changes the execution status of a step.
    ///
    /// # Errors
    ///
    /// [`PlanStoreError::UnknownStep`] when `step_id` is not in the plan, plus
    /// the shared variants from [`PlanHandle::propose`].
    pub async fn set_step_status(
        &self,
        session_id: &str,
        step_id: &str,
        status: StepStatus,
        reason: Option<String>,
    ) -> Result<Plan, PlanStoreError> {
        self.mutate(
            session_id,
            MutationOp::SetStepStatus {
                step_id: step_id.to_string(),
                status,
                reason,
            },
        )
        .await
    }

    /// Submits the plan for approval (status becomes `AwaitingApproval`).
    ///
    /// # Errors
    ///
    /// - [`PlanStoreError::NoPlan`] when the session has no plan yet.
    /// - [`PlanStoreError::Sqlite`] / [`PlanStoreError::Serde`] on a write failure.
    /// - [`PlanStoreError::ActorGone`] when the actor has stopped.
    pub async fn submit(&self, session_id: &str) -> Result<Plan, PlanStoreError> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(PlanMessage::Submit {
                session_id: session_id.to_string(),
                reply,
            })
            .await
            .map_err(|_| PlanStoreError::ActorGone)?;
        rx.await.map_err(|_| PlanStoreError::ActorGone)?
    }

    /// Returns the recorded mutation history for `session_id`, oldest first.
    ///
    /// The history is the source of truth for replay and audit: it includes the
    /// `before` tombstone of every removed step.
    ///
    /// # Errors
    ///
    /// - [`PlanStoreError::Sqlite`] / [`PlanStoreError::Serde`] on a read failure.
    /// - [`PlanStoreError::ActorGone`] when the actor has stopped.
    pub async fn read_mutations(
        &self,
        session_id: &str,
    ) -> Result<Vec<PlanMutation>, PlanStoreError> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(PlanMessage::ReadMutations {
                session_id: session_id.to_string(),
                reply,
            })
            .await
            .map_err(|_| PlanStoreError::ActorGone)?;
        rx.await.map_err(|_| PlanStoreError::ActorGone)?
    }

    /// Returns the plan for `session_id` including tombstoned steps, or `None`.
    ///
    /// Unlike [`PlanHandle::get_plan`], the returned plan re-includes every
    /// removed step as a tombstone carrying its removal provenance (origin,
    /// reason, timestamp), so the construction history can replay a removal
    /// without recomposing the step from the raw mutation log. Tombstones are
    /// appended after the live steps, ordered by removal time (oldest first).
    ///
    /// # Errors
    ///
    /// - [`PlanStoreError::Sqlite`] / [`PlanStoreError::Serde`] on a read failure.
    /// - [`PlanStoreError::ActorGone`] when the actor has stopped.
    pub async fn plan_with_tombstones(
        &self,
        session_id: &str,
    ) -> Result<Option<Plan>, PlanStoreError> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(PlanMessage::PlanWithTombstones {
                session_id: session_id.to_string(),
                reply,
            })
            .await
            .map_err(|_| PlanStoreError::ActorGone)?;
        rx.await.map_err(|_| PlanStoreError::ActorGone)?
    }

    /// Signals the actor to stop. Best-effort; ignores a closed channel.
    pub async fn shutdown(&self) {
        let _ = self.tx.send(PlanMessage::Shutdown).await;
    }

    async fn mutate(&self, session_id: &str, op: MutationOp) -> Result<Plan, PlanStoreError> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(PlanMessage::Mutate {
                session_id: session_id.to_string(),
                op: Box::new(op),
                reply,
            })
            .await
            .map_err(|_| PlanStoreError::ActorGone)?;
        rx.await.map_err(|_| PlanStoreError::ActorGone)?
    }
}

/// Actor owning the SQLite connection backing the plan store.
///
/// Created via [`spawn_plan_actor`]; never constructed directly by callers.
pub struct PlanActor {
    conn: Connection,
    /// Optional event bus for emitting plan-mode events after a successful
    /// write. `None` in tests that do not assert on events.
    event_bus: Option<EventBusSender>,
}

impl PlanActor {
    /// Apply the plan migration on `conn`, validating it at startup (fail fast).
    fn migrate(conn: &Connection) -> Result<(), PlanStoreError> {
        conn.execute_batch(PLAN_MIGRATION_SQL)?;
        Ok(())
    }

    /// Emit a [`RuntimeEvent::PlanUpdated`] for a successful mutation.
    ///
    /// Fire-and-forget: a saturated or closed bus is silently ignored, exactly
    /// like the todo actor. Never called on a no-op or a rejected validation.
    fn emit_updated(&self, session_id: &str, plan: &Plan, mutation: &PlanMutation) {
        if let Some(bus) = &self.event_bus {
            let _ = bus.send(RuntimeEvent::PlanUpdated {
                session_id: session_id.to_string(),
                plan: Box::new(plan.clone()),
                mutation: Box::new(mutation.clone()),
            });
        }
    }

    /// Emit a [`RuntimeEvent::PlanSubmitted`] for a successful submission.
    fn emit_submitted(&self, session_id: &str, plan: &Plan) {
        if let Some(bus) = &self.event_bus {
            let _ = bus.send(RuntimeEvent::PlanSubmitted {
                session_id: session_id.to_string(),
                plan: Box::new(plan.clone()),
            });
        }
    }

    /// Replace the draft plan of a session with a fresh, validated step list.
    fn propose(
        &mut self,
        session_id: &str,
        steps: Vec<PlanStep>,
        summary: Option<String>,
    ) -> Result<Plan, PlanStoreError> {
        validate_plan(&steps)?;

        let plan_id = uuid::Uuid::new_v4().to_string();
        let mutation = PlanMutation {
            kind: PlanMutationKind::Propose,
            step_id: None,
            reason: summary.clone(),
            before: None,
            after: None,
            at: now_unix(),
        };

        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO session_plans (session_id, plan_id, revision, status, summary, updated_at)
             VALUES (?1, ?2, 0, ?3, ?4, datetime('now'))
             ON CONFLICT(session_id) DO UPDATE SET
                 plan_id = excluded.plan_id,
                 revision = 0,
                 status = excluded.status,
                 summary = excluded.summary,
                 updated_at = datetime('now')",
            params![
                session_id,
                plan_id,
                status_as_sql(PlanStatus::Draft),
                summary
            ],
        )?;
        write_steps(&tx, session_id, &steps)?;
        write_mutation(&tx, session_id, &mutation)?;
        tx.commit()?;

        event!(
            Level::INFO,
            session_id = %session_id,
            steps = steps.len(),
            "plan.action"
        );
        let plan = self
            .load_plan(session_id)?
            .ok_or_else(|| PlanStoreError::NoPlan {
                session_id: session_id.to_string(),
            })?;
        self.emit_updated(session_id, &plan, &mutation);
        Ok(plan)
    }

    /// Apply one mutation: compute the resulting plan in memory, validate it,
    /// then write steps and the mutation in a single transaction. A rejected
    /// validation leaves the tables untouched (the transaction is never opened).
    fn apply_mutation(&mut self, session_id: &str, op: MutationOp) -> Result<Plan, PlanStoreError> {
        if !self.plan_exists(session_id)? {
            return Err(PlanStoreError::NoPlan {
                session_id: session_id.to_string(),
            });
        }

        let mut steps = self.load_steps(session_id)?;
        let mutation = build_mutation(&mut steps, op, now_unix())?;
        validate_plan(&steps)?;

        let tx = self.conn.transaction()?;
        tx.execute(
            "UPDATE session_plans SET updated_at = datetime('now') WHERE session_id = ?1",
            params![session_id],
        )?;
        write_steps(&tx, session_id, &steps)?;
        write_mutation(&tx, session_id, &mutation)?;
        tx.commit()?;

        event!(
            Level::INFO,
            session_id = %session_id,
            kind = ?mutation.kind,
            "plan.action"
        );
        let plan = self
            .load_plan(session_id)?
            .ok_or_else(|| PlanStoreError::NoPlan {
                session_id: session_id.to_string(),
            })?;
        self.emit_updated(session_id, &plan, &mutation);
        Ok(plan)
    }

    /// Submit the plan for approval, recording a `Submit` mutation.
    fn submit(&mut self, session_id: &str) -> Result<Plan, PlanStoreError> {
        if !self.plan_exists(session_id)? {
            return Err(PlanStoreError::NoPlan {
                session_id: session_id.to_string(),
            });
        }

        let mutation = PlanMutation {
            kind: PlanMutationKind::Submit,
            step_id: None,
            reason: None,
            before: None,
            after: None,
            at: now_unix(),
        };

        let tx = self.conn.transaction()?;
        tx.execute(
            "UPDATE session_plans SET status = ?2, updated_at = datetime('now')
             WHERE session_id = ?1",
            params![session_id, status_as_sql(PlanStatus::AwaitingApproval)],
        )?;
        write_mutation(&tx, session_id, &mutation)?;
        tx.commit()?;

        event!(
            Level::INFO,
            session_id = %session_id,
            kind = "submit",
            "plan.action"
        );
        let plan = self
            .load_plan(session_id)?
            .ok_or_else(|| PlanStoreError::NoPlan {
                session_id: session_id.to_string(),
            })?;
        self.emit_updated(session_id, &plan, &mutation);
        self.emit_submitted(session_id, &plan);
        Ok(plan)
    }

    /// Returns `true` when a plan row exists for the session.
    fn plan_exists(&self, session_id: &str) -> Result<bool, PlanStoreError> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM session_plans WHERE session_id = ?1",
            params![session_id],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    /// Load the steps of a session ordered by `ordinal`.
    fn load_steps(&self, session_id: &str) -> Result<Vec<PlanStep>, PlanStoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT payload FROM session_plan_steps
             WHERE session_id = ?1 ORDER BY ordinal ASC",
        )?;
        let rows = stmt.query_map(params![session_id], |row| {
            let payload: String = row.get(0)?;
            Ok(payload)
        })?;

        let mut steps = Vec::new();
        for row in rows {
            let payload = row?;
            let step: PlanStep = serde_json::from_str(&payload)?;
            steps.push(step);
        }
        Ok(steps)
    }

    /// Load the recorded mutation history for a session, oldest first.
    fn load_mutations(&self, session_id: &str) -> Result<Vec<PlanMutation>, PlanStoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT payload FROM session_plan_mutations
             WHERE session_id = ?1 ORDER BY seq ASC",
        )?;
        let rows = stmt.query_map(params![session_id], |row| {
            let payload: String = row.get(0)?;
            Ok(payload)
        })?;

        let mut mutations = Vec::new();
        for row in rows {
            let payload = row?;
            let mutation: PlanMutation = serde_json::from_str(&payload)?;
            mutations.push(mutation);
        }
        Ok(mutations)
    }

    /// Load the full plan for a session, or `None` when the session has none.
    fn load_plan(&self, session_id: &str) -> Result<Option<Plan>, PlanStoreError> {
        let header = self
            .conn
            .query_row(
                "SELECT plan_id, revision, status FROM session_plans WHERE session_id = ?1",
                params![session_id],
                |row| {
                    let plan_id: String = row.get(0)?;
                    let revision: i64 = row.get(1)?;
                    let status: String = row.get(2)?;
                    Ok((plan_id, revision, status))
                },
            )
            .ok();

        let Some((plan_id, revision, status_raw)) = header else {
            return Ok(None);
        };

        let steps = self.load_steps(session_id)?;
        // The status column is written only from `status_as_sql`; an unexpected
        // value degrades to `Draft` rather than failing a read.
        let status = status_from_sql(&status_raw).unwrap_or(PlanStatus::Draft);
        Ok(Some(Plan {
            plan_id,
            scope: PlanScope::Session(session_id.to_string()),
            revision: revision.max(0) as u32,
            status,
            steps,
        }))
    }

    /// Load the plan including tombstoned steps, or `None` when none exists.
    ///
    /// The live plan is read from `session_plan_steps`; the tombstones are
    /// reconstructed from the `before` image of every `RemoveStep` mutation,
    /// which already carries the removal provenance. Tombstones are appended
    /// after the live steps in removal order (the mutation log is oldest first).
    fn load_plan_with_tombstones(&self, session_id: &str) -> Result<Option<Plan>, PlanStoreError> {
        let Some(mut plan) = self.load_plan(session_id)? else {
            return Ok(None);
        };
        for mutation in self.load_mutations(session_id)? {
            if mutation.kind == PlanMutationKind::RemoveStep {
                if let Some(tombstone) = mutation.before {
                    plan.steps.push(tombstone);
                }
            }
        }
        Ok(Some(plan))
    }

    /// Drive the actor until the channel closes or `Shutdown` is received.
    async fn run(mut self, mut rx: mpsc::Receiver<PlanMessage>) {
        while let Some(msg) = rx.recv().await {
            match msg {
                PlanMessage::GetPlan { session_id, reply } => {
                    let _ = reply.send(self.load_plan(&session_id));
                }
                PlanMessage::Propose {
                    session_id,
                    steps,
                    summary,
                    reply,
                } => {
                    let _ = reply.send(self.propose(&session_id, steps, summary));
                }
                PlanMessage::Mutate {
                    session_id,
                    op,
                    reply,
                } => {
                    let _ = reply.send(self.apply_mutation(&session_id, *op));
                }
                PlanMessage::Submit { session_id, reply } => {
                    let _ = reply.send(self.submit(&session_id));
                }
                PlanMessage::ReadMutations { session_id, reply } => {
                    let _ = reply.send(self.load_mutations(&session_id));
                }
                PlanMessage::PlanWithTombstones { session_id, reply } => {
                    let _ = reply.send(self.load_plan_with_tombstones(&session_id));
                }
                PlanMessage::Shutdown => break,
            }
        }
        warn!("plan.actor.stopped");
    }
}

/// Apply the migration on `conn`, spawn the [`PlanActor`], and return its handle.
///
/// The migration runs synchronously before the actor task is spawned, so a
/// schema failure surfaces at startup (principle: fail fast) instead of on the
/// first write.
///
/// `event_bus` receives a [`RuntimeEvent::PlanUpdated`] after every successful
/// mutation and a [`RuntimeEvent::PlanSubmitted`] on submit; pass `None` to
/// disable event emission.
///
/// # Errors
///
/// - [`PlanStoreError::Sqlite`] when the plan migration cannot be applied.
pub fn spawn_plan_actor(
    conn: Connection,
    event_bus: Option<EventBusSender>,
) -> Result<PlanHandle, PlanStoreError> {
    PlanActor::migrate(&conn)?;
    let (tx, rx) = mpsc::channel(PLAN_CHANNEL_CAPACITY);
    let actor = PlanActor { conn, event_bus };
    tokio::spawn(actor.run(rx));
    Ok(PlanHandle { tx })
}

/// Apply a [`MutationOp`] to `steps` in place and return the recorded mutation.
///
/// The returned [`PlanMutation`] carries the `before` and `after` images and
/// the reason so the history is a faithful, replayable record. A removal keeps
/// the `before` image (tombstone) and a `None` `after`.
fn build_mutation(
    steps: &mut Vec<PlanStep>,
    op: MutationOp,
    at: i64,
) -> Result<PlanMutation, PlanStoreError> {
    match op {
        MutationOp::AddStep { step, reason } => {
            let after = step.clone();
            steps.push(step);
            Ok(PlanMutation {
                kind: PlanMutationKind::AddStep,
                step_id: Some(after.step_id.clone()),
                reason,
                before: None,
                after: Some(after),
                at,
            })
        }
        MutationOp::ModifyStep {
            step_id,
            after,
            reason,
        } => {
            let pos = step_position(steps, &step_id)?;
            let before = steps[pos].clone();
            steps[pos] = after.clone();
            Ok(PlanMutation {
                kind: PlanMutationKind::ModifyStep,
                step_id: Some(step_id),
                reason,
                before: Some(before),
                after: Some(after),
                at,
            })
        }
        MutationOp::RemoveStep {
            step_id,
            reason,
            origin,
        } => {
            let pos = step_position(steps, &step_id)?;
            let mut tombstone = steps.remove(pos);
            // Stamp the removal provenance on the tombstone so the history read
            // surfaces the origin, reason and timestamp of the removal itself,
            // not the provenance the step carried while it was live.
            tombstone.provenance = StepProvenance {
                origin,
                reason: reason.clone(),
                at,
            };
            Ok(PlanMutation {
                kind: PlanMutationKind::RemoveStep,
                step_id: Some(step_id),
                reason,
                before: Some(tombstone),
                after: None,
                at,
            })
        }
        MutationOp::Reorder {
            ordered_ids,
            reason,
        } => {
            reorder_steps(steps, &ordered_ids)?;
            Ok(PlanMutation {
                kind: PlanMutationKind::Reorder,
                step_id: None,
                reason,
                before: None,
                after: None,
                at,
            })
        }
        MutationOp::SetStepStatus {
            step_id,
            status,
            reason,
        } => {
            let pos = step_position(steps, &step_id)?;
            let before = steps[pos].clone();
            steps[pos].status = status;
            let after = steps[pos].clone();
            Ok(PlanMutation {
                kind: PlanMutationKind::StatusChange,
                step_id: Some(step_id),
                reason,
                before: Some(before),
                after: Some(after),
                at,
            })
        }
    }
}

/// Locate a step by identifier, erroring when it is absent.
fn step_position(steps: &[PlanStep], step_id: &str) -> Result<usize, PlanStoreError> {
    steps
        .iter()
        .position(|s| s.step_id == step_id)
        .ok_or_else(|| PlanStoreError::UnknownStep {
            step_id: step_id.to_string(),
        })
}

/// Reorder `steps` in place to match `ordered_ids` exactly.
fn reorder_steps(steps: &mut Vec<PlanStep>, ordered_ids: &[String]) -> Result<(), PlanStoreError> {
    if ordered_ids.len() != steps.len() {
        return Err(PlanStoreError::ReorderMismatch);
    }
    let mut reordered = Vec::with_capacity(steps.len());
    for id in ordered_ids {
        let pos = steps
            .iter()
            .position(|s| &s.step_id == id)
            .ok_or(PlanStoreError::ReorderMismatch)?;
        reordered.push(steps.remove(pos));
    }
    *steps = reordered;
    Ok(())
}

/// Rewrite the full step set for a session inside the given transaction.
fn write_steps(
    tx: &rusqlite::Transaction<'_>,
    session_id: &str,
    steps: &[PlanStep],
) -> Result<(), PlanStoreError> {
    tx.execute(
        "DELETE FROM session_plan_steps WHERE session_id = ?1",
        params![session_id],
    )?;
    for (ordinal, step) in steps.iter().enumerate() {
        let payload = serde_json::to_string(step)?;
        tx.execute(
            "INSERT INTO session_plan_steps (session_id, step_id, ordinal, payload)
             VALUES (?1, ?2, ?3, ?4)",
            params![session_id, step.step_id, ordinal as i64, payload],
        )?;
    }
    Ok(())
}

/// Append one mutation row to the session's history.
fn write_mutation(
    tx: &rusqlite::Transaction<'_>,
    session_id: &str,
    mutation: &PlanMutation,
) -> Result<(), PlanStoreError> {
    let payload = serde_json::to_string(mutation)?;
    tx.execute(
        "INSERT INTO session_plan_mutations (session_id, payload) VALUES (?1, ?2)",
        params![session_id, payload],
    )?;
    Ok(())
}

/// Current Unix timestamp in seconds.
fn now_unix() -> i64 {
    chrono::Utc::now().timestamp()
}

/// SQL-storable string for a [`PlanStatus`].
fn status_as_sql(status: PlanStatus) -> &'static str {
    match status {
        PlanStatus::Draft => "draft",
        PlanStatus::AwaitingApproval => "awaiting_approval",
        PlanStatus::Executing => "executing",
        PlanStatus::Completed => "completed",
        PlanStatus::Abandoned => "abandoned",
    }
}

/// Parse a [`PlanStatus`] from its SQL string, `None` on an unknown value.
fn status_from_sql(raw: &str) -> Option<PlanStatus> {
    match raw {
        "draft" => Some(PlanStatus::Draft),
        "awaiting_approval" => Some(PlanStatus::AwaitingApproval),
        "executing" => Some(PlanStatus::Executing),
        "completed" => Some(PlanStatus::Completed),
        "abandoned" => Some(PlanStatus::Abandoned),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_core::plan::StepProvenance;

    fn handle() -> PlanHandle {
        spawn_plan_actor(
            Connection::open_in_memory().expect("open in-memory db"),
            None,
        )
        .expect("spawn")
    }

    /// Spawn a plan actor wired to a fresh broadcast bus, returning the handle
    /// and a receiver so a test can assert on emitted events.
    fn handle_with_bus() -> (PlanHandle, tokio::sync::broadcast::Receiver<RuntimeEvent>) {
        let (bus, rx) = tokio::sync::broadcast::channel(16);
        let h = spawn_plan_actor(
            Connection::open_in_memory().expect("open in-memory db"),
            Some(bus),
        )
        .expect("spawn");
        (h, rx)
    }

    fn step(id: &str, deps: &[&str]) -> PlanStep {
        PlanStep {
            step_id: id.into(),
            title: id.into(),
            description: format!("desc {id}"),
            status: StepStatus::Pending,
            depends_on: deps.iter().map(|s| (*s).to_string()).collect(),
            tool_hint: None,
            model_hint: None,
            rationale: None,
            provenance: StepProvenance::default(),
            args: None,
        }
    }

    async fn load_mutations(h: &PlanHandle, session_id: &str) -> Vec<PlanMutation> {
        h.read_mutations(session_id).await.expect("mutations")
    }

    #[tokio::test]
    async fn test_propose_then_get_plan() {
        // GIVEN a handle backed by an in-memory db
        let h = handle();

        // WHEN proposing a valid two-step plan
        h.propose(
            "s1",
            vec![step("a", &[]), step("b", &["a"])],
            Some("sum".into()),
        )
        .await
        .expect("propose ok");

        // THEN get_plan returns it as a Draft at revision 0
        let plan = h.get_plan("s1").await.expect("get").expect("some plan");
        assert_eq!(plan.steps.len(), 2);
        assert_eq!(plan.status, PlanStatus::Draft);
        assert_eq!(plan.revision, 0);
    }

    #[tokio::test]
    async fn test_modify_step_records_mutation_with_provenance() {
        // GIVEN a proposed plan
        let h = handle();
        h.propose("s1", vec![step("a", &[])], None)
            .await
            .expect("propose");

        // WHEN modifying a step with a reason
        let mut after = step("a", &[]);
        after.title = "titre revise".into();
        h.modify_step("s1", "a", after, Some("clarification".into()))
            .await
            .expect("modify");

        // THEN the mutation log carries the ModifyStep entry with before/after/reason
        let muts = load_mutations(&h, "s1").await;
        assert!(muts
            .iter()
            .any(|m| matches!(m.kind, PlanMutationKind::ModifyStep)
                && m.reason.as_deref() == Some("clarification")
                && m.before.is_some()
                && m.after.is_some()));
    }

    #[tokio::test]
    async fn test_remove_step_keeps_tombstone() {
        // GIVEN a two-step plan
        let h = handle();
        h.propose("s1", vec![step("a", &[]), step("b", &[])], None)
            .await
            .expect("propose");

        // WHEN removing a step with a reason
        h.remove_step("s1", "b", Some("inutile".into()))
            .await
            .expect("remove");

        // THEN the step is gone but a RemoveStep mutation keeps its before image
        let plan = h.get_plan("s1").await.expect("get").expect("plan");
        assert!(!plan.steps.iter().any(|s| s.step_id == "b"));
        let muts = load_mutations(&h, "s1").await;
        assert!(muts
            .iter()
            .any(|m| matches!(m.kind, PlanMutationKind::RemoveStep)
                && m.before.as_ref().is_some_and(|b| b.step_id == "b")
                && m.reason.as_deref() == Some("inutile")));
    }

    #[tokio::test]
    async fn test_remove_step_creates_tombstone_with_reason_and_timestamp() {
        // GIVEN a two-step plan with an active step b
        let h = handle();
        h.propose("s1", vec![step("a", &[]), step("b", &[])], None)
            .await
            .expect("propose");

        // WHEN b is removed with a reason
        h.remove_step("s1", "b", Some("redondante avec a".into()))
            .await
            .expect("remove");

        // THEN b survives as a tombstone carrying the reason and a non-zero timestamp
        let plan = h
            .plan_with_tombstones("s1")
            .await
            .expect("read")
            .expect("plan");
        let tombstone = plan
            .steps
            .iter()
            .find(|s| s.step_id == "b")
            .expect("b kept as tombstone");
        assert_eq!(
            tombstone.provenance.reason.as_deref(),
            Some("redondante avec a")
        );
        assert!(tombstone.provenance.at > 0);
    }

    #[tokio::test]
    async fn test_default_get_plan_excludes_tombstones() {
        // GIVEN a plan where b was removed
        let h = handle();
        h.propose("s1", vec![step("a", &[]), step("b", &[])], None)
            .await
            .expect("propose");
        h.remove_step("s1", "b", None).await.expect("remove");

        // WHEN reading the active plan
        let plan = h.get_plan("s1").await.expect("get").expect("plan");

        // THEN b is absent and a remains
        assert!(plan.steps.iter().all(|s| s.step_id != "b"));
        assert!(plan.steps.iter().any(|s| s.step_id == "a"));
    }

    #[tokio::test]
    async fn test_history_includes_tombstones_with_replan_origin() {
        // GIVEN a plan where b was removed by a replan revision
        let h = handle();
        h.propose("s1", vec![step("a", &[]), step("b", &[])], None)
            .await
            .expect("propose");
        h.remove_step_with_origin("s1", "b", Some("doublon".into()), StepOrigin::Replan(2))
            .await
            .expect("remove");

        // WHEN reading the history view
        let plan = h
            .plan_with_tombstones("s1")
            .await
            .expect("read")
            .expect("plan");

        // THEN b is present with the replan origin and its removal reason
        let tombstone = plan
            .steps
            .iter()
            .find(|s| s.step_id == "b")
            .expect("b kept as tombstone");
        assert!(matches!(tombstone.provenance.origin, StepOrigin::Replan(2)));
        assert_eq!(tombstone.provenance.reason.as_deref(), Some("doublon"));
    }

    #[tokio::test]
    async fn test_remove_unknown_step_is_typed_error_without_tombstone() {
        // GIVEN a plan without step s9
        let h = handle();
        h.propose("s1", vec![step("a", &[])], None)
            .await
            .expect("propose");
        let before = load_mutations(&h, "s1").await.len();

        // WHEN removing the unknown step s9
        let err = h
            .remove_step("s1", "s9", Some("oups".into()))
            .await
            .expect_err("should be an error");

        // THEN it is a typed UnknownStep error, no tombstone and no mutation persisted
        assert!(matches!(err, PlanStoreError::UnknownStep { ref step_id } if step_id == "s9"));
        let plan = h
            .plan_with_tombstones("s1")
            .await
            .expect("read")
            .expect("plan");
        assert!(plan.steps.iter().all(|s| s.step_id != "s9"));
        assert_eq!(load_mutations(&h, "s1").await.len(), before);
    }

    #[tokio::test]
    async fn test_unknown_dependency_rejected_without_mutating() {
        // GIVEN a valid two-step plan
        let h = handle();
        h.propose("s1", vec![step("a", &[]), step("b", &["a"])], None)
            .await
            .expect("propose");

        // WHEN modifying a to depend on an unknown step
        let result = h
            .modify_step("s1", "a", step("a", &["ghost"]), Some("oops".into()))
            .await;

        // THEN it fails validation and a keeps its original (empty) deps
        assert!(matches!(
            result,
            Err(PlanStoreError::Validation(
                PlanValidationError::UnknownDependency { .. }
            ))
        ));
        let plan = h.get_plan("s1").await.expect("get").expect("plan");
        let a = plan
            .steps
            .iter()
            .find(|s| s.step_id == "a")
            .expect("step a");
        assert!(a.depends_on.is_empty());
    }

    #[tokio::test]
    async fn test_add_step_cycle_rejected_without_mutating() {
        // GIVEN a plan a -> b (b depends on a) plus c depending on both
        let h = handle();
        h.propose("s1", vec![step("a", &[]), step("b", &["a"])], None)
            .await
            .expect("propose");
        h.add_step("s1", step("c", &["a", "b"]), Some("add c".into()))
            .await
            .expect("add c");

        // WHEN modifying a to depend on c, closing a cycle a -> c -> a
        let result = h
            .modify_step("s1", "a", step("a", &["c"]), Some("oops".into()))
            .await;

        // THEN it fails with a Cycle validation error and a keeps no deps
        assert!(matches!(
            result,
            Err(PlanStoreError::Validation(PlanValidationError::Cycle))
        ));
        let plan = h.get_plan("s1").await.expect("get").expect("plan");
        let a = plan
            .steps
            .iter()
            .find(|s| s.step_id == "a")
            .expect("step a");
        assert!(a.depends_on.is_empty());
    }

    #[tokio::test]
    async fn test_mutation_not_recorded_on_validation_reject() {
        // GIVEN a valid plan a -> b with one Propose mutation
        let h = handle();
        h.propose("s1", vec![step("a", &[]), step("b", &["a"])], None)
            .await
            .expect("propose");
        let before = load_mutations(&h, "s1").await.len();

        // WHEN a mutation is rejected by validation
        let _ = h
            .modify_step("s1", "a", step("a", &["ghost"]), Some("oops".into()))
            .await;

        // THEN no mutation is appended for the rejected write
        let after = load_mutations(&h, "s1").await.len();
        assert_eq!(before, after);
    }

    #[tokio::test]
    async fn test_two_sessions_isolated() {
        // GIVEN a plan only in session s1
        let h = handle();
        h.propose("s1", vec![step("a", &[])], None)
            .await
            .expect("propose");

        // WHEN reading session s2
        let plan = h.get_plan("s2").await.expect("get");

        // THEN s2 has no plan (no cross-session leak)
        assert!(plan.is_none());
    }

    #[tokio::test]
    async fn test_set_step_status_updates_and_records() {
        // GIVEN a proposed plan
        let h = handle();
        h.propose("s1", vec![step("a", &[])], None)
            .await
            .expect("propose");

        // WHEN marking the step in progress with a reason
        h.set_step_status("s1", "a", StepStatus::InProgress, Some("start".into()))
            .await
            .expect("status");

        // THEN the step status changes and a StatusChange mutation is recorded
        let plan = h.get_plan("s1").await.expect("get").expect("plan");
        assert_eq!(plan.steps[0].status, StepStatus::InProgress);
        let muts = load_mutations(&h, "s1").await;
        assert!(muts
            .iter()
            .any(|m| matches!(m.kind, PlanMutationKind::StatusChange)));
    }

    #[tokio::test]
    async fn test_submit_moves_to_awaiting_approval() {
        // GIVEN a proposed plan
        let h = handle();
        h.propose("s1", vec![step("a", &[])], None)
            .await
            .expect("propose");

        // WHEN submitting it
        let plan = h.submit("s1").await.expect("submit");

        // THEN its status is AwaitingApproval and a Submit mutation is recorded
        assert_eq!(plan.status, PlanStatus::AwaitingApproval);
        let muts = load_mutations(&h, "s1").await;
        assert!(muts
            .iter()
            .any(|m| matches!(m.kind, PlanMutationKind::Submit)));
    }

    #[tokio::test]
    async fn test_propose_emits_plan_updated_with_plan_and_mutation() {
        // GIVEN a plan actor wired to a broadcast bus
        let (h, mut rx) = handle_with_bus();

        // WHEN a valid plan is proposed
        h.propose("s1", vec![step("a", &[])], Some("draft".into()))
            .await
            .expect("propose");

        // THEN a PlanUpdated event carries the session, the plan and the mutation
        let event = rx.recv().await.expect("event emitted");
        match event {
            RuntimeEvent::PlanUpdated {
                session_id,
                plan,
                mutation,
            } => {
                assert_eq!(session_id, "s1");
                assert_eq!(plan.steps.len(), 1);
                assert_eq!(mutation.kind, PlanMutationKind::Propose);
            }
            other => panic!("expected PlanUpdated, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_mutation_emits_plan_updated_carrying_the_mutation() {
        // GIVEN a proposed plan whose initial PlanUpdated has been drained
        let (h, mut rx) = handle_with_bus();
        h.propose("s1", vec![step("a", &[])], None)
            .await
            .expect("propose");
        let _ = rx.recv().await.expect("drain propose");

        // WHEN a step is added
        h.add_step("s1", step("b", &[]), Some("more work".into()))
            .await
            .expect("add");

        // THEN a PlanUpdated carries the AddStep mutation and the two-step plan
        let event = rx.recv().await.expect("event emitted");
        match event {
            RuntimeEvent::PlanUpdated { plan, mutation, .. } => {
                assert_eq!(plan.steps.len(), 2);
                assert_eq!(mutation.kind, PlanMutationKind::AddStep);
            }
            other => panic!("expected PlanUpdated, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_submit_emits_plan_updated_then_plan_submitted() {
        // GIVEN a proposed plan with its propose PlanUpdated drained
        let (h, mut rx) = handle_with_bus();
        h.propose("s1", vec![step("a", &[])], None)
            .await
            .expect("propose");
        let _ = rx.recv().await.expect("drain propose");

        // WHEN the plan is submitted
        h.submit("s1").await.expect("submit");

        // THEN PlanUpdated (Submit mutation) is emitted, then PlanSubmitted
        let first = rx.recv().await.expect("event 1");
        assert!(matches!(
            first,
            RuntimeEvent::PlanUpdated { mutation, .. } if mutation.kind == PlanMutationKind::Submit
        ));
        let second = rx.recv().await.expect("event 2");
        assert!(matches!(
            second,
            RuntimeEvent::PlanSubmitted { session_id, plan }
                if session_id == "s1" && plan.status == PlanStatus::AwaitingApproval
        ));
    }

    #[tokio::test]
    async fn test_no_event_on_validation_reject() {
        // GIVEN a proposed plan with its PlanUpdated drained
        let (h, mut rx) = handle_with_bus();
        h.propose("s1", vec![step("a", &[])], None)
            .await
            .expect("propose");
        let _ = rx.recv().await.expect("drain propose");

        // WHEN a mutation that fails validation is attempted (unknown dependency)
        let result = h
            .modify_step("s1", "a", step("a", &["ghost"]), Some("oops".into()))
            .await;
        assert!(matches!(
            result,
            Err(PlanStoreError::Validation(
                PlanValidationError::UnknownDependency { .. }
            ))
        ));

        // THEN no event is emitted for the rejected write
        assert!(matches!(
            rx.try_recv(),
            Err(tokio::sync::broadcast::error::TryRecvError::Empty)
        ));
    }

    #[tokio::test]
    async fn test_mutate_before_propose_is_no_plan() {
        // GIVEN a session with no plan
        let h = handle();

        // WHEN adding a step before any proposal
        let result = h.add_step("s1", step("a", &[]), Some("x".into())).await;

        // THEN it reports NoPlan (graceful, no panic)
        assert!(matches!(result, Err(PlanStoreError::NoPlan { .. })));
    }

    #[tokio::test]
    async fn test_propose_replaces_previous_draft() {
        // GIVEN a first proposed plan
        let h = handle();
        h.propose("s1", vec![step("a", &[])], None)
            .await
            .expect("propose 1");

        // WHEN proposing a fresh list
        h.propose("s1", vec![step("z", &[])], None)
            .await
            .expect("propose 2");

        // THEN only the new step remains
        let plan = h.get_plan("s1").await.expect("get").expect("plan");
        let ids: Vec<&str> = plan.steps.iter().map(|s| s.step_id.as_str()).collect();
        assert_eq!(ids, vec!["z"]);
    }

    #[tokio::test]
    async fn test_read_mutations_returns_insertion_order() {
        // GIVEN a session whose plan recorded a Propose then two AddStep mutations
        let h = handle();
        h.propose("s1", vec![step("a", &[])], None)
            .await
            .expect("propose");
        h.add_step("s1", step("b", &[]), Some("add b".into()))
            .await
            .expect("add b");
        h.add_step("s1", step("c", &[]), Some("add c".into()))
            .await
            .expect("add c");

        // WHEN reading the mutation history
        let muts = h.read_mutations("s1").await.expect("read mutations");

        // THEN they come back oldest first: Propose, then AddStep b, then AddStep c
        assert_eq!(muts.len(), 3);
        assert_eq!(muts[0].kind, PlanMutationKind::Propose);
        assert_eq!(muts[1].kind, PlanMutationKind::AddStep);
        assert_eq!(muts[1].step_id.as_deref(), Some("b"));
        assert_eq!(muts[2].kind, PlanMutationKind::AddStep);
        assert_eq!(muts[2].step_id.as_deref(), Some("c"));
    }

    #[tokio::test]
    async fn test_read_mutations_unknown_session_is_empty() {
        // GIVEN a handle with no plan for session s9
        let h = handle();

        // WHEN reading the mutation history of the unknown session
        let muts = h.read_mutations("s9").await.expect("read mutations");

        // THEN the history is empty (graceful, no error, no panic)
        assert!(muts.is_empty());
    }

    #[tokio::test]
    async fn test_get_plan_after_shutdown_is_actor_gone() {
        // GIVEN a handle whose actor has been told to stop
        let h = handle();
        h.shutdown().await;
        tokio::task::yield_now().await;

        // WHEN reading after the channel is closed
        let result = h.get_plan("s1").await;

        // THEN the call reports the actor is gone (graceful, no panic)
        assert!(matches!(result, Err(PlanStoreError::ActorGone)));
    }
}
