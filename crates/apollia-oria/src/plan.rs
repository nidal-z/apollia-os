//! Execution plan types for ORIA's Orchestrated mode.
//!
//! The local `ExecutionPlan` / `PlanStep` structures are replaced by the
//! unified type from [`apollia_core::plan`]. Cycle detection at validation now
//! goes through [`apollia_core::plan::validate_plan`]; [`crate::topo`] keeps
//! `topological_levels` and `topological_sort` for execution ordering.

pub use apollia_core::plan::PlanStep;

/// Execution plan for ORIA's Orchestrated mode.
///
/// Keeps the (`plan_id`, `task_id`) pair specific to ORIA; the steps are
/// unified [`apollia_core::plan::PlanStep`] values, produced by
/// [`crate::reasoner::Reasoner`] and consumed by the `ActorLoop`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExecutionPlan {
    /// Unique plan identifier (UUID v4), generated at creation.
    pub plan_id: String,
    /// Identifier of the task this plan belongs to.
    pub task_id: String,
    /// Steps to execute (unified plan step), ordered by the topological sort.
    pub steps: Vec<PlanStep>,
}
