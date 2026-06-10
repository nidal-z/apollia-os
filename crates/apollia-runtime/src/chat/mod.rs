//! Chat subsystem, types, SQLite repository, and approval mechanics.
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
pub mod native_wrappers;
pub mod plan_actor;
pub mod plan_prompt;
pub mod plan_tool;
pub mod project_context;
pub mod repository;
pub mod summarizer;
pub mod todo_actor;
pub mod todo_handle;
pub mod todo_tool;
pub mod turn_router;
pub mod types;

pub use a2a_tools::{generate_a2a_tool_specs, CompositeToolInvoker};
pub use agent_chat::{AgentChatExecutor, ChatAgentRunner};
pub use builtin_agent::{
    BuiltInChatAgent, BuiltInChatAgentDeps, ChatAgentResponse, NativeChatToolInvoker,
    DEFAULT_CONTEXT_WINDOW_SIZE, DEFAULT_SYSTEM_PROMPT,
};
pub use extractor::{
    extract_user_memory, spawn_extraction, ExtractionError, ExtractionResult, UserMemoryExtractor,
};
pub use manager::{
    ChatSessionManagerHandle, ChatToolsConfig, InjectError, PauseError, PendingUserInputView,
    SessionAuthorizationView, APOLLIA_CHAT_AGENT_ID,
};
pub use plan_actor::{spawn_plan_actor, PlanHandle, PlanStoreError};
pub use project_context::DefaultProjectContextProvider;
pub use repository::{
    AppendMessageParams, ChatApprovalLogRow, ChatSessionRepository, MessageRow, SessionRow,
};
pub use summarizer::{summarize, SummarizerError};
pub use todo_actor::spawn_todo_actor;
pub use todo_handle::TodoHandle;
pub use types::PastSessionSummary;
pub use types::RecentSessionSummary;
pub use types::{
    AlwaysAcceptScope, ChatError, ChatMessage, ChatMode, ChatRole, ChatSession, ChatSessionConfig,
    ExchangeState, FsHitlDecision, InjectedInstruction, MessageId, PauseState,
    PendingChatApprovals, ProjectContextProvider, SessionDetail, SessionId, SessionInfo,
    SessionMetrics, SessionStatus, ToolCallRecord, ToolCallStatus, ToolDecision, ToolStatEntry,
    TurnOutcome,
};
