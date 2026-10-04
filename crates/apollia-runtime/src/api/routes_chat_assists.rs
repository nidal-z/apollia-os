//! The assists around the chat over HTTP: `POST /api/v1/assists/rewrite`,
//! `POST /api/v1/assists/next-steps`, `POST /api/v1/assists/automation` and
//! `POST /api/v1/assists/coaching`.
//!
//! Only the desktop running the runtime in its own process could rewrite a
//! prompt, draw the next steps cards, parse an automation or coach a new
//! connector. These routes serve the functions of [`crate::chat_assists`] to
//! any client of the daemon, the attached desktop first, with the daemon's own
//! model router. None of them fails once its body parses: each degrades and
//! says so in its answer.

use apollia_llm::meta::next_steps::NextStepsRequest;
use apollia_llm::meta::parse_automation::ParsedAutomation;
use apollia_llm::meta::rewrite_input::RewriteInputRequest;
use axum::extract::State;
use axum::routing::post;
use axum::{Json, Router};

use crate::api::server::AppState;
use crate::chat_assists::{
    self, CoachingExample, CoachingRequest, NextStepsReply, ParseAutomationRequest, RewriteReply,
};
use crate::coordinator::ExecutionBackend;

/// Build the router of the chat assists.
pub fn chat_assists_router<B: ExecutionBackend + Clone>() -> Router<AppState<B>> {
    Router::new()
        .route("/api/v1/assists/rewrite", post(rewrite_prompt::<B>))
        .route("/api/v1/assists/next-steps", post(generate_next_steps::<B>))
        .route("/api/v1/assists/automation", post(parse_automation::<B>))
        .route("/api/v1/assists/coaching", post(coach_connector::<B>))
}

/// `POST /api/v1/assists/rewrite`: rewrites a terse prompt with the daemon's
/// default model, or returns it unchanged with the reason.
#[utoipa::path(
    post,
    path = "/api/v1/assists/rewrite",
    tag = "assists",
    request_body(content = Object, content_type = "application/json", description = "`originalText`, and the optional `workContext` of the user profile"),
    responses(
        (status = 200, description = "The rewritten text, or the original one and why", body = RewriteReply),
        (status = 422, description = "The body is not a rewrite request"),
    )
)]
pub(crate) async fn rewrite_prompt<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Json(request): Json<RewriteInputRequest>,
) -> Json<RewriteReply> {
    let router = state.llm_router.read().await.clone();
    Json(chat_assists::rewrite(router.as_ref(), request).await)
}

/// `POST /api/v1/assists/next-steps`: up to three next steps cards, drawn by
/// the daemon's default model or by the heuristic.
#[utoipa::path(
    post,
    path = "/api/v1/assists/next-steps",
    tag = "assists",
    request_body(content = Object, content_type = "application/json", description = "`context`, `mode` and the `facts` the window computed, each optional"),
    responses(
        (status = 200, description = "The cards, and whether a model wrote them", body = NextStepsReply),
        (status = 422, description = "The body is not a next steps request"),
    )
)]
pub(crate) async fn generate_next_steps<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Json(request): Json<NextStepsRequest>,
) -> Json<NextStepsReply> {
    let router = state.llm_router.read().await.clone();
    Json(chat_assists::next_steps(router.as_ref(), request).await)
}

/// `POST /api/v1/assists/automation`: parses an automation described in
/// words into a schedule, an agent and a payload, with its ambiguities.
#[utoipa::path(
    post,
    path = "/api/v1/assists/automation",
    tag = "assists",
    request_body = ParseAutomationRequest,
    responses(
        (status = 200, description = "The parsed automation, ambiguities included", body = Object),
        (status = 422, description = "The body is not an automation description"),
    )
)]
pub(crate) async fn parse_automation<B: ExecutionBackend + Clone>(
    State(_state): State<AppState<B>>,
    Json(request): Json<ParseAutomationRequest>,
) -> Json<ParsedAutomation> {
    Json(chat_assists::parse(&request, chrono::Utc::now()))
}

/// `POST /api/v1/assists/coaching`: examples to try a connector just added.
#[utoipa::path(
    post,
    path = "/api/v1/assists/coaching",
    tag = "assists",
    request_body = CoachingRequest,
    responses(
        (status = 200, description = "One to three example cards", body = [CoachingExample]),
        (status = 422, description = "The body names no server"),
    )
)]
pub(crate) async fn coach_connector<B: ExecutionBackend + Clone>(
    State(_state): State<AppState<B>>,
    Json(request): Json<CoachingRequest>,
) -> Json<Vec<CoachingExample>> {
    Json(chat_assists::coaching(request))
}

#[cfg(test)]
mod tests;
