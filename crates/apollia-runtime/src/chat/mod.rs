//! Chat subsystem — types, SQLite repository, and approval mechanics (Sprint 18).
//!
//! This module provides the foundational types and persistence layer for the
//! chat hybride feature. The `ChatSessionManager` actor and
//! `BuiltInChatAgent` build on top of these primitives.
//! `AgentChatExecutor` handles Chat Agent mode via Python agents.

pub mod a2a_tools;
pub mod agent_chat;
pub mod builtin_agent;
pub mod extractor;
pub mod manager;
pub mod project_context;
pub mod repository;
pub mod summarizer;
pub mod types;

pub use a2a_tools::{generate_a2a_tool_specs, CompositeToolInvoker};
pub use agent_chat::{AgentChatExecutor, ChatAgentRunner};
pub use builtin_agent::{
    BuiltInChatAgent, ChatAgentResponse, NativeChatToolInvoker, DEFAULT_CONTEXT_WINDOW_SIZE,
    DEFAULT_SYSTEM_PROMPT,
};
pub use extractor::{
    extract_user_memory, spawn_extraction, ExtractionError, ExtractionResult, UserMemoryExtractor,
};
pub use manager::ChatSessionManagerHandle;
pub use project_context::DefaultProjectContextProvider;
pub use repository::{
    AppendMessageParams, ChatApprovalLogRow, ChatSessionRepository, MessageRow, SessionRow,
};
pub use summarizer::{summarize, SummarizerError};
pub use types::PastSessionSummary;
pub use types::RecentSessionSummary;
pub use types::{
    ChatError, ChatMessage, ChatMode, ChatRole, ChatSession, ChatSessionConfig, ExchangeState,
    FsHitlDecision, MessageId, PendingChatApprovals, ProjectContextProvider, SessionDetail,
    SessionId, SessionInfo, SessionMetrics, SessionStatus, ToolCallRecord, ToolCallStatus,
    ToolDecision, ToolStatEntry,
};
