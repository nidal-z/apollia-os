//! The folders of the conversation list, and the search in the messages.
//!
//! - `GET    /api/v1/sessions/folders`         , the folders and what each holds
//! - `POST   /api/v1/sessions/folders`         , create one
//! - `PUT    /api/v1/sessions/folders/{id}`    , rename one
//! - `DELETE /api/v1/sessions/folders/{id}`    , remove one, its conversations kept
//! - `PUT    /api/v1/sessions/{id}/folder`     , put a conversation in a folder, or out
//! - `GET    /api/v1/sessions/search`          , the conversations that contain the words
//!
//! A folder holds conversations and never owns them: removing it puts them
//! back out of any folder. The search reads what a person and the model
//! wrote, and never answers a conversation in the trash.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, put};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use super::conversations::{chat_refusal, done, manager, Refusal};
use crate::api::server::AppState;
use crate::chat::{ChatFolder, MessageSearchHit, MESSAGE_SEARCH_MAX_RESULTS};
use crate::coordinator::ExecutionBackend;

/// Conversations a search answers when the request names no limit.
const DEFAULT_SEARCH_RESULTS: usize = 20;

/// Body of `POST /api/v1/sessions/folders` and of
/// `PUT /api/v1/sessions/folders/{id}`.
#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct FolderNameBody {
    /// Name of the folder: not blank, 100 characters at most.
    pub name: String,
}

/// Body of `PUT /api/v1/sessions/{id}/folder`.
#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct SessionFolderBody {
    /// The folder the conversation goes in; `null` takes it out of any
    /// folder.
    pub folder_id: Option<String>,
}

/// Query of `GET /api/v1/sessions/search`.
#[derive(Debug, Deserialize)]
pub struct MessageSearchQuery {
    /// The words searched.
    #[serde(default)]
    pub q: String,
    /// Most conversations answered.
    pub limit: Option<usize>,
}

/// Build the router of these routes.
pub fn folders_router<B: ExecutionBackend + Clone>() -> Router<AppState<B>> {
    Router::new()
        .route(
            "/api/v1/sessions/folders",
            get(list_chat_folders::<B>).post(create_chat_folder::<B>),
        )
        .route(
            "/api/v1/sessions/folders/:id",
            put(rename_chat_folder::<B>).delete(delete_chat_folder::<B>),
        )
        .route("/api/v1/sessions/:id/folder", put(set_session_folder::<B>))
        .route("/api/v1/sessions/search", get(search_chat_messages::<B>))
}

/// Handler for `GET /api/v1/sessions/folders`, the folders in the order of
/// the list, each with the conversations it holds.
#[utoipa::path(
    get,
    path = "/api/v1/sessions/folders",
    tag = "chat",
    responses(
        (status = 200, description = "The folders of the conversation list", body = Vec<ChatFolder>),
        (status = 503, description = "Chat subsystem unavailable", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn list_chat_folders<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Result<Json<Vec<ChatFolder>>, Refusal> {
    manager(&state)?
        .list_folders()
        .await
        .map(Json)
        .map_err(chat_refusal)
}

/// Handler for `POST /api/v1/sessions/folders`, create a folder, placed
/// after the existing ones.
#[utoipa::path(
    post,
    path = "/api/v1/sessions/folders",
    tag = "chat",
    request_body = FolderNameBody,
    responses(
        (status = 201, description = "Folder created", body = ChatFolder),
        (status = 400, description = "Blank or oversized name", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Chat subsystem unavailable", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn create_chat_folder<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Json(body): Json<FolderNameBody>,
) -> Result<(StatusCode, Json<ChatFolder>), Refusal> {
    manager(&state)?
        .create_folder(body.name)
        .await
        .map(|folder| (StatusCode::CREATED, Json(folder)))
        .map_err(chat_refusal)
}

/// Handler for `PUT /api/v1/sessions/folders/{id}`, rename a folder.
#[utoipa::path(
    put,
    path = "/api/v1/sessions/folders/{id}",
    tag = "chat",
    params(("id" = String, Path, description = "Folder id")),
    request_body = FolderNameBody,
    responses(
        (status = 200, description = "Folder renamed"),
        (status = 400, description = "Blank or oversized name", body = crate::api::openapi::ApiErrorBody),
        (status = 404, description = "Folder not found", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Chat subsystem unavailable", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn rename_chat_folder<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
    Json(body): Json<FolderNameBody>,
) -> Result<axum::response::Response, Refusal> {
    manager(&state)?
        .rename_folder(id, body.name)
        .await
        .map_err(chat_refusal)?;
    Ok(done("renamed"))
}

/// Handler for `DELETE /api/v1/sessions/folders/{id}`, remove a folder. Its
/// conversations are kept and go back out of any folder.
#[utoipa::path(
    delete,
    path = "/api/v1/sessions/folders/{id}",
    tag = "chat",
    params(("id" = String, Path, description = "Folder id")),
    responses(
        (status = 200, description = "Folder removed, its conversations kept"),
        (status = 404, description = "Folder not found", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Chat subsystem unavailable", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn delete_chat_folder<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
) -> Result<axum::response::Response, Refusal> {
    manager(&state)?
        .delete_folder(id)
        .await
        .map_err(chat_refusal)?;
    Ok(done("deleted"))
}

/// Handler for `PUT /api/v1/sessions/{id}/folder`, put the conversation in a
/// folder, or out of any folder.
#[utoipa::path(
    put,
    path = "/api/v1/sessions/{id}/folder",
    tag = "chat",
    params(("id" = String, Path, description = "Session id")),
    request_body = SessionFolderBody,
    responses(
        (status = 200, description = "Conversation moved"),
        (status = 404, description = "Session or folder not found", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Chat subsystem unavailable", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn set_session_folder<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
    Json(body): Json<SessionFolderBody>,
) -> Result<axum::response::Response, Refusal> {
    manager(&state)?
        .move_session_to_folder(id, body.folder_id)
        .await
        .map_err(chat_refusal)?;
    Ok(done("moved"))
}

/// Handler for `GET /api/v1/sessions/search`, the conversations whose
/// messages contain every word of `q`, the best match first, each with an
/// excerpt. A word is matched from its start, and a conversation in the
/// trash is never answered.
#[utoipa::path(
    get,
    path = "/api/v1/sessions/search",
    tag = "chat",
    params(
        ("q" = String, Query, description = "The words searched in the messages"),
        ("limit" = Option<usize>, Query, description = "Most conversations answered (default 20, at most 50)"),
    ),
    responses(
        (status = 200, description = "The conversations found, each with an excerpt", body = Vec<MessageSearchHit>),
        (status = 503, description = "Chat subsystem unavailable", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn search_chat_messages<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Query(query): Query<MessageSearchQuery>,
) -> Result<Json<Vec<MessageSearchHit>>, Refusal> {
    let limit = query
        .limit
        .unwrap_or(DEFAULT_SEARCH_RESULTS)
        .min(MESSAGE_SEARCH_MAX_RESULTS);
    manager(&state)?
        .search_messages(query.q, limit)
        .await
        .map(Json)
        .map_err(chat_refusal)
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use serde_json::{json, Value};
    use tower::ServiceExt;

    use super::super::tests::chat_state;
    use crate::api::server::APIServer;
    use crate::chat::{AppendMessageParams, ChatRole, ChatSessionRepository};

    async fn call(
        router: &axum::Router,
        method: &str,
        uri: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let mut request = Request::builder().method(method).uri(uri);
        let payload = match body {
            Some(body) => {
                request = request.header("content-type", "application/json");
                Body::from(serde_json::to_vec(&body).unwrap())
            }
            None => Body::empty(),
        };
        let response = router
            .clone()
            .oneshot(request.body(payload).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    async fn new_session(router: &axum::Router) -> String {
        let (status, created) = call(
            router,
            "POST",
            "/api/v1/sessions",
            Some(json!({ "mode": "libre" })),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{created}");
        created["id"].as_str().unwrap().to_owned()
    }

    async fn new_folder(router: &axum::Router, name: &str) -> String {
        let (status, created) = call(
            router,
            "POST",
            "/api/v1/sessions/folders",
            Some(json!({ "name": name })),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{created}");
        created["id"].as_str().unwrap().to_owned()
    }

    /// Write one message of a person in `session`, the way a turn does.
    fn say(dir: &tempfile::TempDir, session: &str, content: &str) {
        let repo = ChatSessionRepository::open(&dir.path().join("chat.db")).unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        repo.append_message(&AppendMessageParams {
            id: &id,
            session_id: session,
            role: &ChatRole::User,
            content,
            tool_calls_json: None,
            tool_name: None,
            created_at: "2026-10-05T00:00:00Z",
            metadata: None,
        })
        .unwrap();
    }

    #[tokio::test]
    async fn test_a_folder_is_created_holds_a_session_and_is_renamed() {
        // GIVEN a runtime holding one session and no folder
        let dir = tempfile::tempdir().unwrap();
        let router = APIServer::build_router_for_test(chat_state(&dir));
        let session = new_session(&router).await;
        let (_, none) = call(&router, "GET", "/api/v1/sessions/folders", None).await;
        assert_eq!(none, json!([]));

        // WHEN a folder is created, the session put in it, and the folder
        // renamed
        let folder = new_folder(&router, "Clients").await;
        let (moved, body) = call(
            &router,
            "PUT",
            &format!("/api/v1/sessions/{session}/folder"),
            Some(json!({ "folder_id": folder })),
        )
        .await;
        assert_eq!(moved, StatusCode::OK, "{body}");
        let (renamed, body) = call(
            &router,
            "PUT",
            &format!("/api/v1/sessions/folders/{folder}"),
            Some(json!({ "name": "Customers" })),
        )
        .await;
        assert_eq!(renamed, StatusCode::OK, "{body}");

        // THEN the folders are that one, under its new name, holding the
        // session
        let (status, folders) = call(&router, "GET", "/api/v1/sessions/folders", None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            folders,
            json!([{
                "id": folder,
                "name": "Customers",
                "position": 0,
                "session_ids": [session],
            }])
        );
    }

    #[tokio::test]
    async fn test_deleting_a_folder_keeps_its_session_in_the_list_out_of_any_folder() {
        // GIVEN a folder holding a session
        let dir = tempfile::tempdir().unwrap();
        let router = APIServer::build_router_for_test(chat_state(&dir));
        let session = new_session(&router).await;
        let folder = new_folder(&router, "Clients").await;
        call(
            &router,
            "PUT",
            &format!("/api/v1/sessions/{session}/folder"),
            Some(json!({ "folder_id": folder })),
        )
        .await;

        // WHEN the folder is deleted
        let (status, body) = call(
            &router,
            "DELETE",
            &format!("/api/v1/sessions/folders/{folder}"),
            None,
        )
        .await;

        // THEN no folder is left and the session is still listed
        assert_eq!(status, StatusCode::OK, "{body}");
        let (_, folders) = call(&router, "GET", "/api/v1/sessions/folders", None).await;
        assert_eq!(folders, json!([]));
        let (_, list) = call(&router, "GET", "/api/v1/sessions", None).await;
        assert_eq!(list.as_array().unwrap().len(), 1);
        assert_eq!(list[0]["id"], json!(session));
    }

    #[tokio::test]
    async fn test_a_session_leaves_its_folder_with_a_null_folder() {
        // GIVEN a folder holding a session
        let dir = tempfile::tempdir().unwrap();
        let router = APIServer::build_router_for_test(chat_state(&dir));
        let session = new_session(&router).await;
        let folder = new_folder(&router, "Clients").await;
        let uri = format!("/api/v1/sessions/{session}/folder");
        call(&router, "PUT", &uri, Some(json!({ "folder_id": folder }))).await;

        // WHEN the session is put in no folder
        let (status, body) = call(&router, "PUT", &uri, Some(json!({ "folder_id": null }))).await;

        // THEN the folder is kept, empty
        assert_eq!(status, StatusCode::OK, "{body}");
        let (_, folders) = call(&router, "GET", "/api/v1/sessions/folders", None).await;
        assert_eq!(folders[0]["session_ids"], json!([]));
    }

    #[tokio::test]
    async fn test_blank_names_and_unknown_folders_or_sessions_are_refused_with_a_reason() {
        // GIVEN a runtime holding one session and one folder
        let dir = tempfile::tempdir().unwrap();
        let router = APIServer::build_router_for_test(chat_state(&dir));
        let session = new_session(&router).await;
        let folder = new_folder(&router, "Clients").await;

        // WHEN a folder is created blank, an unknown one renamed and deleted,
        // the session sent to an unknown folder, an unknown session to the
        // real one
        let (blank, blank_body) = call(
            &router,
            "POST",
            "/api/v1/sessions/folders",
            Some(json!({ "name": "  " })),
        )
        .await;
        let (rename, _) = call(
            &router,
            "PUT",
            "/api/v1/sessions/folders/ghost",
            Some(json!({ "name": "Anything" })),
        )
        .await;
        let (delete, _) = call(&router, "DELETE", "/api/v1/sessions/folders/ghost", None).await;
        let (no_folder, no_folder_body) = call(
            &router,
            "PUT",
            &format!("/api/v1/sessions/{session}/folder"),
            Some(json!({ "folder_id": "ghost" })),
        )
        .await;
        let (no_session, _) = call(
            &router,
            "PUT",
            "/api/v1/sessions/ghost/folder",
            Some(json!({ "folder_id": folder })),
        )
        .await;

        // THEN the blank name is a bad request and the four others are not
        // found, each saying why
        assert_eq!(blank, StatusCode::BAD_REQUEST);
        assert!(blank_body["error"].is_string(), "{blank_body}");
        assert_eq!(rename, StatusCode::NOT_FOUND);
        assert_eq!(delete, StatusCode::NOT_FOUND);
        assert_eq!(no_folder, StatusCode::NOT_FOUND);
        assert_eq!(no_folder_body["error"], json!("folder not found: ghost"));
        assert_eq!(no_session, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_search_answers_the_session_that_contains_the_words_with_an_excerpt() {
        // GIVEN two sessions, one of which holds a message about a lease
        let dir = tempfile::tempdir().unwrap();
        let router = APIServer::build_router_for_test(chat_state(&dir));
        let lease = new_session(&router).await;
        let other = new_session(&router).await;
        say(
            &dir,
            &lease,
            "Please review the commercial lease of the shop",
        );
        say(&dir, &other, "What is on my calendar today?");

        // WHEN two words of that message are searched, the second half typed
        let (status, hits) = call(
            &router,
            "GET",
            "/api/v1/sessions/search?q=lease%20comm",
            None,
        )
        .await;

        // THEN that session alone is answered, with the passage
        assert_eq!(status, StatusCode::OK, "{hits}");
        assert_eq!(hits.as_array().unwrap().len(), 1, "{hits}");
        assert_eq!(hits[0]["session_id"], json!(lease));
        assert_eq!(
            hits[0]["excerpt"],
            json!("Please review the commercial lease of the shop")
        );
    }

    #[tokio::test]
    async fn test_search_leaves_out_a_session_in_the_trash() {
        // GIVEN two sessions that both speak of a lease, one in the trash
        let dir = tempfile::tempdir().unwrap();
        let router = APIServer::build_router_for_test(chat_state(&dir));
        let kept = new_session(&router).await;
        let trashed = new_session(&router).await;
        say(&dir, &kept, "the lease of the office");
        say(&dir, &trashed, "the lease of the warehouse");
        call(
            &router,
            "POST",
            &format!("/api/v1/sessions/{trashed}/trash"),
            None,
        )
        .await;

        // WHEN the word is searched
        let (_, hits) = call(&router, "GET", "/api/v1/sessions/search?q=lease", None).await;

        // THEN only the listed session is answered
        assert_eq!(hits.as_array().unwrap().len(), 1, "{hits}");
        assert_eq!(hits[0]["session_id"], json!(kept));
    }

    #[tokio::test]
    async fn test_search_with_no_word_answers_nothing() {
        // GIVEN a session holding a message
        let dir = tempfile::tempdir().unwrap();
        let router = APIServer::build_router_for_test(chat_state(&dir));
        let session = new_session(&router).await;
        say(&dir, &session, "anything at all");

        // WHEN the search carries no query, then only punctuation
        let (bare, none) = call(&router, "GET", "/api/v1/sessions/search", None).await;
        let (punct, nothing) = call(&router, "GET", "/api/v1/sessions/search?q=%22*", None).await;

        // THEN both answer an empty list, not an error
        assert_eq!(bare, StatusCode::OK);
        assert_eq!(none, json!([]));
        assert_eq!(punct, StatusCode::OK);
        assert_eq!(nothing, json!([]));
    }
}
