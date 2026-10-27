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

/// Name of the memory write tool.
pub const MEMORY_WRITE_TOOL: &str = "memory_write";

/// Name of the profile proposal tool.
pub const PROFILE_PROPOSE_TOOL: &str = "profile_propose";

/// Where an accepted fact for the profile is written, and in whose name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileStore {
    /// The database of the profiles (`user_memory.db`).
    pub database: std::path::PathBuf,
    /// The agent that proposes, recorded as the author of the fact.
    pub author: String,
}

/// `tools` with the profile proposal tool present exactly when the agent's
/// profile source is checked and its store is known: the tool is granted by
/// the source, never by the access.
#[must_use]
pub fn granted_with_profile(mut tools: Vec<String>, profile_checked: bool) -> Vec<String> {
    tools.retain(|t| t != PROFILE_PROPOSE_TOOL);
    if profile_checked {
        tools.push(PROFILE_PROPOSE_TOOL.to_owned());
    }
    tools
}

/// `tools`, the tools an agent's access grants, with the two memory tools
/// present exactly when the agent's memory is checked.
///
/// The memory is granted by the source, never by the access: an access that
/// names a memory tool without the memory checked does not keep it, and the
/// memory checked adds both to an access that names neither. Checking Memory
/// on the sheet of an agent is the one thing that opens its memory, to read
/// and to write.
#[must_use]
pub fn granted_with_memory(mut tools: Vec<String>, memory_checked: bool) -> Vec<String> {
    tools.retain(|t| t != MEMORY_SEARCH_TOOL && t != MEMORY_WRITE_TOOL);
    if memory_checked {
        tools.push(MEMORY_SEARCH_TOOL.to_owned());
        tools.push(MEMORY_WRITE_TOOL.to_owned());
    }
    tools
}

/// The sources an agent may consult through its native tools.
///
/// The default checks nothing: neither `memory_search` nor `source_search`
/// is registered.
#[derive(Debug, Clone, Default)]
pub struct SourceAccess {
    /// Whether the agent's memory is checked: `memory_search` and
    /// `memory_write` are registered only then.
    pub memory: bool,
    /// The search over the indexable sources checked; `None` when none is,
    /// which leaves `source_search` out.
    pub search: Option<SourceSearchTool>,
    /// Whether the profile is checked for the agent.
    pub profile_checked: bool,
    /// The proposal of facts for the profile; `None` when the profile is not
    /// checked or its store is unknown, which leaves `profile_propose` out.
    pub profile: Option<ProfileStore>,
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
            profile_checked: checked.contains(&CheckedSource::Profile),
            profile: None,
        }
    }

    /// The same access, where an agent whose profile is checked may propose
    /// facts for the profiles of `database`, in the name of `author`.
    #[must_use]
    pub fn with_profile_store(mut self, database: std::path::PathBuf, author: &str) -> Self {
        self.profile = self.profile_checked.then(|| ProfileStore {
            database,
            author: author.to_owned(),
        });
        self
    }

    /// The same access, where the sources `paused` names, held by a
    /// connection the user paused, are answered paused by the search instead
    /// of searched.
    #[must_use]
    pub fn with_paused_sources(mut self, paused: Vec<String>) -> Self {
        self.search = self.search.map(|search| search.with_paused(paused));
        self
    }

    /// The same access, where the search reads from `live`, at each call,
    /// which of its sources a paused connection holds
    /// ([`SourceSearchTool::with_live_pauses`]).
    #[must_use]
    pub fn with_live_source_pauses(mut self, live: crate::LiveSourcePauses) -> Self {
        self.search = self.search.map(|search| search.with_live_pauses(live));
        self
    }

    /// The access of an agent whose only possible source is its memory.
    #[must_use]
    pub fn memory_only(memory: bool) -> Self {
        Self {
            memory,
            search: None,
            profile_checked: false,
            profile: None,
        }
    }

    /// The source tools this access leaves out, so that they are offered to
    /// the model no more than they are registered.
    #[must_use]
    pub fn withheld_tools(&self) -> Vec<&'static str> {
        let mut withheld = Vec::new();
        if !self.memory {
            withheld.push(MEMORY_SEARCH_TOOL);
            withheld.push(MEMORY_WRITE_TOOL);
        }
        if self.search.is_none() {
            withheld.push(SOURCE_SEARCH_TOOL);
        }
        if self.profile.is_none() {
            withheld.push(PROFILE_PROPOSE_TOOL);
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
        assert_eq!(
            access.withheld_tools(),
            vec![MEMORY_SEARCH_TOOL, MEMORY_WRITE_TOOL, PROFILE_PROPOSE_TOOL]
        );
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
        assert_eq!(
            with.withheld_tools(),
            vec![SOURCE_SEARCH_TOOL, PROFILE_PROPOSE_TOOL]
        );
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
