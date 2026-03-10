//! Core types for pipeline orchestration.
//!
//! Defines the declarative pipeline model (`PipelineDefinition`, `PipelineStepDef`)
//! and the runtime state (`PipelineRun`, `StepRun`) used by the executor, repository,
//! CLI and config parser.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ── Newtype wrappers ──────────────────────────────────────────────────────────

/// Unique identifier of a pipeline, matching the `id` field in `[[pipelines]]`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PipelineId(pub String);

/// Unique identifier of a step within a pipeline definition.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StepId(pub String);

/// Unique identifier of a pipeline run generated at execution time (e.g. `"r-0017"`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RunId(pub String);

impl std::fmt::Display for PipelineId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::fmt::Display for StepId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::fmt::Display for RunId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ── Pipeline definition ───────────────────────────────────────────────────────

/// Declarative definition of a multi-agent pipeline read from `apollia.toml`.
///
/// A `PipelineDefinition` describes the static topology of a pipeline: which agents
/// run, in what order, and how failures are handled. It is immutable after parsing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineDefinition {
    /// Unique pipeline identifier used in triggers, CLI, and API routes.
    pub id: PipelineId,
    /// Human-readable description shown in `pipeline list`.
    pub description: String,
    /// Global failure policy applied when a step fails without a local `on_failure` override.
    #[serde(default)]
    pub on_failure: GlobalFailurePolicy,
    /// Ordered list of step definitions; topological ordering is computed at runtime.
    pub steps: Vec<PipelineStepDef>,
}

/// Global failure policy applied when no step-level `on_failure` takes effect.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum GlobalFailurePolicy {
    /// The pipeline stops immediately on the first unhandled step failure. (default)
    #[default]
    Fail,
    /// The pipeline continues to execute remaining steps even after a failure.
    Continue,
}

/// Definition of a single step within a pipeline.
///
/// A step maps one agent to one input template. Dependencies between steps
/// express the execution DAG; the executor computes topological layers from them.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineStepDef {
    /// Step identifier, unique within the pipeline.
    pub id: StepId,
    /// Name of the target agent that must be running in the runtime.
    pub agent: String,
    /// Input template supporting `{{steps.x.output}}`, `{{trigger.payload}}` etc.
    pub input: String,
    /// Steps that must complete before this step can be submitted.
    #[serde(default)]
    pub depends_on: Vec<StepId>,
    /// How this step behaves when it fails.
    #[serde(default)]
    pub on_failure: StepFailurePolicy,
    /// Optional condition; if present, the step is skipped when the condition evaluates to false.
    #[serde(default)]
    pub condition: Option<StepCondition>,
    /// If set, this step is the fallback for the referenced step and is inactive by default.
    #[serde(default)]
    pub fallback_for: Option<StepId>,
}

/// Per-step failure policy controlling what happens when the step's task fails.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum StepFailurePolicy {
    /// Propagate the failure and stop the pipeline. (default)
    #[default]
    Fail,
    /// Mark the step as skipped and continue the pipeline; template variable resolves to empty.
    Skip,
    /// Activate the designated fallback step and re-evaluate the execution graph.
    Fallback,
}

/// Condition that must be satisfied for a step to be executed.
///
/// If the condition evaluates to `false`, the step status is set to `Skipped`
/// and downstream steps are not blocked by it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepCondition {
    /// Comparison operator to apply.
    pub when: ConditionKind,
    /// Dot-path to the value to inspect, e.g. `"steps.validation.output"`.
    pub field: String,
    /// Reference value used by the operator.
    pub value: String,
}

/// Supported comparison operators for step conditions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConditionKind {
    /// Field value contains the reference value as a substring.
    Contains,
    /// Field value is exactly equal to the reference value.
    Equals,
    /// Field value starts with the reference value.
    StartsWith,
    /// Field value ends with the reference value.
    EndsWith,
    /// Field value matches the reference value interpreted as a regular expression.
    Regex,
}

// ── Run state ─────────────────────────────────────────────────────────────────

/// Runtime state of a pipeline run — in progress or terminated.
///
/// A `PipelineRun` is created when a `PipelineRunRequest` is received and
/// persisted in SQLite via `PipelineRepository`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineRun {
    /// Unique identifier of this run.
    pub run_id: RunId,
    /// Pipeline this run belongs to.
    pub pipeline_id: PipelineId,
    /// Trigger that initiated this run; `None` if started manually via CLI or API.
    pub trigger_id: Option<String>,
    /// Current aggregate status of the run.
    pub status: PipelineStatus,
    /// Per-step execution state, keyed by `StepId`.
    pub step_runs: HashMap<StepId, StepRun>,
    /// Payload forwarded from the trigger (e.g. filename, webhook body, ISO timestamp).
    pub trigger_payload: Option<String>,
    /// Wall-clock time when the run was created.
    pub started_at: DateTime<Utc>,
    /// Wall-clock time when the run reached a terminal state; `None` while still running.
    pub ended_at: Option<DateTime<Utc>>,
}

/// Aggregate status of a pipeline run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum PipelineStatus {
    /// At least one step is still executing or pending.
    Running,
    /// Pipeline is paused waiting for a HITL approval on the given step.
    WaitingApproval {
        /// The step whose task is awaiting approval.
        step_id: StepId,
        /// The task identifier sent to the approver.
        task_id: String,
    },
    /// All steps have completed or been skipped successfully.
    Completed,
    /// A step failed with `on_failure = fail` and the pipeline was aborted.
    Failed {
        /// The step that caused the failure.
        step_id: StepId,
        /// Human-readable failure reason.
        reason: String,
    },
}

/// Execution record for a single step within a run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepRun {
    /// Step this record belongs to.
    pub step_id: StepId,
    /// Task ID submitted to the `TaskRouter`; `None` if pending, skipped, or fallback inactive.
    pub task_id: Option<String>,
    /// Current status of this step within the run.
    pub status: StepRunStatus,
    /// Final output produced by the agent; `None` until the step completes.
    pub output: Option<String>,
    /// Error message if the step failed; `None` otherwise.
    pub error: Option<String>,
    /// Wall-clock time when the task was submitted to the router.
    pub started_at: Option<DateTime<Utc>>,
    /// Wall-clock time when the step reached a terminal state.
    pub ended_at: Option<DateTime<Utc>>,
}

/// Status of an individual step within a pipeline run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum StepRunStatus {
    /// Step has not been submitted yet — waiting for its dependencies. (default)
    #[default]
    Pending,
    /// Task has been submitted to the `TaskRouter` and is executing.
    Running,
    /// Task is paused waiting for HITL approval.
    WaitingApproval,
    /// Task completed successfully; `output` is populated.
    Completed,
    /// Task failed; `error` is populated.
    Failed,
    /// Step was skipped due to `on_failure = skip` or `condition = false`.
    Skipped,
    /// Step was superseded by its designated fallback step.
    FallbackActive,
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // AC-1 — PipelineDefinition serialization round-trip
    #[test]
    fn test_ac1_pipeline_definition_roundtrip() {
        // GIVEN
        let def = PipelineDefinition {
            id: PipelineId("traitement-facture".into()),
            description: "Pipeline test".into(),
            on_failure: GlobalFailurePolicy::Fail,
            steps: vec![
                PipelineStepDef {
                    id: StepId("ocr".into()),
                    agent: "ocr-agent".into(),
                    input: "{{trigger.payload}}".into(),
                    depends_on: vec![],
                    on_failure: StepFailurePolicy::Fail,
                    condition: None,
                    fallback_for: None,
                },
                PipelineStepDef {
                    id: StepId("validation".into()),
                    agent: "validation-agent".into(),
                    input: "{{steps.ocr.output}}".into(),
                    depends_on: vec![StepId("ocr".into())],
                    on_failure: StepFailurePolicy::Fallback,
                    condition: Some(StepCondition {
                        when: ConditionKind::Contains,
                        field: "steps.ocr.output".into(),
                        value: "FRAUDE".into(),
                    }),
                    fallback_for: None,
                },
            ],
        };

        // WHEN
        let json = serde_json::to_string(&def).expect("serialization must succeed");
        let restored: PipelineDefinition =
            serde_json::from_str(&json).expect("deserialization must succeed");

        // THEN
        assert_eq!(restored.id, def.id);
        assert_eq!(restored.steps.len(), 2);
        assert_eq!(restored.steps[1].on_failure, StepFailurePolicy::Fallback);
        assert_eq!(restored.description, def.description);
    }

    // AC-2 — PipelineRun with WaitingApproval status round-trip
    #[test]
    fn test_ac2_pipeline_run_waiting_approval_roundtrip() {
        // GIVEN
        let run = PipelineRun {
            run_id: RunId("r-0017".into()),
            pipeline_id: PipelineId("traitement-facture".into()),
            trigger_id: None,
            status: PipelineStatus::WaitingApproval {
                step_id: StepId("comptabilite".into()),
                task_id: "t-0051".into(),
            },
            step_runs: HashMap::default(),
            trigger_payload: Some("acme-2026-03.pdf".into()),
            started_at: Utc::now(),
            ended_at: None,
        };

        // WHEN
        let json = serde_json::to_string(&run).expect("serialization must succeed");
        let restored: PipelineRun =
            serde_json::from_str(&json).expect("deserialization must succeed");

        // THEN
        assert_eq!(restored.run_id, run.run_id);
        assert!(matches!(
            restored.status,
            PipelineStatus::WaitingApproval { .. }
        ));
        assert_eq!(restored.trigger_payload, run.trigger_payload);
    }

    // AC-3 — Newtype distinction enforced at compile time (proven by the type system;
    // this test documents the intent and exercises the Display impls).
    #[test]
    fn test_ac3_newtype_display() {
        let pid = PipelineId("traitement-facture".into());
        let sid = StepId("ocr".into());
        let rid = RunId("r-0017".into());

        assert_eq!(pid.to_string(), "traitement-facture");
        assert_eq!(sid.to_string(), "ocr");
        assert_eq!(rid.to_string(), "r-0017");
    }

    // AC-4 — StepFailurePolicy defaults to Fail when absent from JSON
    #[test]
    fn test_ac4_step_failure_policy_default_is_fail() {
        // GIVEN — JSON without an on_failure field
        let json = r#"{"id":"step1","agent":"agent1","input":"hello","depends_on":[]}"#;

        // WHEN
        let step: PipelineStepDef =
            serde_json::from_str(json).expect("deserialization must succeed");

        // THEN
        assert_eq!(step.on_failure, StepFailurePolicy::Fail);
    }

    // AC-5 — GlobalFailurePolicy defaults to Fail when absent from JSON
    #[test]
    fn test_ac5_global_failure_policy_default_is_fail() {
        // GIVEN — JSON without an on_failure field
        let json = r#"{"id":"p1","description":"test","steps":[]}"#;

        // WHEN
        let def: PipelineDefinition =
            serde_json::from_str(json).expect("deserialization must succeed");

        // THEN
        assert_eq!(def.on_failure, GlobalFailurePolicy::Fail);
    }

    // StepRunStatus default is Pending
    #[test]
    fn test_step_run_status_default_is_pending() {
        // GIVEN / WHEN
        let status = StepRunStatus::default();

        // THEN
        assert_eq!(status, StepRunStatus::Pending);
    }

    // PipelineStatus::Failed round-trip preserves step_id and reason
    #[test]
    fn test_pipeline_status_failed_roundtrip() {
        // GIVEN
        let status = PipelineStatus::Failed {
            step_id: StepId("validation".into()),
            reason: "timeout".into(),
        };

        // WHEN
        let json = serde_json::to_string(&status).expect("serialization must succeed");
        let restored: PipelineStatus =
            serde_json::from_str(&json).expect("deserialization must succeed");

        // THEN
        assert_eq!(status, restored);
    }

    // StepRun with all optional fields None serializes and restores cleanly
    #[test]
    fn test_step_run_pending_roundtrip() {
        // GIVEN
        let sr = StepRun {
            step_id: StepId("ocr".into()),
            task_id: None,
            status: StepRunStatus::Pending,
            output: None,
            error: None,
            started_at: None,
            ended_at: None,
        };

        // WHEN
        let json = serde_json::to_string(&sr).expect("serialization must succeed");
        let restored: StepRun = serde_json::from_str(&json).expect("deserialization must succeed");

        // THEN
        assert_eq!(restored.step_id, sr.step_id);
        assert_eq!(restored.status, StepRunStatus::Pending);
        assert!(restored.task_id.is_none());
    }
}
