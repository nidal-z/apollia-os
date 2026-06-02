#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
//! Apollia OS: native SaaS connectors.
//!
//! This crate implements connectors that bind Apollia agents to external SaaS
//! APIs **without any cloud relay**: every request is made from the user's
//! machine, OAuth tokens stay in [`apollia_auth`]'s keyring, and Apollia's
//! infrastructure never sees the user's data.
//!
//! Connectors live alongside the MCP client (in `apollia-mcp`): MCP is for
//! third-party servers (Notion, Slack, GitHub), connectors are the native
//! path for SaaS that don't publish an official MCP server (Google Workspace,
//! Microsoft 365).
//!
//! # Trait surface
//!
//! Every connector implements [`Connector`]:
//! - Identity ([`Connector::id`])
//! - Manifest describing services exposed ([`Connector::manifest`])
//! - Operation list mapped onto native tools ([`Connector::operations`])
//! - Health check using the stored token ([`Connector::check`])
//!
//! # Lifecycle
//!
//! 1. Runtime startup wires the static list of [`Connector`] instances into a
//!    shared [`ConnectorRegistry`].
//! 2. For each connector, `apollia-tools::registry` registers the operations
//!    as native tools, mapping each [`OperationSpec`] to a handler that calls
//!    the appropriate method on the connector.
//! 3. When a tool is invoked, the handler asks
//!    [`apollia_auth::AuthManager`] for a valid token, then drives the
//!    upstream API through this crate's [`HttpClient`] (with refresh-once on
//!    401 and exponential backoff on 5xx/429).
//!
//! Connectors are **stateless**. Tokens live in the auth crate; this crate is
//! pure I/O.

pub mod error;
pub mod google;
pub mod http;
pub mod manifest;
pub mod microsoft;
pub mod operation;
pub mod registry;
pub mod trait_def;

pub use error::ConnectorError;
pub use google::GoogleConnector;
pub use http::HttpClient;
pub use manifest::{ConnectorManifest, ConnectorSummary};
pub use microsoft::MicrosoftConnector;
pub use operation::{ApprovalPolicy, OperationSpec};
pub use registry::ConnectorRegistry;
pub use trait_def::{Connector, HealthReport};
