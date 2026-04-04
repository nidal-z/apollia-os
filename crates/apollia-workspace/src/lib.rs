//! Collecteur de contexte workspace pour Apollia OS.
//!
//! Cette crate collecte automatiquement les informations locales d'un projet
//! au démarrage d'une tâche ORIA via une architecture multi-provider :
//! - État du dépôt git (branche, statut, commits récents)
//! - Contenu de `APOLLIA.md` (règles projet, contexte utilisateur)
//! - Arborescence du répertoire courant
//!
//! Les informations collectées sont agrégées en [`ContextSnapshot`]s
//! puis injectées dans le system prompt LLM par `apollia-oria` et `apollia-aip`.
//!
//! # Architecture
//!
//! ```text
//! WorkspaceAssembler              ← orchestrateur multi-provider avec cache TTL
//!   ├── GitWorkspaceProvider      ← git, APOLLIA.md, arborescence, style (ContextProvider)
//!   └── ScriptContextProvider     ← script/binaire produisant du JSON (ContextProvider)
//! ```
//!
//! # Exemple
//!
//! ```rust,no_run
//! use apollia_workspace::{WorkspaceAssembler, WorkspaceConfig};
//!
//! # #[tokio::main]
//! # async fn main() {
//! let assembler = WorkspaceAssembler::new(WorkspaceConfig::default());
//! let cwd = std::env::current_dir().unwrap();
//! let snapshots = assembler.collect_all(&cwd).await;
//! let prompt = WorkspaceAssembler::format_for_prompt(&snapshots);
//! println!("{}", prompt);
//! # }
//! ```

pub mod apollia_md;
pub mod assembler;
pub mod config;
pub mod git;
pub mod providers;
pub mod style;
pub mod tree;

pub use assembler::WorkspaceAssembler;
pub use config::{ProviderConfig, WorkspaceConfig};
pub use style::StyleDetector;

// Re-exports from apollia-core for consumer convenience
pub use apollia_core::context::{ContextProvider, ContextSection, ContextSnapshot};
