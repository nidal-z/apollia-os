//! The actions under an answer, beside regenerating it: branch off into a
//! new conversation, continue the last answer, rewrite an answer in place,
//! remove a message.
//!
//! Embedded, each command asks the chat manager; attached, the daemon, over
//! `/api/v1/sessions/{id}/fork`, `/api/v1/sessions/{id}/continuation` and
//! the routes under `/api/v1/sessions/{id}/messages/{message_id}`.

use apollia_runtime::chat::{ChatMessage, ChatSessionManagerHandle, Continuation};
use serde::Deserialize;
use serde_json::{json, Value};
use tauri::State;

use crate::attached::{segment, AttachedClient, RuntimeLink};

/// What a command of this module acts on: the chat manager of the embedded
/// runtime, or the daemon the application is attached to.
enum Target<'a> {
    Embedded(&'a ChatSessionManagerHandle),
    Attached(&'a AttachedClient),
}

fn target(link: &RuntimeLink) -> Result<Target<'_>, String> {
    match link {
        RuntimeLink::Embedded(runtime) => runtime
            .chat_manager
            .as_ref()
            .map(Target::Embedded)
            .ok_or_else(|| "chat subsystem not available".to_string()),
        RuntimeLink::Attached(remote) => Ok(Target::Attached(remote)),
    }
}

fn message_path(session_id: &str, message_id: &str) -> String {
    format!(
        "/api/v1/sessions/{}/messages/{}",
        segment(session_id),
        segment(message_id)
    )
}

/// How many messages a branch taken at `message_id` copies: every message up
/// to and including it.
fn messages_up_to(history: &[ChatMessage], message_id: &str) -> Result<usize, String> {
    history
        .iter()
        .position(|m| m.id == message_id)
        .map(|at| at + 1)
        .ok_or_else(|| format!("message not found: {message_id}"))
}

/// The attached arm of `fork_chat_session`: reads the session to place the
/// message, then asks `POST /api/v1/sessions/{id}/fork` for that many.
pub(crate) async fn fork_remote(
    remote: &AttachedClient,
    session_id: &str,
    message_id: &str,
) -> Result<String, String> {
    #[derive(Deserialize)]
    struct Forked {
        id: String,
    }
    let detail = remote.get_session(session_id).await?;
    let up_to_index = messages_up_to(&detail.session.history, message_id)?;
    let path = format!("/api/v1/sessions/{}/fork", segment(session_id));
    let forked: Forked = remote
        .post(&path, &json!({ "up_to_index": up_to_index }))
        .await?;
    Ok(forked.id)
}

/// `GET /api/v1/sessions/{id}/continuation`, for `get_chat_continuation`.
pub(crate) async fn continuation_remote(
    remote: &AttachedClient,
    session_id: &str,
) -> Result<Continuation, String> {
    let path = format!("/api/v1/sessions/{}/continuation", segment(session_id));
    Ok(remote.get(&path).await?)
}

/// `POST .../messages/{message_id}/continue`, for `continue_chat_response`.
pub(crate) async fn continue_remote(
    remote: &AttachedClient,
    session_id: &str,
    message_id: &str,
) -> Result<(), String> {
    let path = format!("{}/continue", message_path(session_id, message_id));
    let _: Value = remote.post(&path, &json!({})).await?;
    Ok(())
}

/// `PUT .../messages/{message_id}`, for `edit_chat_response`.
pub(crate) async fn edit_remote(
    remote: &AttachedClient,
    session_id: &str,
    message_id: &str,
    content: &str,
) -> Result<(), String> {
    let _: Value = remote
        .put(
            &message_path(session_id, message_id),
            &json!({ "content": content }),
        )
        .await?;
    Ok(())
}

/// `DELETE .../messages/{message_id}`, for `delete_chat_message`.
pub(crate) async fn delete_remote(
    remote: &AttachedClient,
    session_id: &str,
    message_id: &str,
) -> Result<(), String> {
    let _: Value = remote.delete(&message_path(session_id, message_id)).await?;
    Ok(())
}

/// Branches off into a new conversation holding the messages up to and
/// including `message_id`, with the agent, the model and the profile of the
/// conversation it leaves. Returns the id of the new conversation.
#[tauri::command]
pub async fn fork_chat_session(
    link: State<'_, RuntimeLink>,
    session_id: String,
    message_id: String,
) -> Result<String, String> {
    match target(link.inner())? {
        Target::Embedded(manager) => {
            let detail = manager
                .get_session(session_id.clone())
                .await
                .ok_or_else(|| "session not found".to_string())?;
            let up_to_index = messages_up_to(&detail.session.history, &message_id)?;
            manager
                .fork_session(session_id, Some(up_to_index))
                .await
                .map(|forked| forked.id)
                .map_err(|e| e.to_string())
        }
        Target::Attached(remote) => fork_remote(remote, &session_id, &message_id).await,
    }
}

/// Whether the last answer of a conversation can be continued on the model
/// its next turn runs on, and why not.
#[tauri::command]
pub async fn get_chat_continuation(
    link: State<'_, RuntimeLink>,
    session_id: String,
) -> Result<Continuation, String> {
    match target(link.inner())? {
        Target::Embedded(manager) => manager
            .continuation(session_id)
            .await
            .map_err(|e| e.to_string()),
        Target::Attached(remote) => continuation_remote(remote, &session_id).await,
    }
}

/// Continues `message_id`, the last answer of the conversation: its turn runs
/// again with the answer as the prefill, and the rest streams as an answer
/// does. Refused where the model does not continue an answer.
#[tauri::command]
pub async fn continue_chat_response(
    link: State<'_, RuntimeLink>,
    session_id: String,
    message_id: String,
) -> Result<(), String> {
    match target(link.inner())? {
        Target::Embedded(manager) => manager
            .continue_answer(session_id, message_id)
            .await
            .map_err(|e| e.to_string()),
        Target::Attached(remote) => continue_remote(remote, &session_id, &message_id).await,
    }
}

/// Replaces the text of the answer `message_id`, marked edited. No turn is
/// played again; the next one reads the new text.
#[tauri::command]
pub async fn edit_chat_response(
    link: State<'_, RuntimeLink>,
    session_id: String,
    message_id: String,
    content: String,
) -> Result<(), String> {
    match target(link.inner())? {
        Target::Embedded(manager) => manager
            .edit_answer(session_id, message_id, content)
            .await
            .map_err(|e| e.to_string()),
        Target::Attached(remote) => edit_remote(remote, &session_id, &message_id, &content).await,
    }
}

/// Removes `message_id` from the conversation and, for an answer, the tool
/// messages of its turn.
#[tauri::command]
pub async fn delete_chat_message(
    link: State<'_, RuntimeLink>,
    session_id: String,
    message_id: String,
) -> Result<(), String> {
    match target(link.inner())? {
        Target::Embedded(manager) => manager
            .delete_message(session_id, message_id)
            .await
            .map(|_| ())
            .map_err(|e| e.to_string()),
        Target::Attached(remote) => delete_remote(remote, &session_id, &message_id).await,
    }
}

#[cfg(test)]
mod tests {
    use wiremock::matchers::{body_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::attached::{gate, LinkMode};

    const TOKEN: &str = "token-of-this-installation";

    /// The session `s 1` as the daemon answers it: two turns, with the hole
    /// a deleted message left in the sequence numbers.
    fn two_turns() -> Value {
        use apollia_runtime::chat::{
            ChatMode, ChatRole, ChatSession, SessionDetail, SessionStatus,
        };
        let message = |id: &str, role: ChatRole, seq: u32| ChatMessage {
            id: id.to_owned(),
            role,
            content: id.to_owned(),
            tool_calls: None,
            tool_name: None,
            created_at: "2026-10-06T10:00:00Z".to_owned(),
            seq,
            metadata: None,
        };
        let detail = SessionDetail {
            message_count: 4,
            session: ChatSession {
                id: "s 1".to_owned(),
                mode: ChatMode::Agent,
                agent_name: Some("guide".to_owned()),
                system_prompt: String::new(),
                status: SessionStatus::Active,
                history: vec![
                    message("u-1", ChatRole::User, 1),
                    message("a-1", ChatRole::Assistant, 2),
                    message("u-2", ChatRole::User, 5),
                    message("a-2", ChatRole::Assistant, 6),
                ],
                authorized_tools: Default::default(),
                available_tools: Vec::new(),
                created_at: "2026-10-06T10:00:00Z".to_owned(),
                active_exchange: None,
                llm_backend: Some("qwen".to_owned()),
                title: None,
                parent_session_id: None,
                fork_depth: 0,
                plan_mode: false,
                plan_phase: Default::default(),
                profile_id: None,
                use_profile: true,
            },
        };
        serde_json::to_value(detail).unwrap()
    }

    async fn daemon_with_session() -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/sessions/s%201"))
            .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
            .respond_with(ResponseTemplate::new(200).set_body_json(two_turns()))
            .mount(&server)
            .await;
        server
    }

    #[tokio::test]
    async fn test_a_branch_asks_the_fork_route_for_the_messages_up_to_the_answer() {
        // GIVEN a daemon holding a session of two turns, whose fork route
        // answers the new session only when asked for three messages
        let server = daemon_with_session().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/sessions/s%201/fork"))
            .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
            .and(body_json(json!({ "up_to_index": 3 })))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({
                "id": "s-2", "mode": "agent", "agent_name": "guide", "status": "active",
                "created_at": "2026-10-06T10:05:00Z", "title": null,
            })))
            .expect(1)
            .mount(&server)
            .await;
        let client = AttachedClient::new(server.uri(), TOKEN).unwrap();

        // WHEN the attached arm branches off at the third message, whose
        // sequence number is not its place
        let forked = fork_remote(&client, "s 1", "u-2").await;

        // THEN the route was asked for the messages up to it, by their place
        // (the mock verifies it on drop), and the new session id comes back
        assert_eq!(forked.unwrap(), "s-2");
    }

    #[tokio::test]
    async fn test_a_branch_at_a_message_the_session_does_not_hold_asks_nothing() {
        // GIVEN the same daemon, with no fork route mounted
        let server = daemon_with_session().await;
        let client = AttachedClient::new(server.uri(), TOKEN).unwrap();

        // WHEN the attached arm branches off at an unknown message
        let forked = fork_remote(&client, "s 1", "ghost").await;

        // THEN it is refused here, before any fork is asked
        assert_eq!(forked.unwrap_err(), "message not found: ghost");
    }

    #[tokio::test]
    async fn test_continue_edit_and_delete_reach_their_daemon_routes() {
        // GIVEN a daemon answering each route of an answer, only with the
        // token, and saying why an answer cannot be continued
        let server = MockServer::start().await;
        let auth = format!("Bearer {TOKEN}");
        Mock::given(method("GET"))
            .and(path("/api/v1/sessions/s%201/continuation"))
            .and(header("authorization", auth.as_str()))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({ "available": false, "reason": "engine" })),
            )
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/v1/sessions/s%201/messages/a-2/continue"))
            .and(header("authorization", auth.as_str()))
            .respond_with(
                ResponseTemplate::new(422)
                    .set_body_json(json!({ "error": "the answer cannot be continued: engine" })),
            )
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("PUT"))
            .and(path("/api/v1/sessions/s%201/messages/a-2"))
            .and(header("authorization", auth.as_str()))
            .and(body_json(json!({ "content": "Rewritten." })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "status": "edited" })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("DELETE"))
            .and(path("/api/v1/sessions/s%201/messages/a-2"))
            .and(header("authorization", auth.as_str()))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "deleted": 1 })))
            .expect(1)
            .mount(&server)
            .await;
        let client = AttachedClient::new(server.uri(), TOKEN).unwrap();

        // WHEN the attached arm of each command runs
        let support = continuation_remote(&client, "s 1").await.unwrap();
        let continued = continue_remote(&client, "s 1", "a-2").await;
        let edited = edit_remote(&client, "s 1", "a-2", "Rewritten.").await;
        let deleted = delete_remote(&client, "s 1", "a-2").await;

        // THEN each reached its own route once (the mocks verify it on drop),
        // the reason comes back as the daemon said, and its refusal is shown
        // in its own words
        assert!(!support.available);
        assert_eq!(
            serde_json::to_value(support).unwrap()["reason"],
            json!("engine")
        );
        assert_eq!(
            continued.unwrap_err(),
            "the answer cannot be continued: engine"
        );
        assert!(edited.is_ok(), "{edited:?}");
        assert!(deleted.is_ok(), "{deleted:?}");
    }

    #[test]
    fn test_the_answer_commands_pass_the_gate_attached() {
        // GIVEN the five commands of this module
        let commands = [
            "continue_chat_response",
            "delete_chat_message",
            "edit_chat_response",
            "fork_chat_session",
            "get_chat_continuation",
        ];

        // WHEN each is gated while attached
        let refused: Vec<&str> = commands
            .into_iter()
            .filter(|command| gate(LinkMode::Attached, command).is_err())
            .collect();

        // THEN none is refused
        assert!(refused.is_empty(), "refused: {refused:?}");
    }
}
