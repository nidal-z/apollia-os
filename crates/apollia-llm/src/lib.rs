#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
//! `apollia-llm`, cloud LLM clients for Apollia OS.
//!
//! Local inference backends (llama-cpp) no longer live here: they are hosted
//! by the `apollia-runner` crate (sidecar) and exposed to the daemon via
//! `RunnerLlmBackend` (apollia-runtime).
//!
//! # Features
//!
//! - `cloud` (default): enables `OpenAICompatibleClient`, `AnthropicClient` and
//!   `VertexClient` via `async-openai` / `reqwest`.
//!
//! The core types (`CompletionModel`, `CompletionRequest`, etc.) are available
//! regardless of which feature is enabled.

pub mod backends;
pub mod downloader;
pub mod hardware;
pub mod hf_registry;
pub mod meta;
pub mod meta_orchestrator;
pub mod model_defaults;
pub mod pricing;
pub mod repository;
pub mod retry;
pub mod router;
pub mod routing_level;
pub mod token_budget;
pub mod tool_helper;
pub mod tool_performance_hints;
pub mod types;

pub use meta::apollia_coach::{
    invoke_apollia_coach, ActionButton as CoachActionButton, ApolliaCoachError, CoachAction,
    CoachContext, CoachMode, CoachResponse, CoachTurn,
};
pub use meta::parse_automation::{
    parse_automation as parse_automation_description, AgentMatch as ParsedAgentMatch,
    Confidence as ParseAutomationConfidence, ParsedAutomation, ParsedSchedule,
};

pub use meta_orchestrator::{
    MetaLlmBudget, MetaLlmOrchestrator, MetaLlmSettings, MetaOrchestratorHandle, MetaRoutine,
    ThinkingContradiction, ThinkingQuality, ThinkingSummary, DEFAULT_SESSION_BUDGET_TOKENS,
};

#[cfg(feature = "cloud")]
pub use downloader::{
    DownloadError, DownloadId, DownloadManager, DownloadProgress, DownloadRequest, DownloadStatus,
};
pub use hardware::{AcceleratorProfile, CompatibilityBadge, HardwareProfile};
#[cfg(feature = "cloud")]
pub use hf_registry::{
    CompatIssue, GenerationConfig, HfError, HfFile, HfModelCard, HfModelTypeCache,
    HfRegistryClient, HfSearchFilter, SearchPage,
};
pub use repository::{
    spawn_subscriber as spawn_llm_subscriber, LlmCallRecord, LlmCallRepository, LlmCostSummary,
    LlmDailyCostSummary, LlmRepositoryError,
};
pub use retry::{IsCancelled, IsRetryable, RetryPolicy};
pub use apollia_core::config::LlmRoutingConfig;
pub use router::{BackendConfig, BackendKind, LlmConfig, LlmRouter, ObservabilityConfig};
pub use routing_level::LlmRoutingLevel;
pub use token_budget::SessionBudgetTracker;
pub use tool_helper::{StepBudgetView, ToolCallHelper, ToolInvoker};
pub use types::{
    BackendInfo, CacheControl, ChatMessage, CompletionModel, CompletionRequest, CompletionResponse,
    FinishReason, LlmError, MessageContent, Role, StreamChunk, TokenUsage, ToolCall, ToolSpec,
};
