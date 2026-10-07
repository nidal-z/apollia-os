//! The attached arm of the chat commands that play a turn again, decide on a
//! tool call, and read the history of those decisions and the skills of the
//! worker agents: each reaches one route of the daemon and reads its answer
//! as the type the embedded command returns.

use apollia_runtime::a2a::SkillListing;
use serde::{Deserialize, Serialize};

use super::A2ASkillView;
use crate::attached::{segment, AttachedClient, AttachedError};

/// The body of `POST /api/v1/sessions/{id}/authorize`.
#[derive(Debug, Serialize)]
pub(crate) struct AuthorizeBody {
    pub(crate) message_id: String,
    pub(crate) tool_call_id: String,
    pub(crate) tool_name: String,
    pub(crate) decision: String,
    pub(crate) reason: Option<String>,
}

/// Runs the user turn answered by `message_id` again.
pub(crate) async fn regenerate(
    client: &AttachedClient,
    session_id: &str,
    message_id: &str,
) -> Result<(), AttachedError> {
    let path = format!(
        "/api/v1/sessions/{}/messages/{}/regenerate",
        segment(session_id),
        segment(message_id)
    );
    let _: serde_json::Value = client.post(&path, &serde_json::json!({})).await?;
    Ok(())
}

/// Replaces the user message `message_id` with `content` and sends it again;
/// answers the id of the new user message.
pub(crate) async fn edit_and_resend(
    client: &AttachedClient,
    session_id: &str,
    message_id: &str,
    content: &str,
) -> Result<String, AttachedError> {
    #[derive(Deserialize)]
    struct Accepted {
        message_id: String,
    }
    let path = format!(
        "/api/v1/sessions/{}/messages/{}/resend",
        segment(session_id),
        segment(message_id)
    );
    let accepted: Accepted = client
        .post(&path, &serde_json::json!({ "content": content }))
        .await?;
    Ok(accepted.message_id)
}

/// Resolves the pending approval of a tool call in a session.
pub(crate) async fn authorize(
    client: &AttachedClient,
    session_id: &str,
    body: &AuthorizeBody,
) -> Result<(), AttachedError> {
    let path = format!("/api/v1/sessions/{}/authorize", segment(session_id));
    let _: serde_json::Value = client.post(&path, body).await?;
    Ok(())
}

/// The skills the worker agents declare. The route also lists the agents that
/// take a message and the workflows; the embedded command lists only skills,
/// so only those are kept.
pub(crate) async fn a2a_skills(
    client: &AttachedClient,
) -> Result<Vec<A2ASkillView>, AttachedError> {
    #[derive(Deserialize)]
    struct Skills {
        skills: Vec<SkillListing>,
    }
    let listed: Skills = client.get("/api/v1/a2a/skills").await?;
    Ok(listed
        .skills
        .into_iter()
        .filter(|s| s.kind == apollia_runtime::a2a::invoker::SKILL_KIND)
        .map(|s| A2ASkillView {
            skill_id: s.skill_id,
            agent_name: s.agent_name,
            skill_name: s.skill_name,
            description: s.description,
        })
        .collect())
}

#[cfg(test)]
mod tests;
