//! REST route for agent-to-agent messages, `GET /api/v1/agents/:name/messages`.
//!
//! Exposes the in-memory mailbox contents so the desktop frontend can display
//! inter-agent communication. Returns `503` when the mailbox is not available.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::api::server::AppState;
use crate::coordinator::{DynBackend, ExecutionBackend};
use crate::mailbox::AgentMessage;

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
