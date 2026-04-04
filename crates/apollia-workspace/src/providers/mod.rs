//! Implémentations de [`ContextProvider`] pour différentes sources de contexte.
//!
//! Providers natifs :
//! - [`GitWorkspaceProvider`] — état git, commits récents, `APOLLIA.md`, arborescence.
//! - [`ScriptContextProvider`] — script shell ou binaire qui écrit du JSON sur stdout.

pub mod git;
pub mod script;

pub use git::GitWorkspaceProvider;
pub use script::ScriptContextProvider;
