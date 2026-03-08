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
pub mod router;
pub mod tool_helper;
pub mod types;

pub use types::{
    BackendInfo, ChatMessage, CompletionModel, CompletionRequest, CompletionResponse, FinishReason,
    LlmError, MessageContent, Role, TokenUsage, ToolCall, ToolSpec,
};
