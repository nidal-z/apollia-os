//! Answering one decision through the call that answers it where it was born.

use std::collections::BTreeSet;

use axum::http::StatusCode;
use tracing::Level;

use super::collect::{batch_answers, held, task_answers, BATCH, CONSENT, QUESTION, TASK};
use super::{refusal, DecideRequest, DecideResponse, PendingAnswer, PendingError};
use crate::api::routes_tasks::ResumeRequest;
use crate::api::server::AppState;
use crate::batch::{BatchControl, BatchError, BatchHandle, BatchRetry};
use crate::chat::manager::ChatSessionManagerHandle;
use crate::chat::types::ChatError;
use crate::chat::ToolDecision;
use crate::coordinator::ExecutionBackend;

/// Applies `request` to the item it names.
pub(crate) async fn decide<B: ExecutionBackend + Clone>(
    state: &AppState<B>,
    request: DecideRequest,
) -> Result<DecideResponse, PendingError> {
    let id = request.id.clone();
    let response = if let Some(key) = id.strip_prefix(CONSENT) {
        decide_consent(state, key, &request).await?
    } else if let Some(task_id) = id.strip_prefix(TASK) {
        decide_task(state, task_id, &request).await?
    } else if let Some(rest) = id.strip_prefix(BATCH) {
        decide_batch_item(state, rest, &request).await?
    } else if let Some(request_id) = id.strip_prefix(QUESTION) {
        decide_question(state, request_id, &request).await?
    } else {
        return Err(unknown(&id));
    };
    tracing::event!(
        Level::INFO,
        id = %response.id,
        answer = ?response.answer,
        resumed = response.resumed,
        "pending.decided"
    );
    Ok(response)
}

fn unknown(id: &str) -> PendingError {
    refusal(
        StatusCode::NOT_FOUND,
        "UNKNOWN_ITEM",
        format!("no decision '{id}' waits"),
    )
}

fn not_offered(answer: PendingAnswer, offered: &[PendingAnswer]) -> PendingError {
    refusal(
        StatusCode::UNPROCESSABLE_ENTITY,
        "ANSWER_NOT_OFFERED",
        format!("this decision takes {offered:?}, not {answer:?}"),
    )
}

fn offered(answer: PendingAnswer, offered: &[PendingAnswer]) -> Result<(), PendingError> {
    if offered.contains(&answer) {
        Ok(())
    } else {
        Err(not_offered(answer, offered))
    }
}

fn chat<B: ExecutionBackend + Clone>(
    state: &AppState<B>,
) -> Result<&ChatSessionManagerHandle, PendingError> {
    state.chat_manager.as_ref().ok_or_else(|| {
        refusal(
            StatusCode::SERVICE_UNAVAILABLE,
            "UNAVAILABLE",
            "the chat subsystem is not running in this runtime",
        )
    })
}

fn chat_refusal(e: ChatError) -> PendingError {
    let status = match &e {
        ChatError::SessionNotFound(_) => StatusCode::NOT_FOUND,
        ChatError::InternalError(_) => StatusCode::INTERNAL_SERVER_ERROR,
        _ => StatusCode::CONFLICT,
    };
    refusal(status, "CHAT", e)
}

fn response(request: &DecideRequest, resumed: u64, message: impl ToString) -> DecideResponse {
    DecideResponse {
        id: request.id.clone(),
        answer: request.answer,
        resumed,
        message: message.to_string(),
    }
}

/// A tool consent, answered as the chat answers it.
async fn decide_consent<B: ExecutionBackend + Clone>(
    state: &AppState<B>,
    key: &str,
    request: &DecideRequest,
) -> Result<DecideResponse, PendingError> {
    let chat = chat(state)?;
    let asked = chat
        .pending_tool_approval(key)
        .ok_or_else(|| unknown(&request.id))?;
    let answers: Vec<PendingAnswer> = asked
        .answers
        .iter()
        .map(|a| match a {
            crate::governance::ConsentAnswer::Once => PendingAnswer::Once,
            crate::governance::ConsentAnswer::Always => PendingAnswer::Always,
            crate::governance::ConsentAnswer::Refuse => PendingAnswer::Refuse,
        })
        .collect();
    offered(request.answer, &answers)?;
    let (decision, message) = match request.answer {
        PendingAnswer::Once => (ToolDecision::Accept, "Allowed once; the agent goes on."),
        PendingAnswer::Always => (
            ToolDecision::always_accept_default(),
            "Allowed for this agent from now on; the agent goes on.",
        ),
        _ => (
            ToolDecision::Refuse {
                reason: request.reason.clone(),
            },
            "Refused; the agent is told and goes on without it.",
        ),
    };
    chat.resolve_tool(
        asked.session_id,
        asked.message_id,
        asked.tool_call_id,
        asked.tool_name,
        decision,
    )
    .await
    .map_err(chat_refusal)?;
    Ok(response(request, 1, message))
}

/// A task pause, answered as `POST /api/v1/tasks/{id}/resume` answers it.
async fn decide_task<B: ExecutionBackend + Clone>(
    state: &AppState<B>,
    task_id: &str,
    request: &DecideRequest,
) -> Result<DecideResponse, PendingError> {
    let pause = crate::api::routes_approvals::task_pauses(state)
        .await
        .into_iter()
        .find(|p| p.task_id == task_id)
        .ok_or_else(|| unknown(&request.id))?;
    offered(request.answer, &task_answers(pause.payload.as_ref()))?;
    let (approved, answer) = match request.answer {
        PendingAnswer::Refuse => (false, None),
        PendingAnswer::Answer => (true, request.value.clone()),
        _ => (true, None),
    };
    let resumed = crate::api::routes_tasks::resume_paused_task(
        state,
        task_id.to_string(),
        ResumeRequest {
            approved,
            reason: request.reason.clone(),
            answer,
        },
    )
    .await
    .map_err(|(status, body)| {
        let code = body.0.code.clone().unwrap_or_else(|| "TASK".to_string());
        refusal(status, &code, body.0.error)
    })?;
    let message = if resumed.approved {
        "The task goes on."
    } else {
        "Refused; the task is told and goes on without it."
    };
    Ok(response(request, 1, message))
}

fn engine<B: ExecutionBackend + Clone>(state: &AppState<B>) -> Result<BatchHandle, PendingError> {
    state.batches.clone().ok_or_else(|| {
        refusal(
            StatusCode::SERVICE_UNAVAILABLE,
            "UNAVAILABLE",
            "the batch engine is not running in this runtime",
        )
    })
}

fn batch_refusal(e: BatchError) -> PendingError {
    let status = match &e {
        BatchError::NotFound(_) => StatusCode::NOT_FOUND,
        BatchError::State(_) => StatusCode::CONFLICT,
        BatchError::Invalid(_) | BatchError::Agent(_) => StatusCode::UNPROCESSABLE_ENTITY,
        BatchError::Stopped => StatusCode::SERVICE_UNAVAILABLE,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    refusal(status, "BATCH", e)
}

/// A batch item waiting for an access, answered as the batch is: once grants
/// the access for the rest of the batch and runs its waiting items again,
/// or, for executing code, runs this item again allowed one call; always gives the agent a standing consent for the family and runs again
/// every waiting item of that agent it covers; refuse fails the item.
async fn decide_batch_item<B: ExecutionBackend + Clone>(
    state: &AppState<B>,
    rest: &str,
    request: &DecideRequest,
) -> Result<DecideResponse, PendingError> {
    let (batch_id, rank) = rest
        .rsplit_once(':')
        .and_then(|(b, r)| r.parse::<u64>().ok().map(|r| (b, r)))
        .ok_or_else(|| unknown(&request.id))?;
    let batches = engine(state)?;
    let detail = batches
        .detail(batch_id)
        .await
        .map_err(batch_refusal)?
        .ok_or_else(|| unknown(&request.id))?;
    let item = detail
        .items
        .iter()
        .find(|i| i.rank == rank && i.status == "awaiting")
        .ok_or_else(|| unknown(&request.id))?;
    let agent = detail.batch.agent.clone();
    let access = item
        .output
        .as_deref()
        .and_then(crate::batch::awaited_access)
        .map(str::to_owned);
    let accesses = batches.accesses(&agent).await.unwrap_or_default();
    let known = access.as_deref().and_then(|a| held(&accesses, a)).cloned();
    offered(request.answer, &batch_answers(known.as_ref()))?;

    match (request.answer, known, access) {
        (PendingAnswer::Once, Some(known), Some(access)) if known.executes_code() => {
            batches
                .control(
                    batch_id,
                    BatchControl::RunOnce {
                        rank,
                        access: access.clone(),
                    },
                )
                .await
                .map_err(batch_refusal)?;
            Ok(response(
                request,
                1,
                format!(
                    "'{access}' is allowed once; the item runs again and asks at its next call."
                ),
            ))
        }
        (PendingAnswer::Once, _, Some(access)) => {
            batches
                .control(
                    batch_id,
                    BatchControl::Retry(BatchRetry {
                        grants: vec![access.clone()],
                    }),
                )
                .await
                .map_err(batch_refusal)?;
            Ok(response(
                request,
                1,
                format!("'{access}' is allowed for this batch; its waiting items run again."),
            ))
        }
        (PendingAnswer::Always, Some(known), _) => {
            always_for_agent(state, &batches, &agent, &known.family, request).await
        }
        _ => {
            batches
                .control(
                    batch_id,
                    BatchControl::Refuse {
                        rank,
                        reason: request.reason.clone(),
                    },
                )
                .await
                .map_err(batch_refusal)?;
            Ok(response(
                request,
                0,
                "Refused; the item fails and the batch goes on.",
            ))
        }
    }
}

/// Gives `agent` a standing consent for `family`, then runs again every batch
/// of that agent with an item waiting for an access of that family.
async fn always_for_agent<B: ExecutionBackend + Clone>(
    state: &AppState<B>,
    batches: &BatchHandle,
    agent: &str,
    family: &str,
    request: &DecideRequest,
) -> Result<DecideResponse, PendingError> {
    let governance = state.governance.as_ref().ok_or_else(|| {
        refusal(
            StatusCode::SERVICE_UNAVAILABLE,
            "UNAVAILABLE",
            "governance is not running in this runtime",
        )
    })?;
    let action_family: apollia_tools::ActionFamily =
        serde_json::from_value(serde_json::Value::String(family.to_string())).map_err(|e| {
            refusal(
                StatusCode::UNPROCESSABLE_ENTITY,
                "ANSWER_NOT_OFFERED",
                format!("unknown family '{family}': {e}"),
            )
        })?;
    governance
        .grant_always(agent, action_family)
        .await
        .map_err(|e| refusal(StatusCode::CONFLICT, "GOVERNANCE", e))?;

    let accesses = batches.accesses(agent).await.unwrap_or_default();
    let waiting = batches.awaiting_items().await.map_err(batch_refusal)?;
    let mut covered: BTreeSet<String> = BTreeSet::new();
    let mut resumed = 0u64;
    for (batch, item) in waiting {
        if batch.agent != agent {
            continue;
        }
        let covers = item
            .output
            .as_deref()
            .and_then(crate::batch::awaited_access)
            .and_then(|a| held(&accesses, a))
            .is_some_and(|a| a.family == family);
        if covers {
            covered.insert(batch.batch_id);
            resumed += 1;
        }
    }
    for batch_id in &covered {
        batches
            .control(batch_id, BatchControl::Retry(BatchRetry::default()))
            .await
            .map_err(batch_refusal)?;
    }
    Ok(response(
        request,
        resumed,
        format!(
            "Allowed for {agent} from now on: {resumed} waiting item(s) in {} batch(es) run again.",
            covered.len()
        ),
    ))
}

/// A question an agent asked, answered as the `ask_user` routes answer it.
async fn decide_question<B: ExecutionBackend + Clone>(
    state: &AppState<B>,
    request_id: &str,
    request: &DecideRequest,
) -> Result<DecideResponse, PendingError> {
    let chat = chat(state)?;
    let waits = chat
        .list_pending_user_inputs()
        .await
        .map_err(chat_refusal)?
        .iter()
        .any(|q| q.request_id == request_id);
    if !waits {
        return Err(unknown(&request.id));
    }
    offered(
        request.answer,
        &[PendingAnswer::Answer, PendingAnswer::Refuse],
    )?;
    if request.answer == PendingAnswer::Refuse {
        let reason = request
            .reason
            .clone()
            .filter(|r| !r.trim().is_empty())
            .unwrap_or_else(|| "The user declined to answer".to_string());
        chat.reject_user_input(request_id.to_string(), reason)
            .await
            .map_err(chat_refusal)?;
        return Ok(response(request, 1, "Declined; the agent is told."));
    }
    let answers = request
        .answers
        .clone()
        .filter(|a| !a.is_empty())
        .ok_or_else(|| {
            refusal(
                StatusCode::UNPROCESSABLE_ENTITY,
                "INVALID_ANSWER",
                "an answer to a question carries `answers`, one per question",
            )
        })?;
    chat.resolve_user_input(request_id.to_string(), answers)
        .await
        .map_err(chat_refusal)?;
    Ok(response(request, 1, "Answered; the agent goes on."))
}
