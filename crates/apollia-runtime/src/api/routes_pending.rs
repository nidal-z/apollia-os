//! The decisions waiting for the user, gathered in one list and answered from
//! it.
//!
//! `GET /api/v1/pending-decisions` lists, in one answer, every decision a run
//! waits on: the tool consents of the conversations and of the runs outside
//! any conversation, the pauses of tasks (`hitl.db`), the batch items waiting
//! for an access, and the questions agents asked the user. Each item carries
//! the answers it takes: once, always and refuse for a consent; once and
//! refuse for executing code; the proposed choices and a free answer for a
//! question.
//!
//! `POST /api/v1/pending-decisions/decide` answers one item, through the very
//! call that answers it where it was born: the chat consent through the chat
//! manager, the pause through the task resume, the batch item through the batch
//! engine, the question through the `ask_user` registry. Deciding here or there
//! is the same decision.
//!
//! An installed package waits for nothing: its update is confirmed when it is
//! installed, so no package update is listed.

mod collect;
mod decide;

#[cfg(test)]
mod tests;

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;

pub(crate) use collect::collect;
pub(crate) use decide::decide;

/// Where a decision waiting for the user comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PendingKind {
    /// A tool call an agent asked to make, in a conversation or in a run.
    Consent,
    /// A task paused for an approval or a typed question.
    TaskPause,
    /// A batch item that stopped on an access nobody granted.
    BatchItem,
    /// A question an agent asked the user.
    Question,
}

/// An answer a decision takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PendingAnswer {
    /// This call, or this item, only.
    Once,
    /// This one and every later one of the same agent in the same family.
    Always,
    /// Not this one.
    Refuse,
    /// The answer to a question: a proposed choice, or free text.
    Answer,
}

/// Where the decision is to be found where it was born.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PendingOrigin {
    /// The conversation, for a consent or a question asked in one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// The message whose turn asked.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
    /// The tool call a consent is about.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// The paused task.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    /// The batch of an item.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub batch_id: Option<String>,
    /// Position of the item in its source, from 0.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rank: Option<u64>,
    /// The `ask_user` request of a question.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
}

/// One decision waiting for the user.
#[derive(Debug, Clone, PartialEq, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PendingDecision {
    /// Identifier to answer it by, stable while it waits.
    pub id: String,
    /// Where it comes from.
    pub kind: PendingKind,
    /// The agent that waits, when known.
    pub agent_id: Option<String>,
    /// What is asked, in a sentence.
    pub summary: String,
    /// The tool or the access asked for, for a consent and a batch item.
    pub access: Option<String>,
    /// Its action family (`read_personal`, `write_or_delete`, `send_external`,
    /// `execute_code`, `unclassified`), when known.
    pub family: Option<String>,
    /// The full arguments of the call a consent is about, as the chat card
    /// shows them behind "see more".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// The whole command or source, when the consent is about running code.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    /// The folder that code runs in.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub working_dir: Option<String>,
    /// The answers it takes.
    pub answers: Vec<PendingAnswer>,
    /// The questions of a question, each with its proposed choices, or the
    /// typed payload of a paused task.
    #[schema(value_type = Option<Object>)]
    pub questions: Option<serde_json::Value>,
    /// A free answer is taken besides the proposed choices.
    pub allow_other: bool,
    /// Where it was born.
    pub origin: PendingOrigin,
    /// When it was asked, RFC 3339, when known.
    pub asked_at: Option<String>,
}

/// The decisions waiting for the user.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct PendingDecisionList {
    pub items: Vec<PendingDecision>,
}

/// Body of `POST /api/v1/pending-decisions/decide`.
#[derive(Debug, Clone, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DecideRequest {
    /// The `id` of the item, as listed.
    pub id: String,
    /// One of the answers the item takes.
    pub answer: PendingAnswer,
    /// Why, for a refusal; handed to the agent.
    #[serde(default)]
    pub reason: Option<String>,
    /// The answers to a question, one per question, matched by its `id`.
    #[serde(default)]
    #[schema(value_type = Option<Vec<Object>>)]
    pub answers: Option<Vec<apollia_tools::tools::ask_user::UserAnswer>>,
    /// The answer to the typed question of a paused task: a proposition id,
    /// free text when it allows it, or a value.
    #[serde(default)]
    #[schema(value_type = Option<Object>)]
    pub value: Option<serde_json::Value>,
}

/// What a decision did.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DecideResponse {
    /// The item answered.
    pub id: String,
    /// The answer applied.
    pub answer: PendingAnswer,
    /// How many waiting runs, conversations or items went on.
    pub resumed: u64,
    /// How many other batch items, waiting on an access this answer now
    /// covers, ran again besides the item answered.
    pub batch_items_resumed: u64,
    /// What happened, to show the user.
    pub message: String,
}

/// Error body of these routes.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct PendingErrorBody {
    /// What went wrong.
    pub error: String,
    /// A stable code: `UNKNOWN_ITEM`, `ANSWER_NOT_OFFERED`, `INVALID_ANSWER`,
    /// `UNAVAILABLE`, or the code of the call the decision went through.
    pub code: String,
}

pub(crate) type PendingError = (StatusCode, Json<PendingErrorBody>);

pub(crate) fn refusal(status: StatusCode, code: &str, error: impl ToString) -> PendingError {
    (
        status,
        Json(PendingErrorBody {
            error: error.to_string(),
            code: code.to_string(),
        }),
    )
}

/// `GET /api/v1/pending-decisions`: every decision waiting for the user.
#[utoipa::path(
    get,
    path = "/api/v1/pending-decisions",
    tag = "governance",
    responses(
        (status = 200, description = "The decisions waiting for the user, empty when none", body = PendingDecisionList),
    )
)]
pub async fn list_pending_decisions<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Json<PendingDecisionList> {
    Json(PendingDecisionList {
        items: collect(&state).await,
    })
}

/// `POST /api/v1/pending-decisions/decide`: answer one waiting decision.
///
/// The answer goes through the call that answers the item where it was born,
/// so the conversation, the task or the batch item goes on as if it had been
/// answered there. After `always` on a batch item, every waiting item of the
/// same agent the consent covers runs again.
#[utoipa::path(
    post,
    path = "/api/v1/pending-decisions/decide",
    tag = "governance",
    request_body = DecideRequest,
    responses(
        (status = 200, description = "The decision was applied", body = DecideResponse),
        (status = 404, description = "No such item waits", body = PendingErrorBody),
        (status = 409, description = "The item can no longer take this answer", body = PendingErrorBody),
        (status = 422, description = "The item does not take this answer, or the answer is malformed", body = PendingErrorBody),
        (status = 503, description = "The subsystem the item belongs to is not running", body = PendingErrorBody),
    )
)]
pub async fn decide_pending<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Json(request): Json<DecideRequest>,
) -> Result<Json<DecideResponse>, PendingError> {
    decide(&state, request).await.map(Json)
}
