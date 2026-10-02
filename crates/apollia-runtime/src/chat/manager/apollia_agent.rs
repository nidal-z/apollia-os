//! The settings of the Apollia agent, read for each free-chat session.
//!
//! The chat is the Apollia agent: its mission and its model are those of the
//! agent as resolved, its definition in `agents.db` then the user's
//! overrides, with the model assigned to it in `system.db`. Both files sit
//! beside `governance.db`, whose path is the one this actor is given.
//!
//! The access list of the agent is not read here: the chat still offers the
//! whole tool registry, and what runs without a question comes from the
//! standing consents and the allow rules.

use std::path::Path;

use apollia_core::paths::DataFile;
use apollia_tools::{AgentRepository, APOLLIA_AGENT_ID};

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
        assert!(out.pre_authorized_tools.is_empty());
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
}
