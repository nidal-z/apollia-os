//! Whether an agent may read the user profile.
//!
//! The profile is a source of the agent, like the others its definition
//! lists. Its state resolves in two levels: the `sources` default its package
//! declares (`[agents.defaults] sources`), then the user's `sources` override
//! stored in `agents.db`, which replaces the whole list. The profile is
//! checked when the resolved list holds [`PROFILE_SOURCE`]; when neither level
//! says anything, it is unchecked and `ctx.profile` is `None`.

use std::path::Path;

use apollia_tools::apollia_manifest::PROFILE_SOURCE;
use apollia_tools::{AgentOverride, AgentRepository, OverrideField, StoredAgent};
use serde_json::Value;

/// Whether the source list `stored` declares, replaced by the user's
/// `sources` override when there is one, holds the profile.
pub fn profile_source_checked_in(stored: &StoredAgent, overrides: &[AgentOverride]) -> bool {
    let sources = overrides
        .iter()
        .find(|o| o.field == OverrideField::Sources)
        .map(|o| &o.value)
        .or_else(|| stored.definition.pointer("/defaults/sources"));
    sources
        .and_then(Value::as_array)
        .is_some_and(|items| items.iter().any(|v| v.as_str() == Some(PROFILE_SOURCE)))
}

/// Whether `agent_id` may read the user profile, read from the `agents.db`
/// of `data_dir`.
///
/// An agent `agents.db` does not hold, an absent `agents.db`, and one that
/// cannot be read all answer `false`: the profile is never served on a
/// guess.
pub fn profile_source_checked(data_dir: &Path, agent_id: &str) -> bool {
    let path = apollia_core::paths::DataFile::Agents.path(data_dir);
    if !path.exists() {
        return false;
    }
    let read = AgentRepository::open(&path).and_then(|repo| {
        let stored = repo.stored(agent_id)?;
        let overrides = repo.overrides(agent_id)?;
        Ok(stored.map(|stored| profile_source_checked_in(&stored, &overrides)))
    });
    match read {
        Ok(checked) => checked.unwrap_or(false),
        Err(e) => {
            tracing::event!(
                tracing::Level::WARN,
                agent = %agent_id,
                error = %e,
                "profile.source.unreadable"
            );
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_tools::AgentOrigin;
    use serde_json::json;

    fn stored(definition: Value) -> StoredAgent {
        StoredAgent {
            name: "cours".to_owned(),
            origin: AgentOrigin::Package,
            package: Some("formation".to_owned()),
            version: "1".to_owned(),
            definition,
            enabled: true,
            installed_at: "2026-10-03T00:00:00Z".to_owned(),
            updated_at: "2026-10-03T00:00:00Z".to_owned(),
        }
    }

    fn sources_override(items: &[&str]) -> AgentOverride {
        AgentOverride {
            field: OverrideField::Sources,
            value: json!(items),
        }
    }

    #[test]
    fn the_profile_source_resolves_package_default_then_user_override() {
        // GIVEN a definition declaring the profile source and one declaring none
        let declared = stored(json!({ "id": "cours", "defaults": { "sources": ["profile"] } }));
        let silent = stored(json!({ "id": "cours", "defaults": {} }));
        let checks = [sources_override(&["profile"])];
        let unchecks = [sources_override(&["crm"])];

        // WHEN the four combinations of declaration and override are resolved
        let neither = profile_source_checked_in(&silent, &[]);
        let declared_only = profile_source_checked_in(&declared, &[]);
        let declared_then_unchecked = profile_source_checked_in(&declared, &unchecks);
        let checked_by_user = profile_source_checked_in(&silent, &checks);

        // THEN only the declaration or the user's check opens it, and the user's list wins
        assert!(!neither, "no declaration, no override: unchecked");
        assert!(declared_only, "package default applies without an override");
        assert!(
            !declared_then_unchecked,
            "the user's override replaces the package default"
        );
        assert!(checked_by_user, "the user's override checks it");
    }

    #[test]
    fn an_agent_agents_db_does_not_hold_cannot_read_the_profile() {
        // GIVEN a data directory without agents.db, then one holding an agent declaring the source
        let dir = tempfile::tempdir().expect("tempdir");
        let before = profile_source_checked(dir.path(), "cours");
        let repo = AgentRepository::open(&apollia_core::paths::DataFile::Agents.path(dir.path()))
            .expect("agents.db opens");
        repo.save_definition(&stored(
            json!({ "id": "cours", "defaults": { "sources": ["profile"] } }),
        ))
        .expect("agent stored");

        // WHEN the stored agent and an unknown one are checked
        let cours = profile_source_checked(dir.path(), "cours");
        let ghost = profile_source_checked(dir.path(), "ghost");

        // THEN only the stored agent that declares the source may read the profile
        assert!(!before);
        assert!(cours);
        assert!(!ghost);
    }
}
