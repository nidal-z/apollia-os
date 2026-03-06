//! Apollia OS — Tool Registry and native tools.
//!
//! Provides the tooling infrastructure for agents:
//! - `ToolRegistry` — in-memory catalogue of available tools (STORY-011)
//! - `ToolResolver` — validates tool availability at INITIALIZING (STORY-012)
//! - `SandboxProfile` — Linux namespace isolation profiles (STORY-013)
//! - `AuditTrail` — SQLite-persisted tool invocation log (STORY-016)
//!
//! Native tools (Sprint 2):
//! - `bash_executor` — sandboxed shell execution via unshare(1) (STORY-013)
//! - `python_executor` — isolated virtualenv execution (STORY-014)
//! - `file_io` — filesystem operations with path traversal protection (STORY-015)
//! - `http_client` — network-restricted HTTP client
//! - `mcp_consumer` — MCP server protocol consumer

pub mod descriptor;

pub use descriptor::{McpTransport, ToolDescriptor, ToolDescriptorError, ToolKind};
