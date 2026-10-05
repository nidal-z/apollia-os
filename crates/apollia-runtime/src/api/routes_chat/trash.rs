//! The trash of the conversations.
//!
//! - `GET  /api/v1/sessions/trash`        , the conversations in the trash
//! - `POST /api/v1/sessions/{id}/trash`   , send one there
//! - `POST /api/v1/sessions/{id}/restore` , take one back out
//!
//! A conversation in the trash keeps its messages and leaves every listing.
//! `POST /api/v1/sessions/{id}/erase` removes a conversation for good,
//! whether it is in the trash or not.

use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};

use super::conversations::{chat_refusal, done, manager, Refusal};
use crate::api::server::AppState;
use crate::chat::TrashedSession;
use crate::coordinator::ExecutionBackend;

/// Build the router of these routes.
pub fn trash_router<B: ExecutionBackend + Clone>() -> Router<AppState<B>> {
    Router::new()
        .route("/api/v1/sessions/trash", get(list_trashed_sessions::<B>))
        .route("/api/v1/sessions/:id/trash", post(trash_session::<B>))
        .route("/api/v1/sessions/:id/restore", post(restore_session::<B>))
}

/// Handler for `GET /api/v1/sessions/trash`, the conversations in the trash,
/// the last removed first.
#[utoipa::path(
    get,
    path = "/api/v1/sessions/trash",
    tag = "chat",
    responses(
        (status = 200, description = "The conversations in the trash", body = Vec<TrashedSession>),
        (status = 503, description = "Chat subsystem unavailable", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn list_trashed_sessions<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Result<Json<Vec<TrashedSession>>, Refusal> {
    manager(&state)?
        .list_trashed_sessions()
        .await
        .map(Json)
        .map_err(chat_refusal)
}

/// Handler for `POST /api/v1/sessions/{id}/trash`, send the session to the
/// trash: it leaves the list and keeps its messages.
#[utoipa::path(
    post,
    path = "/api/v1/sessions/{id}/trash",
    tag = "chat",
    params(("id" = String, Path, description = "Session id")),
    responses(
        (status = 200, description = "Session sent to the trash"),
        (status = 404, description = "Session not found", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Chat subsystem unavailable", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn trash_session<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
) -> Result<axum::response::Response, Refusal> {
    manager(&state)?
        .trash_session(id)
        .await
        .map_err(chat_refusal)?;
    Ok(done("trashed"))
}

/// Handler for `POST /api/v1/sessions/{id}/restore`, take the session out of
/// the trash, back into the list with its messages.
#[utoipa::path(
    post,
    path = "/api/v1/sessions/{id}/restore",
    tag = "chat",
    params(("id" = String, Path, description = "Session id")),
    responses(
        (status = 200, description = "Session restored"),
        (status = 404, description = "No such session in the trash", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Chat subsystem unavailable", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn restore_session<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
) -> Result<axum::response::Response, Refusal> {
    manager(&state)?
        .restore_session(id)
        .await
        .map_err(chat_refusal)?;
    Ok(done("restored"))
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use serde_json::{json, Value};
    use tower::ServiceExt;

    use super::super::tests::chat_state;
    use crate::api::server::APIServer;

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

    fn ids(list: &Value) -> Vec<String> {
        list.as_array()
            .unwrap()
            .iter()
            .map(|s| s["id"].as_str().unwrap().to_owned())
            .collect()
    }

    #[tokio::test]
    async fn test_trash_takes_the_session_out_of_the_list_and_into_the_trash() {
        // GIVEN a runtime holding two sessions
        let dir = tempfile::tempdir().unwrap();
        let router = APIServer::build_router_for_test(chat_state(&dir));
        let kept = new_session(&router).await;
        let removed = new_session(&router).await;

        // WHEN one is sent to the trash
        let (status, body) = call(
            &router,
            "POST",
            &format!("/api/v1/sessions/{removed}/trash"),
            None,
        )
        .await;

        // THEN the list and the recent sessions hold the other one only, and
        // the trash holds this one with the date it was removed
        assert_eq!(status, StatusCode::OK, "{body}");
        let (_, list) = call(&router, "GET", "/api/v1/sessions", None).await;
        assert_eq!(ids(&list), std::slice::from_ref(&kept));
        let (_, recent) = call(&router, "GET", "/api/v1/sessions/recent", None).await;
        assert_eq!(ids(&recent), [kept]);
        let (status, trash) = call(&router, "GET", "/api/v1/sessions/trash", None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(ids(&trash), [removed]);
        assert!(trash[0]["deleted_at"].is_string(), "{trash}");
    }

    #[tokio::test]
    async fn test_restore_puts_the_session_back_in_the_list() {
        // GIVEN a session in the trash
        let dir = tempfile::tempdir().unwrap();
        let router = APIServer::build_router_for_test(chat_state(&dir));
        let id = new_session(&router).await;
        call(
            &router,
            "POST",
            &format!("/api/v1/sessions/{id}/trash"),
            None,
        )
        .await;

        // WHEN it is restored
        let (status, body) = call(
            &router,
            "POST",
            &format!("/api/v1/sessions/{id}/restore"),
            None,
        )
        .await;

        // THEN the list holds it again and the trash is empty
        assert_eq!(status, StatusCode::OK, "{body}");
        let (_, list) = call(&router, "GET", "/api/v1/sessions", None).await;
        assert_eq!(ids(&list), [id]);
        let (_, trash) = call(&router, "GET", "/api/v1/sessions/trash", None).await;
        assert_eq!(trash, json!([]));
    }

    #[tokio::test]
    async fn test_erase_removes_a_trashed_session_for_good() {
        // GIVEN a session in the trash
        let dir = tempfile::tempdir().unwrap();
        let router = APIServer::build_router_for_test(chat_state(&dir));
        let id = new_session(&router).await;
        call(
            &router,
            "POST",
            &format!("/api/v1/sessions/{id}/trash"),
            None,
        )
        .await;

        // WHEN it is erased
        let (status, _) = call(
            &router,
            "POST",
            &format!("/api/v1/sessions/{id}/erase"),
            None,
        )
        .await;

        // THEN the trash is empty and the session cannot be restored
        assert_eq!(status, StatusCode::OK);
        let (_, trash) = call(&router, "GET", "/api/v1/sessions/trash", None).await;
        assert_eq!(trash, json!([]));
        let (status, _) = call(
            &router,
            "POST",
            &format!("/api/v1/sessions/{id}/restore"),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_trash_of_an_unknown_session_and_restore_of_a_listed_one_are_404() {
        // GIVEN a runtime holding one session, in the list
        let dir = tempfile::tempdir().unwrap();
        let router = APIServer::build_router_for_test(chat_state(&dir));
        let id = new_session(&router).await;

        // WHEN an unknown session is trashed and the listed one is restored
        let (unknown, body) = call(&router, "POST", "/api/v1/sessions/ghost/trash", None).await;
        let (listed, _) = call(
            &router,
            "POST",
            &format!("/api/v1/sessions/{id}/restore"),
            None,
        )
        .await;

        // THEN both are refused as not found, with a reason
        assert_eq!(unknown, StatusCode::NOT_FOUND);
        assert!(body["error"].is_string(), "{body}");
        assert_eq!(listed, StatusCode::NOT_FOUND);
    }
}
