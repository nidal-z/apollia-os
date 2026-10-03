//! One-time migration of the single global profile into the `profiles` table.
//!
//! Before profiles existed, the profile was sixteen flat keys of the `__user__`
//! semantic namespace. The first open of `user_memory.db` by this binary turns
//! them into the default profile, `Mon profil`: four keys land in the field
//! that replaces them, the twelve others are copied as `key: value` lines of
//! the free text, so no value is lost. The sixteen keys then leave the
//! namespace; every other key there (facts written by agents, free entries)
//! stays where it is.
//!
//! The migration runs only while the table is empty, inside one transaction,
//! so a second open finds a profile and does nothing.

use rusqlite::Connection;

use crate::semantic::SemanticMemory;
use crate::store::MemoryStore;

use super::profiles::{self, DEFAULT_PROFILE_NAME};
use super::{UserMemoryError, USER_NAMESPACE};

/// The sixteen keys of the former profile schema, in its order, each with the
/// field of the new schema that receives it, or `None` when its value is
/// copied into the free text.
const LEGACY_SCHEMA_KEYS: [(&str, Option<&str>); 16] = [
    ("name", Some("name")),
    ("role", Some("role")),
    ("goals", None),
    ("domain.sector", None),
    ("domain.team_size", None),
    ("tech.stack", None),
    ("tech.proficiency", None),
    ("tools.daily", Some("tools")),
    ("tech.integrations", None),
    ("preferences.language", Some("preferences")),
    ("preferences.llm", None),
    ("agents.hitl", None),
    ("agents.domains", None),
    ("agents.trigger", None),
    ("constraints.sovereignty", None),
    ("constraints.compliance", None),
];

fn storage(e: impl std::fmt::Display) -> UserMemoryError {
    UserMemoryError::StorageError(e.to_string())
}

/// What one run of the migration did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MigrationOutcome {
    /// A profile already existed: nothing was touched.
    AlreadyDone,
    /// The default profile was created, carrying `moved` former keys.
    Migrated {
        /// Number of former schema keys moved into the profile.
        moved: usize,
    },
}

/// Creates the default profile from the former `__user__` keys when the
/// `profiles` table is empty. Idempotent.
pub(crate) fn run(store: &MemoryStore) -> Result<MigrationOutcome, UserMemoryError> {
    let conn: &Connection = store.conn();
    if profiles::count(conn)? > 0 {
        return Ok(MigrationOutcome::AlreadyDone);
    }

    let tx = conn.unchecked_transaction().map_err(storage)?;
    let profile = profiles::insert(conn, DEFAULT_PROFILE_NAME, true)?;
    let sem = SemanticMemory::new(store);

    let mut free_text_lines: Vec<String> = Vec::new();
    let mut moved = 0usize;
    for (legacy_key, target) in LEGACY_SCHEMA_KEYS {
        let Some(entry) = sem.recall(USER_NAMESPACE, legacy_key).map_err(storage)? else {
            continue;
        };
        let value = entry
            .value
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| entry.value.to_string());
        match target {
            Some(field) => profiles::set_field(conn, &profile.id, field, Some(&value))?,
            None => free_text_lines.push(format!("{legacy_key}: {value}")),
        }
        sem.forget(USER_NAMESPACE, legacy_key).map_err(storage)?;
        moved += 1;
    }
    if !free_text_lines.is_empty() {
        profiles::set_field(
            conn,
            &profile.id,
            crate::profile_schema::FREE_TEXT_KEY,
            Some(&free_text_lines.join("\n")),
        )?;
    }
    tx.commit().map_err(storage)?;

    tracing::event!(
        tracing::Level::INFO,
        profile_id = %profile.id,
        moved,
        "user_memory.profile_migrated"
    );
    Ok(MigrationOutcome::Migrated { moved })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::semantic::RememberInput;
    use crate::user_memory::UserMemoryRepository;

    /// The sixteen former keys with a distinct value each.
    const SAMPLE: [(&str, &str); 16] = [
        ("name", "Ada"),
        ("role", "Analyste"),
        ("goals", "Automatiser le reporting"),
        ("domain.sector", "fintech"),
        ("domain.team_size", "small team"),
        ("tech.stack", "Python, SQL"),
        ("tech.proficiency", "avance"),
        ("tools.daily", "tableur, Notion"),
        ("tech.integrations", "Gmail"),
        ("preferences.language", "fr"),
        ("preferences.llm", "qwen3"),
        ("agents.hitl", "critical-only"),
        ("agents.domains", "veille"),
        ("agents.trigger", "cron"),
        ("constraints.sovereignty", "local-only"),
        ("constraints.compliance", "RGPD"),
    ];

    /// Writes a pre-profiles `user_memory.db`: the sixteen keys, two facts
    /// written by agents, and one onboarding marker.
    fn legacy_db(dir: &std::path::Path) -> std::path::PathBuf {
        let path = dir.join("user_memory.db");
        let store = MemoryStore::open(&path).unwrap();
        let sem = SemanticMemory::new(&store);
        let write = |key: &str, value: &str, source: &str| {
            sem.remember(RememberInput {
                namespace: USER_NAMESPACE,
                key,
                value: &serde_json::Value::String(value.to_owned()),
                confidence: 1.0,
                source: Some(source),
                expires_at: None,
            })
            .unwrap();
        };
        for (key, value) in SAMPLE {
            write(key, value, "onboarding");
        }
        write("favorite_editor", "vim", "agent:veille-ia");
        write("timezone", "Europe/Paris", "agent:planner");
        write("__onboarding_skipped", "true", "onboarding");
        path
    }

    fn agent_fact_count(path: &std::path::Path) -> usize {
        let store = MemoryStore::open(path).unwrap();
        SemanticMemory::new(&store)
            .recall_all(USER_NAMESPACE, None)
            .unwrap()
            .into_iter()
            .filter(|e| e.source.as_deref().is_some_and(|s| s.starts_with("agent:")))
            .count()
    }

    #[test]
    fn sixteen_keys_become_the_default_profile_without_losing_a_value() {
        // GIVEN a database holding the sixteen former keys
        let dir = tempfile::tempdir().unwrap();
        let path = legacy_db(dir.path());

        // WHEN the repository opens it
        let repo = UserMemoryRepository::new(&path).unwrap();
        let profile = repo.default_profile().unwrap();

        // THEN the default profile is « Mon profil », four fields carry their value
        assert_eq!(profile.name, DEFAULT_PROFILE_NAME);
        assert!(profile.is_default);
        assert_eq!(profile.person_name.as_deref(), Some("Ada"));
        assert_eq!(profile.role.as_deref(), Some("Analyste"));
        assert_eq!(profile.tools.as_deref(), Some("tableur, Notion"));
        assert_eq!(profile.preferences.as_deref(), Some("fr"));
        assert_eq!(profile.function, None);
        // AND every other value is a `key: value` line of the free text
        let free_text = profile.free_text.unwrap();
        for (key, value) in SAMPLE {
            if matches!(
                key,
                "name" | "role" | "tools.daily" | "preferences.language"
            ) {
                continue;
            }
            let line = format!("{key}: {value}");
            assert!(
                free_text.lines().any(|l| l == line),
                "missing line {line:?} in {free_text:?}"
            );
        }
        assert_eq!(free_text.lines().count(), 12);
        // AND none of the sixteen keys is left as a free entry
        let keys: Vec<String> = repo
            .list_extras()
            .unwrap()
            .into_iter()
            .map(|e| e.key)
            .collect();
        assert_eq!(keys, vec!["favorite_editor", "timezone"]);
    }

    #[test]
    fn a_second_pass_changes_nothing() {
        // GIVEN a database already migrated once
        let dir = tempfile::tempdir().unwrap();
        let path = legacy_db(dir.path());
        let first = UserMemoryRepository::new(&path)
            .unwrap()
            .list_profiles()
            .unwrap();
        let store = MemoryStore::open(&path).unwrap();

        // WHEN the migration runs again, directly and through a second open
        let outcome = run(&store).unwrap();
        let second = UserMemoryRepository::new(&path)
            .unwrap()
            .list_profiles()
            .unwrap();

        // THEN it reports nothing to do and the stored profile is identical
        assert_eq!(outcome, MigrationOutcome::AlreadyDone);
        assert_eq!(first, second);
        assert_eq!(second.len(), 1);
    }

    #[test]
    fn agent_facts_stay_in_semantic_memory() {
        // GIVEN a former database holding two facts written by agents
        let dir = tempfile::tempdir().unwrap();
        let path = legacy_db(dir.path());
        let before = agent_fact_count(&path);

        // WHEN it is migrated
        let repo = UserMemoryRepository::new(&path).unwrap();

        // THEN the same number of agent facts is still in the semantic namespace
        assert_eq!(before, 2);
        assert_eq!(agent_fact_count(&path), before);
        // AND the onboarding marker survived
        assert!(repo.get_onboarding_skipped().unwrap());
    }

    #[test]
    fn a_fresh_database_gets_an_empty_default_profile() {
        // GIVEN no database at all
        let dir = tempfile::tempdir().unwrap();

        // WHEN the repository opens one
        let repo = UserMemoryRepository::new(&dir.path().join("user_memory.db")).unwrap();

        // THEN a blank default profile exists and the profile reads empty
        let profile = repo.default_profile().unwrap();
        assert!(profile.is_default);
        assert!(profile.is_blank());
        assert!(repo.is_empty().unwrap());
        assert!(repo.list_all().unwrap().is_empty());
    }
}
