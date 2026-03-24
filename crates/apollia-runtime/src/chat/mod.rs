//! Chat subsystem — types, SQLite repository, and approval mechanics (Sprint 18).
//!
//! This module provides the foundational types and persistence layer for the
//! chat hybride feature. The `ChatSessionManager` actor and
//! `BuiltInChatAgent` build on top of these primitives.
//! `AgentChatExecutor` handles Chat Agent mode via Python agents.

pub mod agent_chat;
pub mod builtin_agent;
pub mod extractor;
pub mod manager;
pub mod repository;
pub mod summarizer;
pub mod types;

pub use agent_chat::{AgentChatExecutor, ChatAgentRunner};
pub use builtin_agent::{
    BuiltInChatAgent, ChatAgentResponse, NativeChatToolInvoker, DEFAULT_CONTEXT_WINDOW_SIZE,
    DEFAULT_SYSTEM_PROMPT,
};
pub use extractor::{extract_user_memory, spawn_extraction, ExtractionError, ExtractionResult};
pub use manager::ChatSessionManagerHandle;
pub use repository::{AppendMessageParams, ChatSessionRepository, MessageRow, SessionRow};
pub use summarizer::{summarize, SummarizerError};
pub use types::{
    ChatError, ChatMessage, ChatMode, ChatRole, ChatSession, ChatSessionConfig, ExchangeState,
    MessageId, PendingChatApprovals, SessionDetail, SessionId, SessionInfo, SessionStatus,
    ToolCallRecord, ToolCallStatus, ToolDecision,
};
