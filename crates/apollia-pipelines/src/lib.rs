//! `apollia-pipelines` — declarative multi-agent pipeline orchestration.
//!
//! This crate provides the foundational types for Sprint 12 pipeline support:
//! - [`types`] — `PipelineDefinition`, `PipelineRun`, `StepRun` and related enums.
//!
//! Future stories will add the template renderer, topological sorter,
//! executor, engine actor, and SQLite repository.

pub mod types;

pub use types::{
    ConditionKind, GlobalFailurePolicy, PipelineDefinition, PipelineId, PipelineRun,
    PipelineStatus, PipelineStepDef, RunId, StepCondition, StepFailurePolicy, StepId, StepRun,
    StepRunStatus,
};
