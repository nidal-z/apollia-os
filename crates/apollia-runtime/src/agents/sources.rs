//! The sources an agent has checked, read in the runtime.
//!
//! An agent's source list resolves like every other field of its definition:
//! the package default, then the user's `sources` override in `agents.db`,
//! which replaces it whole. The same resolution answers whether the agent may
//! read the profile (`crate::profile::source`); this module reads the list
//! against the registry of sources, owned by its actor.

use std::path::Path;

use apollia_core::paths::DataFile;
use apollia_sources::{CheckedSource, SourceKind, SourceRegistryHandle, SourcesError};
use apollia_tools::{AgentRepository, AgentRepositoryError};

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
