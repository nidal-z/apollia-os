//! The tools of the granted agents and workflows, and the invoker that runs
//! their calls, over a scripted composer.

use std::sync::Mutex;

use apollia_core::StepBudgetConfig;

use super::*;
use crate::workflows::WorkflowDescriptor;

mod runtime;

/// What the scripted composer saw of one call.
struct Seen {
    target: String,
    payload: Value,
    caller_agent: String,
    caller_depth: u32,
    budget: Arc<StepBudget>,
    cancel: CancellationToken,
}

/// Knows the agents `accountant` (a mission) and `clerk` (none), and the
/// workflow `books/close`; answers every call and records it.
#[derive(Default)]
struct Scripted {
    seen: Mutex<Vec<Seen>>,
}

impl Scripted {
    fn record(&self, target: String, payload: Value, caller: &CallerRun) {
        self.seen.lock().unwrap().push(Seen {
            target,
            payload,
            caller_agent: caller.agent_id.clone(),
            caller_depth: caller.depth,
            budget: Arc::clone(&caller.budget),
            cancel: caller.cancel.clone(),
        });
    }
}

impl Composer for Scripted {
    fn agent_mission(&self, agent: String) -> BoxFuture<'_, Option<String>> {
        Box::pin(async move {
            match agent.as_str() {
                "accountant" => Some("Answer about the books.".to_string()),
                "clerk" | "writer" => Some(String::new()),
                _ => None,
            }
        })
    }

    fn workflow(&self, workflow: String) -> BoxFuture<'_, Option<WorkflowDescriptor>> {
        Box::pin(async move {
            (workflow == "books/close").then(|| WorkflowDescriptor {
                id: workflow,
                version: "1.0.0".to_string(),
                description: "Close the books of a month.".to_string(),
                steps: vec!["sum".to_string()],
                input_schema: Some(json!({
                    "type": "object",
                    "properties": {"month": {"type": "string"}},
                    "required": ["month"]
                })),
            })
        })
    }

    fn run_agent(
        &self,
        agent: String,
        message: String,
        caller: CallerRun,
    ) -> BoxFuture<'_, Result<String, String>> {
        self.record(agent.clone(), json!(message), &caller);
        Box::pin(async move { Ok(format!("{agent} answers {message}")) })
    }

    fn run_workflow(
        &self,
        workflow: String,
        inputs: Value,
        caller: CallerRun,
    ) -> BoxFuture<'_, Result<Value, String>> {
        self.record(workflow, inputs, &caller);
        Box::pin(async { Ok(json!({"closed": true})) })
    }
}

/// Answers `native` for any tool it is handed.
struct Native;

#[async_trait::async_trait]
impl ToolInvoker for Native {
    async fn invoke(&self, tool_name: &str, _: &Value) -> Result<String, String> {
        Ok(format!("native {tool_name}"))
    }
}

fn caller(depth: u32) -> CallerRun {
    CallerRun {
        agent_id: "writer".to_string(),
        depth,
        cancel: CancellationToken::new(),
        budget: Arc::new(StepBudget::new(&StepBudgetConfig::default())),
    }
}

fn invoker(composer: &Arc<Scripted>, caller: CallerRun) -> ComposingInvoker {
    ComposingInvoker::new(
        Arc::new(Native),
        Arc::clone(composer) as Arc<dyn Composer>,
        caller,
    )
}

fn granted(names: &[&str]) -> Vec<String> {
    names.iter().map(|n| (*n).to_string()).collect()
}

#[tokio::test]
async fn a_granted_agent_and_workflow_become_tools_described_by_mission_and_description() {
    // GIVEN an agent granted the accountant and the closing of the books
    let composer = Scripted::default();
    let access = granted(&["agent:accountant", "workflow:books/close"]);

    // WHEN the tools of its calls are built
    let specs = tool_specs(&composer, &access, "writer").await;

    // THEN one tool each, described by the agent's mission and the
    //      workflow's description, the workflow taking its own inputs
    assert_eq!(specs.len(), 2);
    assert_eq!(specs[0].name, "agent:accountant");
    assert_eq!(specs[0].description, "Answer about the books.");
    assert_eq!(specs[0].parameters["required"], json!(["message"]));
    assert_eq!(specs[1].name, "workflow:books/close");
    assert_eq!(specs[1].description, "Close the books of a month.");
    assert_eq!(specs[1].parameters["required"], json!(["month"]));
}

#[tokio::test]
async fn an_agent_is_never_offered_itself_nor_what_cannot_be_called() {
    // GIVEN an agent granted itself, an unknown agent, an unknown workflow,
    //       an agent without a mission, and a native tool
    let composer = Scripted::default();
    let access = granted(&[
        "agent:writer",
        "agent:ghost",
        "workflow:books/ghost",
        "agent:clerk",
        "file_read",
    ]);

    // WHEN the tools of its calls are built
    let specs = tool_specs(&composer, &access, "writer").await;

    // THEN only the agent without a mission is offered, under a plain
    //      description; neither itself, nor the unknown ones, nor the native
    let names: Vec<&str> = specs.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, vec!["agent:clerk"]);
    assert!(specs[0].description.contains("clerk"));
}

#[tokio::test]
async fn calling_a_granted_agent_runs_it_on_the_message_nested_in_the_caller() {
    // GIVEN the invoker of a run at the first level
    let composer = Arc::new(Scripted::default());
    let run = caller(1);
    let invoker = invoker(&composer, run.clone());

    // WHEN the model calls the accountant
    let answer = invoker
        .invoke(
            "agent:accountant",
            &json!({"message": "What is the total?"}),
        )
        .await;

    // THEN the accountant ran on the message, on behalf of the caller, with
    //      its budget and its token
    assert_eq!(answer.unwrap(), "accountant answers What is the total?");
    let seen = composer.seen.lock().unwrap();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].target, "accountant");
    assert_eq!(seen[0].caller_agent, "writer");
    assert_eq!(seen[0].caller_depth, 1);
    assert!(Arc::ptr_eq(&seen[0].budget, &run.budget));
    run.cancel.cancel();
    assert!(seen[0].cancel.is_cancelled());
}

#[tokio::test]
async fn calling_a_granted_workflow_hands_it_the_arguments_as_inputs() {
    // GIVEN the invoker of a run at the second level
    let composer = Arc::new(Scripted::default());
    let invoker = invoker(&composer, caller(2));

    // WHEN the model calls the closing of the books
    let output = invoker
        .invoke("workflow:books/close", &json!({"month": "2026-09"}))
        .await;

    // THEN the workflow ran on those inputs and its output is the result
    assert_eq!(output.unwrap(), r#"{"closed":true}"#);
    let seen = composer.seen.lock().unwrap();
    assert_eq!(seen[0].target, "books/close");
    assert_eq!(seen[0].payload, json!({"month": "2026-09"}));
    assert_eq!(seen[0].caller_depth, 2);
}

#[tokio::test]
async fn a_call_from_the_third_level_is_refused_with_the_level_it_would_reach() {
    // GIVEN the invoker of a run already at the third level
    let composer = Arc::new(Scripted::default());
    let invoker = invoker(&composer, caller(3));

    // WHEN the model calls an agent, then a workflow
    let agent = invoker
        .invoke("agent:accountant", &json!({"message": "Total?"}))
        .await;
    let workflow = invoker
        .invoke("workflow:books/close", &json!({"month": "2026-09"}))
        .await;

    // THEN both are refused, naming the bound and the fourth level, and
    //      nothing ran
    for refusal in [agent.unwrap_err(), workflow.unwrap_err()] {
        assert!(refusal.contains("3 levels deep"), "{refusal}");
        assert!(refusal.contains("level 4"), "{refusal}");
    }
    assert!(composer.seen.lock().unwrap().is_empty());
}

#[tokio::test]
async fn an_agent_calling_itself_is_refused() {
    // GIVEN the invoker of a run of the writer
    let composer = Arc::new(Scripted::default());
    let invoker = invoker(&composer, caller(1));

    // WHEN the model calls the writer
    let refusal = invoker
        .invoke("agent:writer", &json!({"message": "Again"}))
        .await;

    // THEN the call is refused and nothing ran
    assert!(refusal.unwrap_err().contains("cannot call itself"));
    assert!(composer.seen.lock().unwrap().is_empty());
}

#[tokio::test]
async fn any_other_tool_reaches_the_inner_invoker() {
    // GIVEN the invoker of a run
    let composer = Arc::new(Scripted::default());
    let invoker = invoker(&composer, caller(1));

    // WHEN the model calls a native tool
    let output = invoker.invoke("file_read", &json!({"path": "a"})).await;

    // THEN the inner invoker ran it, the composer saw nothing
    assert_eq!(output.unwrap(), "native file_read");
    assert!(composer.seen.lock().unwrap().is_empty());
}
