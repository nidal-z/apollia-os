//! An agent's source list, read against the registry.

use apollia_core::source_registry::{Source, SourceKind};

/// A source an agent has checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckedSource {
    /// The user profile, named by its token.
    Profile,
    /// The agent's memory, named by its token.
    Memory,
    /// A source of the registry, named by its identifier.
    Registered(Source),
}

/// The sources `entries` names, in their order, each once.
///
/// `entries` is the agent's resolved source list: the package default, or the
/// user's override that replaces it. An entry is `profile`, `memory`, or the
/// identifier of a source of `registry`. Any other entry (a name a package
/// declares, a source removed since) is left out, with a trace: what an agent
/// may consult is never guessed.
pub fn checked_sources(agent: &str, entries: &[String], registry: &[Source]) -> Vec<CheckedSource> {
    let mut checked: Vec<CheckedSource> = Vec::new();
    for entry in entries {
        let source = match SourceKind::parse(entry) {
            Ok(SourceKind::Profile) => Some(CheckedSource::Profile),
            Ok(SourceKind::Memory) => Some(CheckedSource::Memory),
            _ => registry
                .iter()
                .find(|source| source.id == *entry)
                .cloned()
                .map(CheckedSource::Registered),
        };
        match source {
            Some(source) if !checked.contains(&source) => checked.push(source),
            Some(_) => {}
            None => tracing::event!(
                tracing::Level::WARN,
                agent = %agent,
                entry = %entry,
                "sources.unknown_ignored"
            ),
        }
    }
    checked
}

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_core::source_registry::IndexState;

    fn folder(id: &str) -> Source {
        Source {
            id: id.to_owned(),
            kind: SourceKind::Folder,
            label: "Contracts".to_owned(),
            group: None,
            reference: Some("conn".to_owned()),
            index_state: IndexState::Absent,
            added_at: "2026-10-03T00:00:00.000Z".to_owned(),
        }
    }

    fn entries(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn test_known_entries_are_kept_in_order_and_unknown_names_left_out() {
        // GIVEN a registry with one folder, and a list mixing it with the two
        // token kinds, a package name, a duplicate and a bare kind
        let registry = vec![folder("src-1")];
        let list = entries(&["memory", "documents", "src-1", "profile", "src-1", "folder"]);

        // WHEN the list is read against the registry
        let checked = checked_sources("veille", &list, &registry);

        // THEN the known entries remain once each, in order, and the others are dropped
        assert_eq!(
            checked,
            vec![
                CheckedSource::Memory,
                CheckedSource::Registered(folder("src-1")),
                CheckedSource::Profile,
            ]
        );
    }

    #[test]
    fn test_an_empty_list_checks_nothing() {
        // GIVEN a registry with one folder and an empty list
        let registry = vec![folder("src-1")];

        // WHEN the list is read
        let checked = checked_sources("veille", &[], &registry);

        // THEN nothing is checked, the registry alone opens nothing
        assert!(checked.is_empty());
    }
}
