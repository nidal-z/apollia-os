//! The list of decisions waiting for the user, one source at a time, and a
//! decision taken from it reaching the run that waits.

use std::collections::{BTreeMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use apollia_core::batch_source::{BatchSource, MemorySource, SourceRegistry};
use apollia_core::PendingApprovals;
use apollia_tools::tools::ask_user::{PendingUserInput, QuestionType, UserQuestion};
use apollia_tools::TaskRepository;
use axum::http::StatusCode;
use serde_json::{json, Value};
use tokio::sync::oneshot;

use crate::api::server::AppState;
use crate::api::test_support::{app_state, call, seeded_chat, MockBackend};
use crate::batch::{
    BatchAccess, BatchHandle, BatchLaunch, ItemEnd, ItemRunner, ItemStatus, SourceDescriptor,
    StartedItem,
};
use crate::chat::{ChatApprovalAsked, ToolDecision};
use crate::governance::{ConsentAnswer, ALL_ANSWERS, NO_ALWAYS_ANSWERS};
use crate::test_support::poll_until_async;

const WAIT: Duration = Duration::from_secs(10);
const LIST: &str = "/api/v1/pending-decisions";
const DECIDE: &str = "/api/v1/pending-decisions/decide";

fn empty_state(dir: &tempfile::TempDir) -> AppState<MockBackend> {
    app_state(dir.path().to_path_buf())
}

async fn items(state: &AppState<MockBackend>) -> Vec<Value> {
    let (status, body) = call(state.clone(), "GET", LIST, None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["items"].as_array().cloned().unwrap_or_default()
}

fn asked(tool_call_id: &str, tool: &str, answers: &[ConsentAnswer]) -> ChatApprovalAsked {
    ChatApprovalAsked {
        session_id: "s-1".into(),
        message_id: "a-1".into(),
        tool_call_id: tool_call_id.into(),
        tool_name: tool.into(),
        agent_id: Some("apollia-chat".into()),
        family: Some("write_or_delete".into()),
        prompt: format!("Tool '{tool}' asks to run"),
        answers: answers.to_vec(),
        asked_at: "2026-10-03T09:00:00Z".into(),
    }
}

// ─── no source ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_a_runtime_with_nothing_waiting_lists_nothing() {
    // GIVEN a runtime with no chat, no task, no batch waiting
    let dir = tempfile::tempdir().unwrap();
    let state = empty_state(&dir);

    // WHEN the decisions waiting are listed
    let listed = items(&state).await;

    // THEN the list is empty, and an unknown item cannot be decided
    assert!(listed.is_empty());
    let (status, body) = call(
        state,
        "POST",
        DECIDE,
        Some(json!({"id": "consent:nope", "answer": "once"})),
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert_eq!(body["code"], "UNAVAILABLE");
}

// ─── chat consents ──────────────────────────────────────────────────────────

#[tokio::test]
async fn test_a_chat_consent_is_listed_with_its_three_answers_and_once_resumes_it() {
    // GIVEN a conversation whose tool call waits for a consent
    let dir = tempfile::tempdir().unwrap();
    let state = seeded_chat(dir.path());
    let chat = state.chat_manager.clone().unwrap();
    let mut waiting = chat.pending_chat_approvals().register_asked(
        "s-1::a-1::c-1".into(),
        asked("c-1", "file_write", &ALL_ANSWERS),
    );

    // WHEN the decisions are listed, then the consent is answered once
    let listed = items(&state).await;
    let (status, body) = call(
        state.clone(),
        "POST",
        DECIDE,
        Some(json!({"id": "consent:s-1::a-1::c-1", "answer": "once"})),
    )
    .await;

    // THEN the consent was listed with its three answers
    assert_eq!(listed.len(), 1, "{listed:?}");
    assert_eq!(listed[0]["kind"], "consent");
    assert_eq!(listed[0]["access"], "file_write");
    assert_eq!(listed[0]["answers"], json!(["once", "always", "refuse"]));
    assert_eq!(listed[0]["origin"]["sessionId"], "s-1");
    // AND the conversation waiting on it goes on with the answer
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["resumed"], 1);
    let decision = waiting.try_recv().unwrap();
    assert_eq!(decision, ToolDecision::Accept);
    // AND it no longer waits
    assert!(items(&state).await.is_empty());
}

#[tokio::test]
async fn test_executing_code_offers_two_answers_and_refuses_always() {
    // GIVEN a conversation whose code execution waits for a consent
    let dir = tempfile::tempdir().unwrap();
    let state = seeded_chat(dir.path());
    let chat = state.chat_manager.clone().unwrap();
    let mut waiting = chat.pending_chat_approvals().register_asked(
        "s-1::a-1::c-2".into(),
        asked("c-2", "bash_executor", &NO_ALWAYS_ANSWERS),
    );

    // WHEN it is listed, answered "always", then refused with a reason
    let listed = items(&state).await;
    let (always, refused_body) = call(
        state.clone(),
        "POST",
        DECIDE,
        Some(json!({"id": "consent:s-1::a-1::c-2", "answer": "always"})),
    )
    .await;
    let (refuse, _) = call(
        state.clone(),
        "POST",
        DECIDE,
        Some(json!({"id": "consent:s-1::a-1::c-2", "answer": "refuse", "reason": "not here"})),
    )
    .await;

    // THEN it took once and refuse only, "always" left it waiting
    assert_eq!(listed[0]["answers"], json!(["once", "refuse"]));
    assert_eq!(always, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(refused_body["code"], "ANSWER_NOT_OFFERED");
    // AND the refusal reached the run with its reason
    assert_eq!(refuse, StatusCode::OK);
    let decision = waiting.try_recv().unwrap();
    assert_eq!(
        decision,
        ToolDecision::Refuse {
            reason: Some("not here".into())
        }
    );
}

// ─── task pauses ────────────────────────────────────────────────────────────

async fn paused_task(dir: &tempfile::TempDir, payload: Option<Value>) -> AppState<MockBackend> {
    let repo = TaskRepository::open(&dir.path().join("hitl.db"))
        .await
        .unwrap();
    repo.save_pause(apollia_tools::PauseRecord {
        task_id: "t-1",
        step_id: None,
        prompt: "Send the report?",
        context: &json!({}),
        payload: payload.as_ref(),
        agent_name: Some("reporter"),
        skill_id: Some("send"),
    })
    .await
    .unwrap();
    let mut state = empty_state(dir);
    state.task_repository = Some(Arc::new(repo));
    state.pending_approvals = Some(Arc::new(PendingApprovals::new()));
    state
}

#[tokio::test]
async fn test_a_task_pause_is_listed_and_once_resumes_the_task() {
    // GIVEN a task paused for an approval, its run waiting on the pause
    let dir = tempfile::tempdir().unwrap();
    let state = paused_task(&dir, None).await;
    let mut waiting = state.pending_approvals.as_ref().unwrap().register("t-1");

    // WHEN the decisions are listed, then the pause is answered once
    let listed = items(&state).await;
    let (status, body) = call(
        state.clone(),
        "POST",
        DECIDE,
        Some(json!({"id": "task:t-1", "answer": "once"})),
    )
    .await;

    // THEN the pause was listed with its two answers
    assert_eq!(listed.len(), 1, "{listed:?}");
    assert_eq!(listed[0]["kind"], "task_pause");
    assert_eq!(listed[0]["agentId"], "reporter");
    assert_eq!(listed[0]["answers"], json!(["once", "refuse"]));
    // AND the task goes on, approved, through the resume of the task
    assert_eq!(status, StatusCode::OK, "{body}");
    let response = waiting.try_recv().unwrap();
    assert!(response.approved);
    let repo = state.task_repository.as_ref().unwrap();
    assert_eq!(
        repo.get_task_status("t-1").await.unwrap().as_deref(),
        Some("working")
    );
}

#[tokio::test]
async fn test_a_task_question_takes_its_answer_checked_as_the_resume_checks_it() {
    // GIVEN a task paused on a choice between two propositions
    let dir = tempfile::tempdir().unwrap();
    let payload = json!({
        "genre": "choix",
        "question": "Which list?",
        "propositions": [{"id": "a", "libelle": "A"}, {"id": "b", "libelle": "B"}]
    });
    let state = paused_task(&dir, Some(payload)).await;
    let mut waiting = state.pending_approvals.as_ref().unwrap().register("t-1");

    // WHEN it is answered with an id it does not propose, then with one it does
    let listed = items(&state).await;
    let (bad, bad_body) = call(
        state.clone(),
        "POST",
        DECIDE,
        Some(json!({"id": "task:t-1", "answer": "answer", "value": "z"})),
    )
    .await;
    let (good, _) = call(
        state.clone(),
        "POST",
        DECIDE,
        Some(json!({"id": "task:t-1", "answer": "answer", "value": "b"})),
    )
    .await;

    // THEN it was listed as a question with its payload
    assert_eq!(listed[0]["answers"], json!(["answer", "refuse"]));
    assert_eq!(listed[0]["questions"]["genre"], "choix");
    // AND the wrong answer was refused by the resume's own check
    assert_eq!(bad, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(bad_body["code"], "INVALID_ANSWER");
    // AND the right one reached the task
    assert_eq!(good, StatusCode::OK);
    let response = waiting.try_recv().unwrap();
    assert_eq!(response.answer, Some(json!("b")));
}

// ─── batch items ────────────────────────────────────────────────────────────

const AGENT: &str = "clerk";

/// Each item waits for `send_email` the first time it runs, and completes
/// every later time; an item on `bash_executor` waits unless allowed one call
/// of it.
#[derive(Default)]
struct WaitsOnce {
    seen: Mutex<HashSet<String>>,
    started: Mutex<u32>,
    granted: Mutex<Vec<(String, Vec<String>)>>,
    once: Mutex<Vec<Option<String>>>,
}

impl ItemRunner for Arc<WaitsOnce> {
    async fn check(&self, agent: &str) -> Result<Vec<BatchAccess>, String> {
        if agent != AGENT {
            return Err(format!("agent '{agent}' cannot run a batch"));
        }
        Ok(vec![
            BatchAccess {
                access_id: "send_email".into(),
                permits: "Send an email.".into(),
                family: "send_external".into(),
                grantable: true,
            },
            BatchAccess {
                access_id: "bash_executor".into(),
                permits: "Run a command.".into(),
                family: "execute_code".into(),
                grantable: false,
            },
        ])
    }

    async fn grant(
        &self,
        batch_id: &str,
        _agent: &str,
        grants: &[BatchAccess],
    ) -> Result<(), String> {
        self.granted.lock().unwrap().push((
            batch_id.to_string(),
            grants.iter().map(|g| g.access_id.clone()).collect(),
        ));
        Ok(())
    }

    async fn allow_original_write(&self, _agent: &str, _location: &str) -> Result<(), String> {
        Ok(())
    }

    async fn record_original_write(
        &self,
        _batch_id: &str,
        _agent: &str,
        _location: &str,
    ) -> Result<(), String> {
        Ok(())
    }

    async fn release(&self, _batch_id: &str) -> Result<(), String> {
        Ok(())
    }

    async fn start(
        &self,
        batch_id: &str,
        _agent: &str,
        input: &Value,
        once: Option<&str>,
    ) -> Result<StartedItem, String> {
        let n = {
            let mut started = self.started.lock().unwrap();
            *started += 1;
            *started
        };
        let key = format!("{batch_id}/{input}");
        let first = self.seen.lock().unwrap().insert(key);
        let access = input["access"].as_str().unwrap_or("send_email").to_string();
        self.once.lock().unwrap().push(once.map(str::to_owned));
        let waits = if access == "bash_executor" {
            once != Some("bash_executor")
        } else {
            first
        };
        let (tx, end) = oneshot::channel();
        let _ = tx.send(if waits {
            ItemEnd {
                status: ItemStatus::Awaiting,
                output: "stopped".into(),
                awaiting: Some(access),
            }
        } else {
            ItemEnd {
                status: ItemStatus::Completed,
                output: "sent".into(),
                awaiting: None,
            }
        });
        Ok(StartedItem {
            task_id: format!("task-{n}"),
            end,
        })
    }

    async fn cancel(&self, _task_id: &str) {}
}

fn sources(accesses: &'static [&'static str]) -> Arc<SourceRegistry> {
    let mut registry = SourceRegistry::new();
    registry
        .register(
            "memory",
            Box::new(move |spec| {
                let mut source = MemorySource::new(&spec.location, &["access"]);
                for (i, access) in accesses.iter().enumerate() {
                    source = source.with_item(&format!("row-{i}"), &[("access", access)]);
                }
                Ok(Box::new(source) as Box<dyn BatchSource>)
            }),
        )
        .unwrap();
    Arc::new(registry)
}

fn launch() -> BatchLaunch {
    BatchLaunch {
        agent: AGENT.into(),
        source: SourceDescriptor {
            kind: "memory".into(),
            location: "/data/requests.csv".into(),
            options: BTreeMap::new(),
            write: None,
        },
        grants: vec![],
        concurrency: None,
        max_failures: None,
    }
}

async fn batch_state(
    dir: &tempfile::TempDir,
    accesses: &'static [&'static str],
) -> (AppState<MockBackend>, Arc<TaskRepository>, Arc<WaitsOnce>) {
    let repo = Arc::new(
        TaskRepository::open(&dir.path().join("hitl.db"))
            .await
            .unwrap(),
    );
    let runner = Arc::new(WaitsOnce::default());
    let (bus, _) = tokio::sync::broadcast::channel(256);
    let handle = BatchHandle::spawn(repo.clone(), sources(accesses), runner.clone(), bus);
    let mut state = empty_state(dir);
    state.batches = Some(handle);
    (state, repo, runner)
}

async fn awaiting(state: &AppState<MockBackend>, batch_id: &str, n: u64) {
    let batches = state.batches.clone().unwrap();
    let reached = poll_until_async(WAIT, || {
        let batches = batches.clone();
        async move {
            let detail = batches.detail(batch_id).await.unwrap().unwrap();
            detail.batch.awaiting == n
                && detail
                    .items
                    .iter()
                    .all(|i| i.status != "running" && i.status != "pending")
        }
    })
    .await;
    assert!(reached, "batch {batch_id} never had {n} waiting item(s)");
}

async fn item_status(repo: &TaskRepository, batch_id: &str, rank: u64) -> String {
    repo.batch_items(batch_id)
        .await
        .unwrap()
        .into_iter()
        .find(|i| i.rank == rank)
        .map(|i| i.status)
        .unwrap_or_default()
}

#[tokio::test]
async fn test_a_waiting_batch_item_is_listed_and_once_runs_it_again_with_the_grant() {
    // GIVEN a batch whose one item waits for `send_email`
    let dir = tempfile::tempdir().unwrap();
    let (state, repo, runner) = batch_state(&dir, &["send_email"]).await;
    let batch = state
        .batches
        .clone()
        .unwrap()
        .launch(launch())
        .await
        .unwrap();
    awaiting(&state, &batch.batch_id, 1).await;

    // WHEN the decisions are listed, then the item is answered once
    let listed = items(&state).await;
    let id = format!("batch:{}:0", batch.batch_id);
    let (status, body) = call(
        state.clone(),
        "POST",
        DECIDE,
        Some(json!({"id": id, "answer": "once"})),
    )
    .await;

    // THEN the item was listed with the access it waits for and three answers
    assert_eq!(listed.len(), 1, "{listed:?}");
    assert_eq!(listed[0]["kind"], "batch_item");
    assert_eq!(listed[0]["agentId"], AGENT);
    assert_eq!(listed[0]["access"], "send_email");
    assert_eq!(listed[0]["family"], "send_external");
    assert_eq!(listed[0]["answers"], json!(["once", "always", "refuse"]));
    // AND the batch granted the access and ran the item again, to its end
    assert_eq!(status, StatusCode::OK, "{body}");
    let reached = poll_until_async(WAIT, || async {
        item_status(&repo, &batch.batch_id, 0).await == "completed"
    })
    .await;
    assert!(reached, "the item never completed");
    assert!(runner
        .granted
        .lock()
        .unwrap()
        .iter()
        .any(|(b, g)| b == &batch.batch_id && g == &vec!["send_email".to_string()]));
    assert!(items(&state).await.is_empty());
}

#[tokio::test]
async fn test_an_item_waiting_on_code_takes_once_or_refuse_and_once_resumes_it() {
    // GIVEN a batch whose two items wait for executing code
    let dir = tempfile::tempdir().unwrap();
    let (state, repo, runner) = batch_state(&dir, &["bash_executor", "bash_executor"]).await;
    let batch = state
        .batches
        .clone()
        .unwrap()
        .launch(launch())
        .await
        .unwrap();
    awaiting(&state, &batch.batch_id, 2).await;
    let first = format!("batch:{}:0", batch.batch_id);
    let second = format!("batch:{}:1", batch.batch_id);

    // WHEN they are listed; the first is answered "always", then once; the
    // second is refused with a reason
    let listed = items(&state).await;
    let (always, always_body) = call(
        state.clone(),
        "POST",
        DECIDE,
        Some(json!({"id": first, "answer": "always"})),
    )
    .await;
    let (once, once_body) = call(
        state.clone(),
        "POST",
        DECIDE,
        Some(json!({"id": first, "answer": "once"})),
    )
    .await;
    let (refuse, refuse_body) = call(
        state.clone(),
        "POST",
        DECIDE,
        Some(json!({"id": second, "answer": "refuse", "reason": "no scripts"})),
    )
    .await;

    // THEN each took the two answers code takes, "always" left it waiting
    assert_eq!(listed.len(), 2, "{listed:?}");
    assert_eq!(listed[0]["access"], "bash_executor");
    assert_eq!(listed[0]["family"], "execute_code");
    assert_eq!(listed[0]["answers"], json!(["once", "refuse"]));
    assert_eq!(always, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(always_body["code"], "ANSWER_NOT_OFFERED");
    // AND once ran the first item again, allowed one call of the tool, to its end
    assert_eq!(once, StatusCode::OK, "{once_body}");
    assert_eq!(once_body["resumed"], 1);
    let reached = poll_until_async(WAIT, || async {
        item_status(&repo, &batch.batch_id, 0).await == "completed"
    })
    .await;
    assert!(reached, "the item allowed once never completed");
    assert_eq!(
        runner
            .once
            .lock()
            .unwrap()
            .last()
            .cloned()
            .flatten()
            .as_deref(),
        Some("bash_executor")
    );
    assert!(runner
        .granted
        .lock()
        .unwrap()
        .iter()
        .all(|(_, g)| g.is_empty()));
    // AND the refusal failed the second item, with the reason
    assert_eq!(refuse, StatusCode::OK, "{refuse_body}");
    let items_now = repo.batch_items(&batch.batch_id).await.unwrap();
    assert_eq!(items_now[1].status, "failed");
    assert_eq!(
        items_now[1].output.as_deref(),
        Some("Refused by the user: no scripts")
    );
    assert!(items(&state).await.is_empty());
}

#[tokio::test]
async fn test_always_gives_a_standing_consent_and_resumes_every_item_it_covers() {
    // GIVEN governance over an empty governance.db, and two batches of the
    // same agent, each with an item waiting for `send_email`
    let dir = tempfile::tempdir().unwrap();
    let (mut state, repo, _) = batch_state(&dir, &["send_email"]).await;
    let governance =
        crate::governance::GovernanceHandle::spawn(crate::governance::GovernanceSources {
            governance_db: Some(dir.path().join("governance.db")),
            config_path: None,
            policy: Arc::new(apollia_permissions::PolicyVerdict::Absent),
            journal: None,
            events: None,
            forbidden_connections: Default::default(),
        });
    state.governance = Some(governance.clone());
    let batches = state.batches.clone().unwrap();
    let first = batches.launch(launch()).await.unwrap();
    awaiting(&state, &first.batch_id, 1).await;
    let second = batches.launch(launch()).await.unwrap();
    awaiting(&state, &second.batch_id, 1).await;
    assert_eq!(items(&state).await.len(), 2);

    // WHEN the first item is answered "always"
    let (status, body) = call(
        state.clone(),
        "POST",
        DECIDE,
        Some(json!({"id": format!("batch:{}:0", first.batch_id), "answer": "always"})),
    )
    .await;

    // THEN both items covered by the consent run again and leave the list,
    // with a message that says so
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["resumed"], 2);
    assert!(body["message"]
        .as_str()
        .unwrap()
        .contains("2 waiting item(s)"));
    for batch_id in [&first.batch_id, &second.batch_id] {
        let reached = poll_until_async(WAIT, || async {
            item_status(&repo, batch_id, 0).await == "completed"
        })
        .await;
        assert!(reached, "an item of {batch_id} never completed");
    }
    assert!(items(&state).await.is_empty());
    // AND the agent holds a standing consent for the family
    let again = governance
        .grant_always(AGENT, apollia_tools::ActionFamily::SendExternal)
        .await
        .unwrap();
    assert!(!again, "the consent was already there");
}

// ─── questions ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_a_question_is_listed_with_its_choices_and_takes_a_free_answer() {
    // GIVEN an agent that asked the user to choose, and waits for the answer
    let dir = tempfile::tempdir().unwrap();
    let state = seeded_chat(dir.path());
    let chat = state.chat_manager.clone().unwrap();
    let (reply_tx, mut reply_rx) = oneshot::channel();
    chat.pending_user_inputs()
        .register(
            "r-1".into(),
            PendingUserInput {
                questions: vec![UserQuestion {
                    id: "format".into(),
                    question: "Which format?".into(),
                    question_type: QuestionType::SingleChoice,
                    options: vec!["PDF".into(), "Word".into()],
                    hint: None,
                }],
                context: None,
                session_id: Some("s-1".into()),
                reply_tx,
            },
        )
        .await;
    let listed_once = poll_until_async(WAIT, || {
        let state = state.clone();
        async move { !items(&state).await.is_empty() }
    })
    .await;
    assert!(listed_once, "the question was never listed");

    // WHEN it is listed, then answered with a choice it does not propose
    let listed = items(&state).await;
    let (status, body) = call(
        state.clone(),
        "POST",
        DECIDE,
        Some(json!({
            "id": "question:r-1",
            "answer": "answer",
            "answers": [{"id": "format", "value": "Markdown"}]
        })),
    )
    .await;

    // THEN it carried its choices and a free answer
    assert_eq!(listed[0]["kind"], "question");
    assert_eq!(listed[0]["summary"], "Which format?");
    assert_eq!(listed[0]["questions"][0]["options"], json!(["PDF", "Word"]));
    assert_eq!(listed[0]["allowOther"], true);
    assert_eq!(listed[0]["answers"], json!(["answer", "refuse"]));
    // AND the agent waiting on it received the free answer
    assert_eq!(status, StatusCode::OK, "{body}");
    let output = reply_rx.try_recv().unwrap();
    assert_eq!(output.answers[0].value.as_deref(), Some("Markdown"));
}
