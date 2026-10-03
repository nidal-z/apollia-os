//! A conversation with a declarative agent.
//!
//! An agent session names its agent. When that agent runs code, the session
//! goes to the agent runner as before. When it is a declarative agent, stored
//! in `agents.db` without code, the session runs it through the agent
//! executor, as the Apollia agent runs: its mission is the system prompt, its
//! granted accesses are the tools offered, its model and its autonomy tier
//! are those of the agent as resolved when each message is sent, so a change
//! on its card applies to an open conversation. The injections principle 6
//! reserves to Apollia are never made for it.
//!
//! The stores are read beside `governance.db`, whose path this actor is
//! given, as for the Apollia agent.

use std::path::Path;

use apollia_core::paths::DataFile;
use apollia_tools::{ResolvedAgent, APOLLIA_AGENT_ID};

use super::{ChatError, ChatMode, ChatSession, ChatSessionManager};

/// The declarative agent `name` as resolved now beside `governance_db`.
///
/// `None` for the Apollia agent (a free chat, not an agent session), an agent
/// that runs code, an unknown name, a mount without a data directory, and a
/// store that cannot be read (logged).
pub(in crate::chat::manager) fn declarative_agent(
    governance_db: Option<&Path>,
    name: &str,
) -> Option<ResolvedAgent> {
    if name == APOLLIA_AGENT_ID {
        return None;
    }
    let data_dir = governance_db?.parent()?;
    let system = DataFile::System.path(data_dir);
    let models = if system.exists() {
        apollia_core::LlmBackendRepository::open(&system).ok()
    } else {
        None
    };
    match crate::agents::task::resolve_without_code(data_dir, models.as_ref(), name) {
        Ok(agent) => agent,
        Err(error) => {
            tracing::event!(
                tracing::Level::WARN,
                agent = %name,
                error = %error,
                detail = "the session goes to the agent runner",
                "chat.declarative.unreadable"
            );
            None
        }
    }
}

/// Refuses a disabled declarative agent: an agent the user switched off is
/// unavailable, in a conversation as anywhere else.
pub(in crate::chat::manager) fn require_enabled(agent: &ResolvedAgent) -> Result<(), ChatError> {
    if agent.enabled {
        Ok(())
    } else {
        Err(ChatError::AgentLoadFailed(format!(
            "agent '{}' is disabled",
            agent.id
        )))
    }
}

impl ChatSessionManager {
    /// The declarative agent `session` runs, when it is an agent session on
    /// one.
    pub(in crate::chat::manager) fn declarative_session_agent(
        &self,
        session: &ChatSession,
    ) -> Option<ResolvedAgent> {
        if session.mode != ChatMode::Agent {
            return None;
        }
        declarative_agent(
            self.governance_db_path.as_deref(),
            session.agent_name.as_deref()?,
        )
    }

    /// The model `session_id` runs on, resolved as the dispatch of its next
    /// message resolves it: the model of its declarative agent, else the
    /// backend the session prefers, `None` for the runtime default. A session
    /// no longer in memory is read from its stored row.
    pub(in crate::chat::manager) fn handle_session_model(
        &self,
        session_id: &str,
    ) -> Result<Option<String>, ChatError> {
        let (agent_session, agent_name, preferred) = match self.sessions.get(session_id) {
            Some(session) => (
                session.mode == ChatMode::Agent,
                session.agent_name.clone(),
                session.llm_backend.clone(),
            ),
            None => {
                let row = self
                    .repository
                    .get_session(session_id)?
                    .ok_or_else(|| ChatError::SessionNotFound(session_id.to_owned()))?;
                (row.mode == "agent", row.agent_name, row.llm_backend)
            }
        };
        let declarative = agent_session
            .then(|| declarative_agent(self.governance_db_path.as_deref(), agent_name.as_deref()?))
            .flatten();
        Ok(match declarative {
            Some(agent) => agent.model.map(|m| m.model_id),
            None => preferred,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_tools::{AgentOrigin, AgentRepository, OverrideField, StoredAgent};

    fn store_guide(dir: &Path) -> AgentRepository {
        let repo = AgentRepository::open(&DataFile::Agents.path(dir)).unwrap();
        repo.save_definition(&StoredAgent {
            name: "guide".into(),
            origin: AgentOrigin::Package,
            package: Some("guide".into()),
            version: "1.0.0".into(),
            definition: serde_json::json!({
                "id": "guide",
                "name": "Guide",
                "mission": "Explain the product.",
                "defaults": {"autonomy": "assisted"}
            }),
            enabled: true,
            installed_at: "2026-10-02T00:00:00Z".into(),
            updated_at: "2026-10-02T00:00:00Z".into(),
        })
        .unwrap();
        repo
    }

    #[test]
    fn a_declarative_agent_is_resolved_with_the_overrides_in_force() {
        // GIVEN a declarative agent whose mission the user overrode
        let dir = tempfile::tempdir().unwrap();
        let repo = store_guide(dir.path());
        repo.set_override(
            "guide",
            OverrideField::Mission,
            &serde_json::json!("Explain it briefly."),
        )
        .unwrap();
        let governance = DataFile::Governance.path(dir.path());

        // WHEN a session on it resolves its agent
        let agent = declarative_agent(Some(&governance), "guide").unwrap();

        // THEN the mission is the overridden one and the tier the declared one
        assert_eq!(agent.mission, "Explain it briefly.");
        assert_eq!(agent.autonomy, apollia_core::AutonomyLevel::Assisted);
        assert!(require_enabled(&agent).is_ok());
    }

    #[test]
    fn apollia_unknown_names_and_disabled_agents_are_not_run_as_declarative() {
        // GIVEN a stored declarative agent the user disabled
        let dir = tempfile::tempdir().unwrap();
        let repo = store_guide(dir.path());
        repo.set_enabled("guide", false).unwrap();
        let governance = DataFile::Governance.path(dir.path());

        // WHEN Apollia, an unknown name and the disabled agent are resolved
        let apollia = declarative_agent(Some(&governance), APOLLIA_AGENT_ID);
        let unknown = declarative_agent(Some(&governance), "nobody");
        let disabled = declarative_agent(Some(&governance), "guide").unwrap();

        // THEN only the disabled agent resolves, and it is refused
        assert!(apollia.is_none());
        assert!(unknown.is_none());
        assert!(declarative_agent(None, "guide").is_none());
        assert!(matches!(
            require_enabled(&disabled),
            Err(ChatError::AgentLoadFailed(m)) if m.contains("disabled")
        ));
    }
}
