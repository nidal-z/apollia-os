//! Apollia OS — shared types.
//!
//! `apollia-core` is the dependency foundation of the entire workspace.
//! All other crates depend on this one; this crate depends on nothing
//! else within the workspace.
//!
//! Types implemented in Sprint 0:
//! - [`AgentManifest`], [`AgentSkill`] — STORY-002
//! - [`AIPTask`], [`AIPInput`], [`AIPPart`], [`TextPart`], [`FilePart`], [`DataPart`], [`AIPMessage`] — STORY-002
//! - [`AIPResult`], [`AIPError`], [`AIPArtifact`], [`TaskStatus`], [`StepBudgetConfig`] — STORY-002
//! - `ProcessState` — STORY-003
//! - Full `StepBudgetConfig` expansion, `SandboxProfile` — STORY-004

pub mod manifest;
pub mod result;
pub mod task;

pub use manifest::{AgentManifest, AgentSkill};
pub use result::{AIPArtifact, AIPError, AIPResult, StepBudgetConfig, TaskStatus};
pub use task::{AIPInput, AIPMessage, AIPPart, AIPTask, DataPart, FilePart, TextPart};
