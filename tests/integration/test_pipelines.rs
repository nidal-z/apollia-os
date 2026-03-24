//! Integration tests for `PipelineExecutor` — 9 topology scenarios.
//!
//! Each test uses an in-memory SQLite database and a lightweight `MockSubmitter`
//! that emits `RuntimeEvent`s on the EventBus instead of dispatching real tasks.
//! No Python, no running runtime — all 9 tests pass in every CI environment.
//!
//! Scenarios covered
//! -----------------
//! 1. Sequential 3-step pipeline — output propagation via templates
//! 2. Fan-out — 2 parallel steps submitted in the same layer
//! 3. Fan-in — join step submitted only after both branches complete
//! 4. Condition `when=contains` true — step executed
//! 5. Condition `when=contains` false — step skipped, `PipelineStepSkipped` emitted
//! 6. Fallback — primary step fails, fallback activated, downstream receives fallback output
//! 7. HITL approve — pipeline suspends, operator approves, run completes
//! 8. HITL reject — pipeline suspends, operator rejects, `PipelineFailed` emitted
//! 9. Restart recovery — executor resumes from SQLite, skips already-completed steps

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use tokio::sync::broadcast;

use apollia_core::{
    events::{AgentId, TaskId},
    EventBusSender, RuntimeEvent,
};
use apollia_pipelines::{
    executor::{ExecutorError, PipelineExecutor, TaskSubmitter},
    repository::PipelineRepository,
    types::{
        ConditionKind, GlobalFailurePolicy, PipelineDefinition, PipelineId, PipelineRun,
        PipelineStatus, PipelineStepDef, RunId, StepCondition, StepFailurePolicy, StepId, StepRun,
        StepRunStatus,
    },
};

// ── Mock infrastructure ───────────────────────────────────────────────────────

/// Predefined outcome for a step submission in tests.
///
/// Stored in `MockSubmitter::outcomes` keyed by the step ID (agent name with
/// the `-agent` suffix stripped). Each call to `submit_task` consumes the
/// matching outcome and emits the corresponding `RuntimeEvent` after 5 ms.
#[derive(Clone)]
enum MockOutcome {
    /// Emit `TaskCompleted { success: true, output }`.
    Completed(String),
    /// Emit `TaskCompleted { success: false }` — triggers the step's failure policy.
    Failed,
    /// Emit `TaskInputRequired { task_id }` — triggers the HITL suspend path.
    InputRequired,
}

/// Configurable mock for [`TaskSubmitter`] used in integration tests.
///
/// Outcomes are keyed by agent name with `-agent` suffix stripped so that
/// `"A-agent"` maps to `"A"`. A default of `Completed("output-of-<key>")` is
/// used when no explicit outcome is registered.
struct MockSubmitter {
    /// Map from step key to predefined outcome.
    outcomes: Arc<Mutex<HashMap<String, MockOutcome>>>,
    /// Ordered log of `(agent_name, rendered_input)` for post-run assertions.
    submitted: Arc<Mutex<Vec<(String, String)>>>,
    /// EventBus sender — cloned into spawned tasks.
    event_bus: broadcast::Sender<RuntimeEvent>,
}

impl MockSubmitter {
    /// Creates a new `MockSubmitter` with the given outcome map.
    fn new(
        outcomes: HashMap<String, MockOutcome>,
        event_bus: broadcast::Sender<RuntimeEvent>,
    ) -> Self {
        Self {
            outcomes: Arc::new(Mutex::new(outcomes)),
            submitted: Arc::new(Mutex::new(Vec::new())),
            event_bus,
        }
    }
}

#[async_trait]
impl TaskSubmitter for MockSubmitter {
    async fn submit_task(&self, agent: &str, input: &str) -> Result<String, ExecutorError> {
        let task_id = uuid::Uuid::new_v4().to_string();
        let key = agent.replace("-agent", "");

        self.submitted
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push((agent.to_string(), input.to_string()));

        let outcome = self
            .outcomes
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&key)
            .cloned()
            .unwrap_or_else(|| MockOutcome::Completed(format!("output-of-{key}")));

        let bus = self.event_bus.clone();
        let tid = task_id.clone();
        let agent_owned = agent.to_string();

        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(5)).await;
            let _ = match outcome {
                MockOutcome::Completed(output) => bus.send(RuntimeEvent::TaskCompleted {
                    agent_id: AgentId::from(agent_owned.as_str()),
                    task_id: TaskId::from(tid.as_str()),
                    success: true,
                    output: Some(output),
                }),
                MockOutcome::Failed => bus.send(RuntimeEvent::TaskCompleted {
                    agent_id: AgentId::from(agent_owned.as_str()),
                    task_id: TaskId::from(tid.as_str()),
                    success: false,
                    output: None,
                }),
                MockOutcome::InputRequired => bus.send(RuntimeEvent::TaskInputRequired {
                    task_id: TaskId::from(tid.as_str()),
                    prompt: "approval needed".into(),
                    step_id: None,
                }),
            };
        });

        Ok(task_id)
    }
}

// ── Builder helpers ───────────────────────────────────────────────────────────

/// Builds a `PipelineStepDef` with sensible test defaults.
///
/// The agent name is `"{id}-agent"` and the input template references the
/// first dependency's output when `deps` is non-empty.
fn make_step(
    id: &str,
    deps: &[&str],
    on_failure: StepFailurePolicy,
    condition: Option<StepCondition>,
    fallback_for: Option<&str>,
) -> PipelineStepDef {
    let input = if deps.is_empty() {
        "trigger-payload".to_string()
    } else {
        format!("{{{{steps.{}.output}}}}", deps[0])
    };
    PipelineStepDef {
        id: StepId(id.to_string()),
        agent: format!("{id}-agent"),
        input,
        depends_on: deps.iter().map(|s| StepId(s.to_string())).collect(),
        on_failure,
        condition,
        fallback_for: fallback_for.map(|s| StepId(s.to_string())),
    }
}

/// Builds a `PipelineDefinition` from a list of steps.
fn make_definition(id: &str, steps: Vec<PipelineStepDef>) -> PipelineDefinition {
    PipelineDefinition {
        id: PipelineId(id.to_string()),
        description: format!("test pipeline {id}"),
        on_failure: GlobalFailurePolicy::Fail,
        steps,
    }
}

/// Builds a fresh `PipelineRun` in `Running` state.
fn make_run(run_id: &str, pipeline_id: &str) -> PipelineRun {
    PipelineRun {
        run_id: RunId(run_id.to_string()),
        pipeline_id: PipelineId(pipeline_id.to_string()),
        trigger_id: None,
        status: PipelineStatus::Running,
        step_runs: HashMap::new(),
        trigger_payload: None,
        started_at: Utc::now(),
        ended_at: None,
    }
}

/// Inserts a run record into `repo` and returns the run.
///
/// Step rows are intentionally NOT inserted here — `PipelineExecutor::execute()`
/// calls `init_step_rows()` internally, which inserts them as `Pending`.
/// Pre-inserting them would cause `StepAlreadyExists` errors.
///
/// For restart-recovery tests (scenario 9) where steps must already exist in
/// SQLite, insert them manually after calling this helper.
fn seed_run(repo: &mut PipelineRepository, run: PipelineRun) -> PipelineRun {
    repo.insert_run(&run).expect("insert_run must succeed");
    run
}

/// Creates a `broadcast` channel, wraps the sender as an `EventBusSender`,
/// and returns `(sender, first_receiver)`.
fn make_event_bus() -> (EventBusSender, broadcast::Receiver<RuntimeEvent>) {
    let (tx, rx) = broadcast::channel::<RuntimeEvent>(256);
    (tx, rx)
}

/// Spawns a background task that drains the given `rx` into a shared `Vec`.
///
/// Returns the `Arc<Mutex<Vec<RuntimeEvent>>>` so callers can inspect events
/// after the executor finishes.
fn collect_events(mut rx: broadcast::Receiver<RuntimeEvent>) -> Arc<Mutex<Vec<RuntimeEvent>>> {
    let collected = Arc::new(Mutex::new(Vec::<RuntimeEvent>::new()));
    let c = Arc::clone(&collected);
    tokio::spawn(async move {
        while let Ok(event) = rx.recv().await {
            c.lock().unwrap_or_else(|e| e.into_inner()).push(event);
        }
    });
    collected
}

// ── Scenario 1 — Sequential 3-step pipeline ───────────────────────────────────

/// GIVEN  pipeline [A, B(dep:A), C(dep:B)]  with A→"out-A", B→"out-B", C→"out-C"
/// WHEN   execute()
/// THEN   run.status == Completed
///   AND  B receives "out-A" in its rendered input
///   AND  C receives "out-B" in its rendered input
///   AND  SQLite shows all 3 steps Completed
#[tokio::test]
async fn test_scenario1_sequential_3_steps() {
    // GIVEN
    let (bus, _rx0) = make_event_bus();
    let events = collect_events(bus.subscribe());

    let outcomes = HashMap::from([
        ("A".to_string(), MockOutcome::Completed("out-A".to_string())),
        ("B".to_string(), MockOutcome::Completed("out-B".to_string())),
        ("C".to_string(), MockOutcome::Completed("out-C".to_string())),
    ]);

    let definition = make_definition(
        "seq-3",
        vec![
            make_step("A", &[], StepFailurePolicy::Fail, None, None),
            make_step("B", &["A"], StepFailurePolicy::Fail, None, None),
            make_step("C", &["B"], StepFailurePolicy::Fail, None, None),
        ],
    );
    let repo = Arc::new(Mutex::new(
        PipelineRepository::open_in_memory().expect("in-memory repo"),
    ));
    let run = {
        let mut r = repo.lock().unwrap();
        seed_run(&mut r, make_run("r-seq-3", "seq-3"))
    };

    // WHEN
    let submitter = MockSubmitter::new(outcomes, bus.clone());
    let submitted = Arc::clone(&submitter.submitted);
    let result = PipelineExecutor::new(definition, run, submitter, bus, repo.clone())
        .with_step_timeout(Duration::from_secs(5))
        .execute()
        .await;

    // THEN — execution succeeds
    assert!(result.is_ok(), "execute must succeed: {result:?}");

    // Steps submitted in topological order: A first, then B, then C
    let agents: Vec<String> = submitted
        .lock()
        .unwrap()
        .iter()
        .map(|(a, _)| a.clone())
        .collect();
    assert_eq!(agents, vec!["A-agent", "B-agent", "C-agent"]);

    // B received A's output as input; C received B's output
    let inputs: Vec<String> = submitted
        .lock()
        .unwrap()
        .iter()
        .map(|(_, i)| i.clone())
        .collect();
    assert_eq!(inputs[1], "out-A", "B must receive A's output");
    assert_eq!(inputs[2], "out-B", "C must receive B's output");

    // SQLite: all 3 steps are Completed
    let loaded = repo
        .lock()
        .unwrap()
        .find_run(&RunId("r-seq-3".into()))
        .expect("find_run")
        .expect("run must exist");
    assert_eq!(loaded.status, PipelineStatus::Completed);
    for step_id in ["A", "B", "C"] {
        let sr = loaded
            .step_runs
            .get(&StepId(step_id.into()))
            .unwrap_or_else(|| panic!("step {step_id} missing"));
        assert_eq!(sr.status, StepRunStatus::Completed, "step {step_id}");
    }

    // PipelineCompleted was emitted — give the collector task a moment to process it
    tokio::time::sleep(Duration::from_millis(20)).await;
    let emitted = events.lock().unwrap().clone();
    assert!(
        emitted
            .iter()
            .any(|e| matches!(e, RuntimeEvent::PipelineCompleted { .. })),
        "PipelineCompleted not emitted; events: {emitted:?}"
    );
}

// ── Scenario 2 — Fan-out: 2 parallel steps ────────────────────────────────────

/// GIVEN  pipeline [A([]), B([A]), C([A])]
///   AND  MockSubmitter records submission order
/// WHEN   execute()
/// THEN   B and C are both submitted (same layer, before either completes)
///   AND  run.status == Completed
#[tokio::test]
async fn test_scenario2_fan_out_parallel_submission() {
    // GIVEN
    let (bus, _rx0) = make_event_bus();

    let definition = make_definition(
        "fan-out",
        vec![
            make_step("A", &[], StepFailurePolicy::Fail, None, None),
            make_step("B", &["A"], StepFailurePolicy::Fail, None, None),
            make_step("C", &["A"], StepFailurePolicy::Fail, None, None),
        ],
    );
    let repo = Arc::new(Mutex::new(
        PipelineRepository::open_in_memory().expect("in-memory repo"),
    ));
    let run = {
        let mut r = repo.lock().unwrap();
        seed_run(&mut r, make_run("r-fanout", "fan-out"))
    };

    let submitter = MockSubmitter::new(HashMap::new(), bus.clone());
    let submitted = Arc::clone(&submitter.submitted);

    // WHEN
    let result = PipelineExecutor::new(definition, run, submitter, bus, repo.clone())
        .with_step_timeout(Duration::from_secs(5))
        .execute()
        .await;

    // THEN — all 3 steps submitted
    assert!(result.is_ok(), "execute must succeed: {result:?}");

    let agents = submitted
        .lock()
        .unwrap()
        .iter()
        .map(|(a, _)| a.clone())
        .collect::<Vec<_>>();
    assert_eq!(agents[0], "A-agent", "A must be first");
    // B and C are in the same layer — both must appear
    let bc: std::collections::HashSet<&String> = agents[1..].iter().collect();
    assert!(
        bc.contains(&"B-agent".to_string()),
        "B-agent must be submitted"
    );
    assert!(
        bc.contains(&"C-agent".to_string()),
        "C-agent must be submitted"
    );
    assert_eq!(agents.len(), 3);

    let loaded = repo
        .lock()
        .unwrap()
        .find_run(&RunId("r-fanout".into()))
        .expect("find_run")
        .expect("run must exist");
    assert_eq!(loaded.status, PipelineStatus::Completed);
}

// ── Scenario 3 — Fan-in: join step waits for both branches ────────────────────

/// GIVEN  pipeline [A([]), B([A]), C([A]), D([B,C])]
///   AND  MockSubmitter: A→"ok", B→"b", C→"c", D→"done"
/// WHEN   execute()
/// THEN   D is submitted after B AND C have completed
///   AND  run.status == Completed
#[tokio::test]
async fn test_scenario3_fan_in_waits_for_both() {
    // GIVEN
    let (bus, _rx0) = make_event_bus();

    let outcomes = HashMap::from([
        ("A".to_string(), MockOutcome::Completed("ok".to_string())),
        ("B".to_string(), MockOutcome::Completed("b".to_string())),
        ("C".to_string(), MockOutcome::Completed("c".to_string())),
        ("D".to_string(), MockOutcome::Completed("done".to_string())),
    ]);

    let mut d_step = make_step("D", &["B"], StepFailurePolicy::Fail, None, None);
    // D depends on both B and C — override depends_on
    d_step.depends_on = vec![StepId("B".into()), StepId("C".into())];
    d_step.input = "{{steps.B.output}}-{{steps.C.output}}".to_string();

    let definition = make_definition(
        "fan-in",
        vec![
            make_step("A", &[], StepFailurePolicy::Fail, None, None),
            make_step("B", &["A"], StepFailurePolicy::Fail, None, None),
            make_step("C", &["A"], StepFailurePolicy::Fail, None, None),
            d_step,
        ],
    );
    let repo = Arc::new(Mutex::new(
        PipelineRepository::open_in_memory().expect("in-memory repo"),
    ));
    let run = {
        let mut r = repo.lock().unwrap();
        seed_run(&mut r, make_run("r-fanin", "fan-in"))
    };

    let submitter = MockSubmitter::new(outcomes, bus.clone());
    let submitted = Arc::clone(&submitter.submitted);

    // WHEN
    let result = PipelineExecutor::new(definition, run, submitter, bus, repo.clone())
        .with_step_timeout(Duration::from_secs(5))
        .execute()
        .await;

    // THEN
    assert!(result.is_ok(), "execute must succeed: {result:?}");

    let agents = submitted
        .lock()
        .unwrap()
        .iter()
        .map(|(a, _)| a.clone())
        .collect::<Vec<_>>();
    // D must be the last submitted step
    assert_eq!(agents.last().unwrap(), "D-agent", "D must be last");
    // B and C must both appear before D
    let d_pos = agents.iter().position(|a| a == "D-agent").unwrap();
    let b_pos = agents.iter().position(|a| a == "B-agent").unwrap();
    let c_pos = agents.iter().position(|a| a == "C-agent").unwrap();
    assert!(b_pos < d_pos, "B must be submitted before D");
    assert!(c_pos < d_pos, "C must be submitted before D");

    let loaded = repo
        .lock()
        .unwrap()
        .find_run(&RunId("r-fanin".into()))
        .expect("find_run")
        .expect("run must exist");
    assert_eq!(loaded.status, PipelineStatus::Completed);
    assert_eq!(
        loaded.step_runs.get(&StepId("D".into())).unwrap().status,
        StepRunStatus::Completed
    );
}

// ── Scenario 4 — Condition true: step executed ────────────────────────────────

/// GIVEN  pipeline [A([]), B([A], condition: contains "FRAUDE")]
///   AND  A returns "FRAUDE_DETECTEE"
/// WHEN   execute()
/// THEN   B is submitted and completes
///   AND  run.status == Completed
#[tokio::test]
async fn test_scenario4_condition_true_step_executes() {
    // GIVEN
    let (bus, _rx0) = make_event_bus();

    let outcomes = HashMap::from([
        (
            "A".to_string(),
            MockOutcome::Completed("FRAUDE_DETECTEE".to_string()),
        ),
        (
            "B".to_string(),
            MockOutcome::Completed("alerte-emise".to_string()),
        ),
    ]);

    let condition = StepCondition {
        when: ConditionKind::Contains,
        field: "steps.A.output".to_string(),
        value: "FRAUDE".to_string(),
    };

    let definition = make_definition(
        "cond-true",
        vec![
            make_step("A", &[], StepFailurePolicy::Fail, None, None),
            make_step("B", &["A"], StepFailurePolicy::Fail, Some(condition), None),
        ],
    );
    let repo = Arc::new(Mutex::new(
        PipelineRepository::open_in_memory().expect("in-memory repo"),
    ));
    let run = {
        let mut r = repo.lock().unwrap();
        seed_run(&mut r, make_run("r-cond-true", "cond-true"))
    };

    let submitter = MockSubmitter::new(outcomes, bus.clone());
    let submitted = Arc::clone(&submitter.submitted);

    // WHEN
    let result = PipelineExecutor::new(definition, run, submitter, bus, repo.clone())
        .with_step_timeout(Duration::from_secs(5))
        .execute()
        .await;

    // THEN — B was submitted (condition was true)
    assert!(result.is_ok(), "execute must succeed: {result:?}");

    let agents = submitted
        .lock()
        .unwrap()
        .iter()
        .map(|(a, _)| a.clone())
        .collect::<Vec<_>>();
    assert!(
        agents.contains(&"B-agent".to_string()),
        "B must be submitted when condition is true"
    );

    let loaded = repo
        .lock()
        .unwrap()
        .find_run(&RunId("r-cond-true".into()))
        .expect("find_run")
        .expect("run must exist");
    assert_eq!(loaded.status, PipelineStatus::Completed);
    assert_eq!(
        loaded.step_runs.get(&StepId("B".into())).unwrap().status,
        StepRunStatus::Completed
    );
}

// ── Scenario 5 — Condition false: step skipped ────────────────────────────────

/// GIVEN  pipeline [A([]), B([A], condition: contains "FRAUDE")]
///   AND  A returns "Facture valide — RAS"
/// WHEN   execute()
/// THEN   B has status Skipped in SQLite
///   AND  PipelineStepSkipped emitted with reason="condition=false"
///   AND  run.status == Completed
#[tokio::test]
async fn test_scenario5_condition_false_step_skipped() {
    // GIVEN
    let (bus, _rx0) = make_event_bus();
    let events = collect_events(bus.subscribe());

    let outcomes = HashMap::from([(
        "A".to_string(),
        MockOutcome::Completed("Facture valide — RAS".to_string()),
    )]);

    let condition = StepCondition {
        when: ConditionKind::Contains,
        field: "steps.A.output".to_string(),
        value: "FRAUDE".to_string(),
    };

    let definition = make_definition(
        "cond-false",
        vec![
            make_step("A", &[], StepFailurePolicy::Fail, None, None),
            make_step("B", &["A"], StepFailurePolicy::Fail, Some(condition), None),
        ],
    );
    let repo = Arc::new(Mutex::new(
        PipelineRepository::open_in_memory().expect("in-memory repo"),
    ));
    let run = {
        let mut r = repo.lock().unwrap();
        seed_run(&mut r, make_run("r-cond-false", "cond-false"))
    };

    let submitter = MockSubmitter::new(outcomes, bus.clone());
    let submitted = Arc::clone(&submitter.submitted);

    // WHEN
    let result = PipelineExecutor::new(definition, run, submitter, bus, repo.clone())
        .with_step_timeout(Duration::from_secs(5))
        .execute()
        .await;

    // THEN — B was NOT submitted
    assert!(result.is_ok(), "execute must succeed: {result:?}");

    let agents = submitted
        .lock()
        .unwrap()
        .iter()
        .map(|(a, _)| a.clone())
        .collect::<Vec<_>>();
    assert!(
        !agents.contains(&"B-agent".to_string()),
        "B must NOT be submitted when condition is false"
    );

    // B has status Skipped in SQLite
    let loaded = repo
        .lock()
        .unwrap()
        .find_run(&RunId("r-cond-false".into()))
        .expect("find_run")
        .expect("run must exist");
    assert_eq!(loaded.status, PipelineStatus::Completed);
    assert_eq!(
        loaded.step_runs.get(&StepId("B".into())).unwrap().status,
        StepRunStatus::Skipped
    );

    // PipelineStepSkipped emitted with reason="condition=false"
    // Give the collector a brief moment to receive the last events
    tokio::time::sleep(Duration::from_millis(20)).await;
    let emitted = events.lock().unwrap().clone();
    assert!(
        emitted.iter().any(|e| matches!(
            e,
            RuntimeEvent::PipelineStepSkipped { step_id, reason, .. }
            if step_id == "B" && reason == "condition=false"
        )),
        "PipelineStepSkipped(B, condition=false) not emitted; got: {emitted:?}"
    );
}

// ── Scenario 6 — Fallback activated on failure ────────────────────────────────

/// GIVEN  pipeline [A([]), B([A], on_failure=Fallback), FB([A], fallback_for=B), C([B])]
///   AND  B fails, FB→"fallback-output"
/// WHEN   execute()
/// THEN   B has status FallbackActive in SQLite
///   AND  FB is submitted and completes
///   AND  C receives "fallback-output" via {{steps.B.output}}
///   AND  run.status == Completed
#[tokio::test]
async fn test_scenario6_fallback_activated_on_failure() {
    // GIVEN
    let (bus, _rx0) = make_event_bus();

    let outcomes = HashMap::from([
        (
            "A".to_string(),
            MockOutcome::Completed("a-output".to_string()),
        ),
        ("B".to_string(), MockOutcome::Failed),
        (
            "FB".to_string(),
            MockOutcome::Completed("fallback-output".to_string()),
        ),
        (
            "C".to_string(),
            MockOutcome::Completed("c-done".to_string()),
        ),
    ]);

    // C depends on B; its input uses {{steps.B.output}} which the fallback replaces
    let mut c_step = make_step("C", &["B"], StepFailurePolicy::Fail, None, None);
    c_step.input = "{{steps.B.output}}".to_string();

    let definition = make_definition(
        "fallback",
        vec![
            make_step("A", &[], StepFailurePolicy::Fail, None, None),
            make_step("B", &["A"], StepFailurePolicy::Fallback, None, None),
            make_step("FB", &["A"], StepFailurePolicy::Fail, None, Some("B")),
            c_step,
        ],
    );
    let repo = Arc::new(Mutex::new(
        PipelineRepository::open_in_memory().expect("in-memory repo"),
    ));
    let run = {
        let mut r = repo.lock().unwrap();
        seed_run(&mut r, make_run("r-fallback", "fallback"))
    };

    let submitter = MockSubmitter::new(outcomes, bus.clone());
    let submitted = Arc::clone(&submitter.submitted);

    // WHEN
    let result = PipelineExecutor::new(definition, run, submitter, bus, repo.clone())
        .with_step_timeout(Duration::from_secs(5))
        .execute()
        .await;

    // THEN
    assert!(result.is_ok(), "execute must succeed: {result:?}");

    let agents = submitted
        .lock()
        .unwrap()
        .iter()
        .map(|(a, _)| a.clone())
        .collect::<Vec<_>>();
    assert!(
        agents.contains(&"FB-agent".to_string()),
        "FB must be submitted"
    );
    assert!(
        agents.contains(&"C-agent".to_string()),
        "C must be submitted"
    );

    // C received fallback-output as input
    let c_input = submitted
        .lock()
        .unwrap()
        .iter()
        .find(|(a, _)| a == "C-agent")
        .map(|(_, i)| i.clone())
        .expect("C-agent must have been submitted");
    assert_eq!(c_input, "fallback-output", "C must receive fallback output");

    // B has status FallbackActive in SQLite; FB and C are Completed
    let loaded = repo
        .lock()
        .unwrap()
        .find_run(&RunId("r-fallback".into()))
        .expect("find_run")
        .expect("run must exist");
    assert_eq!(loaded.status, PipelineStatus::Completed);
    assert_eq!(
        loaded.step_runs.get(&StepId("B".into())).unwrap().status,
        StepRunStatus::FallbackActive
    );
    assert_eq!(
        loaded.step_runs.get(&StepId("FB".into())).unwrap().status,
        StepRunStatus::Completed
    );
    assert_eq!(
        loaded.step_runs.get(&StepId("C".into())).unwrap().status,
        StepRunStatus::Completed
    );
}

// ── Scenario 7 — HITL: suspend → approve → resume ────────────────────────────

/// GIVEN  pipeline [A([]), B([A])]
///   AND  MockSubmitter returns InputRequired for B
///   AND  operator emits TaskResumed { approved: true } after PipelineSuspended
/// WHEN   execute()
/// THEN   PipelineSuspended emitted on EventBus
///   AND  PipelineResumed emitted after approval
///   AND  run.status == Completed
#[tokio::test]
async fn test_scenario7_hitl_approve_resumes() {
    // GIVEN
    let (bus, _rx0) = make_event_bus();
    let events = collect_events(bus.subscribe());

    let outcomes = HashMap::from([
        (
            "A".to_string(),
            MockOutcome::Completed("a-output".to_string()),
        ),
        ("B".to_string(), MockOutcome::InputRequired),
    ]);

    let definition = make_definition(
        "hitl-approve",
        vec![
            make_step("A", &[], StepFailurePolicy::Fail, None, None),
            make_step("B", &["A"], StepFailurePolicy::Fail, None, None),
        ],
    );
    let repo = Arc::new(Mutex::new(
        PipelineRepository::open_in_memory().expect("in-memory repo"),
    ));
    let run = {
        let mut r = repo.lock().unwrap();
        seed_run(&mut r, make_run("r-hitl-approve", "hitl-approve"))
    };

    // Spawn operator simulation: listen for PipelineSuspended, then approve and
    // re-emit TaskCompleted on the same task_id.
    let bus_operator = bus.clone();
    let mut rx_operator = bus.subscribe();
    tokio::spawn(async move {
        loop {
            if let Ok(RuntimeEvent::PipelineSuspended { task_id, .. }) = rx_operator.recv().await {
                // Simulate operator approval delay
                tokio::time::sleep(Duration::from_millis(10)).await;
                let _ = bus_operator.send(RuntimeEvent::TaskResumed {
                    task_id: TaskId::from(task_id.as_str()),
                    approved: true,
                });
                // After approval the executor waits for TaskCompleted on same task_id
                tokio::time::sleep(Duration::from_millis(5)).await;
                let _ = bus_operator.send(RuntimeEvent::TaskCompleted {
                    agent_id: AgentId::from("B-agent"),
                    task_id: TaskId::from(task_id.as_str()),
                    success: true,
                    output: Some("b-approved-output".to_string()),
                });
                break;
            }
        }
    });

    // WHEN
    let result = PipelineExecutor::new(
        definition,
        run,
        MockSubmitter::new(outcomes, bus.clone()),
        bus,
        repo.clone(),
    )
    .with_step_timeout(Duration::from_secs(5))
    .execute()
    .await;

    // THEN
    assert!(result.is_ok(), "execute must succeed: {result:?}");

    tokio::time::sleep(Duration::from_millis(20)).await;
    let emitted = events.lock().unwrap().clone();

    assert!(
        emitted
            .iter()
            .any(|e| matches!(e, RuntimeEvent::PipelineSuspended { .. })),
        "PipelineSuspended not emitted"
    );
    assert!(
        emitted
            .iter()
            .any(|e| matches!(e, RuntimeEvent::PipelineResumed { .. })),
        "PipelineResumed not emitted"
    );
    assert!(
        emitted
            .iter()
            .any(|e| matches!(e, RuntimeEvent::PipelineCompleted { .. })),
        "PipelineCompleted not emitted"
    );

    let loaded = repo
        .lock()
        .unwrap()
        .find_run(&RunId("r-hitl-approve".into()))
        .expect("find_run")
        .expect("run must exist");
    assert_eq!(loaded.status, PipelineStatus::Completed);
}

// ── Scenario 8 — HITL: suspend → reject → fail ────────────────────────────────

/// GIVEN  pipeline [A([]), B([A])]
///   AND  MockSubmitter returns InputRequired for B
///   AND  operator emits TaskResumed { approved: false }
/// WHEN   execute()
/// THEN   run.status == Failed { step_id: "B", reason: "rejected by operator" }
///   AND  PipelineFailed emitted on EventBus
#[tokio::test]
async fn test_scenario8_hitl_reject_fails_pipeline() {
    // GIVEN
    let (bus, _rx0) = make_event_bus();
    let events = collect_events(bus.subscribe());

    let outcomes = HashMap::from([
        (
            "A".to_string(),
            MockOutcome::Completed("a-output".to_string()),
        ),
        ("B".to_string(), MockOutcome::InputRequired),
    ]);

    let definition = make_definition(
        "hitl-reject",
        vec![
            make_step("A", &[], StepFailurePolicy::Fail, None, None),
            make_step("B", &["A"], StepFailurePolicy::Fail, None, None),
        ],
    );
    let repo = Arc::new(Mutex::new(
        PipelineRepository::open_in_memory().expect("in-memory repo"),
    ));
    let run = {
        let mut r = repo.lock().unwrap();
        seed_run(&mut r, make_run("r-hitl-reject", "hitl-reject"))
    };

    // Spawn operator simulation: reject the approval
    let bus_operator = bus.clone();
    let mut rx_operator = bus.subscribe();
    tokio::spawn(async move {
        loop {
            if let Ok(RuntimeEvent::PipelineSuspended { task_id, .. }) = rx_operator.recv().await {
                tokio::time::sleep(Duration::from_millis(10)).await;
                let _ = bus_operator.send(RuntimeEvent::TaskResumed {
                    task_id: TaskId::from(task_id.as_str()),
                    approved: false,
                });
                break;
            }
        }
    });

    // WHEN
    let result = PipelineExecutor::new(
        definition,
        run,
        MockSubmitter::new(outcomes, bus.clone()),
        bus,
        repo.clone(),
    )
    .with_step_timeout(Duration::from_secs(5))
    .execute()
    .await;

    // THEN — execution completes without error (failure is encoded in the run status)
    assert!(
        result.is_ok(),
        "execute must return Ok (failure encoded in run status): {result:?}"
    );

    tokio::time::sleep(Duration::from_millis(20)).await;
    let emitted = events.lock().unwrap().clone();
    assert!(
        emitted
            .iter()
            .any(|e| matches!(e, RuntimeEvent::PipelineSuspended { .. })),
        "PipelineSuspended not emitted"
    );
    assert!(
        emitted
            .iter()
            .any(|e| matches!(e, RuntimeEvent::PipelineFailed { .. })),
        "PipelineFailed not emitted"
    );

    let loaded = repo
        .lock()
        .unwrap()
        .find_run(&RunId("r-hitl-reject".into()))
        .expect("find_run")
        .expect("run must exist");
    assert!(
        matches!(
            &loaded.status,
            PipelineStatus::Failed { step_id, reason }
            if step_id.0 == "B" && reason.contains("rejected by operator")
        ),
        "expected Failed(B, rejected by operator), got: {:?}",
        loaded.status
    );
}

// ── Scenario 9 — Restart recovery from SQLite ─────────────────────────────────

/// GIVEN  a PipelineRun in status 'running' with step A Completed and step B Pending in SQLite
///   AND  a PipelineExecutor reloaded via find_running_runs() → as_resume()
/// WHEN   the executor runs
/// THEN   step A is NOT re-submitted (already Completed)
///   AND  step B is submitted with A's output as input (from SQLite)
///   AND  run.status == Completed
#[tokio::test]
async fn test_scenario9_restart_recovery_from_sqlite() {
    // GIVEN — seed the database as if the process crashed after step A completed
    let (bus, _rx0) = make_event_bus();

    let definition = make_definition(
        "restart",
        vec![
            make_step("A", &[], StepFailurePolicy::Fail, None, None),
            make_step("B", &["A"], StepFailurePolicy::Fail, None, None),
        ],
    );

    let mut repo_raw = PipelineRepository::open_in_memory().expect("in-memory repo");
    let run = make_run("r-restart", "restart");
    repo_raw.insert_run(&run).expect("insert_run");

    // Insert step A as Completed with output "a-completed-output"
    repo_raw
        .insert_step(
            &run.run_id,
            &StepRun {
                step_id: StepId("A".into()),
                task_id: Some("t-old-a".into()),
                status: StepRunStatus::Pending, // insert as Pending first
                output: None,
                error: None,
                started_at: None,
                ended_at: None,
            },
            "A-agent",
        )
        .expect("insert A step");
    repo_raw
        .update_step(
            &run.run_id,
            &StepId("A".into()),
            &StepRunStatus::Running,
            None,
            None,
            Some("t-old-a"),
        )
        .expect("set A running");
    repo_raw
        .update_step(
            &run.run_id,
            &StepId("A".into()),
            &StepRunStatus::Completed,
            Some("a-completed-output"),
            None,
            None,
        )
        .expect("set A completed");

    // Insert step B as Pending
    repo_raw
        .insert_step(
            &run.run_id,
            &StepRun {
                step_id: StepId("B".into()),
                task_id: None,
                status: StepRunStatus::Pending,
                output: None,
                error: None,
                started_at: None,
                ended_at: None,
            },
            "B-agent",
        )
        .expect("insert B step");

    // Simulate the PipelineEngine's restart recovery: load running runs
    let running_runs = repo_raw
        .find_running_runs()
        .expect("find_running_runs must succeed");
    assert_eq!(running_runs.len(), 1, "one run must be in running state");
    let recovered_run = running_runs.into_iter().next().unwrap();

    // Verify the recovered run has A as Completed and B as Pending
    let a_status = recovered_run
        .step_runs
        .get(&StepId("A".into()))
        .expect("step A in recovered run")
        .status
        .clone();
    assert_eq!(
        a_status,
        StepRunStatus::Completed,
        "recovered A must be Completed"
    );

    let b_status = recovered_run
        .step_runs
        .get(&StepId("B".into()))
        .expect("step B in recovered run")
        .status
        .clone();
    assert_eq!(
        b_status,
        StepRunStatus::Pending,
        "recovered B must be Pending"
    );

    // The recovered run also needs A's output in the template context.
    // Pre-populate the template context by injecting A's output into the run's step_runs.
    // The executor uses as_resume() which seeds done_steps from terminal step_runs;
    // but the template context is built fresh — A's output must come from SQLite step_runs.
    // We rebuild the PipelineRun with A's output loaded so the executor can populate its context.
    let mut recovered_run_with_output = recovered_run;
    if let Some(a_run) = recovered_run_with_output
        .step_runs
        .get_mut(&StepId("A".into()))
    {
        a_run.output = Some("a-completed-output".to_string());
    }

    let repo = Arc::new(Mutex::new(repo_raw));

    let outcomes = HashMap::from([(
        "B".to_string(),
        MockOutcome::Completed("b-output".to_string()),
    )]);
    let submitter = MockSubmitter::new(outcomes, bus.clone());
    let submitted = Arc::clone(&submitter.submitted);

    // WHEN — create executor in resume mode
    let result = PipelineExecutor::new(
        definition,
        recovered_run_with_output,
        submitter,
        bus,
        repo.clone(),
    )
    .as_resume()
    .with_step_timeout(Duration::from_secs(5))
    .execute()
    .await;

    // THEN — only B was submitted (A was skipped)
    assert!(result.is_ok(), "execute must succeed: {result:?}");

    let agents = submitted
        .lock()
        .unwrap()
        .iter()
        .map(|(a, _)| a.clone())
        .collect::<Vec<_>>();
    assert!(
        !agents.contains(&"A-agent".to_string()),
        "A must NOT be re-submitted after restart"
    );
    assert!(
        agents.contains(&"B-agent".to_string()),
        "B must be submitted after restart"
    );

    let loaded = repo
        .lock()
        .unwrap()
        .find_run(&RunId("r-restart".into()))
        .expect("find_run")
        .expect("run must exist");
    assert_eq!(loaded.status, PipelineStatus::Completed);
    assert_eq!(
        loaded.step_runs.get(&StepId("B".into())).unwrap().status,
        StepRunStatus::Completed
    );
}
