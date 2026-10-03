//! The onboarding over HTTP, under `/api/v1/onboarding`: its state, the move
//! to the next phase, the profile, the conversation with the onboarding agent
//! (started, resumed, dismissed) and the completion that agent records.
//!
//! The onboarding had no route: only the desktop running the runtime in its
//! own process could drive it. These routes serve the functions of
//! [`crate::onboarding`] to any client of the daemon, the attached desktop
//! first.

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::api::openapi::ApiErrorBody;
use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;
use crate::onboarding::state::{OnboardingError, OnboardingState, TriggerResult};
use crate::onboarding::Onboarding;

type ApiError = (StatusCode, Json<ApiErrorBody>);

/// Request body for `POST /api/v1/onboarding/phase`.
#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct AdvancePhaseBody {
    /// The phase to move to, in its snake_case name (`profile_choice`...).
    pub phase: String,
}

/// Request body for `PUT /api/v1/onboarding/profile`.
#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct OnboardingProfileBody {
    /// `operator` or `builder`.
    pub profile: String,
}

/// Request body for `POST /api/v1/onboarding/sessions`.
#[derive(Debug, Default, Deserialize, Serialize, utoipa::ToSchema)]
pub struct TriggerOnboardingBody {
    /// One topic to run again; absent for a full onboarding.
    #[serde(default)]
    pub topic: Option<String>,
    /// The profile the agent adapts its questions to, when one was chosen.
    #[serde(default)]
    pub profile: Option<String>,
}

/// Answer of `POST /api/v1/onboarding/dismiss`.
#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct OnboardingDismissed {
    /// Always `true`: the onboarding is marked as skipped.
    pub dismissed: bool,
}

/// Answer of `GET /api/v1/onboarding/finalized` and
/// `POST /api/v1/onboarding/finalize`.
#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct OnboardingFinalized {
    /// Whether the onboarding agent recorded its completion.
    pub finalized: bool,
}

/// Build the onboarding router.
pub fn onboarding_router<B: ExecutionBackend + Clone>() -> Router<AppState<B>> {
    Router::new()
        .route("/api/v1/onboarding", get(get_onboarding_state::<B>))
        .route(
            "/api/v1/onboarding/phase",
            post(advance_onboarding_phase::<B>),
        )
        .route(
            "/api/v1/onboarding/profile",
            put(set_onboarding_profile::<B>),
        )
        .route("/api/v1/onboarding/sessions", post(trigger_onboarding::<B>))
        .route("/api/v1/onboarding/resume", post(resume_onboarding::<B>))
        .route("/api/v1/onboarding/dismiss", post(dismiss_onboarding::<B>))
        .route(
            "/api/v1/onboarding/finalize",
            post(finalize_onboarding_chat::<B>),
        )
        .route(
            "/api/v1/onboarding/finalized",
            get(check_onboarding_finalized::<B>),
        )
}

fn onboarding<B: ExecutionBackend + Clone>(state: &AppState<B>) -> Onboarding<'_> {
    Onboarding {
        user_memory: state.user_memory.as_ref(),
        chat_manager: state.chat_manager.as_ref(),
        events: &state.event_sender,
        memory_dir: state.data_dir.join("memory"),
    }
}

fn refusal(error: OnboardingError) -> ApiError {
    let status = match error {
        OnboardingError::RepositoryNotInitialized | OnboardingError::ChatNotAvailable => {
            StatusCode::SERVICE_UNAVAILABLE
        }
        OnboardingError::AgentNotInstalled | OnboardingError::GuideAgentNotInstalled => {
            StatusCode::NOT_FOUND
        }
        OnboardingError::InvalidTopic(_)
        | OnboardingError::InvalidProfile(_)
        | OnboardingError::UnknownPhase(_) => StatusCode::BAD_REQUEST,
        OnboardingError::InvalidTransition { .. } => StatusCode::CONFLICT,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    tracing::event!(
        tracing::Level::WARN,
        status = status.as_u16(),
        error = %error,
        "onboarding.request.refused"
    );
    (
        status,
        Json(ApiErrorBody {
            error: error.to_string(),
        }),
    )
}

/// `GET /api/v1/onboarding`: the full state, `welcome` on first launch.
#[utoipa::path(
    get,
    path = "/api/v1/onboarding",
    tag = "onboarding",
    responses(
        (status = 200, description = "The onboarding state", body = OnboardingState),
        (status = 503, description = "The user memory did not open", body = ApiErrorBody),
    )
)]
pub(crate) async fn get_onboarding_state<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Result<Json<OnboardingState>, ApiError> {
    onboarding(&state).state().await.map(Json).map_err(refusal)
}

/// `POST /api/v1/onboarding/phase`: moves to the next phase.
#[utoipa::path(
    post,
    path = "/api/v1/onboarding/phase",
    tag = "onboarding",
    request_body = AdvancePhaseBody,
    responses(
        (status = 200, description = "The state after the move", body = OnboardingState),
        (status = 400, description = "Unknown phase", body = ApiErrorBody),
        (status = 409, description = "The phase is out of sequence", body = ApiErrorBody),
        (status = 503, description = "The user memory did not open", body = ApiErrorBody),
    )
)]
pub(crate) async fn advance_onboarding_phase<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Json(body): Json<AdvancePhaseBody>,
) -> Result<Json<OnboardingState>, ApiError> {
    onboarding(&state)
        .advance(&body.phase)
        .await
        .map(Json)
        .map_err(refusal)
}

/// `PUT /api/v1/onboarding/profile`: records the profile chosen.
#[utoipa::path(
    put,
    path = "/api/v1/onboarding/profile",
    tag = "onboarding",
    request_body = OnboardingProfileBody,
    responses(
        (status = 200, description = "The state with its profile", body = OnboardingState),
        (status = 400, description = "Neither operator nor builder", body = ApiErrorBody),
        (status = 503, description = "The user memory did not open", body = ApiErrorBody),
    )
)]
pub(crate) async fn set_onboarding_profile<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Json(body): Json<OnboardingProfileBody>,
) -> Result<Json<OnboardingState>, ApiError> {
    onboarding(&state)
        .set_profile(body.profile)
        .await
        .map(Json)
        .map_err(refusal)
}

/// `POST /api/v1/onboarding/sessions`: opens a conversation with the
/// onboarding agent, for the whole onboarding or one topic.
#[utoipa::path(
    post,
    path = "/api/v1/onboarding/sessions",
    tag = "onboarding",
    request_body = TriggerOnboardingBody,
    responses(
        (status = 200, description = "The conversation opened", body = TriggerResult),
        (status = 400, description = "Unknown topic or profile", body = ApiErrorBody),
        (status = 404, description = "The onboarding agent is not installed", body = ApiErrorBody),
        (status = 503, description = "The chat did not start", body = ApiErrorBody),
    )
)]
pub(crate) async fn trigger_onboarding<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Json(body): Json<TriggerOnboardingBody>,
) -> Result<Json<TriggerResult>, ApiError> {
    onboarding(&state)
        .trigger(body.topic, body.profile, false)
        .await
        .map(Json)
        .map_err(refusal)
}

/// `POST /api/v1/onboarding/resume`: resumes the enrichment of the profile,
/// keeping what the agent already learnt.
#[utoipa::path(
    post,
    path = "/api/v1/onboarding/resume",
    tag = "onboarding",
    responses(
        (status = 200, description = "The conversation opened", body = TriggerResult),
        (status = 404, description = "The onboarding agent is not installed", body = ApiErrorBody),
        (status = 503, description = "The chat did not start", body = ApiErrorBody),
    )
)]
pub(crate) async fn resume_onboarding<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Result<Json<TriggerResult>, ApiError> {
    onboarding(&state)
        .trigger(None, None, true)
        .await
        .map(Json)
        .map_err(refusal)
}

/// `POST /api/v1/onboarding/dismiss`: marks the onboarding as skipped.
#[utoipa::path(
    post,
    path = "/api/v1/onboarding/dismiss",
    tag = "onboarding",
    responses(
        (status = 200, description = "The onboarding is skipped", body = OnboardingDismissed),
        (status = 503, description = "The user memory did not open", body = ApiErrorBody),
    )
)]
pub(crate) async fn dismiss_onboarding<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Result<Json<OnboardingDismissed>, ApiError> {
    onboarding(&state).dismiss().await.map_err(refusal)?;
    Ok(Json(OnboardingDismissed { dismissed: true }))
}

/// `POST /api/v1/onboarding/finalize`: records the completion of the
/// conversation without a model turn.
#[utoipa::path(
    post,
    path = "/api/v1/onboarding/finalize",
    tag = "onboarding",
    responses(
        (status = 200, description = "The completion is recorded", body = OnboardingFinalized),
        (status = 500, description = "The agent's memory cannot be written", body = ApiErrorBody),
    )
)]
pub(crate) async fn finalize_onboarding_chat<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Result<Json<OnboardingFinalized>, ApiError> {
    onboarding(&state).finalize_chat().await.map_err(refusal)?;
    Ok(Json(OnboardingFinalized { finalized: true }))
}

/// `GET /api/v1/onboarding/finalized`: whether the onboarding agent recorded
/// its completion.
#[utoipa::path(
    get,
    path = "/api/v1/onboarding/finalized",
    tag = "onboarding",
    responses(
        (status = 200, description = "Whether the completion is recorded", body = OnboardingFinalized),
    )
)]
pub(crate) async fn check_onboarding_finalized<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Result<Json<OnboardingFinalized>, ApiError> {
    let finalized = onboarding(&state).is_finalized().await.map_err(refusal)?;
    Ok(Json(OnboardingFinalized { finalized }))
}

#[cfg(test)]
mod tests;
