//! The sources a turn may consult: those checked for the agent of the run.
//!
//! An agent consults a source through a tool it calls, never through its
//! prompt (principle 6). `memory_search` and `memory_write` are offered and
//! registered exactly when the agent's memory is checked, whatever its access
//! lists; `source_search` only when an indexable source is, bounded to those. The agent's resolved source list is read at
//! every turn, so a box checked or cleared takes effect on the next message.

use apollia_tools::SourceAccess;

use super::ChatToolsConfig;

pub(in crate::chat::manager) use apollia_tools::agent_definition::memory_namespace;

/// The sources `agent_id` has checked, read beside `cfg`'s data directory
/// and searched through its registry of sources. Without a configuration, or
/// when the list cannot be read, nothing is checked: no source tool is
/// offered.
pub(in crate::chat::manager) async fn source_access(
    cfg: Option<&ChatToolsConfig>,
    agent_id: &str,
) -> SourceAccess {
    let Some(cfg) = cfg else {
        return SourceAccess::default();
    };
    let Some(registry) = cfg.source_registry.as_ref() else {
        let data_dir = cfg.data_dir.clone();
        let agent = agent_id.to_owned();
        let memory = tokio::task::spawn_blocking(move || {
            apollia_tools::memory_is_checked(&data_dir, &agent)
        })
        .await
        .unwrap_or(false);
        return SourceAccess::memory_only(memory);
    };
    match crate::agents::sources::checked_sources(&cfg.data_dir, registry, agent_id).await {
        // An agent whose profile is checked may propose facts for it, each
        // one put to the person before it is written.
        Ok(Some(checked)) => SourceAccess::from_checked(&checked, Some(registry))
            .with_profile_store(
                apollia_core::paths::DataFile::UserMemory.path(&cfg.data_dir),
                &memory_namespace(agent_id),
            ),
        Ok(None) => SourceAccess::default(),
        Err(e) => {
            tracing::event!(
                tracing::Level::WARN,
                agent = %agent_id,
                error = %e,
                detail = "no source is consulted",
                "chat.sources.unreadable"
            );
            SourceAccess::default()
        }
    }
}

/// `available` as the sources `access` leave it, so that the model is offered
/// exactly the source tools the dispatcher registers: the memory tools when
/// the memory is checked and only then, `source_search` only when a source
/// can be searched.
pub(in crate::chat::manager) fn offered(
    available: Vec<String>,
    access: &SourceAccess,
) -> Vec<String> {
    let withheld = access.withheld_tools();
    let kept = available
        .into_iter()
        .filter(|name| !withheld.contains(&name.as_str()))
        .collect();
    let kept = apollia_tools::granted_with_memory(kept, access.memory);
    apollia_tools::granted_with_profile(kept, access.profile.is_some())
}

#[cfg(test)]
mod tests;
