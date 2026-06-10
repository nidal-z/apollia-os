//! Unified plan model shared by ORIA headless and the conversational chat path.
//!
//! This module defines a single execution-plan representation, its steps, their
//! provenance and the mutations applied to a plan over time, plus the pure
//! [`validate_plan`] helper that checks a step set forms a valid DAG (unique
//! identifiers, resolvable dependencies, acyclic).
//!
//! It is types-only and framework-agnostic: no async, no persistence, no actor
//! logic. The validation logic centralises what was historically split between
//! the ORIA reasoner (semantic checks) and the ORIA topological sort (cycle
//! detection), so every consumer shares one source of truth.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

/// Scope of a plan: a chat session or a headless ORIA task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanScope {
    /// Plan attached to a conversational chat session.
    Session(String),
    /// Plan attached to an ORIA task in Orchestrated mode.
    Task(String),
}

/// Life-cycle status of a [`Plan`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanStatus {
    /// Under construction, not yet submitted for approval.
    Draft,
    /// Submitted, awaiting the operator's decision.
    AwaitingApproval,
    /// Approved, currently executing.
    Executing,
    /// Finished successfully.
    Completed,
    /// Abandoned (definitive rejection or cancellation).
    Abandoned,
}

/// Execution status of a [`PlanStep`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus {
    /// Not started yet.
    #[default]
    Pending,
    /// Currently executing.
    InProgress,
    /// Finished successfully.
    Completed,
    /// Deliberately skipped.
    Skipped,
    /// Failed.
    Failed,
}

/// Origin of a step: how and when it appeared in the plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepOrigin {
    /// Created during the initial construction of the plan.
    Initial,
    /// Produced by an automatic revision (plan revision N).
    Replan(u32),
    /// Injected by the operator in natural language.
    UserInject,
    /// Edited by the agent itself during construction.
    AgentEdit,
}

/// Provenance of a step: origin, documented reason, timestamp.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StepProvenance {
    /// How the step appeared.
    pub origin: StepOrigin,
    /// Free-form reason justifying the appearance or modification.
    #[serde(default)]
    pub reason: Option<String>,
    /// Unix timestamp (seconds) of the provenance event.
    pub at: i64,
}

impl Default for StepProvenance {
    fn default() -> Self {
        Self {
            origin: StepOrigin::Initial,
            reason: None,
            at: 0,
        }
    }
}

/// A typed step of a [`Plan`], a node in the execution DAG.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanStep {
    /// Unique identifier of the step within the plan.
    pub step_id: String,
    /// Short human-readable title.
    ///
    /// Defaults to empty when absent from the payload, so a plan serialized
    /// with the historical step shape (which had no title) stays loadable.
    #[serde(default)]
    pub title: String,
    /// Detailed description of the action to perform.
    pub description: String,
    /// Current execution status.
    ///
    /// Defaults to [`StepStatus::Pending`] when absent, so an LLM-generated
    /// plan (which omits the status) deserializes without error.
    #[serde(default)]
    pub status: StepStatus,
    /// Identifiers of the steps that must finish before this one (DAG edges).
    #[serde(default)]
    pub depends_on: Vec<String>,
    /// Suggested tool (`None` when the step needs no native tool).
    #[serde(default)]
    pub tool_hint: Option<String>,
    /// Routing hint towards a specific LLM backend.
    #[serde(default)]
    pub model_hint: Option<String>,
    /// Free-form justification of the step, filled in by the agent.
    #[serde(default)]
    pub rationale: Option<String>,
    /// Provenance of the step (origin, reason, timestamp).
    #[serde(default)]
    pub provenance: StepProvenance,
}

impl PlanStep {
    /// Creates a pending step with the given identifier and description.
    ///
    /// The `title` mirrors the description, `status` is
    /// [`StepStatus::Pending`], dependencies and hints are empty, `rationale`
    /// is `None` and `provenance` is the default ([`StepOrigin::Initial`] at
    /// timestamp `0`). Callers refine the optional fields after construction.
    pub fn new(step_id: impl Into<String>, description: impl Into<String>) -> Self {
        let description = description.into();
        Self {
            step_id: step_id.into(),
            title: description.clone(),
            description,
            status: StepStatus::Pending,
            depends_on: Vec::new(),
            tool_hint: None,
            model_hint: None,
            rationale: None,
            provenance: StepProvenance::default(),
        }
    }
}

/// Unified plan shared by ORIA and the conversational chat path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plan {
    /// Unique identifier of the plan.
    pub plan_id: String,
    /// Scope: chat session or ORIA task.
    pub scope: PlanScope,
    /// Revision number, incremented on every replan.
    pub revision: u32,
    /// Life-cycle status.
    pub status: PlanStatus,
    /// Steps of the plan, in insertion order.
    pub steps: Vec<PlanStep>,
}

/// Nature of a mutation applied to a plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanMutationKind {
    /// Initial proposal of a plan (replaces the draft).
    Propose,
    /// Addition of a step.
    AddStep,
    /// Modification of an existing step.
    ModifyStep,
    /// Removal of a step.
    RemoveStep,
    /// Reordering of the steps.
    Reorder,
    /// Status change of a step.
    StatusChange,
    /// Submission of the plan for approval.
    Submit,
    /// Approval of the plan.
    Approve,
    /// Rejection of the plan.
    Reject,
}

/// A mutation applied to a plan, recorded for replay and audit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanMutation {
    /// Nature of the mutation.
    pub kind: PlanMutationKind,
    /// Step concerned (`None` for Propose, Reorder, Submit, Approve, Reject).
    #[serde(default)]
    pub step_id: Option<String>,
    /// Documented reason for the mutation.
    #[serde(default)]
    pub reason: Option<String>,
    /// State of the step before the mutation (`None` for an addition).
    #[serde(default)]
    pub before: Option<PlanStep>,
    /// State of the step after the mutation (`None` for a removal).
    #[serde(default)]
    pub after: Option<PlanStep>,
    /// Unix timestamp (seconds) of the mutation.
    pub at: i64,
}

/// Validation errors for a set of [`PlanStep`].
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum PlanValidationError {
    /// Two steps share the same identifier.
    #[error("duplicate step id: {step_id}")]
    DuplicateStepId {
        /// The duplicated identifier.
        step_id: String,
    },
    /// A step references a dependency absent from the list.
    #[error("step {step_id} depends on unknown step {depends_on}")]
    UnknownDependency {
        /// Step carrying the invalid dependency.
        step_id: String,
        /// Unresolvable dependency identifier.
        depends_on: String,
    },
    /// The dependencies form a cycle.
    #[error("dependency cycle detected in plan")]
    Cycle,
}

/// Validates a set of plan steps.
///
/// Checks, in order: uniqueness of identifiers, resolvability of every
/// dependency, absence of cycles (Kahn topological sort). An empty list is
/// valid. The first error encountered is returned.
///
/// # Errors
///
/// - [`PlanValidationError::DuplicateStepId`] on a duplicated identifier.
/// - [`PlanValidationError::UnknownDependency`] on an unresolvable dependency.
/// - [`PlanValidationError::Cycle`] on a dependency cycle.
pub fn validate_plan(steps: &[PlanStep]) -> Result<(), PlanValidationError> {
    let mut seen: HashSet<&str> = HashSet::with_capacity(steps.len());
    for step in steps {
        if !seen.insert(step.step_id.as_str()) {
            return Err(PlanValidationError::DuplicateStepId {
                step_id: step.step_id.clone(),
            });
        }
    }

    for step in steps {
        for dep in &step.depends_on {
            if !seen.contains(dep.as_str()) {
                return Err(PlanValidationError::UnknownDependency {
                    step_id: step.step_id.clone(),
                    depends_on: dep.clone(),
                });
            }
        }
    }

    detect_cycle(steps)
}

/// Detects a cycle using Kahn's algorithm (BFS, no recursion).
///
/// Assumes unique identifiers and resolvable dependencies, both verified
/// upstream by [`validate_plan`].
fn detect_cycle(steps: &[PlanStep]) -> Result<(), PlanValidationError> {
    use std::collections::{HashMap, VecDeque};

    let idx: HashMap<&str, usize> = steps
        .iter()
        .enumerate()
        .map(|(i, s)| (s.step_id.as_str(), i))
        .collect();

    let mut in_degree = vec![0usize; steps.len()];
    let mut dependents: HashMap<usize, Vec<usize>> = HashMap::new();
    for (i, step) in steps.iter().enumerate() {
        for dep in &step.depends_on {
            if let Some(&d) = idx.get(dep.as_str()) {
                in_degree[i] += 1;
                dependents.entry(d).or_default().push(i);
            }
        }
    }

    let mut queue: VecDeque<usize> = in_degree
        .iter()
        .enumerate()
        .filter(|(_, &d)| d == 0)
        .map(|(i, _)| i)
        .collect();

    let mut visited = 0usize;
    while let Some(i) = queue.pop_front() {
        visited += 1;
        if let Some(children) = dependents.get(&i) {
            for &c in children {
                in_degree[c] -= 1;
                if in_degree[c] == 0 {
                    queue.push_back(c);
                }
            }
        }
    }

    if visited == steps.len() {
        Ok(())
    } else {
        Err(PlanValidationError::Cycle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(id: &str, deps: &[&str]) -> PlanStep {
        PlanStep {
            step_id: id.to_string(),
            title: format!("Step {id}"),
            description: format!("Description {id}"),
            status: StepStatus::Pending,
            depends_on: deps.iter().map(|s| s.to_string()).collect(),
            tool_hint: None,
            model_hint: None,
            rationale: None,
            provenance: StepProvenance::default(),
        }
    }

    #[test]
    fn test_validate_plan_accepts_linear_dag() {
        // GIVEN a linear chain s1 -> s2 -> s3
        let steps = vec![step("s1", &[]), step("s2", &["s1"]), step("s3", &["s2"])];
        // WHEN validating it, and an empty list
        let result = validate_plan(&steps);
        // THEN both are accepted
        assert_eq!(result, Ok(()));
        assert_eq!(validate_plan(&[]), Ok(()));
    }

    #[test]
    fn test_validate_plan_rejects_duplicate_id() {
        // GIVEN two steps sharing step_id "s1"
        let steps = vec![step("s1", &[]), step("s1", &[])];
        // WHEN validating
        let result = validate_plan(&steps);
        // THEN a DuplicateStepId error names the id
        assert_eq!(
            result,
            Err(PlanValidationError::DuplicateStepId {
                step_id: "s1".into()
            })
        );
    }

    #[test]
    fn test_validate_plan_rejects_unknown_dependency() {
        // GIVEN s2 depending on an absent "s9"
        let steps = vec![step("s1", &[]), step("s2", &["s9"])];
        // WHEN validating
        let result = validate_plan(&steps);
        // THEN an UnknownDependency error names both ids
        assert_eq!(
            result,
            Err(PlanValidationError::UnknownDependency {
                step_id: "s2".into(),
                depends_on: "s9".into(),
            })
        );
    }

    #[test]
    fn test_validate_plan_rejects_cycle() {
        // GIVEN s1 <-> s2
        let steps = vec![step("s1", &["s2"]), step("s2", &["s1"])];
        // WHEN validating
        let result = validate_plan(&steps);
        // THEN a Cycle error is returned
        assert_eq!(result, Err(PlanValidationError::Cycle));
    }

    #[test]
    fn test_plan_serde_round_trip() {
        // GIVEN a full Plan with a step carrying provenance and rationale
        let mut s2 = step("s2", &["s1"]);
        s2.rationale = Some("split by dependency".into());
        s2.provenance = StepProvenance {
            origin: StepOrigin::Replan(1),
            reason: Some("revision after rejection".into()),
            at: 1_700_000_000,
        };
        let plan = Plan {
            plan_id: "p1".into(),
            scope: PlanScope::Session("sess-1".into()),
            revision: 1,
            status: PlanStatus::AwaitingApproval,
            steps: vec![step("s1", &[]), s2],
        };

        // WHEN serializing then deserializing
        let json = serde_json::to_string(&plan).expect("serialize");
        let back: Plan = serde_json::from_str(&json).expect("deserialize");

        // THEN the value is preserved verbatim
        assert_eq!(back, plan);
    }

    #[test]
    fn test_plan_mutation_serde_round_trip() {
        // GIVEN a ModifyStep mutation with before/after
        let mutation = PlanMutation {
            kind: PlanMutationKind::ModifyStep,
            step_id: Some("s1".into()),
            reason: Some("title clarified".into()),
            before: Some(step("s1", &[])),
            after: Some({
                let mut s = step("s1", &[]);
                s.title = "Step s1 revised".into();
                s
            }),
            at: 1_700_000_100,
        };
        // WHEN round-tripping through JSON
        let json = serde_json::to_string(&mutation).expect("serialize");
        let back: PlanMutation = serde_json::from_str(&json).expect("deserialize");
        // THEN it is preserved
        assert_eq!(back, mutation);
    }

    #[test]
    fn test_step_defaults_rationale_and_provenance() {
        // GIVEN a JSON step omitting rationale and provenance
        let raw = r#"{
            "step_id": "s1",
            "title": "t",
            "description": "d",
            "status": "pending"
        }"#;
        // WHEN deserializing
        let s: PlanStep = serde_json::from_str(raw).expect("deserialize");
        // THEN rationale is None and provenance defaults to Initial / 0
        assert_eq!(s.rationale, None);
        assert_eq!(s.provenance.origin, StepOrigin::Initial);
        assert_eq!(s.provenance.reason, None);
        assert_eq!(s.provenance.at, 0);
    }
}
