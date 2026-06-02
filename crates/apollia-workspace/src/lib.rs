#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
//! Workspace context collector for Apollia OS.
//!
//! This crate implements the native [`WorkspaceProvider`]s and the
//! [`ProjectRuntime`] orchestrator that composes them in parallel with a TTL cache.
//!
//! # Architecture
//!
//! ```text
//! ProjectRuntime                  multi-provider orchestrator with TTL cache
//!   ├── GitProvider               branch, status, commits (WorkspaceProvider)
//!   ├── RulesProvider             APOLLIA.md rules file (WorkspaceProvider)
//!   ├── TreeProvider              directory tree (WorkspaceProvider)
//!   ├── StyleProvider             code conventions via LLM (optional)
//!   └── ScriptProvider            shell script producing JSON (WorkspaceProvider)
//! ```
//!
//! # Example
//!
//! ```rust,no_run
//! use apollia_workspace::ProjectRuntime;
//!
//! # #[tokio::main]
//! # async fn main() {
//! let runtime = ProjectRuntime::default_project();
//! let cwd = std::env::current_dir().unwrap();
//! let snapshot = runtime.collect(&cwd).await;
//! let prompt = snapshot.format_for_prompt();
//! println!("{}", prompt);
//! # }
//! ```

pub mod apollia_md;
pub mod assembler;
pub mod commands;
pub mod config;
pub mod git;
pub mod providers;
pub mod style;
pub mod tree;

pub use assembler::{ProjectRuntime, ProviderEntry};
pub use commands::{CommandLoader, LoadedCommand};
pub use config::{GitProviderConfig, RulesProviderConfig, RuntimeConfig, StyleProviderConfig};
pub use providers::{GitProvider, RulesProvider, ScriptProvider, StyleProvider, TreeProvider};
pub use style::StyleDetector;

// Re-exports from apollia-core for consumer convenience
pub use apollia_core::workspace::{
    WorkspaceProvider, WorkspaceSection, WorkspaceSlice, WorkspaceSnapshot,
};
