//! REST route for agent-to-agent messages, `GET /api/v1/agents/:name/messages`.
//!
//! Exposes the in-memory mailbox contents so the desktop frontend can display
//! inter-agent communication. Returns `503` when the mailbox is not available.

use std::convert::Infallible;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::Json;
use serde::{Deserialize, Serialize};
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

use apollia_core::RuntimeEvent;

use crate::api::server::AppState;
use crate::coordinator::{DynBackend, ExecutionBackend};
use crate::mailbox::{AgentMessage, MailboxError};

/// Query parameters for `GET /api/v1/agents/:name/messages`.
#[derive(Debug, Deserialize)]
pub struct ListMessagesQuery {
    /// Maximum number of messages to return (default 50, max 200).
    pub limit: Option<u32>,
}

/// Response body for `GET /api/v1/agents/:name/messages`.
#[derive(Serialize, utoipa::ToSchema)]
pub struct AgentMessagesResponse {
    /// Messages sorted by `sent_at` descending (most recent first).
    pub messages: Vec<AgentMessageDto>,
}

/// DTO for a single agent message.
#[derive(Serialize, utoipa::ToSchema)]
pub struct AgentMessageDto {
    /// Name of the sending agent.
    pub from_agent: String,
    /// Name of the receiving agent.
    pub to_agent: String,
    /// Arbitrary JSON payload.
    #[schema(value_type = Object)]
    pub payload: serde_json::Value,
    /// Timestamp (RFC 3339).
    pub sent_at: String,
}

/// Error response body.
#[derive(Serialize)]
pub struct ErrorResponse {
    /// Human-readable error description.
    error: String,
}

/// Maximum messages the endpoint will return.
const MAX_LIMIT: u32 = 200;
/// Default limit when not specified.
const DEFAULT_LIMIT: u32 = 50;

/// `GET /api/v1/agents/:name/messages`, list messages for an agent.
///
/// Returns messages from the in-memory mailbox, sorted by `sent_at` descending.
/// Returns an empty array when the agent has no messages.
/// Returns `503` when the mailbox is not available.
#[utoipa::path(
    get,
    path = "/api/v1/agents/{name}/messages",
    tag = "agents",
    params(
        ("name" = String, Path, description = "Agent name"),
        ("limit" = Option<u32>, Query, description = "Maximum number of messages (default 50, max 200)"),
    ),
    responses(
        (status = 200, description = "Agent messages", body = AgentMessagesResponse),
        (status = 503, description = "Mailbox not available", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn list_agent_messages<B: ExecutionBackend + Clone + From<DynBackend>>(
    State(state): State<AppState<B>>,
    Path(agent_name): Path<String>,
    Query(query): Query<ListMessagesQuery>,
) -> Result<(StatusCode, Json<AgentMessagesResponse>), (StatusCode, Json<ErrorResponse>)> {
    let mailbox = state.mailbox_handle.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "agent mailbox not available".to_string(),
            }),
        )
    })?;

    let effective_limit = match query.limit {
        Some(0) | None => DEFAULT_LIMIT,
        Some(l) if l > MAX_LIMIT => MAX_LIMIT,
        Some(l) => l,
    };

    let messages = mailbox
        .list_messages(&agent_name, effective_limit as usize)
        .await;

    let dtos: Vec<AgentMessageDto> = messages
        .into_iter()
        .map(|m: AgentMessage| AgentMessageDto {
            from_agent: m.from,
            to_agent: agent_name.clone(),
            payload: m.payload,
            sent_at: m.sent_at,
        })
        .collect();

    Ok((
        StatusCode::OK,
        Json(AgentMessagesResponse { messages: dtos }),
    ))
}

/// Request body for `POST /api/v1/agents/:name/messages`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct InjectMessageBody {
    /// Arbitrary JSON payload to deliver to the agent's inbox.
    #[schema(value_type = Object)]
    pub payload: serde_json::Value,
    /// Optional host identifier; the sender is recorded as `host:<id>` (or
    /// `host` when absent), so injected traffic is distinguishable in the audit.
    #[serde(default)]
    pub from: Option<String>,
}

/// Response body for a successful injection.
#[derive(Serialize, utoipa::ToSchema)]
pub struct InjectMessageResponse {
    /// Identifier assigned to the injected message.
    pub message_id: String,
}

/// `POST /api/v1/agents/:name/messages`, inject a message from the host.
///
/// The host deposits a message into an agent's durable inbox. The recipient is
/// validated against the registry (`404` if unknown). A synthetic host-scoped
/// `run_id` is allocated so the injected message is journaled like any other.
/// Returns `503` when the mailbox is not available.
#[utoipa::path(
    post,
    path = "/api/v1/agents/{name}/messages",
    tag = "agents",
    params(("name" = String, Path, description = "Recipient agent name")),
    request_body = InjectMessageBody,
    responses(
        (status = 201, description = "Message injected", body = InjectMessageResponse),
        (status = 404, description = "Unknown recipient", body = crate::api::openapi::ApiErrorBody),
        (status = 413, description = "Payload too large", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Mailbox not available", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn inject_agent_message<B: ExecutionBackend + Clone + From<DynBackend>>(
    State(state): State<AppState<B>>,
    Path(agent_name): Path<String>,
    Json(body): Json<InjectMessageBody>,
) -> Result<(StatusCode, Json<InjectMessageResponse>), (StatusCode, Json<ErrorResponse>)> {
    let err = |code: StatusCode, msg: String| (code, Json(ErrorResponse { error: msg }));

    let mailbox = state.mailbox_handle.as_ref().ok_or_else(|| {
        err(
            StatusCode::SERVICE_UNAVAILABLE,
            "agent mailbox not available".to_string(),
        )
    })?;

    // Validate the recipient exists (fail-fast on an unknown target).
    let known = state
        .registry_handle
        .find_by_name(&agent_name)
        .await
        .ok()
        .flatten()
        .is_some();
    if !known {
        return Err(err(
            StatusCode::NOT_FOUND,
            format!("unknown recipient '{agent_name}'"),
        ));
    }

    // Host injections carry a synthetic host-scoped run so they are auditable.
    let run_id = apollia_core::RunId::new();
    let from = match body.from {
        Some(id) if !id.is_empty() => format!("host:{id}"),
        _ => "host".to_string(),
    };

    match mailbox
        .send(&from, &agent_name, body.payload, Some(run_id))
        .await
    {
        Ok(message_id) => Ok((
            StatusCode::CREATED,
            Json(InjectMessageResponse { message_id }),
        )),
        Err(MailboxError::PayloadTooLarge { size, max }) => Err(err(
            StatusCode::PAYLOAD_TOO_LARGE,
            format!("payload too large ({size} bytes, max {max})"),
        )),
        Err(MailboxError::QueueFull { agent, capacity }) => Err(err(
            StatusCode::SERVICE_UNAVAILABLE,
            format!("mailbox full for '{agent}' (max {capacity})"),
        )),
        Err(e) => Err(err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    }
}

/// SSE frame emitted by the mailbox observation stream.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct SseMailboxEvent {
    /// Event kind: `sent`, `delivered`, `acked`, `dropped`, or `guard`.
    pub event: String,
    /// Event fields (flattened into the top-level JSON object).
    #[serde(flatten)]
    #[schema(value_type = Object)]
    pub data: serde_json::Value,
}

/// Maps a [`RuntimeEvent`] to a mailbox SSE frame, or `None` if unrelated.
fn mailbox_event_to_sse(event: &RuntimeEvent) -> Option<SseMailboxEvent> {
    let (kind, data) = match event {
        RuntimeEvent::AgentMessageSent {
            from,
            to,
            message_id,
            payload_hash,
            ..
        } => (
            "sent",
            serde_json::json!({ "from": from, "to": to, "message_id": message_id, "payload_hash": payload_hash }),
        ),
        RuntimeEvent::AgentMessageDelivered { to, message_id, .. } => (
            "delivered",
            serde_json::json!({ "to": to, "message_id": message_id }),
        ),
        RuntimeEvent::AgentMessageAcked { to, message_id, .. } => (
            "acked",
            serde_json::json!({ "to": to, "message_id": message_id }),
        ),
        RuntimeEvent::AgentMessageDropped {
            to,
            message_id,
            reason,
            ..
        } => (
            "dropped",
            serde_json::json!({ "to": to, "message_id": message_id, "reason": reason }),
        ),
        RuntimeEvent::MailboxGuardTriggered {
            guard_type,
            caller,
            detail,
        } => (
            "guard",
            serde_json::json!({ "guard_type": guard_type, "caller": caller, "detail": detail }),
        ),
        _ => return None,
    };
    Some(SseMailboxEvent {
        event: kind.to_string(),
        data,
    })
}

/// `GET /api/v1/mailbox/stream`, observe all mailbox traffic as SSE.
///
/// Streams every mailbox event (sent, delivered, acked, dropped, guard) so the
/// host can watch inter-agent messaging live. The stream stays open until the
/// client disconnects.
#[utoipa::path(
    get,
    path = "/api/v1/mailbox/stream",
    tag = "agents",
    responses((status = 200, description = "SSE stream of mailbox events"))
)]
pub async fn stream_mailbox<B: ExecutionBackend + Clone + From<DynBackend>>(
    State(state): State<AppState<B>>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let rx = state.event_sender.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|item| {
        let event = item.ok()?;
        let sse = mailbox_event_to_sse(&event)?;
        Some(Ok(Event::default().json_data(&sse).unwrap_or_default()))
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}
