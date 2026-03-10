//! `apollia-pipelines` — declarative multi-agent pipeline orchestration.
//!
//! This crate provides the foundational building blocks for Sprint 12:
//! - [`types`] — `PipelineDefinition`, `PipelineRun`, `StepRun` and related enums.
//! - [`repository`] — synchronous SQLite repository for pipeline runs and step runs.
//!
//! Future stories will add the template renderer, topological sorter,
//! executor, and engine actor.

pub mod repository;
pub mod types;

pub use repository::{PipelineRepository, PipelineRepositoryError};
pub use types::{
    ConditionKind, GlobalFailurePolicy, PipelineDefinition, PipelineId, PipelineRun,
    PipelineStatus, PipelineStepDef, RunId, StepCondition, StepFailurePolicy, StepId, StepRun,
    StepRunStatus,
};
