//! `apollia-pipelines` — declarative multi-agent pipeline orchestration.
//!
//! This crate provides the foundational building blocks for Sprint 12:
//! - [`types`] — `PipelineDefinition`, `PipelineRun`, `StepRun` and related enums.
//! - [`repository`] — synchronous SQLite repository for pipeline runs and step runs.
//! - [`template`] — `TemplateContext` and `render()` for `{{steps.x.output}}` interpolation.
//! - [`topo`] — `topological_layers()` for parallel step scheduling via Kahn's BFS.
//! - [`executor`] — `PipelineExecutor` with sequential and fan-out step execution.

pub mod executor;
pub mod repository;
pub mod template;
pub mod topo;
pub mod types;

pub use executor::{ExecutorError, PipelineExecutor, StepResult, TaskSubmitter};
pub use repository::{PipelineRepository, PipelineRepositoryError};
pub use template::TemplateContext;
pub use topo::{topological_layers, TopologicalError};
pub use types::{
    ConditionKind, GlobalFailurePolicy, PipelineDefinition, PipelineId, PipelineRun,
    PipelineStatus, PipelineStepDef, RunId, StepCondition, StepFailurePolicy, StepId, StepRun,
    StepRunStatus,
};
