//! `apollia-mcp` — MCP client for Apollia OS.
//!
//! Provides configuration parsing, JSON-RPC transport, session lifecycle,
//! server management, and tool execution for the Model Context Protocol.

pub mod config;
pub mod executor;
pub mod jsonrpc;
pub mod manager;
pub mod protocol;
pub mod session;
