//! Chat subsystem — types, SQLite repository, and approval mechanics (Sprint 18).
//!
//! This module provides the foundational types and persistence layer for the
//! chat hybride feature. The `ChatSessionManager` actor (STORY-199) and
//! `BuiltInChatAgent` (STORY-200) build on top of these primitives.

pub mod repository;
pub mod types;

pub use repository::{AppendMessageParams, ChatSessionRepository, MessageRow, SessionRow};
pub use types::{
    ChatError, ChatMessage, ChatMode, ChatRole, ChatSession, ExchangeState, MessageId,
    PendingChatApprovals, SessionDetail, SessionId, SessionInfo, SessionStatus, ToolCallRecord,
    ToolCallStatus, ToolDecision,
};
