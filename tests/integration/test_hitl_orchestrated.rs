//! Integration tests for a human decision in the middle of a plan.
//!
//! A step holds no approval of its own: the tool proxy the plan runs on
//! decides each call where it is made. These tests hold what `ActorLoop` owes
//! that decision:
//! - a call the proxy holds does not reach the tool before the answer, and
//!   the step runs once the answer is yes;
//! - a call the proxy refuses never reaches the tool, the plan stops, and the
//!   steps after it do not run.
//!
//! All tests use a mock `ToolProxyTrait` and a `Reasoner` with a stub LLM.
//! No Python or real LLM required.

use std::pin::Pin;
use std::sync::{Arc, Mutex};

use apollia_core::{AgentManifest, RuntimeEvent, TaskStatus};
use apollia_llm::{
    CompletionModel, CompletionRequest, CompletionResponse, FinishReason, LlmError, LlmRouter,
    StreamChunk, TokenUsage,
};
use apollia_oria::{
    actor::{ActorLoop, StepDeps as ActorStepDeps, ToolProxyTrait},
    budget::StepBudget,
    plan::{ExecutionPlan, PlanStep},
    plan_repository::PlanRepository,
    reasoner::Reasoner,
    resilience::ResilienceLayer,
};
use async_trait::async_trait;

// ── Stub LLM (required by ActorLoop::execute signature) ─────────────────────

/// Minimal `CompletionModel` that always returns an empty plan.
/// Used to satisfy the `reasoner` parameter: no replan is triggered in these
/// tests, the plan is inserted beforehand and the actor is given no replan.
struct StubLlm;

#[async_trait]
impl CompletionModel for StubLlm {
    async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        Ok(CompletionResponse {
            engine_timings: None,
            content: r#"{"steps":[]}"#.into(),
            tool_calls: vec![],
            usage: TokenUsage {
                prompt_tokens: 0,
                completion_tokens: 0,
                cost_usd: None,
                cache_read_input_tokens: 0,
                cache_write_input_tokens: 0,
            },
            finish_reason: FinishReason::Stop,
            latency_ms: 0,
            ttft_ms: None,
        })
    }

    async fn stream(
        &self,
        _req: CompletionRequest,
    ) -> Result<Pin<Box<dyn futures::Stream<Item = Result<StreamChunk, LlmError>> + Send>>, LlmError>
    {
        Err(LlmError::InferenceError(
            "stub does not support streaming".into(),
        ))
    }

    fn is_available(&self) -> bool {
        true
    }

    fn backend_name(&self) -> &str {
        "stub"
    }

    fn model_id(&self) -> &str {
        "stub-model"
    }
}

// ── Mock ToolProxy ───────────────────────────────────────────────────────────

/// The answer a human gives to a call the proxy asked about.
enum Answer {
    Yes,
    No(&'static str),
}

/// A proxy that decides each call where it is made, as the caller of ORIA
/// does: a call of `sensitive` signals that it asks, then waits for the
/// answer; any other tool runs. Records the tools that actually ran.
struct DecidingProxy {
    sensitive: &'static str,
    asked: tokio::sync::mpsc::Sender<()>,
    answer: tokio::sync::Mutex<tokio::sync::mpsc::Receiver<Answer>>,
    ran: Arc<Mutex<Vec<String>>>,
}

/// What a test keeps of a [`DecidingProxy`]: the signal that a call asks,
/// the way to answer it, and the tools that ran.
struct Human {
    asked: tokio::sync::mpsc::Receiver<()>,
    answer: tokio::sync::mpsc::Sender<Answer>,
    ran: Arc<Mutex<Vec<String>>>,
}

fn deciding_proxy(sensitive: &'static str) -> (DecidingProxy, Human) {
    let (asked_tx, asked_rx) = tokio::sync::mpsc::channel(4);
    let (answer_tx, answer_rx) = tokio::sync::mpsc::channel(4);
    let ran = Arc::new(Mutex::new(Vec::new()));
    (
        DecidingProxy {
            sensitive,
            asked: asked_tx,
            answer: tokio::sync::Mutex::new(answer_rx),
            ran: Arc::clone(&ran),
        },
        Human {
            asked: asked_rx,
            answer: answer_tx,
            ran,
        },
    )
}

/// How many times `tool_name` ran.
fn count_for(ran: &Arc<Mutex<Vec<String>>>, tool_name: &str) -> usize {
    ran.lock()
        .unwrap()
        .iter()
        .filter(|t| t.as_str() == tool_name)
        .count()
}

#[async_trait]
impl ToolProxyTrait for DecidingProxy {
    async fn invoke(&self, tool_name: &str, _input: &serde_json::Value) -> Result<String, String> {
        if tool_name == self.sensitive {
            let _ = self.asked.send(()).await;
            match self.answer.lock().await.recv().await {
                Some(Answer::Yes) => {}
                Some(Answer::No(reason)) => {
                    return Err(format!(
                        "unauthorized: the operator refused '{tool_name}'. Reason: {reason}"
                    ));
                }
                None => return Err(format!("unauthorized: nobody answered for '{tool_name}'")),
            }
        }
        self.ran.lock().unwrap().push(tool_name.to_string());
        Ok(format!("{tool_name}: ok"))
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Opens an in-memory `PlanRepository`.
fn make_db() -> PlanRepository {
    PlanRepository::new(":memory:").expect("in-memory SQLite must always succeed")
}

/// Builds the manifest of the agent the plans belong to. It names no tool to
/// approve: a manifest has no key for that.
fn make_manifest() -> AgentManifest {
    serde_json::from_value(serde_json::json!({
        "name": "hitl-test-agent",
        "version": "0.1.0",
        "description": "HITL integration test agent"
    }))
    .expect("manifest must deserialize")
}

fn step(step_id: &str, description: &str, tool: &str, depends_on: &[&str]) -> PlanStep {
    PlanStep {
        step_id: step_id.into(),
        description: description.into(),
        tool_hint: Some(tool.into()),
        depends_on: depends_on.iter().map(|d| d.to_string()).collect(),
        model_hint: None,
        title: String::new(),
        status: Default::default(),
        rationale: None,
        provenance: Default::default(),
        args: None,
    }
}

// ── a call the proxy asks about -> the step waits -> yes -> the step runs ────

/// GIVEN a plan [s1:file_io, s2:smtp] on a proxy that asks about `smtp`
/// WHEN execute() runs, s1 executes normally, s2 waits for the answer,
///      then the answer is yes
/// THEN `smtp` did not run before the answer and ran once after it,
///      the plan finishes with the Completed status,
///      the actor itself raised no `TaskInputRequired`,
///      and both tools ran once
#[tokio::test]
async fn test_ac3_orchestrated_tool_suspend_approve_resume() {
    // GIVEN
    let plan = ExecutionPlan {
        plan_id: "plan-ac3".into(),
        task_id: "t-orch-001".into(),
        steps: vec![
            step("s1", "Read the file", "file_io", &[]),
            step("s2", "Send the email", "smtp", &["s1"]),
        ],
    };

    let db = make_db();
    db.insert_plan(&plan, "hitl-test-agent")
        .expect("insert_plan");
    db.insert_steps(&plan.plan_id, &plan.steps)
        .expect("insert_steps");

    let (bus_tx, mut bus_rx) = tokio::sync::broadcast::channel::<RuntimeEvent>(64);
    let (tool_proxy, mut human) = deciding_proxy("smtp");
    let ran = Arc::clone(&human.ran);

    let budget = StepBudget::unlimited();
    let resilience = ResilienceLayer::default();
    let reasoner = Reasoner::new(Arc::new(StubLlm), 10);
    let llm = LlmRouter::empty();

    let mut actor = ActorLoop::new(plan, 0, db, bus_tx, make_manifest());

    // WHEN - the human answers yes once the call of s2 asks
    let resolver = tokio::spawn(async move {
        human.asked.recv().await.expect("smtp must ask");
        let ran_before_the_answer = human.ran.lock().unwrap().clone();
        human
            .answer
            .send(Answer::Yes)
            .await
            .expect("the answer must be received");
        ran_before_the_answer
    });

    let result = actor
        .execute(
            ActorStepDeps {
                tool_proxy: &tool_proxy,
                llm_router: &llm,
                budget: &budget,
                reasoner: &reasoner,
            },
            &resilience,
        )
        .await;
    let ran_before_the_answer = resolver.await.expect("resolver must not panic");

    // THEN - when the call asked, s1 had run and smtp had not
    assert_eq!(
        ran_before_the_answer,
        vec!["file_io".to_string()],
        "smtp must not run before its answer"
    );

    // THEN - plan completed successfully
    assert_eq!(
        result.status,
        TaskStatus::Completed,
        "plan must complete after the answer; error: {:?}",
        result.error
    );

    // THEN - both tools ran (s1 file_io + s2 smtp), once each
    assert_eq!(
        ran.lock().unwrap().len(),
        2,
        "both file_io and smtp must run"
    );
    assert_eq!(count_for(&ran, "smtp"), 1, "smtp ran exactly once");
    assert_eq!(count_for(&ran, "file_io"), 1, "file_io ran exactly once");

    // THEN - the actor raised no approval of its own
    let mut raised = false;
    while let Ok(event) = bus_rx.try_recv() {
        if matches!(event, RuntimeEvent::TaskInputRequired { .. }) {
            raised = true;
        }
    }
    assert!(!raised, "a step holds no approval of its own");
}

// ── a call the proxy refuses -> plan stopped, the following steps do not run ─

/// GIVEN a plan [s1:file_io, s2:smtp, s3:file_io] on a proxy that asks about
///       `smtp`, and an actor given no replan
/// WHEN the answer to s2 is no
/// THEN s3 never runs,
///      the plan fails,
///      file_io ran once only (for s1, not s3),
///      and smtp never ran
#[tokio::test]
async fn test_ac4_orchestrated_tool_reject_stops_plan() {
    // GIVEN
    let plan = ExecutionPlan {
        plan_id: "plan-ac4".into(),
        task_id: "t-orch-002".into(),
        steps: vec![
            step("s1", "Read the file", "file_io", &[]),
            step("s2", "Send the email", "smtp", &["s1"]),
            step("s3", "Archive the result", "file_io", &["s2"]),
        ],
    };

    let db = make_db();
    db.insert_plan(&plan, "hitl-test-agent")
        .expect("insert_plan");
    db.insert_steps(&plan.plan_id, &plan.steps)
        .expect("insert_steps");

    let (bus_tx, _bus_rx) = tokio::sync::broadcast::channel::<RuntimeEvent>(64);
    let (tool_proxy, mut human) = deciding_proxy("smtp");
    let ran = Arc::clone(&human.ran);

    let budget = StepBudget::unlimited();
    let resilience = ResilienceLayer::default();
    let reasoner = Reasoner::new(Arc::new(StubLlm), 10);
    let llm = LlmRouter::empty();

    let mut actor = ActorLoop::new(plan, 0, db, bus_tx, make_manifest());

    // WHEN - the human answers no once the call of s2 asks
    let resolver = tokio::spawn(async move {
        human.asked.recv().await.expect("smtp must ask");
        human
            .answer
            .send(Answer::No("invalid recipient"))
            .await
            .expect("the answer must be received");
    });

    let result = actor
        .execute(
            ActorStepDeps {
                tool_proxy: &tool_proxy,
                llm_router: &llm,
                budget: &budget,
                reasoner: &reasoner,
            },
            &resilience,
        )
        .await;
    resolver.await.expect("resolver must not panic");

    // THEN - plan failed
    assert_eq!(
        result.status,
        TaskStatus::Failed,
        "plan must fail after the refusal"
    );

    // THEN - file_io ran exactly once (s1 only, NOT s3)
    assert_eq!(
        count_for(&ran, "file_io"),
        1,
        "file_io must only run once (s1) - s3 must not execute; ran: {:?}",
        ran.lock().unwrap()
    );
    assert_eq!(
        count_for(&ran, "smtp"),
        0,
        "smtp must never run after a refusal"
    );
}
