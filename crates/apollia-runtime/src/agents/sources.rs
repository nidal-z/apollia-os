//! The sources an agent has checked, read in the runtime.
//!
//! An agent's source list resolves like every other field of its definition:
//! the package default, then the user's `sources` override in `agents.db`,
//! which replaces it whole. The same resolution answers whether the agent may
//! read the profile (`crate::profile::source`); this module reads the list
//! against the registry of sources, owned by its actor.

use std::path::Path;

use apollia_core::folder_connection::FolderConnectionStore;
use apollia_core::paths::DataFile;
use apollia_core::source_registry::SourceRegistryStore;
use apollia_sources::{CheckedSource, SourceKind, SourceRegistryHandle, SourcesError};
use apollia_tools::{AgentRepository, AgentRepositoryError, CodeContext, CodeFolder};

/// Errors of reading the sources an agent has checked.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum AgentSourcesError {
    /// `agents.db` could not be read.
    #[error(transparent)]
    Agents(#[from] AgentRepositoryError),
    /// The registry of sources could not answer.
    #[error(transparent)]
    Sources(#[from] SourcesError),
    /// The read of `agents.db` did not complete.
    #[error("the read of agents.db was interrupted: {0}")]
    Interrupted(String),
}

/// Starts the registry actor on the `system.db` at `path`, or says why it
/// cannot and answers `None`: the routes that need it then answer 503.
pub fn open_registry(path: &Path) -> Option<SourceRegistryHandle> {
    match SourceRegistryHandle::open(path) {
        Ok(handle) => {
            tracing::event!(tracing::Level::INFO, "supervisor.source_registry.ready");
            Some(handle)
        }
        Err(e) => {
            tracing::event!(
                tracing::Level::WARN,
                error = %e,
                detail = "source registry disabled",
                "supervisor.source_registry.failed"
            );
            None
        }
    }
}

/// Whether every entry of `entries` is a kind named by its token (`profile`,
/// `memory`), so that the list can be checked without the registry.
pub fn only_token_kinds(entries: &[String]) -> bool {
    entries
        .iter()
        .all(|entry| SourceKind::parse(entry).is_ok_and(SourceKind::listed_by_token))
}

/// The sources `agent` has checked, read from the `agents.db` of `data_dir`
/// against `registry`; `None` when `agents.db` does not hold the agent.
///
/// # Errors
///
/// [`AgentSourcesError::Agents`] when `agents.db` cannot be read,
/// [`AgentSourcesError::Sources`] when the registry cannot answer.
pub async fn checked_sources(
    data_dir: &Path,
    registry: &SourceRegistryHandle,
    agent: &str,
) -> Result<Option<Vec<CheckedSource>>, AgentSourcesError> {
    let path = DataFile::Agents.path(data_dir);
    let name = agent.to_owned();
    let resolved = tokio::task::spawn_blocking(move || {
        AgentRepository::open(&path)?.resolved(
            &name,
            None,
            crate::agents::global_default_autonomy(),
        )
    })
    .await
    .map_err(|e| AgentSourcesError::Interrupted(e.to_string()))??;
    let Some(resolved) = resolved else {
        return Ok(None);
    };
    Ok(Some(registry.checked(agent, resolved.sources).await?))
}

/// The `code_context` tool of `agent`, bound to the code folders it has
/// checked, read from the `agents.db` and `system.db` of `data_dir`; `None`
/// when it has none.
///
/// A code folder is a source of kind `code_folder` whose reference names a
/// folder connection; one whose connection is gone is left out with a trace.
/// Any store that cannot be read answers `None`: a code folder is never
/// served on a guess. The reads are blocking.
pub fn code_context_for(data_dir: &Path, agent: &str) -> Option<CodeContext> {
    let folders = match code_folders(data_dir, agent) {
        Ok(folders) => folders
            .into_iter()
            .map(|(folder, _)| folder)
            .collect::<Vec<_>>(),
        Err(error) => {
            tracing::event!(
                tracing::Level::WARN,
                agent = %agent,
                error = %error,
                "sources.code_folders.unreadable"
            );
            return None;
        }
    };
    (!folders.is_empty()).then(|| CodeContext::new(folders))
}

/// The code folders `agent` has checked, read from the `agents.db` and
/// `system.db` of `data_dir`, each with the identifier of the folder
/// connection that holds it (`folder:<id>`). The reads are blocking.
///
/// # Errors
///
/// What a store that cannot be read says.
pub fn code_folders(data_dir: &Path, agent: &str) -> Result<Vec<(CodeFolder, String)>, String> {
    let agents = DataFile::Agents.path(data_dir);
    let system = DataFile::System.path(data_dir);
    if !agents.exists() || !system.exists() {
        return Ok(Vec::new());
    }
    let resolved = AgentRepository::open(&agents)
        .and_then(|repo| repo.resolved(agent, None, crate::agents::global_default_autonomy()))
        .map_err(|e| e.to_string())?;
    let Some(resolved) = resolved else {
        return Ok(Vec::new());
    };
    let registry = SourceRegistryStore::open(&system).map_err(|e| e.to_string())?;
    let connections = FolderConnectionStore::open(&system).map_err(|e| e.to_string())?;
    let mut folders = Vec::new();
    for entry in &resolved.sources {
        let Some(source) = registry.get(entry).map_err(|e| e.to_string())? else {
            continue;
        };
        if source.kind != SourceKind::CodeFolder {
            continue;
        }
        let connection = match source.reference.as_deref() {
            Some(id) => connections.get(id).map_err(|e| e.to_string())?,
            None => None,
        };
        match connection {
            Some(connection) => folders.push((
                CodeFolder {
                    label: source.label,
                    path: connection.path.into(),
                },
                crate::connections::folder_connection_id(&connection.id),
            )),
            None => tracing::event!(
                tracing::Level::WARN,
                agent = %agent,
                source = %source.id,
                "sources.code_folder.connection_missing"
            ),
        }
    }
    Ok(folders)
}

#[cfg(test)]
mod tests;
