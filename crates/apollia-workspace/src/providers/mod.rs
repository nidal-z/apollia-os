//! Native workspace context providers for Apollia OS.
//!
//! Each provider implements [`WorkspaceProvider`](apollia_core::workspace::WorkspaceProvider)
//! and represents an independent information source:
//!
//! - [`GitProvider`] - branch, git status, recent commits
//! - [`RulesProvider`] - project rules file (APOLLIA.md or custom)
//! - [`TreeProvider`] - tree of the current directory
//! - [`StyleProvider`] - code conventions detected via LLM (optional)
//! - [`ScriptProvider`] - shell script producing JSON on stdout

pub mod git;
pub mod rules;
pub mod script;
pub mod style;
pub mod tree;

pub use git::GitProvider;
pub use rules::RulesProvider;
pub use script::ScriptProvider;
pub use style::StyleProvider;
pub use tree::TreeProvider;
