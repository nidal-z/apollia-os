#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
//! `apollia-mcp`: MCP client and server for Apollia OS.
//!
//! Provides configuration parsing, JSON-RPC transport, session lifecycle,
//! server management, tool execution for the Model Context Protocol, and
//! an MCP stdio server that relays an external client to the agents and
//! workflows the daemon exposes.
//!
//! The [`discovery`] module adds opt-in mDNS-based service discovery for
//! locating MCP servers on the local network without manual configuration.

pub mod classify;
pub mod config;
pub mod discovery;
pub mod executor;
mod health;
pub mod jsonrpc;
pub mod manager;
pub mod mcp_resources;
pub mod npm_package;
pub mod origin;
pub mod package_server;
pub mod protocol;
pub mod registry;
mod sanitize;
pub mod secret_filing;
pub mod server;
pub mod server_repository;
pub mod server_types;
pub mod session;
pub mod tool_search;
pub mod tool_store;
pub mod transport;

pub use discovery::{DiscoveredServer, DiscoveryError};
pub use origin::{ConnectionOrigin, Provenance};
pub use server::{
    BridgeCallOutcome, BridgeError, BridgeFuture, McpBridge, McpServerError, McpStdioServer,
};
pub use server_repository::{McpRepoError, McpServerRepository};
