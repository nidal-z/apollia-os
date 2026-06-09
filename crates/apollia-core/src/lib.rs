#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
//! Apollia OS shared types.
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
pub mod context;
pub mod decision_point;
pub mod error_analysis;
pub mod events;
pub mod hitl_request;
pub mod llm_backend;
pub mod manifest;
pub mod mcp_health;
pub mod notebook;
pub mod observability;
pub mod pending_approvals;
pub mod plan_alternatives;
pub mod process;
pub mod result;
pub mod retry_attempt;
pub mod review;
pub mod sandbox;
pub mod session_metrics;
pub mod stt_config;
pub mod task;
pub mod temporal_context;
pub mod todo;
pub mod token_budget;
pub mod utils;
pub mod workspace;

pub use budget::StepBudgetConfig;
pub use config::{
    validate_bounds, A2AConfig, ApiConfig, AutonomyConfig, AutonomyLevel, AutonomyLevelConfig,
    AutonomyLevelParseError, BashValidatorConfig, BraveBackendConfig, ConfigError,
    DuckDuckGoBackendConfig, FilesystemConfig, FilesystemRiskConfig, HitlConfig,
    HybridRoutingConfig, JournalConfig, LlmRoutingConfig, LlmRunnerConfig, McpConfig,
    McpToolLoading, ORIAConfig, PermissionsConfig, RegistryConfig, RuntimeConfig, ToolsConfig,
    TriggersConfig, VertexConfig, WebReadConfig, WebSearchBackend, WebSearchConfig,
};
pub use context::{ContextProvider, ContextSection, ContextSnapshot};
pub use decision_point::{ConsideredAlternative, DecisionKind, DecisionPoint};
pub use error_analysis::{ErrorAnalysis, ErrorCategory};
pub use events::{
    AgentId, EventBusSender, FilesystemPreview, RuntimeEvent, TaskId, ToolCallRationale,
};
pub use hitl_request::{HitlRequest, ImpactLevel, RiskAnalysis};
pub use llm_backend::{LlmBackendConfig, LlmBackendError, LlmBackendRepository, LlmProvider};
pub use manifest::{AgentManifest, AgentSkill, MemoryConfig};
pub use mcp_health::{McpHealth, McpHealthSeverity};
pub use notebook::{CellType, JupyterCell, NotebookEditOp};
pub use observability::{truncate_with_marker, ObservabilityConfig};
pub use pending_approvals::{PendingApprovalError, PendingApprovals};
pub use plan_alternatives::{ChosenPlan, PlanAlternatives, PlanChoice, TaskPlan, TaskPlanStep};
pub use process::ProcessState;
pub use result::{
    AIPArtifact, AIPError, AIPResult, InputRequiredData, InputResponseData, TaskStatus,
};
pub use retry_attempt::{AttemptOutcome, LlmFallback, RetryAttempt};
pub use review::{ReviewIssue, ReviewReport};
pub use sandbox::SandboxProfile;
pub use session_metrics::{
    BudgetAlertLevel, SessionMetrics, SessionThresholds, SummarizationEvent, ToolTiming,
};
pub use stt_config::{SttConfigError, SttConfigRepository, SttConfigRow};
pub use task::{AIPInput, AIPMessage, AIPPart, AIPTask, DataPart, FilePart, TextPart};
pub use todo::{count_in_progress, TodoError, TodoItem, TodoStatus};
pub use token_budget::{TokenBudget, TokenUsageDelta};
pub use utils::truncate_middle;
pub use workspace::{WorkspaceProvider, WorkspaceSection, WorkspaceSlice, WorkspaceSnapshot};
