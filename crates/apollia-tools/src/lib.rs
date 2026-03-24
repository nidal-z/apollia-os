//! Apollia OS — Tool Registry and native tools.
//!
//! Provides the tooling infrastructure for agents:
//! - `ToolRegistry` — in-memory catalogue of available tools
//! - `ToolResolver` — validates tool availability at INITIALIZING
//! - `SandboxProfile` — Linux namespace isolation profiles
//! - `AuditTrail` — SQLite-persisted tool invocation log
//! - `TaskRepository` — SQLite-persisted HITL task state
//! - `AgentRepository` — SQLite-persisted installed agents
//!
//! Native tools:
//! - `bash_executor` — sandboxed shell execution via unshare(1)
//! - `python_executor` — isolated virtualenv execution
//! - `file_io` — filesystem operations with path traversal protection
//! - `http_client` — network-restricted HTTP client
//! - `mcp_consumer` — MCP server protocol consumer

pub mod agent_repository;
pub mod audit;
pub mod descriptor;
pub mod registry;
pub mod resolver;
pub mod task_repository;
pub mod tools;

pub use agent_repository::{AgentRepository, AgentRepositoryError, InstalledAgent};
pub use audit::{
    compute_input_hash, AuditStats, AuditTrailError, AuditTrailHandle, ToolInvocationRecord,
};
pub use descriptor::{McpTransport, ToolDescriptor, ToolDescriptorError, ToolKind};
pub use registry::{ToolRegistryError, ToolRegistryHandle};
pub use resolver::{resolve, ResolutionReport, ResolutionStatus, ToolResolutionError};
pub use task_repository::{
    ApprovalInfo, PersistedTaskSummary, ResolvedApprovalRow, TaskDetail, TaskRepoError,
    TaskRepository,
};
pub use tools::file_io::{FileIo, FileIoError};
