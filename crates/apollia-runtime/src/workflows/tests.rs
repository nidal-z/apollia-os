//! The workflow actor over a scripted host and a scripted agent runner:
//! inputs, steps, nested calls, depth, shared budget, inherited token, stop.

use std::collections::HashMap;
use std::sync::Mutex;

use apollia_core::RuntimeEvent;
use apollia_oria::budget::StepBudget;
use serde_json::json;
use tokio::sync::broadcast;

use super::*;

type Script = Arc<dyn Fn(WorkflowCall) -> BoxFuture<'static, Result<Value, String>> + Send + Sync>;

/// A host whose workflows are closures, each declaring its steps and inputs.
#[derive(Default)]
struct ScriptedHost {
    workflows: HashMap<String, (WorkflowDescriptor, Script)>,
}

impl ScriptedHost {
    fn with(mut self, id: &str, steps: &[&str], schema: Option<Value>, script: Script) -> Self {
        let descriptor = WorkflowDescriptor {
            id: id.to_string(),
            description: format!("{id} for the tests"),
            steps: steps.iter().map(|s| (*s).to_string()).collect(),
            input_schema: schema,
        };
        self.workflows.insert(id.to_string(), (descriptor, script));
        self
    }
}

impl WorkflowHost for ScriptedHost {
    fn describe(&self, workflow_id: &str) -> Option<WorkflowDescriptor> {
        self.workflows.get(workflow_id).map(|(d, _)| d.clone())
    }

    fn run(&self, call: WorkflowCall) -> BoxFuture<'static, Result<Value, String>> {
        match self.workflows.get(&call.workflow.id) {
            Some((_, script)) => script(call),
            None => Box::pin(async { Err("unknown".to_string()) }),
        }
    }
}

/// What the scripted agent runner saw of one call.
struct AgentCall {
    agent: String,
    budget: Arc<StepBudget>,
    parent: CancellationToken,
}

/// Answers every agent with `answer to <message>`, unless the agent is
/// `waiter`, which waits for its token and reports it was stopped.
#[derive(Default)]
struct ScriptedAgents {
    calls: Mutex<Vec<AgentCall>>,
}

impl AgentRunner for ScriptedAgents {
    fn run(
        &self,
        agent: String,
        message: String,
        nesting: Nesting,
    ) -> BoxFuture<'_, Result<String, String>> {
        nesting.budget.increment_tool_calls();
        let token = nesting.parent.child_token();
        self.calls.lock().unwrap().push(AgentCall {
            agent: agent.clone(),
            budget: Arc::clone(&nesting.budget),
            parent: nesting.parent,
        });
        Box::pin(async move {
            if agent == "waiter" {
                token.cancelled().await;
                return Err(format!("agent '{agent}' canceled"));
            }
            Ok(format!("answer to {message}"))
        })
    }
}

fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "folder": {"type": "string"},
            "limit": {"type": "integer"},
            "notify": {"type": "boolean"}
        },
        "required": ["folder"],
        "additionalProperties": false
    })
}

/// Opens each step, runs `body` inside the first one, closes it, and returns
/// what `body` returned.
fn steps_then(
    steps: &'static [&'static str],
    body: impl Fn(Arc<dyn WorkflowServices>, Value) -> BoxFuture<'static, Result<Value, String>>
        + Send
        + Sync
        + 'static,
) -> Script {
    let body = Arc::new(body);
    Arc::new(move |call: WorkflowCall| {
        let body = Arc::clone(&body);
        Box::pin(async move {
            let mut result = Value::Null;
            for (index, step) in steps.iter().enumerate() {
                call.services.step_started((*step).to_string()).await?;
                let outcome = if index == 0 {
                    body(Arc::clone(&call.services), call.inputs.clone()).await
                } else {
                    Ok(Value::Null)
                };
                match outcome {
                    Ok(value) => {
                        call.services.step_ended((*step).to_string(), None).await?;
                        if index == 0 {
                            result = value;
                        }
                    }
                    Err(error) => {
                        call.services
                            .step_ended((*step).to_string(), Some(error.clone()))
                            .await?;
                        return Err(error);
                    }
                }
            }
            Ok(result)
        })
    })
}

fn noop() -> impl Fn(Arc<dyn WorkflowServices>, Value) -> BoxFuture<'static, Result<Value, String>>
       + Send
       + Sync
       + 'static {
    |_, inputs| Box::pin(async move { Ok(json!({"seen": inputs})) })
}

struct Fixture {
    _dir: tempfile::TempDir,
    repository: Arc<TaskRepository>,
    engine: WorkflowHandle,
    events: broadcast::Receiver<RuntimeEvent>,
    agents: Arc<ScriptedAgents>,
}

async fn fixture(host: ScriptedHost, budget: StepBudgetConfig) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let repository = Arc::new(
        TaskRepository::open(&dir.path().join("hitl.db"))
            .await
            .unwrap(),
    );
    let (bus, events) = broadcast::channel(256);
    let agents = Arc::new(ScriptedAgents::default());
    let engine = WorkflowHandle::spawn(
        repository.clone(),
        bus,
        budget,
        Some(agents.clone() as Arc<dyn AgentRunner>),
    );
    engine.set_host(Arc::new(host)).await.unwrap();
    Fixture {
        _dir: dir,
        repository,
        engine,
        events,
        agents,
    }
}

fn step_events(events: &mut broadcast::Receiver<RuntimeEvent>) -> Vec<String> {
    let mut seen = Vec::new();
    while let Ok(event) = events.try_recv() {
        match event {
            RuntimeEvent::WorkflowStepStarted { step, .. } => seen.push(format!("started {step}")),
            RuntimeEvent::WorkflowStepCompleted { step, .. } => {
                seen.push(format!("completed {step}"))
            }
            RuntimeEvent::WorkflowStepFailed { step, error, .. } => {
                seen.push(format!("failed {step}: {error}"))
            }
            _ => {}
        }
    }
    seen
}

fn statuses(steps: &[apollia_tools::WorkflowStepRow]) -> Vec<String> {
    steps.iter().map(|s| s.status.clone()).collect()
}

#[tokio::test]
async fn a_run_reports_each_step_on_the_bus_and_in_the_store() {
    // GIVEN a workflow of two steps
    let host = ScriptedHost::default().with(
        "invoices/close-month",
        &["extract", "file"],
        Some(schema()),
        steps_then(&["extract", "file"], noop()),
    );
    let mut fx = fixture(host, StepBudgetConfig::default()).await;

    // WHEN it runs to its end with valid inputs
    let run = fx
        .engine
        .run_to_end("invoices/close-month", json!({"folder": "/data/march"}))
        .await
        .unwrap();

    // THEN the run completed with its output, each step is completed with its
    //      timestamps, and the bus carried each start and end in order
    assert_eq!(run.status, "completed");
    assert_eq!(run.output, Some(json!({"seen": {"folder": "/data/march"}})));
    let steps = fx.repository.workflow_steps(&run.run_id).await.unwrap();
    assert_eq!(statuses(&steps), vec!["completed", "completed"]);
    assert!(steps
        .iter()
        .all(|s| s.started_at.is_some() && s.ended_at.is_some()));
    assert_eq!(
        step_events(&mut fx.events),
        vec![
            "started extract",
            "completed extract",
            "started file",
            "completed file"
        ]
    );
}

#[tokio::test]
async fn an_invalid_input_is_refused_with_its_field_and_nothing_is_recorded() {
    // GIVEN a workflow whose `folder` is required and `limit` an integer
    let host = ScriptedHost::default().with(
        "invoices/close-month",
        &["extract"],
        Some(schema()),
        steps_then(&["extract"], noop()),
    );
    let fx = fixture(host, StepBudgetConfig::default()).await;

    // WHEN it is launched without `folder`, then with a `limit` that is no integer
    let missing = fx
        .engine
        .launch("invoices/close-month", json!({"limit": 3}))
        .await
        .unwrap_err();
    let mistyped = fx
        .engine
        .launch(
            "invoices/close-month",
            json!({"folder": "/a", "limit": "many"}),
        )
        .await
        .unwrap_err();

    // THEN each refusal names its field, and no run was recorded
    assert!(matches!(&missing, WorkflowError::InvalidInput(e) if e.field == "folder"));
    assert!(matches!(&mistyped, WorkflowError::InvalidInput(e) if e.field == "limit"));
    let history = fx.engine.history("invoices/close-month", 10).await.unwrap();
    assert!(history.is_empty());
}

#[tokio::test]
async fn an_unknown_workflow_and_an_absent_host_are_refused() {
    // GIVEN an engine with a host, and one without
    let fx = fixture(ScriptedHost::default(), StepBudgetConfig::default()).await;
    let dir = tempfile::tempdir().unwrap();
    let repository = Arc::new(
        TaskRepository::open(&dir.path().join("hitl.db"))
            .await
            .unwrap(),
    );
    let (bus, _rx) = broadcast::channel(16);
    let hostless = WorkflowHandle::spawn(repository, bus, StepBudgetConfig::default(), None);

    // WHEN each is asked for a workflow
    let unknown = fx
        .engine
        .launch("invoices/none", json!({}))
        .await
        .unwrap_err();
    let no_host = hostless
        .launch("invoices/none", json!({}))
        .await
        .unwrap_err();

    // THEN the first names the workflow and the second the missing host
    assert!(matches!(unknown, WorkflowError::NotFound(id) if id == "invoices/none"));
    assert!(matches!(no_host, WorkflowError::NoHost));
}

#[tokio::test]
async fn a_failing_step_fails_the_run_and_leaves_the_next_steps_not_started() {
    // GIVEN a workflow whose first step raises
    let host = ScriptedHost::default().with(
        "invoices/close-month",
        &["extract", "file"],
        None,
        steps_then(&["extract", "file"], |_, _| {
            Box::pin(async { Err("ValueError: no invoice".to_string()) })
        }),
    );
    let mut fx = fixture(host, StepBudgetConfig::default()).await;

    // WHEN it runs
    let run = fx
        .engine
        .run_to_end("invoices/close-month", json!({}))
        .await
        .unwrap();

    // THEN the run failed with the error, the step failed and the next was not started
    assert_eq!(run.status, "failed");
    assert_eq!(run.error.as_deref(), Some("ValueError: no invoice"));
    let steps = fx.repository.workflow_steps(&run.run_id).await.unwrap();
    assert_eq!(statuses(&steps), vec!["failed", "not_started"]);
    assert_eq!(steps[0].error.as_deref(), Some("ValueError: no invoice"));
    assert_eq!(
        step_events(&mut fx.events),
        vec!["started extract", "failed extract: ValueError: no invoice"]
    );
}

#[tokio::test]
async fn an_agent_step_runs_the_agent_under_the_run_token_and_budget() {
    // GIVEN a workflow whose first step runs an agent
    let host = ScriptedHost::default().with(
        "invoices/close-month",
        &["ask"],
        None,
        steps_then(&["ask"], |services, _| {
            Box::pin(async move {
                let answer = services
                    .run_agent("accountant".into(), "total of March".into())
                    .await?;
                Ok(json!(answer))
            })
        }),
    );
    let fx = fixture(host, StepBudgetConfig::default()).await;

    // WHEN it runs
    let run = fx
        .engine
        .run_to_end("invoices/close-month", json!({}))
        .await
        .unwrap();

    // THEN the agent answered, its answer is the step's output, and the agent
    //      ran on a token the run owns, drawing on the run's budget
    assert_eq!(run.output, Some(json!("answer to total of March")));
    let steps = fx.repository.workflow_steps(&run.run_id).await.unwrap();
    assert_eq!(steps[0].output.as_deref(), Some("answer to total of March"));
    let calls = fx.agents.calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].agent, "accountant");
    assert_eq!(
        calls[0].budget.tool_calls_left(),
        calls[0].budget.max_tool_calls - 1
    );
    assert_eq!(calls[0].budget.steps_left(), calls[0].budget.max_steps - 1);
}

/// `outer` calls `inner` by its bare name, and `inner` runs an agent.
fn nested_host() -> ScriptedHost {
    ScriptedHost::default()
        .with(
            "invoices/outer",
            &["delegate"],
            None,
            steps_then(&["delegate"], |services, _| {
                Box::pin(async move {
                    services
                        .run_agent("accountant".into(), "from outer".into())
                        .await?;
                    services.run_workflow("inner".into(), json!({})).await
                })
            }),
        )
        .with(
            "invoices/inner",
            &["ask"],
            None,
            steps_then(&["ask"], |services, _| {
                Box::pin(async move {
                    let answer = services
                        .run_agent("accountant".into(), "from inner".into())
                        .await?;
                    Ok(json!({"inner": answer}))
                })
            }),
        )
}

#[tokio::test]
async fn a_nested_workflow_shares_the_budget_and_records_its_parent() {
    // GIVEN a workflow that calls another of its package by its bare name
    let fx = fixture(nested_host(), StepBudgetConfig::default()).await;

    // WHEN the outer one runs
    let outer = fx
        .engine
        .run_to_end("invoices/outer", json!({}))
        .await
        .unwrap();

    // THEN the inner one ran at depth 2 under the outer one, returned to it,
    //      and both agent calls drew on one budget, the outer run's
    assert_eq!(outer.status, "completed");
    assert_eq!(outer.output, Some(json!({"inner": "answer to from inner"})));
    let inner = fx.engine.history("invoices/inner", 10).await.unwrap();
    assert_eq!(inner.len(), 1);
    assert_eq!(inner[0].depth, 2);
    assert_eq!(
        inner[0].parent_run_id.as_deref(),
        Some(outer.run_id.as_str())
    );
    let calls = fx.agents.calls.lock().unwrap();
    assert_eq!(calls.len(), 2);
    assert!(Arc::ptr_eq(&calls[0].budget, &calls[1].budget));
    // Two declared steps were opened and two tool calls made, all on one budget.
    assert_eq!(calls[1].budget.steps_left(), calls[1].budget.max_steps - 2);
    assert_eq!(
        calls[1].budget.tool_calls_left(),
        calls[1].budget.max_tool_calls - 2
    );
}

/// `level` calls itself until the engine refuses.
fn recursive_host() -> ScriptedHost {
    ScriptedHost::default().with(
        "loops/level",
        &["deeper"],
        None,
        steps_then(&["deeper"], |services, _| {
            Box::pin(async move { services.run_workflow("loops/level".into(), json!({})).await })
        }),
    )
}

#[tokio::test]
async fn nesting_is_bounded_at_three_levels() {
    // GIVEN a workflow that calls itself
    let fx = fixture(recursive_host(), StepBudgetConfig::default()).await;

    // WHEN it runs
    let top = fx
        .engine
        .run_to_end("loops/level", json!({}))
        .await
        .unwrap();

    // THEN three levels ran, the fourth call was refused, and the refusal
    //      failed every level back up
    let runs = fx.engine.history("loops/level", 10).await.unwrap();
    let mut depths: Vec<u32> = runs.iter().map(|r| r.depth).collect();
    depths.sort_unstable();
    assert_eq!(depths, vec![1, 2, 3]);
    assert!(runs.iter().all(|r| r.status == "failed"));
    assert!(top.error.unwrap().contains("level 4"));
}

#[tokio::test]
async fn an_agent_call_at_the_deepest_level_is_refused() {
    // GIVEN two workflows deep, the deepest asking an agent at level 4
    let host = ScriptedHost::default()
        .with(
            "deep/one",
            &["s"],
            None,
            steps_then(&["s"], |services, _| {
                Box::pin(async move { services.run_workflow("two".into(), json!({})).await })
            }),
        )
        .with(
            "deep/two",
            &["s"],
            None,
            steps_then(&["s"], |services, _| {
                Box::pin(async move { services.run_workflow("three".into(), json!({})).await })
            }),
        )
        .with(
            "deep/three",
            &["s"],
            None,
            steps_then(&["s"], |services, _| {
                Box::pin(async move {
                    services
                        .run_agent("accountant".into(), "too deep".into())
                        .await
                        .map(Value::String)
                })
            }),
        );
    let fx = fixture(host, StepBudgetConfig::default()).await;

    // WHEN the first runs
    let top = fx.engine.run_to_end("deep/one", json!({})).await.unwrap();

    // THEN the agent never ran and the refusal names the level
    assert_eq!(top.status, "failed");
    assert!(top.error.unwrap().contains("level 4"));
    assert!(fx.agents.calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_spent_budget_refuses_the_next_step() {
    // GIVEN a budget of one step and a workflow of two
    let host = ScriptedHost::default().with(
        "invoices/close-month",
        &["extract", "file"],
        None,
        steps_then(&["extract", "file"], noop()),
    );
    let budget = StepBudgetConfig {
        max_steps: 1,
        ..StepBudgetConfig::default()
    };
    let fx = fixture(host, budget).await;

    // WHEN it runs
    let run = fx
        .engine
        .run_to_end("invoices/close-month", json!({}))
        .await
        .unwrap();

    // THEN the second step was refused for the budget and never started
    assert_eq!(run.status, "failed");
    assert!(run.error.unwrap().contains("budget"));
    let steps = fx.repository.workflow_steps(&run.run_id).await.unwrap();
    assert_eq!(statuses(&steps), vec!["completed", "not_started"]);
}

/// The first step waits for an agent that only ends when stopped.
fn waiting_host() -> ScriptedHost {
    ScriptedHost::default()
        .with(
            "invoices/close-month",
            &["wait", "file", "notify"],
            None,
            steps_then(&["wait", "file", "notify"], |services, _| {
                Box::pin(async move {
                    services
                        .run_agent("waiter".into(), "hold".into())
                        .await
                        .map(Value::String)
                })
            }),
        )
        .with(
            "invoices/outer",
            &["delegate", "after"],
            None,
            steps_then(&["delegate", "after"], |services, _| {
                Box::pin(
                    async move { services.run_workflow("close-month".into(), json!({})).await },
                )
            }),
        )
}

async fn agent_waiting(fx: &Fixture) {
    loop {
        if !fx.agents.calls.lock().unwrap().is_empty() {
            return;
        }
        tokio::task::yield_now().await;
    }
}

#[tokio::test]
async fn stopping_a_run_stops_its_step_and_marks_the_rest_not_started() {
    // GIVEN a run whose first step waits on an agent
    let fx = fixture(waiting_host(), StepBudgetConfig::default()).await;
    let run = fx
        .engine
        .launch("invoices/close-month", json!({}))
        .await
        .unwrap();
    agent_waiting(&fx).await;

    // WHEN the run is stopped
    let stopped = fx.engine.cancel(&run.run_id).await.unwrap();

    // THEN the agent's token was cancelled with it, the run ended canceled,
    //      its step is canceled and the next two were not started
    assert!(fx.agents.calls.lock().unwrap()[0].parent.is_cancelled());
    assert_eq!(stopped.status, "canceled");
    let steps = fx.repository.workflow_steps(&run.run_id).await.unwrap();
    assert_eq!(
        statuses(&steps),
        vec!["canceled", "not_started", "not_started"]
    );
    let again = fx.engine.cancel(&run.run_id).await.unwrap_err();
    assert!(matches!(again, WorkflowError::AlreadyEnded(_)));
}

#[tokio::test]
async fn stopping_the_outer_run_stops_the_nested_one() {
    // GIVEN an outer run whose nested run waits on an agent
    let fx = fixture(waiting_host(), StepBudgetConfig::default()).await;
    let outer = fx.engine.launch("invoices/outer", json!({})).await.unwrap();
    agent_waiting(&fx).await;

    // WHEN the outer run is stopped
    let stopped = fx.engine.cancel(&outer.run_id).await.unwrap();

    // THEN the nested run inherited the stop: both are canceled
    assert_eq!(stopped.status, "canceled");
    let inner = fx.engine.history("invoices/close-month", 10).await.unwrap();
    assert_eq!(inner.len(), 1);
    assert_eq!(inner[0].status, "canceled");
    let outer_steps = fx.repository.workflow_steps(&outer.run_id).await.unwrap();
    assert_eq!(statuses(&outer_steps), vec!["canceled", "not_started"]);
}

#[tokio::test]
async fn a_run_left_running_by_a_stopped_process_is_failed_at_start() {
    // GIVEN a store holding a run still marked running
    let dir = tempfile::tempdir().unwrap();
    let repository = Arc::new(
        TaskRepository::open(&dir.path().join("hitl.db"))
            .await
            .unwrap(),
    );
    repository
        .create_workflow_run(apollia_tools::NewWorkflowRun {
            run_id: "r-old".into(),
            workflow_id: "invoices/close-month".into(),
            parent_run_id: None,
            depth: 1,
            inputs: json!({}),
            steps: vec!["extract".into()],
        })
        .await
        .unwrap();

    // WHEN the engine starts and is then asked to stop that run
    let (bus, _rx) = broadcast::channel(16);
    let engine = WorkflowHandle::spawn(repository.clone(), bus, StepBudgetConfig::default(), None);
    let answer = engine.cancel("r-old").await.unwrap_err();

    // THEN the run was failed before anything else, so it is no longer running
    assert!(matches!(answer, WorkflowError::AlreadyEnded(_)));
    let run = repository.workflow_run("r-old").await.unwrap().unwrap();
    assert_eq!(run.status, "failed");
}

#[test]
fn a_bare_name_is_read_in_the_callers_package() {
    // GIVEN a caller in the package `invoices`
    let caller = Some("invoices/close-month");
    // WHEN names are qualified
    // THEN a bare name takes the package, a full one is kept, a malformed one is refused
    assert_eq!(qualify("file", caller).unwrap(), "invoices/file");
    assert_eq!(qualify("tax/report", caller).unwrap(), "tax/report");
    assert!(qualify("file", None).is_err());
    assert!(qualify("a/b/c", caller).is_err());
    assert!(qualify("/b", caller).is_err());
}

#[test]
fn text_values_are_read_as_their_declared_type() {
    // GIVEN a schema with an integer and a boolean, and inputs given as text
    let inputs = json!({"folder": "2024", "limit": "12", "notify": "true"});
    // WHEN they are validated
    let checked = inputs::validate(Some(&schema()), &inputs).unwrap();
    // THEN the string stays a string and the others take their type
    assert_eq!(
        checked,
        json!({"folder": "2024", "limit": 12, "notify": true})
    );
}

#[test]
fn an_undeclared_input_is_refused() {
    // GIVEN a closed schema and an input it does not declare
    let inputs = json!({"folder": "/a", "colour": "red"});
    // WHEN validated
    let err = inputs::validate(Some(&schema()), &inputs).unwrap_err();
    // THEN the field is named
    assert_eq!(err.field, "colour");
}
