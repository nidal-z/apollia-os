//! Shared types for the RLHF feedback loop: two alternative plans.
//!
//! Defined in `apollia-core` so it can be used by `apollia-oria` (generation),
//! `apollia-memory` (persisting the choice), `apollia-cli` (terminal display)
//! and `apollia-desktop` (Svelte component) without a circular dependency.

use serde::{Deserialize, Serialize};

// ─────────────────────────────────────────────
// Step / Plan
// ─────────────────────────────────────────────

/// A single step within a [`TaskPlan`].
///
/// Shared representation used in the feedback loop context, distinct from the
/// `ExecutionPlan` / `PlanStep` types internal to `apollia-oria`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskPlanStep {
    /// Unique identifier within the plan (e.g. `"s1"`, `"s2"`).
    pub step_id: String,
    /// Natural-language description of the action to perform.
    pub description: String,
    /// Tool suggested by the LLM (optional).
    #[serde(default)]
    pub tool_hint: Option<String>,
    /// Identifiers of the steps this step depends on.
    #[serde(default)]
    pub depends_on: Vec<String>,
    /// Optional hint to route this step to a specific LLM backend.
    #[serde(default)]
    pub model_hint: Option<String>,
}

/// Serializable execution plan shared across crates.
///
/// Used as the canonical representation in [`PlanAlternatives`]
/// and in [`RuntimeEvent::PlanAlternativesGenerated`] events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskPlan {
    /// Unique plan identifier (UUID v4).
    pub plan_id: String,
    /// Identifier of the associated task.
    pub task_id: String,
    /// Steps to execute.
    pub steps: Vec<TaskPlanStep>,
}

// ─────────────────────────────────────────────
// PlanAlternatives
// ─────────────────────────────────────────────

/// Two alternative plans generated in parallel by the Reasoner.
///
/// Plan A is produced at low temperature (deterministic, conservative).
/// Plan B is produced at high temperature (creative, exploratory).
/// The `session_id` correlates the generation with the operator's choice.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanAlternatives {
    /// Plan A: low temperature, deterministic and conservative.
    pub plan_a: TaskPlan,
    /// Plan B: high temperature, creative and exploratory.
    pub plan_b: TaskPlan,
    /// Session identifier for correlation with [`PlanChoice`].
    pub session_id: String,
    /// Unix timestamp of generation (seconds).
    pub generated_at: i64,
}

// ─────────────────────────────────────────────
// PlanChoice / ChosenPlan
// ─────────────────────────────────────────────

/// The operator's choice between the two alternative plans.
///
/// Persisted to SQLite by `PlanChoiceStore::log_plan_choice()` to form the
/// RLHF signal. Local only: never sent off the machine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanChoice {
    /// Session identifier matching [`PlanAlternatives::session_id`].
    pub session_id: String,
    /// Plan chosen by the operator.
    pub chosen: ChosenPlan,
    /// Unix timestamp of the choice (seconds).
    pub chosen_at: i64,
}

/// Identifies which of the two alternative plans was chosen.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ChosenPlan {
    /// Plan A: the conservative, low-temperature plan.
    PlanA,
    /// Plan B: the exploratory, high-temperature plan.
    PlanB,
}

impl ChosenPlan {
    /// Returns the text representation used in the `plan_choices` table.
    pub fn as_db_str(&self) -> &str {
        match self {
            ChosenPlan::PlanA => "plan_a",
            ChosenPlan::PlanB => "plan_b",
        }
    }
}
