//! What an agent may consult, as its native tools read it.
//!
//! A source is consulted by a tool the agent calls, never by an injection: the
//! memory by `memory_search`, the indexed folders and files by
//! `source_search`. Each tool is registered only when its source is checked
//! for the resolved agent, and `source_search` is bounded to the sources
//! checked. [`SourceAccess`] carries that answer to the dispatcher factory.

use std::path::Path;

use apollia_core::paths::DataFile;
use apollia_core::AutonomyLevel;
use apollia_sources::{searchable_sources, CheckedSource, SourceKind, SourceRegistryHandle};

use crate::tools::source_search::{SourceSearchTool, SOURCE_SEARCH_TOOL};
use crate::AgentRepository;

/// Name of the memory search tool.
pub const MEMORY_SEARCH_TOOL: &str = "memory_search";

/// The sources an agent may consult through its native tools.
///
/// The default checks nothing: neither `memory_search` nor `source_search`
/// is registered.
#[derive(Debug, Clone, Default)]
pub struct SourceAccess {
    /// Whether the agent's memory is checked: `memory_search` is registered
    /// only then.
    pub memory: bool,
    /// The search over the indexable sources checked; `None` when none is,
    /// which leaves `source_search` out.
    pub search: Option<SourceSearchTool>,
}

impl SourceAccess {
    /// The access the checked sources `checked` give, searched through
    /// `registry`. Without a registry no source can be searched; the memory
    /// stays reachable.
    #[must_use]
    pub fn from_checked(
        checked: &[CheckedSource],
        registry: Option<&SourceRegistryHandle>,
    ) -> Self {
        let searchable = searchable_sources(checked);
        let search = match registry {
            Some(registry) if !searchable.is_empty() => {
                Some(SourceSearchTool::new(registry.clone(), searchable))
            }
            _ => None,
        };
        Self {
            memory: checked.contains(&CheckedSource::Memory),
            search,
        }
    }

    /// The access of an agent whose only possible source is its memory.
    #[must_use]
    pub fn memory_only(memory: bool) -> Self {
        Self {
            memory,
            search: None,
        }
    }

    /// The source tools this access leaves out, so that they are offered to
    /// the model no more than they are registered.
    #[must_use]
    pub fn withheld_tools(&self) -> Vec<&'static str> {
        let mut withheld = Vec::new();
        if !self.memory {
            withheld.push(MEMORY_SEARCH_TOOL);
        }
        if self.search.is_none() {
            withheld.push(SOURCE_SEARCH_TOOL);
        }
        withheld
    }
}

/// Whether `agent`'s resolved source list, read from the `agents.db` of
/// `data_dir`, checks its memory. An agent `agents.db` does not hold, or a
/// file that cannot be read, checks nothing.
#[must_use]
pub fn memory_is_checked(data_dir: &Path, agent: &str) -> bool {
    let path = DataFile::Agents.path(data_dir);
    if !path.exists() {
        return false;
    }
    // The tier plays no part in the source list; any one resolves it.
    let resolved = AgentRepository::open(&path)
        .and_then(|repo| repo.resolved(agent, None, AutonomyLevel::Assisted));
    match resolved {
        Ok(Some(agent)) => agent
            .sources
            .iter()
            .any(|entry| SourceKind::parse(entry).is_ok_and(|kind| kind == SourceKind::Memory)),
        Ok(None) => false,
        Err(e) => {
            tracing::event!(
                tracing::Level::WARN,
                agent = %agent,
                error = %e,
                "tools.sources.unreadable"
            );
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_sources::{IndexState, Source};

    fn registered(id: &str, kind: SourceKind) -> CheckedSource {
        CheckedSource::Registered(Source {
            id: id.to_owned(),
            kind,
            label: id.to_owned(),
            group: None,
            reference: Some("ref".to_owned()),
            index_state: IndexState::Absent,
            added_at: "2026-10-04T00:00:00.000Z".to_owned(),
        })
    }

    #[tokio::test]
    async fn test_an_indexable_source_checked_opens_the_search_bounded_to_it() {
        // GIVEN a registry, and an agent that checked a folder, a mailbox
        //       and its profile
        let dir = tempfile::tempdir().unwrap();
        let registry = SourceRegistryHandle::open(&dir.path().join("system.db")).unwrap();
        let checked = vec![
            registered("src-folder", SourceKind::Folder),
            registered("src-mail", SourceKind::Mail),
            CheckedSource::Profile,
        ];

        // WHEN its access is read
        let access = SourceAccess::from_checked(&checked, Some(&registry));

        // THEN the search is open on the folder alone, and the memory closed
        let search = access.search.as_ref().unwrap();
        assert_eq!(search.sources(), ["src-folder".to_owned()]);
        assert!(!access.memory);
        assert_eq!(access.withheld_tools(), vec![MEMORY_SEARCH_TOOL]);
    }

    #[tokio::test]
    async fn test_no_indexable_source_checked_leaves_the_search_out() {
        // GIVEN a registry, and an agent that checked only its memory
        let dir = tempfile::tempdir().unwrap();
        let registry = SourceRegistryHandle::open(&dir.path().join("system.db")).unwrap();

        // WHEN its access is read, with and without a registry
        let with = SourceAccess::from_checked(&[CheckedSource::Memory], Some(&registry));
        let without =
            SourceAccess::from_checked(&[registered("src-folder", SourceKind::Folder)], None);

        // THEN the memory is open, and no search exists in either case
        assert!(with.memory);
        assert!(with.search.is_none());
        assert_eq!(with.withheld_tools(), vec![SOURCE_SEARCH_TOOL]);
        assert!(without.search.is_none());
    }

    #[test]
    fn test_the_memory_is_checked_only_when_the_resolved_list_names_it() {
        // GIVEN an agents.db where Apollia's override checks its memory
        let dir = tempfile::tempdir().unwrap();
        let repo = AgentRepository::open(&DataFile::Agents.path(dir.path())).unwrap();
        let set = |sources: &[&str]| {
            repo.set_override(
                crate::APOLLIA_AGENT_ID,
                crate::OverrideField::Sources,
                &serde_json::json!(sources),
            )
            .unwrap();
        };
        set(&["memory", "profile"]);

        // WHEN it is asked, then asked again once the override names the
        //      profile alone, and asked for an agent the file does not hold
        let checked = memory_is_checked(dir.path(), crate::APOLLIA_AGENT_ID);
        set(&["profile"]);
        let unchecked = memory_is_checked(dir.path(), crate::APOLLIA_AGENT_ID);
        let unknown = memory_is_checked(dir.path(), "absent");

        // THEN the memory is checked in the first case only
        assert!(checked);
        assert!(!unchecked);
        assert!(!unknown);
    }
}
