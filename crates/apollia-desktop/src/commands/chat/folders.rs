//! The folders of the conversation list, and the search in the messages.
//!
//! Embedded, each command asks the chat manager; attached, the daemon, over
//! `/api/v1/sessions/folders`, `/api/v1/sessions/folders/{id}`,
//! `/api/v1/sessions/{id}/folder` and `/api/v1/sessions/search`.

use apollia_runtime::chat::{ChatFolder, ChatSessionManagerHandle, MessageSearchHit};
use serde_json::{json, Value};
use tauri::State;

use crate::attached::{segment, AttachedClient, RuntimeLink};

/// Conversations a search answers when the interface names no limit.
const DEFAULT_SEARCH_RESULTS: usize = 20;

/// What a command of this module does: the chat manager of the embedded
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

/// `GET /api/v1/sessions/folders`, for `list_chat_folders`.
pub(crate) async fn list_remote(remote: &AttachedClient) -> Result<Vec<ChatFolder>, String> {
    Ok(remote.get("/api/v1/sessions/folders").await?)
}

/// `POST /api/v1/sessions/folders`, for `create_chat_folder`.
pub(crate) async fn create_remote(
    remote: &AttachedClient,
    name: &str,
) -> Result<ChatFolder, String> {
    Ok(remote
        .post("/api/v1/sessions/folders", &json!({ "name": name }))
        .await?)
}

/// `PUT /api/v1/sessions/folders/{id}`, for `rename_chat_folder`.
pub(crate) async fn rename_remote(
    remote: &AttachedClient,
    folder_id: &str,
    name: &str,
) -> Result<(), String> {
    let path = format!("/api/v1/sessions/folders/{}", segment(folder_id));
    let _: Value = remote.put(&path, &json!({ "name": name })).await?;
    Ok(())
}

/// `DELETE /api/v1/sessions/folders/{id}`, for `delete_chat_folder`.
pub(crate) async fn delete_remote(remote: &AttachedClient, folder_id: &str) -> Result<(), String> {
    let path = format!("/api/v1/sessions/folders/{}", segment(folder_id));
    let _: Value = remote.delete(&path).await?;
    Ok(())
}

/// `PUT /api/v1/sessions/{id}/folder`, for `move_chat_session_to_folder`.
pub(crate) async fn move_remote(
    remote: &AttachedClient,
    session_id: &str,
    folder_id: Option<&str>,
) -> Result<(), String> {
    let path = format!("/api/v1/sessions/{}/folder", segment(session_id));
    let _: Value = remote
        .put(&path, &json!({ "folder_id": folder_id }))
        .await?;
    Ok(())
}

/// `GET /api/v1/sessions/search`, for `search_chat_messages`.
pub(crate) async fn search_remote(
    remote: &AttachedClient,
    query: &str,
    limit: usize,
) -> Result<Vec<MessageSearchHit>, String> {
    let path = format!("/api/v1/sessions/search?q={}&limit={limit}", segment(query));
    Ok(remote.get(&path).await?)
}

/// Lists the folders of the conversation list, each with the conversations
/// it holds.
#[tauri::command]
pub async fn list_chat_folders(link: State<'_, RuntimeLink>) -> Result<Vec<ChatFolder>, String> {
    match target(link.inner())? {
        Target::Embedded(manager) => manager.list_folders().await.map_err(|e| e.to_string()),
        Target::Attached(remote) => list_remote(remote).await,
    }
}

/// Creates a folder, placed after the existing ones.
#[tauri::command]
pub async fn create_chat_folder(
    link: State<'_, RuntimeLink>,
    name: String,
) -> Result<ChatFolder, String> {
    match target(link.inner())? {
        Target::Embedded(manager) => manager.create_folder(name).await.map_err(|e| e.to_string()),
        Target::Attached(remote) => create_remote(remote, &name).await,
    }
}

/// Renames a folder.
#[tauri::command]
pub async fn rename_chat_folder(
    link: State<'_, RuntimeLink>,
    folder_id: String,
    name: String,
) -> Result<(), String> {
    match target(link.inner())? {
        Target::Embedded(manager) => manager
            .rename_folder(folder_id, name)
            .await
            .map_err(|e| e.to_string()),
        Target::Attached(remote) => rename_remote(remote, &folder_id, &name).await,
    }
}

/// Removes a folder. Its conversations are kept, out of any folder.
#[tauri::command]
pub async fn delete_chat_folder(
    link: State<'_, RuntimeLink>,
    folder_id: String,
) -> Result<(), String> {
    match target(link.inner())? {
        Target::Embedded(manager) => manager
            .delete_folder(folder_id)
            .await
            .map_err(|e| e.to_string()),
        Target::Attached(remote) => delete_remote(remote, &folder_id).await,
    }
}

/// Puts a chat session in a folder, or out of any folder when none is named.
#[tauri::command]
pub async fn move_chat_session_to_folder(
    link: State<'_, RuntimeLink>,
    session_id: String,
    folder_id: Option<String>,
) -> Result<(), String> {
    match target(link.inner())? {
        Target::Embedded(manager) => manager
            .move_session_to_folder(session_id, folder_id)
            .await
            .map_err(|e| e.to_string()),
        Target::Attached(remote) => move_remote(remote, &session_id, folder_id.as_deref()).await,
    }
}

/// Searches the messages: the conversations that contain every word of
/// `query`, each with an excerpt; none from the trash.
#[tauri::command]
pub async fn search_chat_messages(
    link: State<'_, RuntimeLink>,
    query: String,
    limit: Option<usize>,
) -> Result<Vec<MessageSearchHit>, String> {
    let limit = limit.unwrap_or(DEFAULT_SEARCH_RESULTS);
    match target(link.inner())? {
        Target::Embedded(manager) => manager
            .search_messages(query, limit)
            .await
            .map_err(|e| e.to_string()),
        Target::Attached(remote) => search_remote(remote, &query, limit).await,
    }
}

#[cfg(test)]
mod tests {
    use wiremock::matchers::{body_json, header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::attached::{gate, LinkMode};

    const TOKEN: &str = "token-of-this-installation";

    fn folder(name: &str) -> serde_json::Value {
        json!({ "id": "f 1", "name": name, "position": 0, "session_ids": ["s 1"] })
    }

    #[tokio::test]
    async fn test_each_folder_command_reaches_its_daemon_route_with_the_token_and_its_body() {
        // GIVEN a daemon answering each route of the folders, only when the
        // token of this installation and the expected body come with it
        let server = MockServer::start().await;
        let auth = format!("Bearer {TOKEN}");
        let ok = || ResponseTemplate::new(200).set_body_json(json!({ "status": "ok" }));
        Mock::given(method("GET"))
            .and(path("/api/v1/sessions/folders"))
            .and(header("authorization", auth.as_str()))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([folder("Clients")])))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/v1/sessions/folders"))
            .and(header("authorization", auth.as_str()))
            .and(body_json(json!({ "name": "Clients" })))
            .respond_with(ResponseTemplate::new(201).set_body_json(folder("Clients")))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("PUT"))
            .and(path("/api/v1/sessions/folders/f%201"))
            .and(header("authorization", auth.as_str()))
            .and(body_json(json!({ "name": "Customers" })))
            .respond_with(ok())
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("DELETE"))
            .and(path("/api/v1/sessions/folders/f%201"))
            .and(header("authorization", auth.as_str()))
            .respond_with(ok())
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("PUT"))
            .and(path("/api/v1/sessions/s%201/folder"))
            .and(header("authorization", auth.as_str()))
            .and(body_json(json!({ "folder_id": "f 1" })))
            .respond_with(ok())
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("PUT"))
            .and(path("/api/v1/sessions/s%201/folder"))
            .and(header("authorization", auth.as_str()))
            .and(body_json(json!({ "folder_id": null })))
            .respond_with(ok())
            .expect(1)
            .mount(&server)
            .await;
        let client = AttachedClient::new(server.uri(), TOKEN).unwrap();

        // WHEN the attached arm of each command runs, with identifiers that
        // need encoding in a path
        let listed = list_remote(&client).await.unwrap();
        let created = create_remote(&client, "Clients").await.unwrap();
        rename_remote(&client, "f 1", "Customers").await.unwrap();
        move_remote(&client, "s 1", Some("f 1")).await.unwrap();
        move_remote(&client, "s 1", None).await.unwrap();
        delete_remote(&client, "f 1").await.unwrap();

        // THEN each reached its own route once (the mocks verify it on drop),
        // and the folders come back as the daemon said
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].session_ids, ["s 1"]);
        assert_eq!(created.name, "Clients");
    }

    #[tokio::test]
    async fn test_the_search_sends_the_words_and_the_limit_and_reads_the_excerpts() {
        // GIVEN a daemon answering the search of two words, limited to five
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/sessions/search"))
            .and(query_param("q", "lease & shop"))
            .and(query_param("limit", "5"))
            .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([{
                "session_id": "s-1",
                "title": "Shop",
                "agent_name": null,
                "created_at": "2026-10-01T00:00:00Z",
                "excerpt": "the lease of the shop",
            }])))
            .expect(1)
            .mount(&server)
            .await;
        let client = AttachedClient::new(server.uri(), TOKEN).unwrap();

        // WHEN the attached arm searches words that need encoding in a query
        let hits = search_remote(&client, "lease & shop", 5).await.unwrap();

        // THEN the conversation found comes back with its excerpt
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].session_id, "s-1");
        assert_eq!(hits[0].excerpt, "the lease of the shop");
    }

    #[tokio::test]
    async fn test_a_rename_refused_by_the_daemon_shows_its_message() {
        // GIVEN a daemon that does not know the folder
        let server = MockServer::start().await;
        Mock::given(method("PUT"))
            .and(path("/api/v1/sessions/folders/ghost"))
            .respond_with(
                ResponseTemplate::new(404)
                    .set_body_json(json!({ "error": "folder not found: ghost" })),
            )
            .mount(&server)
            .await;
        let client = AttachedClient::new(server.uri(), TOKEN).unwrap();

        // WHEN the attached arm renames it
        let renamed = rename_remote(&client, "ghost", "Anything").await;

        // THEN the interface reads the daemon's own words
        assert_eq!(renamed.unwrap_err(), "folder not found: ghost");
    }

    #[test]
    fn test_the_folder_and_search_commands_pass_the_gate_attached() {
        // GIVEN the six commands of this module
        let commands = [
            "create_chat_folder",
            "delete_chat_folder",
            "list_chat_folders",
            "move_chat_session_to_folder",
            "rename_chat_folder",
            "search_chat_messages",
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
