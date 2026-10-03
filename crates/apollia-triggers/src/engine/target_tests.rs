//! A fire starts the target its trigger names: an agent on its message, a
//! workflow on its inputs; the history records the outcome of each.

use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use apollia_core::{AIPInput, AIPPart, ObservabilityConfig, TaskId};
use tokio::sync::broadcast;

use crate::engine::{TaskSubmitter, TriggerEngineHandle};
use crate::persistence::TriggerPersistence;
use crate::types::{
    InputTemplate, OnBusyPolicy, TriggerDefinition, TriggerSourceConfig, TriggerTarget,
};

/// What the submitter was asked to start.
#[derive(Debug, Clone, PartialEq)]
enum Started {
    Agent(String, String),
    Workflow(String, serde_json::Value),
}

/// Records every start; refuses a workflow when `refuse_workflows` is set.
#[derive(Clone, Default)]
struct Recorder {
    started: Arc<Mutex<Vec<Started>>>,
    refuse_workflows: bool,
}

impl TaskSubmitter for Recorder {
    fn submit<'a>(
        &'a self,
        agent: &'a str,
        input: AIPInput,
    ) -> Pin<Box<dyn Future<Output = Result<TaskId, String>> + Send + 'a>> {
        let text = input
            .parts
            .iter()
            .filter_map(|p| match p {
                AIPPart::Text(t) => Some(t.text.clone()),
                _ => None,
            })
            .collect::<String>();
        self.started
            .lock()
            .unwrap()
            .push(Started::Agent(agent.to_string(), text));
        Box::pin(async { Ok(TaskId::from("task-1".to_string())) })
    }

    fn pending_count<'a>(&'a self, _: &'a str) -> Pin<Box<dyn Future<Output = usize> + Send + 'a>> {
        Box::pin(async { 0 })
    }

    fn submit_workflow<'a>(
        &'a self,
        workflow: &'a str,
        inputs: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<TaskId, String>> + Send + 'a>> {
        self.started
            .lock()
            .unwrap()
            .push(Started::Workflow(workflow.to_string(), inputs));
        let refuse = self.refuse_workflows;
        Box::pin(async move {
            if refuse {
                Err("input 'file': required".to_string())
            } else {
                Ok(TaskId::from("run-1".to_string()))
            }
        })
    }
}

fn definition(id: &str, target: TriggerTarget) -> TriggerDefinition {
    TriggerDefinition {
        id: id.to_string(),
        target,
        enabled: true,
        on_busy: OnBusyPolicy::Queue { max_depth: 10 },
        source: TriggerSourceConfig::Interval { every: "1h".into() },
    }
}

fn workflow_target() -> TriggerTarget {
    let mut inputs = BTreeMap::new();
    inputs.insert("when".to_string(), InputTemplate("{{fired_at}}".into()));
    TriggerTarget::Workflow {
        workflow: "reports/weekly".into(),
        inputs,
    }
}

async fn engine(
    defs: Vec<TriggerDefinition>,
    recorder: Recorder,
) -> (TriggerEngineHandle, tempfile::TempDir) {
    let dir = tempfile::TempDir::new().unwrap();
    let persistence = TriggerPersistence::open(&dir.path().join("triggers.db")).unwrap();
    let handle = TriggerEngineHandle::spawn(
        defs,
        recorder,
        broadcast::channel(64).0,
        Some(persistence),
        ObservabilityConfig::default(),
    )
    .await;
    (handle, dir)
}

#[tokio::test]
async fn test_a_workflow_trigger_launches_the_workflow_and_records_its_run() {
    // GIVEN a trigger on a workflow with one templated input
    let recorder = Recorder::default();
    let (handle, _dir) = engine(
        vec![definition("weekly", workflow_target())],
        recorder.clone(),
    )
    .await;

    // WHEN it fires
    let run = handle.fire_now("weekly").await.unwrap();

    // THEN the workflow is launched on the rendered input, never an agent
    assert_eq!(run.as_ref(), "run-1");
    let started = recorder.started.lock().unwrap().clone();
    assert_eq!(started.len(), 1);
    assert!(
        matches!(
            &started[0],
            Started::Workflow(workflow, inputs)
                if workflow == "reports/weekly"
                    && inputs["when"].as_str().is_some_and(|w| w.contains('T'))
        ),
        "{started:?}"
    );
    // AND the history holds the fire with the run, under the workflow
    let history = handle.query_history("weekly", 10).await;
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].status, "fired");
    assert_eq!(history[0].task_id.as_deref(), Some("run-1"));
    assert_eq!(history[0].agent_name, "reports/weekly");
    // AND the status names the kind of target
    let status = handle.list().await;
    assert_eq!(status[0].target_kind, "workflow");
    assert_eq!(status[0].agent, "reports/weekly");
}

#[tokio::test]
async fn test_a_refused_workflow_launch_is_recorded_as_an_error_with_its_reason() {
    // GIVEN a trigger on a workflow whose launch is refused
    let recorder = Recorder {
        refuse_workflows: true,
        ..Recorder::default()
    };
    let (handle, _dir) = engine(vec![definition("weekly", workflow_target())], recorder).await;

    // WHEN it fires
    let result = handle.fire_now("weekly").await;

    // THEN the fire fails, and the history says why
    assert!(result.is_err());
    let history = handle.query_history("weekly", 10).await;
    assert_eq!(history[0].status, "error");
    assert!(history[0]
        .reason
        .as_deref()
        .is_some_and(|r| r.contains("required")));
}

#[tokio::test]
async fn test_an_agent_trigger_sends_its_message_to_the_agent() {
    // GIVEN a trigger on an agent
    let recorder = Recorder::default();
    let (handle, _dir) = engine(
        vec![definition(
            "veille",
            TriggerTarget::agent("veille", "Daily watch"),
        )],
        recorder.clone(),
    )
    .await;

    // WHEN it fires
    handle.fire_now("veille").await.unwrap();

    // THEN the agent receives its message, and the history records the task
    assert_eq!(
        recorder.started.lock().unwrap().clone(),
        vec![Started::Agent("veille".into(), "Daily watch".into())]
    );
    let history = handle.query_history("veille", 10).await;
    assert_eq!(history[0].task_id.as_deref(), Some("task-1"));
    assert_eq!(handle.list().await[0].target_kind, "agent");
}
