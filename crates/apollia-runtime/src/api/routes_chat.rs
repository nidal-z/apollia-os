//! REST routes for the chat subsystem, `POST/GET/DELETE /api/v1/sessions`.
//!
//! Seven endpoints wrapping the [`ChatSessionManagerHandle`]:
//! - `POST   /api/v1/sessions`                , create session
//! - `GET    /api/v1/sessions`                , list sessions
//! - `GET    /api/v1/sessions/:id`            , session detail
//! - `DELETE /api/v1/sessions/:id`            , close session
//! - `POST   /api/v1/sessions/:id/messages`   , send message
//! - `POST   /api/v1/sessions/:id/authorize`  , resolve approval
//! - `GET    /api/v1/sessions/:id/stream`     , SSE stream

use std::convert::Infallible;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};
use tokio_stream::wrappers::{errors::BroadcastStreamRecvError, BroadcastStream};
use tokio_stream::StreamExt;

use apollia_core::todo::TodoItem;
use apollia_core::RuntimeEvent;

use crate::api::routes_sse::TakeWhileInclusiveExt;
use crate::api::server::AppState;
use crate::chat::types::{ChatMode, SessionStatus, ToolDecision};
use crate::coordinator::ExecutionBackend;

/// Request body for `POST /api/v1/sessions`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CreateSessionRequest {
    /// Session mode: `"libre"` or `"agent"`.
    pub mode: String,
    /// Agent name (required when `mode == "agent"`).
    pub agent_name: Option<String>,
    /// Custom system prompt.
    pub system_prompt: Option<String>,
    /// List of tool names available in this session.
    #[serde(default)]
    pub tools: Vec<String>,
    /// Project to link this session to.
    pub project_id: Option<String>,
}

/// Request body for `POST /api/v1/sessions/:id/messages`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct SendMessageRequest {
    /// Text content of the user message.
    pub content: String,
}

/// Response body for `POST /api/v1/sessions/:id/messages`.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct SendMessageResponse {
    /// Unique message identifier.
    pub message_id: String,
    /// Processing status.
    pub status: String,
}

/// Request body for `POST /api/v1/sessions/:id/authorize`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct AuthorizeToolRequest {
    /// ID of the message that triggered the tool call.
    pub message_id: String,
    /// Unique id of the tool call being resolved. Correlates with the
    /// `approval_required` event so the same tool invoked twice in one turn
    /// resolves the right pending slot. Defaults to the tool name for legacy
    /// clients that do not yet send it.
    #[serde(default)]
    pub tool_call_id: Option<String>,
    /// Name of the tool.
    pub tool_name: String,
    /// Decision: `"accept"`, `"refuse"`, or `"always_accept"`.
    pub decision: String,
    /// Free-form rejection reason shared with the agent. Only honoured when
    /// `decision == "refuse"`.
    #[serde(default)]
    pub reason: Option<String>,
    /// Always-accept scope. Only honoured when `decision == "always_accept"`.
    /// Defaults to [`crate::chat::AlwaysAcceptScope::ThisSession`].
    #[serde(default)]
    #[schema(value_type = Option<String>)]
    pub scope: Option<crate::chat::AlwaysAcceptScope>,
}

/// Standard error response body.
#[derive(Debug, Serialize)]
struct ErrorResponse {
    /// Human-readable error description.
    error: String,
}

/// Query params for `GET /api/v1/sessions`.
#[derive(Debug, Deserialize)]
pub struct ListSessionsQuery {
    /// Optional status filter: `"active"`, `"processing"`, `"closed"`.
    pub status: Option<String>,
}

/// Query params for `GET /api/v1/sessions/recent`.
#[derive(Debug, Deserialize)]
pub struct RecentSessionsQuery {
    /// Maximum number of sessions to return (default 10, capped at 50).
    pub limit: Option<usize>,
}

/// Request body for `POST /api/v1/sessions/:id/fork`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct ForkSessionRequest {
    /// Number of messages to copy from the parent (None = all).
    pub up_to_index: Option<usize>,
}

/// Handler for `POST /api/v1/sessions`, create a new chat session.
#[utoipa::path(
    post,
    path = "/api/v1/sessions",
    tag = "chat",
    request_body = CreateSessionRequest,
    responses(
        (status = 201, description = "Session created"),
        (status = 400, description = "Invalid request", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Chat subsystem unavailable", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn create_session<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Json(body): Json<CreateSessionRequest>,
) -> impl IntoResponse {
    let manager = match &state.chat_manager {
        Some(m) => m,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ErrorResponse {
                    error: "chat subsystem not available".into(),
                }),
            )
                .into_response();
        }
    };

    let mode = match body.mode.as_str() {
        "libre" => ChatMode::Libre,
        "agent" => ChatMode::Agent,
        other => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: format!("invalid mode: {other}"),
                }),
            )
                .into_response();
        }
    };

    match manager
        .create_session(crate::chat::manager::CreateSessionParams {
            mode,
            agent_name: body.agent_name,
            system_prompt: body.system_prompt,
            tools: body.tools,
            project_id: body.project_id,
        })
        .await
    {
        Ok(info) => (StatusCode::CREATED, Json(info)).into_response(),
        Err(e) => chat_error_to_response(e).into_response(),
    }
}

/// Handler for `GET /api/v1/sessions`, list sessions.
#[utoipa::path(
    get,
    path = "/api/v1/sessions",
    tag = "chat",
    params(("status" = Option<String>, Query, description = "Filter by session status: active, processing, closed")),
    responses(
        (status = 200, description = "Session list"),
        (status = 503, description = "Chat subsystem unavailable", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn list_sessions<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Query(query): Query<ListSessionsQuery>,
) -> impl IntoResponse {
    let manager = match &state.chat_manager {
        Some(m) => m,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ErrorResponse {
                    error: "chat subsystem not available".into(),
                }),
            )
                .into_response();
        }
    };

    let status_filter = query.status.as_deref().and_then(SessionStatus::from_sql);

    let sessions = manager.list_sessions(status_filter).await;
    (StatusCode::OK, Json(sessions)).into_response()
}

/// Handler for `GET /api/v1/sessions/:id`, session detail.
#[utoipa::path(
    get,
    path = "/api/v1/sessions/{id}",
    tag = "chat",
    params(("id" = String, Path, description = "Session id")),
    responses(
        (status = 200, description = "Session detail"),
        (status = 404, description = "Session not found", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Chat subsystem unavailable", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn get_session<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let manager = match &state.chat_manager {
        Some(m) => m,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ErrorResponse {
                    error: "chat subsystem not available".into(),
                }),
            )
                .into_response();
        }
    };

    match manager.get_session(id).await {
        Some(detail) => (StatusCode::OK, Json(detail)).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "session not found".into(),
            }),
        )
            .into_response(),
    }
}

/// Response body for `GET /api/v1/sessions/:id/todo`.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct TodoReadResponse {
    /// Session whose todo list is returned.
    pub session_id: String,
    /// Current todo items, ordered by insertion.
    #[schema(value_type = Vec<Object>)]
    pub items: Vec<TodoItem>,
}

/// Handler for `GET /api/v1/sessions/:id/todo`, read the session todo list.
///
/// Returns 200 with the items (an empty array when the session exists but has
/// no todo), and 404 when the session is unknown to the runtime.
#[utoipa::path(
    get,
    path = "/api/v1/sessions/{id}/todo",
    tag = "chat",
    params(("id" = String, Path, description = "Session id")),
    responses(
        (status = 200, description = "Session todo list", body = TodoReadResponse),
        (status = 404, description = "Session not found", body = crate::api::openapi::ApiErrorBody),
        (status = 500, description = "Internal error", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Chat subsystem unavailable", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn get_session_todo<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let manager = match &state.chat_manager {
        Some(m) => m,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ErrorResponse {
                    error: "chat subsystem not available".into(),
                }),
            )
                .into_response();
        }
    };

    match manager.get_session_todo(id.clone()).await {
        Ok(items) => (
            StatusCode::OK,
            Json(TodoReadResponse {
                session_id: id,
                items,
            }),
        )
            .into_response(),
        Err(crate::chat::types::ChatError::SessionNotFound(_)) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "session not found".into(),
            }),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )
            .into_response(),
    }
}

/// Handler for `DELETE /api/v1/sessions/:id`, close session.
#[utoipa::path(
    delete,
    path = "/api/v1/sessions/{id}",
    tag = "chat",
    params(("id" = String, Path, description = "Session id")),
    responses(
        (status = 200, description = "Session closed"),
        (status = 404, description = "Session not found", body = crate::api::openapi::ApiErrorBody),
        (status = 409, description = "Session already closed", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Chat subsystem unavailable", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn close_session<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let manager = match &state.chat_manager {
        Some(m) => m,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ErrorResponse {
                    error: "chat subsystem not available".into(),
                }),
            )
                .into_response();
        }
    };

    match manager.close_session(id).await {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({ "status": "closed" })),
        )
            .into_response(),
        Err(e) => chat_error_to_response(e).into_response(),
    }
}

/// Handler for `POST /api/v1/sessions/:id/messages`, send message.
#[utoipa::path(
    post,
    path = "/api/v1/sessions/{id}/messages",
    tag = "chat",
    params(("id" = String, Path, description = "Session id")),
    request_body = SendMessageRequest,
    responses(
        (status = 202, description = "Message accepted", body = SendMessageResponse),
        (status = 404, description = "Session not found", body = crate::api::openapi::ApiErrorBody),
        (status = 409, description = "Session closed or busy", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Chat subsystem unavailable", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn send_message<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
    Json(body): Json<SendMessageRequest>,
) -> impl IntoResponse {
    let manager = match &state.chat_manager {
        Some(m) => m,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ErrorResponse {
                    error: "chat subsystem not available".into(),
                }),
            )
                .into_response();
        }
    };

    match manager.send_message(id, body.content).await {
        Ok(message_id) => (
            StatusCode::ACCEPTED,
            Json(SendMessageResponse {
                message_id,
                status: "processing".into(),
            }),
        )
            .into_response(),
        Err(e) => chat_error_to_response(e).into_response(),
    }
}

/// Handler for `POST /api/v1/sessions/:id/authorize`, resolve tool approval.
#[utoipa::path(
    post,
    path = "/api/v1/sessions/{id}/authorize",
    tag = "chat",
    params(("id" = String, Path, description = "Session id")),
    request_body = AuthorizeToolRequest,
    responses(
        (status = 200, description = "Approval resolved"),
        (status = 400, description = "Invalid decision", body = crate::api::openapi::ApiErrorBody),
        (status = 409, description = "Session not awaiting approval", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Chat subsystem unavailable", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn authorize_tool<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
    Json(body): Json<AuthorizeToolRequest>,
) -> impl IntoResponse {
    let manager = match &state.chat_manager {
        Some(m) => m,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ErrorResponse {
                    error: "chat subsystem not available".into(),
                }),
            )
                .into_response();
        }
    };

    let decision = match body.decision.as_str() {
        "accept" => ToolDecision::Accept,
        "refuse" => ToolDecision::Refuse {
            reason: body.reason.clone(),
        },
        "always_accept" => ToolDecision::AlwaysAccept {
            scope: body
                .scope
                .unwrap_or_else(crate::chat::AlwaysAcceptScope::safe_default),
        },
        other => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: format!("invalid decision: {other}"),
                }),
            )
                .into_response();
        }
    };

    // Fall back to the tool name when a legacy client omits the id (matches the
    // pre-correlation key, so single-approval turns keep working).
    let tool_call_id = body.tool_call_id.unwrap_or_else(|| body.tool_name.clone());
    match manager
        .resolve_tool(id, body.message_id, tool_call_id, body.tool_name, decision)
        .await
    {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({ "status": "resolved" })),
        )
            .into_response(),
        Err(e) => chat_error_to_response(e).into_response(),
    }
}

/// Handler for `GET /api/v1/sessions/:id/stream`, SSE streaming.
///
/// Opens a persistent SSE stream filtering `ChatXxx` [`RuntimeEvent`]s by `session_id`.
/// The stream closes when `ChatSessionClosed` is emitted.
#[utoipa::path(
    get,
    path = "/api/v1/sessions/{id}/stream",
    tag = "chat",
    params(("id" = String, Path, description = "Session id")),
    responses(
        (status = 200, description = "SSE stream of chat events", body = crate::api::routes_sse::SseTaskEvent, content_type = "text/event-stream"),
        (status = 404, description = "Session not found", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Chat subsystem unavailable", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn stream_session<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
) -> axum::response::Response {
    let manager = match &state.chat_manager {
        Some(m) => m,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ErrorResponse {
                    error: "chat subsystem not available".into(),
                }),
            )
                .into_response();
        }
    };

    // Verifier l'existence de la session avant de s'abonner (parite avec
    // `stream_task`): sinon un id inconnu ouvre un flux qui n'emet jamais rien.
    if manager.get_session(id.clone()).await.is_none() {
        return (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("session not found: {id}"),
            }),
        )
            .into_response();
    }

    let rx = state.event_sender.subscribe();
    let stream = BroadcastStream::new(rx);
    let session_id = id;

    let sse_stream = stream
        .filter_map(move |result| {
            let sid = session_id.clone();
            match result {
                Ok(event) => chat_event_to_sse(&event, &sid),
                // A `BroadcastStream` cannot resubscribe, so the lag rule is
                // held here to its first half: the drop is named, never silent.
                Err(BroadcastStreamRecvError::Lagged(skipped)) => {
                    tracing::warn!(subscriber = "api.chat_sse", skipped, "eventbus.lagged");
                    None
                }
            }
        })
        // Fermer le flux apres l'evenement terminal (`session_closed`).
        .take_while_inclusive(|(_event, is_terminal)| !is_terminal)
        .map(|(sse_event, _)| {
            let json = serde_json::to_string(&sse_event).unwrap_or_else(|_| "{}".into());
            Ok::<_, Infallible>(Event::default().data(json).event(sse_event.event))
        });

    Sse::new(sse_stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

/// SSE event payload for chat events.
#[derive(Debug, Serialize)]
struct SseChatEvent {
    /// Event type discriminator.
    event: String,
    /// Additional event data.
    #[serde(flatten)]
    data: serde_json::Value,
}

/// Convert a [`RuntimeEvent`] to an SSE payload and its terminal flag if it
/// matches the session.
///
/// Returns `None` for events not relevant to this session. The boolean is
/// `true` for the terminal `session_closed` event so the caller can close the
/// stream after emitting it.
fn chat_event_to_sse(event: &RuntimeEvent, session_id: &str) -> Option<(SseChatEvent, bool)> {
    let (sse_event, is_terminal) = match event {
        RuntimeEvent::ChatMessageSent {
            session_id: sid,
            message_id,
        } if sid == session_id => (
            SseChatEvent {
                event: "message_sent".into(),
                data: serde_json::json!({ "message_id": message_id }),
            },
            false,
        ),
        RuntimeEvent::ChatResponseStarted {
            session_id: sid,
            message_id,
            run_id: _,
        } if sid == session_id => (
            SseChatEvent {
                event: "response_started".into(),
                data: serde_json::json!({ "message_id": message_id }),
            },
            false,
        ),
        RuntimeEvent::ChatToken {
            session_id: sid,
            message_id,
            token,
        } if sid == session_id => (
            SseChatEvent {
                event: "token".into(),
                data: serde_json::json!({ "message_id": message_id, "token": token }),
            },
            false,
        ),
        RuntimeEvent::ChatResponseCompleted {
            session_id: sid,
            message_id,
            content,
            run_id: _,
        } if sid == session_id => (
            SseChatEvent {
                event: "response_completed".into(),
                data: serde_json::json!({ "message_id": message_id, "content": content }),
            },
            false,
        ),
        RuntimeEvent::ChatError {
            session_id: sid,
            message_id,
            error,
        } if sid == session_id => (
            SseChatEvent {
                event: "error".into(),
                data: serde_json::json!({ "message_id": message_id, "error": error }),
            },
            false,
        ),
        RuntimeEvent::ChatToolCallStarted {
            session_id: sid,
            message_id,
            tool_name,
            input_preview,
            rationale,
        } if sid == session_id => (
            SseChatEvent {
                event: "tool_call_started".into(),
                data: serde_json::json!({
                    "message_id": message_id,
                    "tool_name": tool_name,
                    "input_preview": input_preview,
                    "rationale": rationale,
                }),
            },
            false,
        ),
        RuntimeEvent::ChatToolCallCompleted {
            session_id: sid,
            message_id,
            tool_name,
            success,
            output_preview,
            analysis,
        } if sid == session_id => (
            SseChatEvent {
                event: "tool_call_completed".into(),
                data: serde_json::json!({
                    "message_id": message_id,
                    "tool_name": tool_name,
                    "success": success,
                    "output_preview": output_preview,
                    "analysis": analysis,
                }),
            },
            false,
        ),
        RuntimeEvent::ChatApprovalRequired {
            session_id: sid,
            message_id,
            tool_call_id,
            tool_name,
            prompt,
        } if sid == session_id => (
            SseChatEvent {
                event: "approval_required".into(),
                data: serde_json::json!({
                    "message_id": message_id,
                    "tool_call_id": tool_call_id,
                    "tool_name": tool_name,
                    "prompt": prompt,
                }),
            },
            false,
        ),
        RuntimeEvent::ChatApprovalResolved {
            session_id: sid,
            message_id,
            tool_call_id,
            tool_name,
            decision,
        } if sid == session_id => (
            SseChatEvent {
                event: "approval_resolved".into(),
                data: serde_json::json!({
                    "message_id": message_id,
                    "tool_call_id": tool_call_id,
                    "tool_name": tool_name,
                    "decision": decision,
                }),
            },
            false,
        ),
        RuntimeEvent::ChatSessionClosed { session_id: sid } if sid == session_id => (
            SseChatEvent {
                event: "session_closed".into(),
                data: serde_json::json!({}),
            },
            true,
        ),
        _ => return None,
    };

    Some((sse_event, is_terminal))
}

/// Handler for `GET /api/v1/sessions/recent`, list recent sessions with first message.
///
/// Query param `?limit=N` (default 10, max 50).
#[utoipa::path(
    get,
    path = "/api/v1/sessions/recent",
    tag = "chat",
    params(("limit" = Option<usize>, Query, description = "Maximum sessions to return (default 10, capped at 50)")),
    responses(
        (status = 200, description = "Recent session summaries"),
        (status = 503, description = "Chat subsystem unavailable", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn list_recent_sessions<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Query(query): Query<RecentSessionsQuery>,
) -> impl IntoResponse {
    let manager = match &state.chat_manager {
        Some(m) => m,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({ "error": "chat subsystem not available" })),
            )
                .into_response();
        }
    };

    let limit = query.limit.unwrap_or(10).min(50);
    let summaries = manager.list_recent_summaries(limit).await;
    (StatusCode::OK, Json(summaries)).into_response()
}

/// Handler for `POST /api/v1/sessions/:id/resume`, resume an existing session.
///
/// Loads the session from SQLite if not already in memory, resets any stale
/// Processing status to Active, and returns the full session detail.
#[utoipa::path(
    post,
    path = "/api/v1/sessions/{id}/resume",
    tag = "chat",
    params(("id" = String, Path, description = "Session id")),
    responses(
        (status = 200, description = "Session resumed"),
        (status = 404, description = "Session not found", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Chat subsystem unavailable", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn resume_session<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let manager = match &state.chat_manager {
        Some(m) => m,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ErrorResponse {
                    error: "chat subsystem not available".into(),
                }),
            )
                .into_response();
        }
    };

    match manager.resume_session(id).await {
        Ok(detail) => (StatusCode::OK, Json(detail)).into_response(),
        Err(e) => chat_error_to_response(e).into_response(),
    }
}

/// Handler for `POST /api/v1/sessions/:id/fork`, fork a session.
///
/// Creates a new child session that copies the parent history up to `up_to_index`
/// messages. When `up_to_index` is omitted, the full history is copied.
/// Returns the new child [`SessionInfo`] with HTTP 201.
#[utoipa::path(
    post,
    path = "/api/v1/sessions/{id}/fork",
    tag = "chat",
    params(("id" = String, Path, description = "Parent session id")),
    request_body = ForkSessionRequest,
    responses(
        (status = 201, description = "Child session created"),
        (status = 404, description = "Session not found", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Chat subsystem unavailable", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn fork_session<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
    Json(body): Json<ForkSessionRequest>,
) -> impl IntoResponse {
    let manager = match &state.chat_manager {
        Some(m) => m,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ErrorResponse {
                    error: "chat subsystem not available".into(),
                }),
            )
                .into_response();
        }
    };

    match manager.fork_session(id, body.up_to_index).await {
        Ok(info) => (StatusCode::CREATED, Json(info)).into_response(),
        Err(e) => chat_error_to_response(e).into_response(),
    }
}

/// Handler for `GET /api/v1/sessions/:id/children`, list fork children of a session.
///
/// Returns a JSON array of [`SessionInfo`] objects, ordered by creation time ascending.
#[utoipa::path(
    get,
    path = "/api/v1/sessions/{id}/children",
    tag = "chat",
    params(("id" = String, Path, description = "Parent session id")),
    responses(
        (status = 200, description = "Child session list"),
        (status = 503, description = "Chat subsystem unavailable", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn list_session_children<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let manager = match &state.chat_manager {
        Some(m) => m,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ErrorResponse {
                    error: "chat subsystem not available".into(),
                }),
            )
                .into_response();
        }
    };

    let children = manager.list_children(id).await;
    (StatusCode::OK, Json(children)).into_response()
}

/// Map [`ChatError`] to an HTTP response.
fn chat_error_to_response(err: crate::chat::types::ChatError) -> (StatusCode, Json<ErrorResponse>) {
    use crate::chat::types::ChatError;
    let (status, msg) = match &err {
        ChatError::SessionNotFound(_) => (StatusCode::NOT_FOUND, err.to_string()),
        ChatError::SessionClosed(_) => (StatusCode::CONFLICT, err.to_string()),
        ChatError::SessionBusy(_) => (StatusCode::CONFLICT, err.to_string()),
        ChatError::AgentNotFound(_) => (StatusCode::BAD_REQUEST, err.to_string()),
        ChatError::AgentLoadFailed(_) => (StatusCode::BAD_REQUEST, err.to_string()),
        ChatError::NoLlmConfigured => (StatusCode::BAD_REQUEST, err.to_string()),
        ChatError::BudgetExhausted => (StatusCode::TOO_MANY_REQUESTS, err.to_string()),
        ChatError::InternalError(_) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        ChatError::ProjectNotFound(_) => (StatusCode::NOT_FOUND, err.to_string()),
        ChatError::NotAwaitingApproval { .. } => (StatusCode::CONFLICT, err.to_string()),
        ChatError::CostCeilingExceeded { .. } => (StatusCode::PAYMENT_REQUIRED, err.to_string()),
    };
    (status, Json(ErrorResponse { error: msg }))
}

#[cfg(test)]
mod tests {
    use crate::api::server::{APIServer, AppState};
    use crate::chat::manager::ChatSessionManagerHandle;
    use crate::coordinator::{DynBackend, ExecutionBackend};
    use crate::eventbus::EventBus;
    use crate::registry::AgentRegistry;
    use crate::router::TaskRouterHandle;
    use apollia_core::{AIPResult, AIPTask, AgentManifest, StepBudgetConfig, TaskStatus};
    use apollia_llm::LlmRouter;
    use apollia_tools::ToolRegistryHandle;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use std::future::Future;
    use std::path::Path;
    use std::pin::Pin;
    use std::sync::Arc;
    use tower::ServiceExt;

    #[derive(Clone)]
    struct MockBackend;

    impl From<DynBackend> for MockBackend {
        fn from(_: DynBackend) -> Self {
            MockBackend
        }
    }

    impl ExecutionBackend for MockBackend {
        fn execute(
            &self,
            _task: AIPTask,
        ) -> Pin<Box<dyn Future<Output = Result<AIPResult, String>> + Send>> {
            Box::pin(async {
                Ok(AIPResult {
                    task_id: String::new(),
                    status: TaskStatus::Completed,
                    output: Vec::new(),
                    error: None,
                    artifacts: Vec::new(),
                    input_required_data: None,
                })
            })
        }
    }

    struct AlwaysOkLoader;
    impl crate::api::routes_agents::AgentLoader for AlwaysOkLoader {
        fn load_and_validate(&self, _path: &Path) -> Result<AgentManifest, String> {
            Ok(AgentManifest {
                format_version: 1,
                name: "test-agent".into(),
                version: "0.1.0".into(),
                description: "test".into(),
                tools_required: vec![],
                tools_optional: vec![],
                supports_streaming: false,
                supports_a2a: false,
                memory_namespace: None,
                shared_memory_namespaces: vec![],
                max_concurrent_tasks: 1,
                step_budget: None,
                network_allowlist: None,
                dangerous_tools_allowed: false,
                tags: vec![],
                skills: vec![],
                execution_mode: "auto".into(),
                supports_mailbox: false,
                mailbox_allowlist: None,
                system_prompt: None,
                tools_requiring_approval: vec![],
                llm_backend: None,
                packages: vec![],
                memory_config: None,
                agent_type: None,
                examples: vec![],
                limitations: vec![],
                setup_notes: None,
                agent_class: None,
                user_memory_write: false,
                datasources: vec![],
                templates: vec![],
                secrets: vec![],
                check_commands: vec![],
            })
        }
    }

    fn test_router_with_chat(dir: &tempfile::TempDir) -> axum::Router {
        let (event_tx, _) = EventBus::new();
        let registry_handle = AgentRegistry::spawn(event_tx.clone());
        let router_handle: TaskRouterHandle<MockBackend> =
            TaskRouterHandle::spawn(registry_handle.clone(), event_tx.clone(), 64);
        let tool_registry = ToolRegistryHandle::start();

        let db_path = dir.path().join("chat.db");
        let chat_manager = ChatSessionManagerHandle::spawn(
            &db_path,
            Some(Arc::new(LlmRouter::empty())),
            tool_registry.clone(),
            Arc::new(AlwaysOkLoader),
            None,
            event_tx.clone(),
            StepBudgetConfig::default(),
            None,
            registry_handle.clone(),
            None,
            None,
            None,
            None,
            None,
            apollia_mcp::session::LoadingMode::Eager,
            20,
            None,  // no hooks in tests
            false, // plan-mode default off in tests
        )
        .expect("spawn chat manager");

        let state: AppState<MockBackend> = AppState {
            router_handle,
            registry_handle,
            event_sender: event_tx,
            agent_loader: Arc::new(AlwaysOkLoader),
            backend: MockBackend,
            llm_router: crate::api::server::shared_llm_router_from(Some(Arc::new(
                LlmRouter::empty(),
            ))),
            trigger_engine: None,
            config_path: None,
            task_repository: None,
            pending_approvals: None,
            plan_gates: None,
            notification_config: None,
            backend_factory: None,
            tool_registry_handle: Some(tool_registry),
            audit_trail: None,
            audit_journal: None,
            obs_config: apollia_core::ObservabilityConfig::default(),
            llm_call_repository: None,
            trigger_def_repo: None,
            notification_repo: None,
            notification_engine_handle: None,
            chat_manager: Some(chat_manager),
            plan_cache: None,
            mailbox_handle: None,
            user_memory: None,
            data_dir: std::path::PathBuf::new(),
            stt_engine: crate::api::server::empty_shared_stt_engine(),
            stt_repository: crate::api::server::empty_shared_stt_repository(),
            mcp_handle: None,
            mcp_server_repo: None,
            llm_backend_repo: None,
            stt_config_repo: None,
            a2a_invoker: None,
            resilience_layer: None,
            runner_proxy: None,
            llama_server_supervisor: None,
        };

        APIServer::build_router_for_test(state)
    }

    #[tokio::test]
    async fn test_post_sessions_creates_session() {
        // GIVEN a router with chat enabled
        let dir = tempfile::tempdir().expect("tempdir");
        let router = test_router_with_chat(&dir);

        // WHEN POST /api/v1/sessions
        let body = serde_json::json!({ "mode": "libre", "tools": ["bash_executor"] });
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/sessions")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();

        // THEN 201 Created
        assert_eq!(resp.status(), StatusCode::CREATED);
        let bytes = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["mode"], "Libre");
        assert_eq!(json["status"], "Active");
    }

    #[tokio::test]
    async fn test_post_sessions_agent_without_name_returns_400() {
        // GIVEN a router
        let dir = tempfile::tempdir().expect("tempdir");
        let router = test_router_with_chat(&dir);

        // WHEN POST with mode=agent but no agent_name
        let body = serde_json::json!({ "mode": "agent" });
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/sessions")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();

        // THEN 400
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_get_sessions_returns_list() {
        // GIVEN 2 sessions
        let dir = tempfile::tempdir().expect("tempdir");
        let router = test_router_with_chat(&dir);

        // Create 2 sessions
        for _ in 0..2 {
            let body = serde_json::json!({ "mode": "libre" });
            let req = Request::builder()
                .method("POST")
                .uri("/api/v1/sessions")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap();
            let _ = router.clone().oneshot(req).await.unwrap();
        }

        // WHEN GET /api/v1/sessions
        let req = Request::builder()
            .uri("/api/v1/sessions")
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();

        // THEN 200 with 2 sessions
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
        let json: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json.len(), 2);
    }

    #[tokio::test]
    async fn test_get_session_not_found_returns_404() {
        // GIVEN a router
        let dir = tempfile::tempdir().expect("tempdir");
        let router = test_router_with_chat(&dir);

        // WHEN GET /api/v1/sessions/nonexistent
        let req = Request::builder()
            .uri("/api/v1/sessions/nonexistent")
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();

        // THEN 404
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_delete_session_closes_it() {
        // GIVEN an active session
        let dir = tempfile::tempdir().expect("tempdir");
        let router = test_router_with_chat(&dir);

        let body = serde_json::json!({ "mode": "libre" });
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/sessions")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = router.clone().oneshot(req).await.unwrap();
        let bytes = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let session_id = json["id"].as_str().unwrap();

        // WHEN DELETE /api/v1/sessions/:id
        let req = Request::builder()
            .method("DELETE")
            .uri(format!("/api/v1/sessions/{session_id}"))
            .body(Body::empty())
            .unwrap();
        let resp = router.clone().oneshot(req).await.unwrap();

        // THEN 200
        assert_eq!(resp.status(), StatusCode::OK);

        // AND GET returns Closed status
        let req = Request::builder()
            .uri(format!("/api/v1/sessions/{session_id}"))
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();
        let bytes = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["session"]["status"], "Closed");
    }

    #[tokio::test]
    async fn test_delete_closed_session_returns_409() {
        // GIVEN a closed session
        let dir = tempfile::tempdir().expect("tempdir");
        let router = test_router_with_chat(&dir);

        let body = serde_json::json!({ "mode": "libre" });
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/sessions")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = router.clone().oneshot(req).await.unwrap();
        let bytes = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let session_id = json["id"].as_str().unwrap();

        // Close it
        let req = Request::builder()
            .method("DELETE")
            .uri(format!("/api/v1/sessions/{session_id}"))
            .body(Body::empty())
            .unwrap();
        let _ = router.clone().oneshot(req).await.unwrap();

        // WHEN DELETE again
        let req = Request::builder()
            .method("DELETE")
            .uri(format!("/api/v1/sessions/{session_id}"))
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();

        // THEN 409 Conflict
        assert_eq!(resp.status(), StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn test_post_message_returns_202() {
        // GIVEN an active session
        let dir = tempfile::tempdir().expect("tempdir");
        let router = test_router_with_chat(&dir);

        let body = serde_json::json!({ "mode": "libre" });
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/sessions")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = router.clone().oneshot(req).await.unwrap();
        let bytes = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let session_id = json["id"].as_str().unwrap();

        // WHEN POST /api/v1/sessions/:id/messages
        let body = serde_json::json!({ "content": "Bonjour" });
        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/v1/sessions/{session_id}/messages"))
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();

        // THEN 202 Accepted
        assert_eq!(resp.status(), StatusCode::ACCEPTED);
        let bytes = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["status"], "processing");
        assert!(json["message_id"].is_string());
    }
}
