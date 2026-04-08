//! Collecteur de contexte workspace pour Apollia OS.
//!
//! Cette crate implémente les [`WorkspaceProvider`]s natifs et l'orchestrateur
//! [`ProjectRuntime`] qui les compose en parallèle avec cache TTL.
//!
//! # Architecture
//!
//! ```text
//! ProjectRuntime                  ← orchestrateur multi-provider avec cache TTL
//!   ├── GitProvider               ← branche, statut, commits (WorkspaceProvider)
//!   ├── RulesProvider             ← fichier de règles APOLLIA.md (WorkspaceProvider)
//!   ├── TreeProvider              ← arborescence du répertoire (WorkspaceProvider)
//!   ├── StyleProvider             ← conventions de code via LLM (optionnel)
//!   └── ScriptProvider            ← script shell produisant du JSON (WorkspaceProvider)
//! ```
//!
//! # Exemple
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
pub use config::{GitProviderConfig, RuntimeConfig, RulesProviderConfig, StyleProviderConfig};
pub use providers::{GitProvider, RulesProvider, ScriptProvider, StyleProvider, TreeProvider};
pub use style::StyleDetector;

// Re-exports from apollia-core for consumer convenience
pub use apollia_core::workspace::{WorkspaceProvider, WorkspaceSection, WorkspaceSlice, WorkspaceSnapshot};
