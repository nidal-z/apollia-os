//! Project deletion as one whole operation.
//!
//! A project row lives in `projects.db`, owned by [`ProjectRepository`] in the
//! `apollia-tools` crate. The chat sessions that reference it live in
//! `chat.db`, owned by the chat session manager of this crate. Neither store
//! knows about the other, so deleting the project row on its own leaves every
//! session that pointed at it with a `project_id` referencing a row that no
//! longer exists.
//!
//! This module composes the two writes, above both stores, so a client deletes
//! a project through one call instead of reaching for the repository directly
//! and forgetting half the job.

use std::path::Path;

use apollia_tools::{ProjectRepository, ProjectRepositoryError};
use tracing::{info, warn};

use crate::chat::ChatSessionManagerHandle;
use crate::chat::ChatSessionRepository;

/// Deletes a project, then unlinks the chat sessions that referenced it.
///
/// Returns `false` when no project matched `project_id`, in which case nothing
/// is unlinked.
///
/// Order matters: the project row goes first. Unlinking runs after and never
/// turns into an error for the caller, because a session holding a stale
/// `project_id` is a smaller defect than a project the operator asked to
/// remove and that is still there. Every failure path leaves a `warn` behind,
/// either here or inside the chat session manager.
pub async fn delete_project(
    projects: &ProjectRepository,
    chat_manager: Option<&ChatSessionManagerHandle>,
    project_id: &str,
) -> Result<bool, ProjectRepositoryError> {
    let deleted = projects
        .delete_project_async(project_id.to_string())
        .await?;

    if !deleted {
        return Ok(false);
    }

    match chat_manager {
        // The manager owns both the chat database and the in-memory session
        // cache, and traces its own failures.
        Some(manager) => {
            manager
                .orphan_project_sessions(project_id.to_string())
                .await;
        }
        None => warn!(
            project_id = %project_id,
            cause = "chat session manager unavailable",
            "project.delete.sessions_not_orphaned"
        ),
    }

    Ok(true)
}

/// Unlinks the chat sessions of a deleted project without a running runtime.
///
/// For clients that own `projects.db` directly and have no chat session
/// manager to talk to, the CLI being the only one today. A runtime that holds
/// `chat.db` open at the same time keeps its cached sessions until it reloads
/// them, so [`delete_project`] stays the preferred path whenever a manager is
/// reachable.
///
/// Never reports to the caller: the project is already gone by the time this
/// runs, and the operator gets its verdict from the deletion itself.
pub fn orphan_sessions_offline(chat_db_path: &Path, project_id: &str) {
    // No chat database means no session ever referenced the project.
    if !chat_db_path.exists() {
        return;
    }

    let outcome = ChatSessionRepository::open(chat_db_path)
        .and_then(|repo| repo.orphan_project_sessions(project_id));

    match outcome {
        Ok(count) => {
            if count > 0 {
                info!(
                    project_id = %project_id,
                    count,
                    "project.delete.sessions_orphaned"
                );
            }
        }
        Err(e) => warn!(
            project_id = %project_id,
            cause = %e,
            "project.delete.sessions_not_orphaned"
        ),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use apollia_core::StepBudgetConfig;
    use apollia_llm::LlmRouter;
    use apollia_mcp::session::LoadingMode;
    use apollia_tools::ToolRegistryHandle;

    use super::*;
    use crate::api::routes_agents::StubAgentLoader;
    use crate::chat::manager::CreateSessionParams;
    use crate::chat::types::ChatMode;

    /// Spawn a chat session manager backed by a `chat.db` inside `dir`.
    fn spawn_manager(dir: &Path) -> ChatSessionManagerHandle {
        let (event_tx, _) = tokio::sync::broadcast::channel(128);
        let registry_handle = crate::registry::AgentRegistry::spawn(event_tx.clone());
        ChatSessionManagerHandle::spawn(
            &dir.join("chat.db"),
            Some(Arc::new(LlmRouter::empty())),
            ToolRegistryHandle::start(),
            Arc::new(StubAgentLoader),
            None,
            event_tx,
            StepBudgetConfig::default(),
            None,
            registry_handle,
            None,
            None,
            None,
            None,
            None,
            LoadingMode::Eager,
            20,
            None,
            false,
        )
        .expect("spawn chat session manager")
    }

    #[tokio::test]
    async fn test_delete_project_orphans_its_chat_sessions() {
        // GIVEN a project and a chat session linked to it
        let dir = tempfile::tempdir().expect("tempdir");
        let projects =
            ProjectRepository::open(&dir.path().join("projects.db")).expect("open projects.db");
        let project_id = projects
            .create_project("Projet", None, None, None)
            .expect("create project");
        let chat = spawn_manager(dir.path());
        let session = chat
            .create_session(CreateSessionParams {
                mode: ChatMode::Libre,
                agent_name: None,
                system_prompt: None,
                tools: vec![],
                project_id: Some(project_id.clone()),
            })
            .await
            .expect("create session");
        assert_eq!(
            chat.list_sessions_by_project(project_id.clone())
                .await
                .len(),
            1
        );

        // WHEN the project is deleted through the composed operation
        let deleted = delete_project(&projects, Some(&chat), &project_id)
            .await
            .expect("delete project");

        // THEN the project is gone
        assert!(deleted);
        assert!(projects.get_project(&project_id).is_err());
        // AND the session survives with no project link left in the database
        assert!(chat
            .list_sessions_by_project(project_id.clone())
            .await
            .is_empty());
        let detail = chat
            .get_session(session.id.clone())
            .await
            .expect("session still exists");
        assert!(detail.session.project_id.is_none());
    }
}
