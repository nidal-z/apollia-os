#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
//! `apollia-llm`, cloud LLM clients for Apollia OS.
//!
//! Local inference backends (llama-cpp) no longer live here: they are hosted
//! by the `apollia-runner` crate (sidecar) and exposed to the daemon via
//! `RunnerLlmBackend` (apollia-runtime).
//!
//! # Features
//!
//! - `cloud` (default): enables `OpenAICompatibleClient`, which serves every
//!   remote server including Ollama, via `async-openai` / `reqwest`.
//!
//! The core types (`CompletionModel`, `CompletionRequest`, etc.) are available
//! regardless of which feature is enabled.

pub mod backends;
pub mod call_measure;
pub mod capabilities;
pub mod context_window;
pub mod downloader;
#[cfg(feature = "cloud")]
pub mod embeddings_client;
pub mod gguf_probe;
pub mod grammar;
pub mod hardware;
pub mod hf_registry;
#[cfg(feature = "cloud")]
pub mod http_client;
pub mod message_image;
pub mod meta;
pub mod meta_orchestrator;
pub mod model_cards;
pub mod model_defaults;
pub mod model_index;
pub mod ocr_data;
pub mod pricing;
pub mod reasoning_markers;
pub mod recommend;
pub mod repository;
pub mod request_defaults;
pub mod resource_usage;
pub mod retry;
pub mod router;
pub mod routing_level;
pub(crate) mod schema_sanitize;
pub mod schema_validate;
#[cfg(feature = "cloud")]
pub mod server_models;
#[cfg(feature = "cloud")]
pub mod server_window;
pub mod token_budget;
pub mod tool_arguments;
pub mod tool_helper;
pub mod tool_performance_hints;
pub mod types;

pub use meta::parse_automation::{
    parse_automation as parse_automation_description, AgentMatch as ParsedAgentMatch,
    Confidence as ParseAutomationConfidence, ParsedAutomation, ParsedSchedule,
};

pub use meta_orchestrator::{
    MetaLlmBudget, MetaLlmOrchestrator, MetaLlmSettings, MetaOrchestratorHandle, MetaRoutine,
    ThinkingContradiction, ThinkingQuality, ThinkingSummary, DEFAULT_SESSION_BUDGET_TOKENS,
};

pub use apollia_core::config::LlmRoutingConfig;
pub use call_measure::{generation_ms, CallContext, CallEnd, CallMeasure, CallMeter};
pub use capabilities::{Capabilities, ReasoningProfile, VisionEngine};
#[cfg(feature = "cloud")]
pub use downloader::{
    DownloadError, DownloadId, DownloadManager, DownloadOutcome, DownloadProgress, DownloadRequest,
    DownloadStatus, ProgressCallback, VariantDownload,
};
pub use gguf_probe::{parse_header, GgufHeaderFacts, GgufParseError};
#[cfg(feature = "cloud")]
pub use gguf_probe::{probe_url, GgufProbeError, ProbeDepth};
pub use grammar::{json_schema_to_gbnf, tool_specs_to_gbnf, GrammarError};
pub use hardware::{AcceleratorProfile, CompatibilityBadge, HardwareProfile};
#[cfg(feature = "cloud")]
pub use hf_registry::{
    BoundedPage, BoundedSearch, BoundedSearchError, CompatIssue, FoundModel, GeneratedCard,
    GenerationConfig, HfError, HfFile, HfModelCard, HfModelTypeCache, HfRegistryClient,
    SearchAuthor, SearchContext, SearchFormat, SearchInputs, SearchSort, VariantError, VariantFile,
    VariantPlan, VariantSelector,
};
pub use model_index::import::{
    check_import, ImportCheck, ImportCheckError, ImportRule as ModelImportRule,
};
pub use model_index::{
    IndexedModel, ModelCategory, ModelFormat, ModelIndex, ModelIndexError, ModelOrigin,
};
pub use recommend::{
    assess as assess_gguf, rank as rank_models, Blocker, Caveat, FamilyManifest, FileCandidate,
    MemoryEstimate, Reason, Recommendation, RuntimeShape, Verdict,
};
#[cfg(feature = "cloud")]
pub use recommend::{resolve as resolve_recommendations, ResolveError, ResolveOptions};
pub use repository::{
    spawn_subscriber as spawn_llm_subscriber, LlmCallRecord, LlmCallRepository, LlmCostSummary,
    LlmDailyCostSummary, LlmRepositoryError,
};
pub use retry::{IsCancelled, IsRetryable, RetryPolicy};
pub use router::{
    call_completed_event, observe_stream, AgentModelBinding, AgentModelFallback, BackendConfig,
    BackendKind, LlmConfig, LlmRouter, ObservabilityConfig, ObservedCall, ServerEndpoint,
};
pub use routing_level::{EscalationSignal, LlmRoutingLevel};
pub use schema_validate::{
    parse_and_validate, schema_fingerprint, validate_against_schema, SchemaViolation,
};
pub use token_budget::SessionBudgetTracker;
pub use tool_helper::{StepBudgetView, ToolCallHelper, ToolInvoker};
pub use types::{
    estimate_tool_specs_tokens, BackendInfo, CacheControl, ChatMessage, CompletionModel,
    CompletionRequest, CompletionResponse, FinishReason, LlmError, MessageContent, ReasoningEffort,
    Role, StreamChunk, TokenUsage, ToolCall, ToolSpec,
};
