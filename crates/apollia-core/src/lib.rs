//! Apollia OS — shared types.
//!
//! `apollia-core` is the dependency foundation of the entire workspace.
//! All other crates depend on this one; this crate depends on nothing
//! else within the workspace.
//!
//! Types implemented:
//! - [`AgentManifest`], [`AgentSkill`]
//! - [`AIPTask`], [`AIPInput`], [`AIPPart`], [`TextPart`], [`FilePart`], [`DataPart`], [`AIPMessage`]
//! - [`AIPResult`], [`AIPError`], [`AIPArtifact`], [`TaskStatus`], [`StepBudgetConfig`]
//! - `ProcessState`
//! - Full `StepBudgetConfig` expansion, `SandboxProfile`
//!
//! Types implemented:
//! - [`RuntimeEvent`], [`AgentId`], [`TaskId`]

pub mod budget;
pub mod config;
pub mod events;
pub mod llm_backend;
pub mod manifest;
pub mod observability;
pub mod pending_approvals;
pub mod process;
pub mod result;
pub mod sandbox;
pub mod stt_config;
pub mod task;
pub mod token_budget;
pub mod user;
pub mod utils;

pub use budget::StepBudgetConfig;
pub use config::{
    validate_bounds, A2AConfig, ApiConfig, ConfigError, HitlConfig, ORIAConfig, PipelinesConfig,
    RuntimeConfig, ToolsConfig,
};
pub use events::{AgentId, EventBusSender, RuntimeEvent, TaskId};
pub use llm_backend::{LlmBackendConfig, LlmBackendError, LlmBackendRepository, LlmProvider};
pub use manifest::{AgentManifest, AgentSkill};
pub use observability::{truncate_with_marker, ObservabilityConfig};
pub use pending_approvals::{PendingApprovalError, PendingApprovals};
pub use process::ProcessState;
pub use result::{
    AIPArtifact, AIPError, AIPResult, InputRequiredData, InputResponseData, TaskStatus,
};
pub use sandbox::SandboxProfile;
pub use stt_config::{SttConfigError, SttConfigRepository, SttConfigRow};
pub use task::{AIPInput, AIPMessage, AIPPart, AIPTask, DataPart, FilePart, TextPart};
pub use token_budget::TokenBudget;
pub use user::{UpdateProfileRequest, UserMemoryEntryResponse, UserMemoryResponse, UserProfile};
pub use utils::truncate_middle;
