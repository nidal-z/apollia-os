//! Which profile an execution runs with.
//!
//! An agent may carry a designated profile: the `profile` override of
//! `agent_overrides` in `agents.db`, written like any other override. At
//! execution, the designated profile is read from there; when the agent
//! designates none, or designates a profile deleted since, the default profile
//! of `user_memory.db` applies.

use std::path::Path;

use apollia_memory::user_memory::{Profile, UserMemoryError, UserMemoryRepository};
use apollia_tools::{AgentRepository, OverrideField};

/// Identifier of the profile the user designated for `agent_id`, read from the
/// `agents.db` of `data_dir`. `None` when the agent designates none, and when
/// `agents.db` does not exist or cannot be read: a profile designation never
/// stops an agent from running, the default profile applies instead.
fn designated_profile_id(data_dir: &Path, agent_id: &str) -> Option<String> {
    let path = apollia_core::paths::DataFile::Agents.path(data_dir);
    if !path.exists() {
        return None;
    }
    let overrides = AgentRepository::open(&path).and_then(|repo| repo.overrides(agent_id));
    match overrides {
        Ok(overrides) => overrides
            .into_iter()
            .find(|o| o.field == OverrideField::Profile)
            .and_then(|o| o.value.as_str().map(str::trim).map(str::to_owned))
            .filter(|id| !id.is_empty()),
        Err(e) => {
            tracing::event!(
                tracing::Level::WARN,
                agent = %agent_id,
                error = %e,
                "profile.resolve.designation_unreadable"
            );
            None
        }
    }
}

/// The profile an execution of `agent_id` runs with: the one designated for
/// the agent if it still exists, otherwise the default profile.
///
/// # Errors
///
/// [`UserMemoryError`] when `user_memory.db` cannot be read.
pub fn profile_for_agent(
    repo: &UserMemoryRepository,
    data_dir: &Path,
    agent_id: &str,
) -> Result<Profile, UserMemoryError> {
    let designated = designated_profile_id(data_dir, agent_id);
    repo.profile_or_default(designated.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_tools::{AgentOrigin, StoredAgent};

    fn store_agent(data_dir: &Path, name: &str) -> AgentRepository {
        let repo = AgentRepository::open(&apollia_core::paths::DataFile::Agents.path(data_dir))
            .expect("agents.db opens");
        repo.save_definition(&StoredAgent {
            name: name.to_owned(),
            origin: AgentOrigin::Interface,
            package: None,
            version: "1".to_owned(),
            definition: serde_json::json!({ "id": name, "name": name, "mission": "" }),
            enabled: true,
            installed_at: "2026-10-03T00:00:00Z".to_owned(),
            updated_at: "2026-10-03T00:00:00Z".to_owned(),
        })
        .expect("agent stored");
        repo
    }

    fn user_memory(data_dir: &Path) -> UserMemoryRepository {
        UserMemoryRepository::new(&apollia_core::paths::DataFile::UserMemory.path(data_dir))
            .expect("user_memory.db opens")
    }

    #[test]
    fn an_agent_designated_formateur_runs_with_formateur_and_another_with_the_default() {
        // GIVEN a default profile, a "Formateur" profile, an agent designating it and one designating none
        let dir = tempfile::tempdir().expect("tempdir");
        let profiles = user_memory(dir.path());
        let default = profiles.default_profile().expect("default profile");
        let formateur = profiles.create_profile("Formateur").expect("created");
        let agents = store_agent(dir.path(), "cours");
        store_agent(dir.path(), "veille");
        agents
            .set_override(
                "cours",
                OverrideField::Profile,
                &serde_json::json!(formateur.id),
            )
            .expect("override set");

        // WHEN each agent's profile is resolved
        let for_cours = profile_for_agent(&profiles, dir.path(), "cours").expect("resolved");
        let for_veille = profile_for_agent(&profiles, dir.path(), "veille").expect("resolved");

        // THEN the designated agent runs with "Formateur", the other with the default
        assert_eq!(for_cours.name, "Formateur");
        assert_eq!(for_cours.id, formateur.id);
        assert_eq!(for_veille.id, default.id);
    }

    #[test]
    fn a_designation_of_a_deleted_profile_reads_as_the_default() {
        // GIVEN an agent designating a profile that is then deleted
        let dir = tempfile::tempdir().expect("tempdir");
        let profiles = user_memory(dir.path());
        let default = profiles.default_profile().expect("default profile");
        let gone = profiles.create_profile("Formateur").expect("created");
        let agents = store_agent(dir.path(), "cours");
        agents
            .set_override("cours", OverrideField::Profile, &serde_json::json!(gone.id))
            .expect("override set");
        profiles.delete_profile(&gone.id).expect("deleted");

        // WHEN the agent's profile is resolved
        let resolved = profile_for_agent(&profiles, dir.path(), "cours").expect("resolved");

        // THEN the orphaned designation reads as empty: the default applies
        assert_eq!(resolved.id, default.id);
    }

    #[test]
    fn without_agents_db_the_default_profile_applies() {
        // GIVEN a data directory holding no agents.db
        let dir = tempfile::tempdir().expect("tempdir");
        let profiles = user_memory(dir.path());

        // WHEN an agent's profile is resolved
        let resolved = profile_for_agent(&profiles, dir.path(), "any").expect("resolved");

        // THEN the default applies and no agents.db is created as a side effect
        assert!(resolved.is_default);
        assert!(!apollia_core::paths::DataFile::Agents
            .path(dir.path())
            .exists());
    }
}
