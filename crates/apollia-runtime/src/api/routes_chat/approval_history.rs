//! `GET /api/v1/sessions/approval-history`, the tool calls a user decided on
//! inside a chat, most recent first.
//!
//! The approvals of tasks have their own history, `/api/v1/approvals/resolved`;
//! this one reads the chat manager's ledger, for the inbox of the desktop
//! attached to a daemon.

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};

use super::{chat_error_to_response, ErrorResponse};
use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;

/// Query of `GET /api/v1/sessions/approval-history`.
#[derive(Debug, Deserialize)]
pub struct ApprovalHistoryQuery {
    /// Maximum number of entries (default 20).
    pub limit: Option<i64>,
    /// Look-back window in days (default 7).
    pub days: Option<i64>,
}

/// One decision on a tool call inside a chat.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ChatApprovalHistoryEntry {
    /// Session identifier.
    pub session_id: String,
    /// Message the approval was attached to.
    pub message_id: String,
    /// Tool that required approval.
    pub tool_name: String,
    /// Decision taken (`accept`, `refuse`, `always_accept`).
    pub decision: String,
    /// ISO-8601 timestamp of the resolution.
    pub resolved_at: String,
    /// The user's refusal reason, only set for `refuse`.
    pub reason: Option<String>,
}

/// Handler for `GET /api/v1/sessions/approval-history`.
#[utoipa::path(
    get,
    path = "/api/v1/sessions/approval-history",
    tag = "chat",
    params(
        ("limit" = Option<i64>, Query, description = "Maximum number of entries (default 20)"),
        ("days" = Option<i64>, Query, description = "Look-back window in days (default 7)"),
    ),
    responses(
        (status = 200, description = "Recent decisions on chat tool calls", body = Vec<ChatApprovalHistoryEntry>),
        (status = 500, description = "The ledger cannot be read", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Chat subsystem unavailable", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn list_approval_history<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Query(query): Query<ApprovalHistoryQuery>,
) -> Response {
    let Some(manager) = &state.chat_manager else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "chat subsystem not available".into(),
            }),
        )
            .into_response();
    };
    match manager
        .list_approval_history(query.limit.unwrap_or(20), query.days.unwrap_or(7))
        .await
    {
        Ok(rows) => Json(
            rows.into_iter()
                .map(|r| ChatApprovalHistoryEntry {
                    session_id: r.session_id,
                    message_id: r.message_id,
                    tool_name: r.tool_name,
                    decision: r.decision,
                    resolved_at: r.resolved_at,
                    reason: r.reason,
                })
                .collect::<Vec<_>>(),
        )
        .into_response(),
        Err(e) => chat_error_to_response(e).into_response(),
    }
}

#[cfg(test)]
mod tests {

    use axum::http::StatusCode;

    use crate::api::test_support::{app_state, call, seeded_chat};

    #[tokio::test]
    async fn test_the_chat_ledger_is_read_with_its_refusal_reason() {
        // GIVEN a daemon whose chat ledger holds one refused tool call
        let data = tempfile::tempdir().unwrap();
        let state = seeded_chat(data.path());

        // WHEN the history is read
        let uri = "/api/v1/sessions/approval-history?limit=5&days=7";
        let (status, body) = call(state, "GET", uri, None).await;

        // THEN the decision comes back as the inbox draws it
        assert_eq!(status, StatusCode::OK, "{body}");
        let entries = body.as_array().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0]["tool_name"], "bash_executor");
        assert_eq!(entries[0]["decision"], "refuse");
        assert_eq!(entries[0]["reason"], "not this folder");
    }

    #[tokio::test]
    async fn test_without_a_chat_manager_the_three_routes_answer_503() {
        // GIVEN a daemon whose chat subsystem did not start
        let data = tempfile::tempdir().unwrap();
        let state = || app_state(data.path().into());
        let content = Some(serde_json::json!({ "content": "Bonsoir" }));

        // WHEN the history is read and a turn played again
        let turn = "/api/v1/sessions/s-1/messages";
        let (history, body) = call(state(), "GET", "/api/v1/sessions/approval-history", None).await;
        let (regenerate, _) = call(state(), "POST", &format!("{turn}/a-1/regenerate"), None).await;
        let (resend, _) = call(state(), "POST", &format!("{turn}/u-1/resend"), content).await;

        // THEN the three routes are mounted and say the subsystem is absent
        assert_eq!(history, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(regenerate, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(resend, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(body["error"], "chat subsystem not available");
    }
}
