//! `apollia-mcp` — MCP client and server for Apollia OS.
//!
//! Provides configuration parsing, JSON-RPC transport, session lifecycle,
//! server management, tool execution for the Model Context Protocol, and
//! an MCP stdio server that exposes native Apollia tools to external clients.

pub mod config;
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

pub use server::{McpServerError, McpStdioServer, SubmitTaskHandler};
pub use server_repository::{McpRepoError, McpServerRepository};
