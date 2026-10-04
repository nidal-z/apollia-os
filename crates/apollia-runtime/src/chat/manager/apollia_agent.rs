//! The settings of the Apollia agent, read for each free-chat session.
//!
//! The chat is the Apollia agent: its mission and its model are those of the
//! agent as resolved, its definition in `agents.db` then the user's
//! overrides, with the model assigned to it in `system.db`. Both files sit
//! beside `governance.db`, whose path is the one this actor is given.
//!
//! Its access list is the tools the chat offers: the agent's granted
//! accesses, nothing else; the reading family until the user chooses others
//! on its card. What runs without a question among them comes from
//! the standing consents and the allow rules.

use std::path::Path;

use apollia_core::paths::DataFile;
use apollia_tools::{AgentAccess, AgentRepository, APOLLIA_AGENT_ID};

use super::ChatLibreOverrides;

/// Applies the mission and the model of the resolved Apollia agent to `out`.
/// Silent fallback when `agents.db` is absent or unreadable: the chat runs on
/// the built-in prompt and the default model.
pub(in crate::chat::manager) fn apply_apollia_agent(
    out: &mut ChatLibreOverrides,
    governance_db: &Path,
) {
    let Some(data_dir) = governance_db.parent() else {
        return;
    };
    let agents_db = DataFile::Agents.path(data_dir);
    if !agents_db.exists() {
        return;
    }
    let model = read_model(&DataFile::System.path(data_dir));
    let resolved = AgentRepository::open(&agents_db).and_then(|repo| {
        repo.resolved(
            APOLLIA_AGENT_ID,
            model,
            crate::agents::global_default_autonomy(),
        )
    });
    let apollia = match resolved {
        Ok(Some(apollia)) => apollia,
        Ok(None) => return,
        Err(error) => {
            tracing::event!(
                tracing::Level::WARN,
                error = %error,
                detail = "the chat runs without the settings of Apollia",
                "chat.apollia.unreadable"
            );
            return;
        }
    };
    if !apollia.mission.trim().is_empty() {
        out.system_prompt = Some(apollia.mission);
    }
    if let Some(model) = apollia.model {
        out.llm_backend = Some(model.model_id);
    }
}

/// The access of the Apollia agent, read beside `governance_db`.
///
/// `None` only for a mount without a data directory, which has no agent
/// store: its sessions run on the tools they were created with. Without
/// `agents.db`, or without an access chosen on its card, Apollia is an agent
/// created without code and reads by default: the reading family active on
/// the machine. An unreadable store grants nothing.
pub(in crate::chat::manager) fn apollia_access(
    governance_db: Option<&Path>,
) -> Option<AgentAccess> {
    let data_dir = governance_db?.parent()?;
    let agents_db = DataFile::Agents.path(data_dir);
    if !agents_db.exists() {
        return Some(AgentAccess::ReadingFamily);
    }
    let resolved = AgentRepository::open(&agents_db).and_then(|repo| {
        repo.resolved(
            APOLLIA_AGENT_ID,
            None,
            crate::agents::global_default_autonomy(),
        )
    });
    match resolved {
        Ok(Some(apollia)) => Some(AgentAccess::of(&apollia)),
        Ok(None) => Some(AgentAccess::ReadingFamily),
        Err(error) => {
            tracing::event!(
                tracing::Level::WARN,
                error = %error,
                detail = "the chat is granted no tool",
                "chat.apollia.access.unreadable"
            );
            Some(AgentAccess::Listed(Vec::new()))
        }
    }
}

/// The model assigned to Apollia in the `system.db` at `path`, if any.
fn read_model(path: &Path) -> Option<apollia_core::model_preset::AgentModel> {
    if !path.exists() {
        return None;
    }
    apollia_core::LlmBackendRepository::open(path)
        .and_then(|repo| repo.agent_model(APOLLIA_AGENT_ID))
        .ok()
        .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_core::model_preset::{AgentModel, ModelSource};
    use apollia_tools::OverrideField;

    // GIVEN an Apollia agent with a mission override and an assigned model
    // WHEN the session overrides are built
    // THEN the mission is the system prompt and the model the backend
    #[test]
    fn the_mission_and_model_of_apollia_reach_the_session() {
        let dir = tempfile::tempdir().unwrap();
        let repo = AgentRepository::open(&DataFile::Agents.path(dir.path())).unwrap();
        repo.set_override(
            APOLLIA_AGENT_ID,
            OverrideField::Mission,
            &serde_json::json!("Answer in French."),
        )
        .unwrap();
        apollia_core::LlmBackendRepository::open(&DataFile::System.path(dir.path()))
            .unwrap()
            .set_agent_model(&AgentModel {
                agent: APOLLIA_AGENT_ID.into(),
                model_source: ModelSource::Local,
                model_id: "qwen".into(),
                preset_id: None,
            })
            .unwrap();
        let mut out = ChatLibreOverrides::default();

        apply_apollia_agent(&mut out, &DataFile::Governance.path(dir.path()));

        assert_eq!(out.system_prompt.as_deref(), Some("Answer in French."));
        assert_eq!(out.llm_backend.as_deref(), Some("qwen"));
    }

    // GIVEN a data directory without agents.db
    // WHEN the session overrides are built
    // THEN nothing is applied and no file is created
    #[test]
    fn no_agents_db_applies_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let mut out = ChatLibreOverrides::default();

        apply_apollia_agent(&mut out, &DataFile::Governance.path(dir.path()));

        assert!(out.system_prompt.is_none());
        assert!(out.llm_backend.is_none());
        assert!(!DataFile::Agents.path(dir.path()).exists());
    }

    // GIVEN an Apollia agent granted a native and an MCP tool on its card
    // WHEN the chat reads its access
    // THEN the chat is offered exactly those tools
    #[test]
    fn the_chat_reads_the_access_list_of_apollia() {
        let dir = tempfile::tempdir().unwrap();
        let repo = AgentRepository::open(&DataFile::Agents.path(dir.path())).unwrap();
        repo.set_override(
            APOLLIA_AGENT_ID,
            OverrideField::Access,
            &serde_json::json!(["web_search", "mcp:notion/search"]),
        )
        .unwrap();

        let access = apollia_access(Some(&DataFile::Governance.path(dir.path())));

        assert_eq!(
            access,
            Some(AgentAccess::Listed(vec![
                "web_search".to_string(),
                "mcp:notion/search".to_string()
            ]))
        );
    }

    // GIVEN a fresh install: agents.db holds Apollia, no override
    // WHEN the chat reads its access
    // THEN Apollia reads by default, as any agent created without code
    #[test]
    fn a_fresh_apollia_reads_by_default() {
        let dir = tempfile::tempdir().unwrap();
        AgentRepository::open(&DataFile::Agents.path(dir.path())).unwrap();

        let access = apollia_access(Some(&DataFile::Governance.path(dir.path())));

        assert_eq!(access, Some(AgentAccess::ReadingFamily));
    }

    // GIVEN a data directory without agents.db, then a mount without one
    // WHEN the chat reads the access of Apollia
    // THEN its built-in definition reads by default, and a mount without a
    // data directory keeps the tools of the session
    #[test]
    fn without_a_store_apollia_reads_by_default() {
        let dir = tempfile::tempdir().unwrap();

        let access = apollia_access(Some(&DataFile::Governance.path(dir.path())));

        assert_eq!(access, Some(AgentAccess::ReadingFamily));
        assert!(!DataFile::Agents.path(dir.path()).exists());
        assert_eq!(apollia_access(None), None);
    }
}
