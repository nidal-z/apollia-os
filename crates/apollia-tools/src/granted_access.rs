//! The access list of an agent, read at each run.
//!
//! An agent may call the tools its resolved definition grants it: the
//! defaults its package declares, then the user's override of the access
//! field. Native tools, MCP tools (`mcp:<server>/<tool>`) and package tools
//! (`pkg:<package>/<tool>`) are granted and revoked through that one field.
//!
//! The list is read when a run starts rather than when the agent is loaded,
//! so an access granted or revoked on the agent card applies to its next run.

use std::path::PathBuf;

use apollia_core::{AgentManifest, AutonomyLevel};

use crate::agent_definition::{definition_from_manifest, OverrideField};
use crate::agent_repository::{AgentRepository, AgentRepositoryError};

/// Where the access list of one agent is read from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrantedAccess {
    agent: String,
    agents_db: Option<PathBuf>,
    declared: Vec<String>,
}

impl GrantedAccess {
    /// The access of `manifest`'s agent, read from the agent store at
    /// `agents_db`. `declared` is the access its definition declares, used
    /// while the agent is not stored: an agent the store does not hold has no
    /// override to apply.
    pub fn new(agents_db: Option<PathBuf>, manifest: &AgentManifest) -> Self {
        let definition = definition_from_manifest(manifest);
        let declared = definition
            .pointer(OverrideField::Access.pointer())
            .and_then(serde_json::Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        Self {
            agent: manifest.name.clone(),
            agents_db,
            declared,
        }
    }

    /// The tools the agent may call now.
    ///
    /// The resolved access when the store holds the agent; the declared
    /// access when there is no store or the store does not hold it. A store
    /// that exists but cannot be read grants nothing: the declared defaults
    /// could hand back an access the user revoked.
    pub fn current(&self) -> Vec<String> {
        let Some(path) = self.agents_db.as_deref().filter(|p| p.exists()) else {
            return self.declared.clone();
        };
        match AgentRepository::open(path) {
            Ok(repo) => self.read(&repo),
            Err(error) => self.unreadable(&error),
        }
    }

    /// The tools the agent may call now, read from an open `repo`: its
    /// resolved access, or the declared access when `repo` does not hold it.
    /// An unreadable row grants nothing.
    pub fn read(&self, repo: &AgentRepository) -> Vec<String> {
        match repo.resolved(&self.agent, None, AutonomyLevel::Assisted) {
            Ok(Some(agent)) => agent.access,
            Ok(None) => self.declared.clone(),
            Err(error) => self.unreadable(&error),
        }
    }

    fn unreadable(&self, error: &AgentRepositoryError) -> Vec<String> {
        tracing::event!(
            tracing::Level::WARN,
            agent = %self.agent,
            error = %error,
            detail = "the run is granted no tool",
            "agent.access.unreadable"
        );
        Vec::new()
    }
}

#[cfg(test)]
mod tests;
