//! Collecteur de contexte workspace pour Apollia OS.
//!
//! Cette crate collecte automatiquement les informations locales d'un projet
//! au démarrage d'une tâche ORIA :
//! - État du dépôt git (branche, statut, commits récents)
//! - Contenu de `APOLLIA.md` (règles projet, contexte utilisateur)
//! - Arborescence du répertoire courant
//!
//! Les informations collectées sont agrégées dans un [`WorkspaceContext`]
//! (défini dans `apollia-core`) puis injectées dans le system prompt LLM
//! par `apollia-oria` et `apollia-aip` (STORY-459).
//!
//! # Architecture
//!
//! ```text
//! WorkspaceAssembler          ← orchestrateur avec cache TTL
//!   ├── GitContextCollector   ← git via sous-processus (fail-silent)
//!   ├── ApolliamdFinder       ← recherche récursive APOLLIA.md
//!   └── DirectoryTreeBuilder  ← BFS avec timeout 1s
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
//! let ctx = assembler.collect(&cwd).await;
//! println!("{}", ctx.format_for_prompt());
//! # }
//! ```

pub mod apollia_md;
pub mod assembler;
pub mod config;
pub mod git;
pub mod tree;

pub use assembler::WorkspaceAssembler;
pub use config::WorkspaceConfig;
