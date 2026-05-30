//! `apollia-mcp`: MCP client and server for Apollia OS.
//!
//! Provides configuration parsing, JSON-RPC transport, session lifecycle,
//! server management, tool execution for the Model Context Protocol, and
//! an MCP stdio server that exposes native Apollia tools to external clients.
//!
//! The [`discovery`] module adds opt-in mDNS-based service discovery for
//! locating MCP servers on the local network without manual configuration.
//!
//! The [`approvals`] module implements the SQLite-backed HITL approval gate
//! for MCP tool calls. Use [`approvals::McpApprovalStore`] directly from CLI
//! commands (`set-approval`, `revoke-approval`, `list-pending`) and inject it
//! into [`manager::McpClientManagerHandle`] via `start_with_approvals`.

pub mod approvals;
pub mod config;
pub mod discovery;
pub mod executor;
pub mod jsonrpc;
pub mod manager;
pub mod protocol;
pub mod server;
pub mod server_repository;
pub mod server_tools;
pub mod server_types;
pub mod session;
pub mod transport;

pub use approvals::{McpApprovalError, McpApprovalStore, PendingApprovalEntry};
pub use discovery::{DiscoveredServer, DiscoveryError};
pub use server::{McpServerError, McpStdioServer, SubmitTaskHandler};
pub use server_repository::{McpRepoError, McpServerRepository};
