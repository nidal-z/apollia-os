//! LLM types: load/unload model, complete, stream, embed.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Role in a chat conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
}

/// A message in a chat conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: Role,
    pub content: String,
}

/// Params for `POST /llm/load_model`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadModelParams {
    pub model_id: String,
    pub model_path: PathBuf,
    #[serde(default = "default_n_ctx")]
    pub n_ctx: u32,
    #[serde(default = "default_n_gpu_layers")]
    pub n_gpu_layers: i32,
    #[serde(default = "default_use_mmap")]
    pub use_mmap: bool,
    #[serde(default)]
    pub use_mlock: bool,
}

fn default_n_ctx() -> u32 {
    4096
}
fn default_n_gpu_layers() -> i32 {
    -1
}
fn default_use_mmap() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadModelData {
    pub model_id: String,
    pub load_time_ms: u64,
    pub context_size: u32,
    pub memory_used_mb: u32,
    /// GGUF `general.architecture` (e.g. `qwen3`, `llama`), empty if absent.
    /// Lets the daemon resolve per-family sampling defaults.
    #[serde(default)]
    pub arch: String,
}

/// Params for `POST /llm/unload_model`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnloadModelParams {
    pub model_id: String,
}

/// Params for `POST /llm/complete` and `POST /llm/stream`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompleteParams {
    pub model_id: String,
    pub messages: Vec<ChatMessage>,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    #[serde(default = "default_top_p")]
    pub top_p: f32,
    #[serde(default = "default_top_k")]
    pub top_k: u32,
    #[serde(default = "default_repeat_penalty")]
    pub repeat_penalty: f32,
    #[serde(default)]
    pub seed: Option<u64>,
    #[serde(default)]
    pub stop: Vec<String>,
    /// OpenAI-format tool specs (`[{"type":"function","function":{...}}]`).
    /// Rendered into the prompt by the model's own chat template, so each
    /// tokenizer family advertises tools in its native convention. `None`
    /// disables tool use for this request.
    #[serde(default)]
    pub tools: Option<serde_json::Value>,
}

fn default_max_tokens() -> u32 {
    256
}
fn default_temperature() -> f32 {
    0.7
}
fn default_top_p() -> f32 {
    0.95
}
fn default_top_k() -> u32 {
    40
}
fn default_repeat_penalty() -> f32 {
    1.1
}

/// Reason inference stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinishReason {
    Stop,
    Length,
    Eos,
    Abort,
}

/// Token counters for an inference.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// Detailed timing of an inference.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Timing {
    pub queue_ms: u64,
    pub prefill_ms: u64,
    pub decode_ms: u64,
    pub total_ms: u64,
}

/// A tool call parsed from the model output.
///
/// Produced by the model's own chat template parser (`common_chat`), so the
/// shape is uniform across tokenizer families (Qwen, Mistral, Hermes, ...).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    /// Arguments object the model passed to the tool.
    pub arguments: serde_json::Value,
}

/// Response from `POST /llm/complete`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompleteData {
    pub text: String,
    pub finish_reason: FinishReason,
    pub usage: TokenUsage,
    pub timing: Timing,
    /// Tool calls parsed from the response, empty when the model produced none
    /// (or when the request carried no tools).
    #[serde(default)]
    pub tool_calls: Vec<ToolCall>,
}

/// SSE chunk from `POST /llm/stream`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamChunk {
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<FinishReason>,
}

/// Params for `POST /llm/embed`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbedParams {
    pub model_id: String,
    pub texts: Vec<String>,
}

/// Response from `POST /llm/embed`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbedData {
    pub embeddings: Vec<Vec<f32>>,
    pub dim: u32,
    pub model_id: String,
}
