//! `apollia-pipelines` — declarative multi-agent pipeline orchestration.
//!
//! This crate provides the foundational building blocks for Sprint 12:
//! - [`types`] — `PipelineDefinition`, `PipelineRun`, `StepRun` and related enums.
//! - [`repository`] — synchronous SQLite repository for pipeline runs and step runs.
//! - [`definition_repository`] — CRUD SQLite for pipeline definitions with DAG validation.
//! - [`validation`] — structural validation (DAG acyclicity, unique step IDs, fallback rules).
//! - [`template`] — `TemplateContext` and `render()` for `{{steps.x.output}}` interpolation.
//! - [`topo`] — `topological_layers()` for parallel step scheduling via Kahn's BFS.
//! - [`executor`] — `PipelineExecutor` with sequential and fan-out step execution.
//! - [`condition`] — `evaluate_condition()` for the 5 step condition operators.
//! - [`engine`] — `PipelineEngine` Tokio actor managing the run lifecycle.

pub mod condition;
pub mod definition_repository;
pub mod engine;
pub mod executor;
pub mod repository;
pub mod template;
pub mod topo;
pub mod types;
pub mod validation;

pub use condition::evaluate_condition;
pub use definition_repository::{
    PipelineDefinitionError, PipelineDefinitionRepository, PipelineDefinitionRow,
};
pub use engine::{PipelineEngine, PipelineEngineError, PipelineEngineHandle};
pub use executor::{ExecutorError, PipelineExecutor, StepResult, TaskSubmitter};
pub use repository::{PipelineRepository, PipelineRepositoryError};
pub use template::TemplateContext;
pub use topo::{topological_layers, TopologicalError};
pub use types::{
    ConditionKind, GlobalFailurePolicy, PipelineDefinition, PipelineId, PipelineRun,
    PipelineStatus, PipelineStepDef, RunId, StepCondition, StepFailurePolicy, StepId, StepRun,
    StepRunStatus,
};
pub use validation::{validate_pipeline, validate_pipeline_dag};
