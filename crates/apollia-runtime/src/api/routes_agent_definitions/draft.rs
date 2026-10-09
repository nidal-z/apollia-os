//! The draft of an agent, before it is saved.
//!
//! - `GET /api/v1/agent-drafts/offer`, what a draft may hold: the accesses,
//!   the sources and the models that exist on the machine now
//! - `POST /api/v1/agent-drafts`, a proposed sheet checked against that
//!   offer; the answer is the draft the form of an agent opens
//!
//! Neither writes. The agent workshop is given the offer, so that the sheet it
//! writes names only what exists; what it hands back is checked here again,
//! because a sheet is a proposal whoever wrote it. An agent is created by
//! `POST /api/v1/agent-definitions`, once the person saved the form.

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;

use crate::agents::draft::{check_draft, AgentDraft, DraftOffer, ProposedSheet};
use crate::api::routes_llm::servers::{api_err, ApiError};
use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;

/// The offer in force, read through the chat, which holds the registries a
/// draft is checked against.
async fn offer<B: ExecutionBackend + Clone>(state: &AppState<B>) -> Result<DraftOffer, ApiError> {
    let chat = state.chat_manager.as_ref().ok_or_else(|| {
        api_err(
            StatusCode::SERVICE_UNAVAILABLE,
            "the chat is not available: what exists on this machine cannot be listed",
        )
    })?;
    chat.agent_draft_offer()
        .await
        .map_err(|e| api_err(StatusCode::SERVICE_UNAVAILABLE, e))
}

/// `GET /api/v1/agent-drafts/offer`: what the draft of an agent may hold.
#[utoipa::path(
    get,
    path = "/api/v1/agent-drafts/offer",
    tag = "agents",
    responses(
        (status = 200, description = "The accesses, the sources and the models that exist on the machine", body = DraftOffer),
        (status = 503, description = "The chat, which holds the registries, is not available", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn get_agent_draft_offer<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Result<Json<DraftOffer>, ApiError> {
    offer(&state).await.map(Json)
}

/// `POST /api/v1/agent-drafts`: check a proposed sheet, write nothing.
#[utoipa::path(
    post,
    path = "/api/v1/agent-drafts",
    tag = "agents",
    request_body = ProposedSheet,
    responses(
        (status = 200, description = "The draft the form opens: what the machine does not hold left out and named, the sensitive accesses unchecked, the default access added", body = AgentDraft),
        (status = 422, description = "The sheet has no name or no mission", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "The chat, which holds the registries, is not available", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn check_agent_draft<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Json(sheet): Json<ProposedSheet>,
) -> Result<Json<AgentDraft>, ApiError> {
    let offer = offer(&state).await?;
    check_draft(&offer, sheet)
        .map(Json)
        .map_err(|refusal| api_err(StatusCode::UNPROCESSABLE_ENTITY, refusal))
}
