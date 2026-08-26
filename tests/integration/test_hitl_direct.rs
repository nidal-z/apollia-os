//! Integration tests for HITL Mode Direct.
//!
//! Tests the full suspend → approve/reject → resume chain using
//! a real `TaskRepository` (temp SQLite) + `ORIAEngine::execute_direct()`.
//! All tests use `#[tokio::test]` and mock `AgentRunner` (no Python required).

use std::pin::Pin;
use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc,
};

use apollia_core::{
    AIPResult, AIPTask, InputResponseData, PendingApprovals, StepBudgetConfig, TaskStatus,
};
use apollia_oria::{
    budget::StepBudget,
    engine::{AgentRunner, ORIAEngine},
};
use apollia_tools::TaskRepository;

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Opens a `TaskRepository` on a unique temp file, returning the guard so the
/// file is cleaned up when the guard drops.
async fn open_test_repo() -> (TaskRepository, tempfile::TempPath) {
    let f = tempfile::NamedTempFile::new().expect("failed to create temp file");
    let path = f.path().to_path_buf();
    // Convert to TempPath so the file is deleted on drop.
    let guard = f.into_temp_path();
    let repo = TaskRepository::open(&path)
        .await
        .expect("TaskRepository::open must succeed on fresh temp file");
    (repo, guard)
}

/// Returns a `StepBudget` with generous limits - never exhausted during tests.
fn make_budget() -> Arc<StepBudget> {
    Arc::new(StepBudget::new(&StepBudgetConfig {
        max_steps: 10,
        max_tool_calls: 20,
        wall_clock_secs: 300,
    }))
}

// ── Mock AgentRunner ─────────────────────────────────────────────────────────

/// Returns `InputRequired` on the first `call_run()` invocation,
/// then `Completed` on the second (after HITL approval).
///
/// Asserts `is_resumed=true` and `input_response.is_some()` on the second call
/// to verify that the engine correctly rebuilds the resumed task.
struct MockHitlRunner {
    /// Shared atomic so the caller can assert the exact call count.
    call_count: Arc<AtomicU32>,
}

impl AgentRunner for MockHitlRunner {
    fn call_run(
        &self,
        task: AIPTask,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<AIPResult, String>> + Send + '_>> {
        let n = self.call_count.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            if n == 0 {
                // First call - suspend for human approval
                Ok(AIPResult::input_required(
                    "Confirm sending the quote?",
                    serde_json::json!({"devis_id": 42, "montant": 12500}),
                ))
            } else {
                // Second call after approval - verify resumed task fields
                assert!(
                    task.is_resumed,
                    "task.is_resumed must be true on second call"
                );
                assert!(
                    task.input_response.is_some(),
                    "task.input_response must be set on resumed task"
                );
                Ok(AIPResult::completed("Quote sent successfully"))
            }
        })
    }
}

// ── Direct mode: approve -> run() called again -> Completed ──────────

/// GIVEN a mock agent returning InputRequired on the first call
///       and Completed on the second (is_resumed=True)
/// WHEN execute_direct() runs, then pending.resolve(approved=true)
/// THEN run() is called twice, the final result is Completed,
///      TaskInputRequired{step_id:None} is emitted on the EventBus
#[tokio::test]
async fn test_ac1_mode_direct_approve_recalls_run() {
    // GIVEN
    let (repo, _guard) = open_test_repo().await;
    let pending = Arc::new(PendingApprovals::new());
    let (event_tx, mut event_rx) = tokio::sync::broadcast::channel(32);

    let call_count = Arc::new(AtomicU32::new(0));
    let runner = MockHitlRunner {
        call_count: call_count.clone(),
    };

    let task = AIPTask {
        task_id: "t-hitl-001".into(),
        ..AIPTask::default()
    };

    let engine = ORIAEngine::new()
        .with_pending_approvals(Arc::clone(&pending))
        .with_task_repository(Arc::new(repo))
        .with_event_bus(event_tx);

    // WHEN - spawn a resolver task that approves once TaskInputRequired is seen
    let pending_for_resolver = Arc::clone(&pending);
    let resolver = tokio::spawn(async move {
        // Poll the EventBus until TaskInputRequired is emitted (max ~2 s)
        for _ in 0..100_u32 {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            if let Ok(apollia_core::RuntimeEvent::TaskInputRequired { .. }) = event_rx.try_recv() {
                break;
            }
        }
        pending_for_resolver
            .resolve(
                "t-hitl-001",
                InputResponseData {
                    approved: true,
                    reason: None,
                    context: serde_json::Value::Null,
                    responded_at: "2026-01-01T00:00:00Z".into(),
                },
            )
            .expect("resolve must succeed for t-hitl-001");
    });

    let result = engine.execute_direct(task, &runner, make_budget()).await;
    resolver.await.expect("resolver task must not panic");

    // THEN - result is Completed
    assert!(
        result.is_ok(),
        "execute_direct must return Ok; got: {:?}",
        result.err()
    );
    assert_eq!(
        result.unwrap().status,
        TaskStatus::Completed,
        "final status must be Completed"
    );

    // THEN - run() was called exactly twice (first → InputRequired, second → Completed)
    assert_eq!(
        call_count.load(Ordering::SeqCst),
        2,
        "run() must be called exactly twice"
    );
}

// ── Mode Direct : reject → AIPResult::failed("REJECTED") ──────────────

/// GIVEN the same mock agent (returns InputRequired on the first call)
/// WHEN pending.resolve(approved=false, reason="Test refusal")
/// THEN run() is called once only (no second call),
///      result: AIPResult::failed("REJECTED", "Test refusal")
#[tokio::test]
async fn test_ac2_mode_direct_reject_no_second_call() {
    // GIVEN
    let (repo, _guard) = open_test_repo().await;
    let pending = Arc::new(PendingApprovals::new());
    let (event_tx, mut event_rx) = tokio::sync::broadcast::channel(32);

    let call_count = Arc::new(AtomicU32::new(0));
    let runner = MockHitlRunner {
        call_count: call_count.clone(),
    };

    let task = AIPTask {
        task_id: "t-hitl-002".into(),
        ..AIPTask::default()
    };

    let engine = ORIAEngine::new()
        .with_pending_approvals(Arc::clone(&pending))
        .with_task_repository(Arc::new(repo))
        .with_event_bus(event_tx);

    // WHEN - spawn a resolver that rejects
    let pending_for_resolver = Arc::clone(&pending);
    let resolver = tokio::spawn(async move {
        for _ in 0..100_u32 {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            if let Ok(apollia_core::RuntimeEvent::TaskInputRequired { .. }) = event_rx.try_recv() {
                break;
            }
        }
        pending_for_resolver
            .resolve(
                "t-hitl-002",
                InputResponseData {
                    approved: false,
                    reason: Some("Refus test".into()),
                    context: serde_json::Value::Null,
                    responded_at: "2026-01-01T00:00:00Z".into(),
                },
            )
            .expect("resolve must succeed for t-hitl-002");
    });

    let result = engine.execute_direct(task, &runner, make_budget()).await;
    resolver.await.expect("resolver task must not panic");

    // THEN - result is Ok (execute_direct wraps rejection as AIPResult, not Err)
    assert!(
        result.is_ok(),
        "execute_direct must return Ok even on rejection; got: {:?}",
        result.err()
    );
    let aip = result.unwrap();

    // THEN - status Failed with code REJECTED
    assert_eq!(aip.status, TaskStatus::Failed, "status must be Failed");
    let code = aip.error.as_ref().map(|e| e.code.as_str()).unwrap_or("");
    assert_eq!(code, "REJECTED", "error code must be REJECTED");

    let msg = aip.error.as_ref().map(|e| e.message.as_str()).unwrap_or("");
    assert!(
        msg.contains("Refus test"),
        "rejection reason must appear in error message; got: {msg}"
    );

    // THEN - run() called exactly once (no second call after rejection)
    assert_eq!(
        call_count.load(Ordering::SeqCst),
        1,
        "run() must be called exactly once when rejected"
    );
}
