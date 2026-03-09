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
//!
//! Types implemented in Sprint 1:
//! - [`RuntimeEvent`], [`AgentId`], [`TaskId`] — STORY-006

pub mod budget;
pub mod events;
pub mod manifest;
pub mod pending_approvals;
pub mod process;
pub mod result;
pub mod sandbox;
pub mod task;

pub use budget::StepBudgetConfig;
pub use events::{AgentId, EventBusSender, RuntimeEvent, TaskId};
pub use manifest::{AgentManifest, AgentSkill};
pub use pending_approvals::{PendingApprovalError, PendingApprovals};
pub use process::ProcessState;
pub use result::{
    AIPArtifact, AIPError, AIPResult, InputRequiredData, InputResponseData, TaskStatus,
};
pub use sandbox::SandboxProfile;
pub use task::{AIPInput, AIPMessage, AIPPart, AIPTask, DataPart, FilePart, TextPart};
