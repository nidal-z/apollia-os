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
pub mod pricing;
pub mod repository;
pub mod retry;
pub mod router;
pub mod routing_level;
pub mod tool_helper;
pub mod types;

pub use repository::{
    spawn_subscriber as spawn_llm_subscriber, LlmCallRecord, LlmCallRepository, LlmCostSummary,
    LlmDailyCostSummary, LlmRepositoryError,
};
pub use retry::{IsCancelled, IsRetryable, RetryPolicy};
pub use router::{BackendConfig, BackendKind, LlmConfig, LlmRouter, ObservabilityConfig};
pub use routing_level::LlmRoutingLevel;
pub use tool_helper::{StepBudgetView, ToolCallHelper, ToolInvoker};
pub use types::{
    BackendInfo, CacheControl, ChatMessage, CompletionModel, CompletionRequest, CompletionResponse,
    FinishReason, LlmError, MessageContent, Role, StreamChunk, TokenUsage, ToolCall, ToolSpec,
};
