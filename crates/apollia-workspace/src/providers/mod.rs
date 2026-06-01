//! Providers natifs de contexte workspace pour Apollia OS.
//!
//! Chaque provider implémente [`WorkspaceProvider`](apollia_core::workspace::WorkspaceProvider)
//! et représente une source d'information indépendante :
//!
//! - [`GitProvider`] - branche, statut git, commits récents
//! - [`RulesProvider`] - fichier de règles projet (APOLLIA.md ou custom)
//! - [`TreeProvider`] - arborescence du répertoire courant
//! - [`StyleProvider`] - conventions de code détectées via LLM (optionnel)
//! - [`ScriptProvider`] - script shell produisant du JSON sur stdout

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
