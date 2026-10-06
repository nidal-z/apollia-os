//! Gathering the decisions waiting for the user from where each was born.

use std::collections::HashMap;

use apollia_core::HitlPayload;
use apollia_tools::{BatchItemRow, BatchRow};

use super::{PendingAnswer, PendingDecision, PendingKind, PendingOrigin};
use crate::api::server::AppState;
use crate::batch::BatchAccess;
use crate::coordinator::ExecutionBackend;
use crate::governance::ConsentAnswer;

pub(super) const CONSENT: &str = "consent:";
pub(super) const TASK: &str = "task:";
pub(super) const BATCH: &str = "batch:";
pub(super) const QUESTION: &str = "question:";
pub(super) const WORKFLOW_QUESTION: &str = "workflow-question:";
/// The `id` of the one question a workflow asks at a time, in the shape the
/// `ask_user` questions take.
pub(super) const WORKFLOW_ANSWER_ID: &str = "answer";

/// Every decision waiting for the user: the consents, the task pauses, the
/// batch items, the questions of agents then of workflows, each oldest first. A subsystem that is not
/// running contributes nothing.
pub(crate) async fn collect<B: ExecutionBackend + Clone>(
    state: &AppState<B>,
) -> Vec<PendingDecision> {
    let mut items = Vec::new();
    if let Some(chat) = &state.chat_manager {
        items.extend(chat.pending_tool_approvals().into_iter().map(consent));
    }
    items.extend(
        crate::api::routes_approvals::task_pauses(state)
            .await
            .into_iter()
            .map(task_pause),
    );
    if let Some(batches) = &state.batches {
        match batches.awaiting_items().await {
            Ok(waiting) => {
                let mut accesses: HashMap<String, Vec<BatchAccess>> = HashMap::new();
                for (batch, item) in waiting {
                    if !accesses.contains_key(&batch.agent) {
                        let held = batches.accesses(&batch.agent).await.unwrap_or_default();
                        accesses.insert(batch.agent.clone(), held);
                    }
                    let held = accesses.get(&batch.agent).map(Vec::as_slice).unwrap_or(&[]);
                    items.push(batch_item(&batch, &item, held));
                }
            }
            Err(e) => tracing::event!(
                tracing::Level::WARN,
                error = %e,
                "pending.batch_items.read_failed"
            ),
        }
    }
    if let Some(chat) = &state.chat_manager {
        match chat.list_pending_user_inputs().await {
            Ok(mut questions) => {
                questions.sort_by(|a, b| a.created_at.cmp(&b.created_at));
                items.extend(questions.into_iter().map(question));
            }
            Err(e) => tracing::event!(
                tracing::Level::WARN,
                error = %e,
                "pending.questions.read_failed"
            ),
        }
    }
    if let Some(workflows) = &state.workflows {
        match workflows.pending_questions().await {
            Ok(questions) => items.extend(questions.into_iter().map(workflow_question)),
            Err(e) => tracing::event!(
                tracing::Level::WARN,
                error = %e,
                "pending.workflow_questions.read_failed"
            ),
        }
    }
    items
}

/// The question a step of a workflow stopped on, in the shape of the
/// questions agents ask: one question, its choices when it proposes some, and
/// a free answer besides them.
fn workflow_question(waiting: crate::workflows::WorkflowQuestion) -> PendingDecision {
    let kind = if waiting.asked.options.is_empty() {
        "open"
    } else {
        "single_choice"
    };
    let mut question = serde_json::json!({
        "id": WORKFLOW_ANSWER_ID,
        "question": waiting.asked.question,
        "type": kind,
    });
    if !waiting.asked.options.is_empty() {
        question["options"] = serde_json::json!(waiting.asked.options);
    }
    if let Some(hint) = &waiting.asked.hint {
        question["hint"] = serde_json::json!(hint);
    }
    PendingDecision {
        id: format!("{WORKFLOW_QUESTION}{}", waiting.request_id),
        kind: PendingKind::Question,
        agent_id: None,
        summary: waiting.asked.question,
        access: None,
        family: None,
        answers: vec![PendingAnswer::Answer, PendingAnswer::Refuse],
        questions: Some(serde_json::Value::Array(vec![question])),
        detail: None,
        code: None,
        working_dir: None,
        allow_other: true,
        origin: PendingOrigin {
            request_id: Some(waiting.request_id),
            run_id: Some(waiting.run_id),
            workflow_id: Some(waiting.workflow_id),
            step: waiting.step,
            ..PendingOrigin::default()
        },
        asked_at: Some(waiting.asked_at),
    }
}

fn answer_of(answer: ConsentAnswer) -> PendingAnswer {
    match answer {
        ConsentAnswer::Once => PendingAnswer::Once,
        ConsentAnswer::Always => PendingAnswer::Always,
        ConsentAnswer::Refuse => PendingAnswer::Refuse,
    }
}

fn consent(asked: crate::chat::ChatApprovalAsked) -> PendingDecision {
    let mut answers: Vec<PendingAnswer> = asked.answers.iter().copied().map(answer_of).collect();
    if answers.is_empty() {
        answers = vec![PendingAnswer::Once, PendingAnswer::Refuse];
    }
    PendingDecision {
        id: format!(
            "{CONSENT}{}::{}::{}",
            asked.session_id, asked.message_id, asked.tool_call_id
        ),
        kind: PendingKind::Consent,
        agent_id: asked.agent_id,
        summary: asked.prompt,
        access: Some(asked.tool_name),
        family: asked.family,
        detail: asked.detail,
        code: asked.code,
        working_dir: asked.working_dir,
        answers,
        questions: None,
        allow_other: false,
        origin: PendingOrigin {
            session_id: Some(asked.session_id),
            message_id: Some(asked.message_id),
            tool_call_id: Some(asked.tool_call_id),
            ..PendingOrigin::default()
        },
        asked_at: Some(asked.asked_at),
    }
}

/// The answers of a paused task: a question is answered or refused, an
/// approval and a pause without payload are approved or refused.
pub(super) fn task_answers(payload: Option<&serde_json::Value>) -> Vec<PendingAnswer> {
    match payload.map(HitlPayload::parse) {
        Some(Ok(HitlPayload::Question(_))) => vec![PendingAnswer::Answer, PendingAnswer::Refuse],
        _ => vec![PendingAnswer::Once, PendingAnswer::Refuse],
    }
}

fn task_pause(pause: crate::api::routes_approvals::PendingApprovalResponse) -> PendingDecision {
    let allow_other = match pause.payload.as_ref().map(HitlPayload::parse) {
        Some(Ok(HitlPayload::Question(question))) => question.autre,
        _ => false,
    };
    PendingDecision {
        id: format!("{TASK}{}", pause.task_id),
        kind: PendingKind::TaskPause,
        agent_id: Some(pause.agent_name).filter(|a| !a.is_empty()),
        summary: pause.prompt,
        access: pause.skill_id,
        family: None,
        detail: None,
        code: None,
        working_dir: None,
        answers: task_answers(pause.payload.as_ref()),
        questions: pause.payload,
        allow_other,
        origin: PendingOrigin {
            task_id: Some(pause.task_id),
            ..PendingOrigin::default()
        },
        asked_at: Some(pause.suspended_at).filter(|t| !t.is_empty()),
    }
}

/// The answers of a batch item waiting for `access`: once, always and refuse
/// for an access a batch can be granted; once and refuse for executing code,
/// as everywhere code asks; refuse alone for an access nobody classified or
/// one the agent no longer holds, which no answer can let through.
pub(super) fn batch_answers(access: Option<&BatchAccess>) -> Vec<PendingAnswer> {
    match access {
        Some(a) if a.grantable => vec![
            PendingAnswer::Once,
            PendingAnswer::Always,
            PendingAnswer::Refuse,
        ],
        Some(a) if a.executes_code() => vec![PendingAnswer::Once, PendingAnswer::Refuse],
        _ => vec![PendingAnswer::Refuse],
    }
}

pub(super) fn held<'a>(accesses: &'a [BatchAccess], access: &str) -> Option<&'a BatchAccess> {
    accesses.iter().find(|a| a.access_id == access)
}

fn batch_item(batch: &BatchRow, item: &BatchItemRow, accesses: &[BatchAccess]) -> PendingDecision {
    let access = item
        .output
        .as_deref()
        .and_then(crate::batch::awaited_access)
        .map(str::to_owned);
    let known = access.as_deref().and_then(|a| held(accesses, a));
    PendingDecision {
        id: format!("{BATCH}{}:{}", batch.batch_id, item.rank),
        kind: PendingKind::BatchItem,
        agent_id: Some(batch.agent.clone()),
        summary: item
            .output
            .clone()
            .unwrap_or_else(|| "Waiting for a decision".to_string()),
        family: known.map(|a| a.family.clone()),
        detail: None,
        code: None,
        working_dir: None,
        answers: batch_answers(known),
        access,
        questions: None,
        allow_other: false,
        origin: PendingOrigin {
            batch_id: Some(batch.batch_id.clone()),
            rank: Some(item.rank),
            task_id: item.task_id.clone(),
            ..PendingOrigin::default()
        },
        asked_at: Some(batch.updated_at.clone()),
    }
}

fn question(view: crate::chat::manager::PendingUserInputView) -> PendingDecision {
    let questions: Option<serde_json::Value> = serde_json::from_str(&view.questions_json).ok();
    let summary = view.context.clone().unwrap_or_else(|| {
        questions
            .as_ref()
            .and_then(|q| q.get(0))
            .and_then(|q| q.get("question"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("A question for you")
            .to_string()
    });
    PendingDecision {
        id: format!("{QUESTION}{}", view.request_id),
        kind: PendingKind::Question,
        agent_id: None,
        summary,
        access: None,
        family: None,
        detail: None,
        code: None,
        working_dir: None,
        answers: vec![PendingAnswer::Answer, PendingAnswer::Refuse],
        questions,
        allow_other: true,
        origin: PendingOrigin {
            session_id: Some(view.session_id).filter(|s| !s.is_empty()),
            request_id: Some(view.request_id),
            ..PendingOrigin::default()
        },
        asked_at: Some(view.created_at),
    }
}
