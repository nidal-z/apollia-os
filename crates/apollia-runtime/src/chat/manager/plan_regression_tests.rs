//! Regression tests for the approve-while-processing race on the chat-native
//! plan gate: a plan decision taken while the submitting turn is still running
//! must survive the turn's completion instead of being dropped and clobbered.

use super::tests::{manager_with_session_in_phase, plan_step_for_test};
use super::*;
use crate::chat::builtin_agent::ChatAgentResponse;
use apollia_core::plan::{PlanMutationKind, PlanStatus};
use apollia_llm::types::TokenUsage;

/// Minimal completed-turn response whose plan-flow snapshot ended in `phase`.
fn response_with_final_phase(phase: Option<PlanPhase>) -> ChatAgentResponse {
    ChatAgentResponse {
        content: "done".into(),
        tool_calls: vec![],
        newly_authorized: vec![],
        tokens_used: TokenUsage::default(),
        thinking_trace: None,
        reasoning_boundaries: vec![],
        verification_report: None,
        frontier_ceiling_reached: false,
        final_plan_phase: phase,
        paused: false,
        context_window_tokens: None,
        context_tokens_used: 0,
    }
}

/// Attach a plan actor holding a submitted (awaiting approval) two-step plan.
async fn attach_submitted_plan(manager: &mut ChatSessionManager) {
    let plan = crate::chat::plan_actor::spawn_plan_actor(
        rusqlite::Connection::open_in_memory().expect("in-memory db"),
        None,
    )
    .expect("spawn plan actor");
    plan.propose(
        "sess-1",
        vec![plan_step_for_test("a"), plan_step_for_test("b")],
        None,
    )
    .await
    .expect("propose");
    plan.submit("sess-1").await.expect("submit");
    manager.plan_handle = Some(plan);
}

#[tokio::test]
async fn test_approve_while_processing_queues_continuation() {
    // GIVEN a submitted plan whose session is still Processing: the operator
    // clicked approve before the submitting turn's ExchangeComplete landed.
    let dir = tempfile::tempdir().expect("tempdir");
    let (mut manager, _rx) = manager_with_session_in_phase(&dir, PlanPhase::Discovery);
    attach_submitted_plan(&mut manager).await;
    manager.sessions.get_mut("sess-1").unwrap().status = SessionStatus::Processing;

    // WHEN the approval is handled mid-turn
    manager
        .handle_approve_plan("sess-1")
        .await
        .expect("approve reconciles via the persisted plan status");

    // THEN the continuation could not dispatch (session busy) and is parked
    // instead of dropped; no new exchange was started by the approval itself.
    assert!(manager.pending_plan_continuations.contains_key("sess-1"));
    assert_eq!(
        manager.sessions.get("sess-1").unwrap().plan_phase,
        PlanPhase::Executing
    );

    // WHEN the in-flight turn completes (its stale snapshot says the turn
    // ended awaiting approval)
    manager.handle_exchange_complete(
        "sess-1",
        "msg-1",
        response_with_final_phase(Some(PlanPhase::AwaitingApproval)),
    );

    // THEN the parked continuation is dispatched: the session accepted the
    // execute directive as a new exchange and the queue is drained.
    assert!(!manager.pending_plan_continuations.contains_key("sess-1"));
    let session = manager.sessions.get("sess-1").unwrap();
    assert_eq!(session.status, SessionStatus::Processing);
    assert!(
        session
            .history
            .iter()
            .any(|m| m.role == ChatRole::User && m.content.contains("The plan was approved")),
        "execute directive should be appended as the continuation user turn"
    );
}

#[tokio::test]
async fn test_exchange_complete_cannot_regress_executing_phase() {
    // GIVEN a session already moved to Executing by a mid-turn approval
    let dir = tempfile::tempdir().expect("tempdir");
    let (mut manager, _rx) = manager_with_session_in_phase(&dir, PlanPhase::Executing);
    manager.sessions.get_mut("sess-1").unwrap().status = SessionStatus::Processing;

    // WHEN the completed turn carries its stale AwaitingApproval snapshot
    manager.handle_exchange_complete(
        "sess-1",
        "msg-1",
        response_with_final_phase(Some(PlanPhase::AwaitingApproval)),
    );

    // THEN the write-back is refused: the phase stays Executing in memory and
    // in SQLite, so the approved gate is not silently re-opened.
    assert_eq!(
        manager.sessions.get("sess-1").unwrap().plan_phase,
        PlanPhase::Executing
    );
    let persisted = manager
        .repository
        .get_session("sess-1")
        .expect("get")
        .expect("row");
    assert_eq!(persisted.plan_phase, PlanPhase::Executing.as_sql());
}

#[tokio::test]
async fn test_approve_persists_plan_status_executing() {
    // GIVEN a submitted plan awaiting approval
    let dir = tempfile::tempdir().expect("tempdir");
    let (mut manager, _rx) = manager_with_session_in_phase(&dir, PlanPhase::AwaitingApproval);
    attach_submitted_plan(&mut manager).await;

    // WHEN the plan is approved
    manager
        .handle_approve_plan("sess-1")
        .await
        .expect("approve ok");

    // THEN the persisted plan row moved to Executing with an Approve mutation
    // in the history, so the gate cannot be reconciled open again later.
    let plan_handle = manager.plan_handle.as_ref().unwrap();
    let plan = plan_handle
        .get_plan("sess-1")
        .await
        .expect("get plan")
        .expect("plan exists");
    assert_eq!(plan.status, PlanStatus::Executing);
    let mutations = plan_handle
        .read_mutations("sess-1")
        .await
        .expect("mutations");
    assert!(
        mutations
            .iter()
            .any(|m| matches!(m.kind, PlanMutationKind::Approve)),
        "approval should be recorded in the mutation history"
    );
}

#[tokio::test]
async fn test_second_approve_after_execution_rejected() {
    // GIVEN an already approved (executing) plan
    let dir = tempfile::tempdir().expect("tempdir");
    let (mut manager, _rx) = manager_with_session_in_phase(&dir, PlanPhase::AwaitingApproval);
    attach_submitted_plan(&mut manager).await;
    manager
        .handle_approve_plan("sess-1")
        .await
        .expect("first approve ok");

    // WHEN the approval is replayed
    let second = manager.handle_approve_plan("sess-1").await;

    // THEN it is rejected: the plan status is no longer AwaitingApproval, so
    // the guard's race fallback cannot re-open the gate.
    assert!(matches!(second, Err(ChatError::NotAwaitingApproval { .. })));
}
