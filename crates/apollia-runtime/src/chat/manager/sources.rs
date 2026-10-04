//! The sources a turn may consult: those checked for the agent of the run.
//!
//! An agent consults a source through a tool it calls, never through its
//! prompt (principle 6). `memory_search` is offered and registered only when
//! the agent's memory is checked, `source_search` only when an indexable
//! source is, bounded to those. The agent's resolved source list is read at
//! every turn, so a box checked or cleared takes effect on the next message.

use apollia_tools::SourceAccess;

use super::{ChatToolsConfig, APOLLIA_CHAT_AGENT_ID};

/// The memory namespace of the Apollia agent: one space for the agent, kept
/// across its conversations, rather than one per conversation that nothing
/// wrote.
pub(in crate::chat::manager) const APOLLIA_MEMORY_NAMESPACE: &str = "apollia";

/// The memory namespace `memory_search` reads for `agent_id` in the session
/// `session_id`: the stable space of Apollia, or the space of the session for
/// another agent.
pub(in crate::chat::manager) fn memory_namespace(agent_id: &str, session_id: &str) -> String {
    if agent_id == APOLLIA_CHAT_AGENT_ID {
        APOLLIA_MEMORY_NAMESPACE.to_owned()
    } else {
        format!("apollia:chat:{session_id}")
    }
}

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
        Ok(Some(checked)) => SourceAccess::from_checked(&checked, Some(registry)),
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

/// `available` without the source tools `access` leaves out, so that the
/// model is offered no tool the dispatcher does not register.
pub(in crate::chat::manager) fn offered(
    available: Vec<String>,
    access: &SourceAccess,
) -> Vec<String> {
    let withheld = access.withheld_tools();
    available
        .into_iter()
        .filter(|name| !withheld.contains(&name.as_str()))
        .collect()
}

#[cfg(test)]
mod tests;
