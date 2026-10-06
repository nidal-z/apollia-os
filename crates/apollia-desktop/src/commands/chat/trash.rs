//! The trash of the conversations: send one there, list what it holds, take
//! one back out. Erasing for good stays `delete_chat_session`.
//!
//! Embedded, each command asks the chat manager; attached, the daemon, over
//! `/api/v1/sessions/trash`, `/api/v1/sessions/{id}/trash` and
//! `/api/v1/sessions/{id}/restore`.

use apollia_runtime::chat::{ChatSessionManagerHandle, TrashedSession};
use serde_json::{json, Value};
use tauri::State;

use crate::attached::{segment, AttachedClient, RuntimeLink};

/// What a command of the trash does: the chat manager of the embedded
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

/// `GET /api/v1/sessions/trash`, for `list_trashed_chat_sessions`.
pub(crate) async fn list_remote(remote: &AttachedClient) -> Result<Vec<TrashedSession>, String> {
    Ok(remote.get("/api/v1/sessions/trash").await?)
}

/// `POST /api/v1/sessions/{id}/trash`, for `trash_chat_session`.
pub(crate) async fn trash_remote(remote: &AttachedClient, session_id: &str) -> Result<(), String> {
    let path = format!("/api/v1/sessions/{}/trash", segment(session_id));
    let _: Value = remote.post(&path, &json!({})).await?;
    Ok(())
}

/// `POST /api/v1/sessions/{id}/restore`, for `restore_chat_session`.
pub(crate) async fn restore_remote(
    remote: &AttachedClient,
    session_id: &str,
) -> Result<(), String> {
    let path = format!("/api/v1/sessions/{}/restore", segment(session_id));
    let _: Value = remote.post(&path, &json!({})).await?;
    Ok(())
}

/// Sends a chat session to the trash: it leaves the list and keeps its
/// messages.
#[tauri::command]
pub async fn trash_chat_session(
    link: State<'_, RuntimeLink>,
    session_id: String,
) -> Result<(), String> {
    match target(link.inner())? {
        Target::Embedded(manager) => manager
            .trash_session(session_id)
            .await
            .map_err(|e| e.to_string()),
        Target::Attached(remote) => trash_remote(remote, &session_id).await,
    }
}

/// Takes a chat session out of the trash, back into the list with its
/// messages.
#[tauri::command]
pub async fn restore_chat_session(
    link: State<'_, RuntimeLink>,
    session_id: String,
) -> Result<(), String> {
    match target(link.inner())? {
        Target::Embedded(manager) => manager
            .restore_session(session_id)
            .await
            .map(|_| ())
            .map_err(|e| e.to_string()),
        Target::Attached(remote) => restore_remote(remote, &session_id).await,
    }
}

/// Lists the chat sessions in the trash, the last removed first.
#[tauri::command]
pub async fn list_trashed_chat_sessions(
    link: State<'_, RuntimeLink>,
) -> Result<Vec<TrashedSession>, String> {
    match target(link.inner())? {
        Target::Embedded(manager) => manager
            .list_trashed_sessions()
            .await
            .map_err(|e| e.to_string()),
        Target::Attached(remote) => list_remote(remote).await,
    }
}

#[cfg(test)]
mod tests {
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::attached::{gate, LinkMode};

    const TOKEN: &str = "token-of-this-installation";

    #[tokio::test]
    async fn test_each_trash_command_reaches_its_daemon_route_with_the_token() {
        // GIVEN a daemon answering each route of the trash, only when the
        // token of this installation comes with the request
        let server = MockServer::start().await;
        let auth = format!("Bearer {TOKEN}");
        for route in [
            "/api/v1/sessions/s%201/trash",
            "/api/v1/sessions/s%201/restore",
        ] {
            Mock::given(method("POST"))
                .and(path(route))
                .and(header("authorization", auth.as_str()))
                .respond_with(
                    ResponseTemplate::new(200).set_body_json(serde_json::json!({ "status": "ok" })),
                )
                .expect(1)
                .mount(&server)
                .await;
        }
        Mock::given(method("GET"))
            .and(path("/api/v1/sessions/trash"))
            .and(header("authorization", auth.as_str()))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!([{
                    "id": "s 1",
                    "mode": "libre",
                    "agent_name": null,
                    "title": "Draft",
                    "created_at": "2026-10-01T00:00:00Z",
                    "deleted_at": "2026-10-05T10:00:00Z",
                }])),
            )
            .expect(1)
            .mount(&server)
            .await;
        let client = AttachedClient::new(server.uri(), TOKEN).unwrap();

        // WHEN the attached arm of each command runs, with an identifier that
        // needs encoding in a path
        trash_remote(&client, "s 1").await.unwrap();
        let listed = list_remote(&client).await.unwrap();
        restore_remote(&client, "s 1").await.unwrap();

        // THEN each reached its own route once (the mocks verify it on drop),
        // and the trash comes back as the daemon said
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].title.as_deref(), Some("Draft"));
        assert_eq!(listed[0].deleted_at, "2026-10-05T10:00:00Z");
    }

    #[tokio::test]
    async fn test_a_restore_refused_by_the_daemon_shows_its_message() {
        // GIVEN a daemon whose trash does not hold the session
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/sessions/ghost/restore"))
            .respond_with(
                ResponseTemplate::new(404)
                    .set_body_json(serde_json::json!({ "error": "session not found: ghost" })),
            )
            .mount(&server)
            .await;
        let client = AttachedClient::new(server.uri(), TOKEN).unwrap();

        // WHEN the attached arm restores it
        let restored = restore_remote(&client, "ghost").await;

        // THEN the interface reads the daemon's own words
        assert_eq!(restored.unwrap_err(), "session not found: ghost");
    }

    #[test]
    fn test_the_trash_commands_pass_the_gate_attached() {
        // GIVEN the three commands of the trash
        let commands = [
            "list_trashed_chat_sessions",
            "restore_chat_session",
            "trash_chat_session",
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
