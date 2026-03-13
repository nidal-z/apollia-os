//! `apollia-llm` — moteur LLM embarqué et clients cloud pour Apollia OS.
//!
//! # Features
//!
//! - `cloud` (**défaut**) : active `OpenAICompatibleClient` et `AnthropicClient`
//!   via `async-openai` / `reqwest`.
//! - `local` : active `EmbeddedBackend` (inférence in-process via `mistral-rs-core`).
//!
//! Les types fondamentaux (`CompletionModel`, `CompletionRequest`, etc.) sont
//! disponibles quelle que soit la feature activée.

pub mod backends;
pub mod repository;
pub mod router;
pub mod tool_helper;
pub mod types;

pub use repository::{
    spawn_subscriber as spawn_llm_subscriber, LlmCallRecord, LlmCallRepository, LlmCostSummary,
    LlmRepositoryError,
};
pub use router::{BackendConfig, BackendKind, LlmConfig, LlmRouter, ObservabilityConfig};
pub use tool_helper::{StepBudgetView, ToolCallHelper, ToolInvoker};
pub use types::{
    BackendInfo, ChatMessage, CompletionModel, CompletionRequest, CompletionResponse, FinishReason,
    LlmError, MessageContent, Role, TokenUsage, ToolCall, ToolSpec,
};
