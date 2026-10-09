//! Integration test: the Python workflows of a package run step by step in
//! the embedded interpreter, through the workflow engine of the runtime.
//!
//! The package is stored the way the package store records one: its row, its
//! `apollia.toml` as JSON, and a link per workflow. The engine records each
//! run and its steps in a throwaway `hitl.db`; the agent a step runs is
//! answered by a scripted runner, so no model is involved.
//!
//! The SDK of this repository is put first on `sys.path`, so the test reads the
//! `@workflow` decorator and its step hook from the tree it builds. Gated
//! behind `python-tests`:
//!
//!   PYO3_PYTHON="$(command -v python3)" \
//!   cargo test -p apollia-e2e-tests --features python-tests --test test_package_workflows

use std::path::Path;
use std::sync::{Arc, Mutex};

use std::pin::Pin;

use apollia_aip::package_tools::workflows::{PackageWorkflows, WorkflowModels};
use apollia_core::{RuntimeEvent, StepBudgetConfig};
use apollia_llm::types::{
    CompletionRequest, CompletionResponse, FinishReason, StreamChunk, TokenUsage,
};
use apollia_llm::{CompletionModel, LlmError, LlmRouter};
use apollia_runtime::agents::task::Nesting;
use apollia_runtime::workflows::{
    AgentRunner, BoxFuture, InputErrorCode, UserReply, WorkflowError, WorkflowHandle,
    WorkflowQuestion,
};
use apollia_tools::{InstalledPackage, PackageItemKind, PackageRepository, TaskRepository};
use pyo3::prelude::*;
use serde_json::json;
use tokio::sync::broadcast;

/// The module of `books`: `close` asks an agent, files the answer through the
/// workflow `file`, then notifies; with `hold`, its first step waits an hour
/// and writes how the wait ended. `remind` reads the file it is given and
/// hands its date to `stamp`, which returns the weekday and the date. `pay`
/// asks the user which amount to pay, then asks to confirm it; a declined
/// confirmation is caught and returned, a declined amount is not.
const FLOWS_MODULE: &str = r#"
import asyncio
import datetime
import pathlib
from typing import NotRequired, TypedDict

from apollia import FileInput, LongText, WorkflowCtx, workflow
from apollia.errors import QuestionDeclined

MARKER = pathlib.Path(__MARKER__)


class Close(TypedDict):
    folder: str
    hold: NotRequired[bool]


class File(TypedDict):
    text: str


@workflow("close", "Close the books.", inputs=Close, steps=["ask", "file", "notify"], action="read")
async def close(ctx: WorkflowCtx, inputs: Close) -> dict:
    async with ctx.step("ask"):
        if inputs.get("hold"):
            MARKER.write_text("started")
            try:
                await asyncio.sleep(3600)
            except asyncio.CancelledError:
                MARKER.write_text("cancelled")
                raise
        answer = await ctx.agents.run("accountant", "total of " + inputs["folder"])
    async with ctx.step("file"):
        filed = await ctx.workflows.run("file", {"text": answer})
    async with ctx.step("notify"):
        ctx.logger.info("closed")
    return {"answer": answer, "filed": filed}


@workflow("file", "File a text.", inputs=File, steps=["write"], action="write_or_delete")
async def file(ctx: WorkflowCtx, inputs: File) -> dict:
    async with ctx.step("write"):
        return {"chars": len(inputs["text"])}


class Remind(TypedDict):
    invoice: FileInput
    due: datetime.date
    note: NotRequired[LongText]


class Stamp(TypedDict):
    day: datetime.date


@workflow("remind", "Remind an invoice.", inputs=Remind, steps=["read"], action="read")
async def remind(ctx: WorkflowCtx, inputs: Remind) -> dict:
    async with ctx.step("read"):
        text = pathlib.Path(inputs["invoice"]).read_text()
        stamped = await ctx.workflows.run("stamp", {"day": inputs["due"]})
    return {"invoice": text, "stamped": stamped, "note": inputs.get("note", "")}


@workflow("stamp", "Stamp a day.", inputs=Stamp, steps=["stamp"], action="read")
async def stamp(ctx: WorkflowCtx, inputs: Stamp) -> dict:
    async with ctx.step("stamp"):
        return {"weekday": inputs["day"].isoweekday(), "day": inputs["day"]}


class Pay(TypedDict):
    supplier: str
    choices: NotRequired[int]


class Think(TypedDict):
    calls: int


ANSWER = {"type": "object", "required": ["answer"], "properties": {"answer": {"type": "string"}}}


@workflow("think", "Ask the model.", inputs=Think, steps=["think"], action="read")
async def think(ctx: WorkflowCtx, inputs: Think) -> dict:
    async with ctx.step("think"):
        for _ in range(inputs["calls"]):
            value = await ctx.llm.complete(
                [{"role": "user", "content": "The answer?"}],
                max_tokens=64,
                schema=ANSWER,
                thinking=False,
            )
    return value


@workflow("pay", "Pay a supplier.", inputs=Pay, steps=["validate", "pay"], action="send_external")
async def pay(ctx: WorkflowCtx, inputs: Pay) -> dict:
    async with ctx.step("validate"):
        options = ["120.00", "240.00"] if "choices" not in inputs else [inputs["choices"]]
        amount = await ctx.user.ask(
            "Which amount for " + inputs["supplier"] + "?",
            options=options,
            hint="an amount in euros",
        )
        try:
            confirmed = await ctx.user.ask("Pay " + amount + " to " + inputs["supplier"] + "?")
        except QuestionDeclined as declined:
            return {"paid": None, "question": declined.question, "reason": declined.reason}
    async with ctx.step("pay"):
        return {"paid": amount, "confirmed": confirmed}
"#;

/// `apollia.toml` of `books`, as `python -m apollia describe` writes it.
const BOOKS_MANIFEST: &str = r#"
[package]
name = "books"
version = "1.0.0"
publisher = "Example"
sdk = ">=0.3, <0.4"

[[workflows]]
name = "close"
module = "flows.py"
description = "Close the books."
action = "read"
steps = ["ask", "file", "notify"]
inputs = { type = "object", properties = { folder = { type = "string" }, hold = { type = "boolean" } }, required = ["folder"], additionalProperties = false }

[[workflows]]
name = "file"
module = "flows.py"
description = "File a text."
action = "write_or_delete"
steps = ["write"]
inputs = { type = "object", properties = { text = { type = "string" } }, required = ["text"], additionalProperties = false }

[[workflows]]
name = "remind"
module = "flows.py"
description = "Remind an invoice."
action = "read"
steps = ["read"]
inputs = { type = "object", properties = { invoice = { type = "string", format = "file-path" }, due = { type = "string", format = "date" }, note = { type = "string", format = "long-text" } }, required = ["due", "invoice"], additionalProperties = false, x-order = ["invoice", "due", "note"] }

[[workflows]]
name = "stamp"
module = "flows.py"
description = "Stamp a day."
action = "read"
steps = ["stamp"]
inputs = { type = "object", properties = { day = { type = "string", format = "date" } }, required = ["day"], additionalProperties = false, x-order = ["day"] }

[[workflows]]
name = "pay"
module = "flows.py"
description = "Pay a supplier."
action = "send_external"
steps = ["validate", "pay"]
inputs = { type = "object", properties = { supplier = { type = "string" }, choices = { type = "integer" } }, required = ["supplier"], additionalProperties = false, x-order = ["supplier", "choices"] }

[[workflows]]
name = "think"
module = "flows.py"
description = "Ask the model."
action = "read"
steps = ["think"]
inputs = { type = "object", properties = { calls = { type = "integer" } }, required = ["calls"], additionalProperties = false, x-order = ["calls"] }
"#;

/// Answers every call with the same JSON object, and remembers each request.
#[derive(Default)]
struct Model {
    requests: Mutex<Vec<CompletionRequest>>,
}

#[async_trait::async_trait]
impl CompletionModel for Model {
    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        self.requests.lock().expect("requests").push(req);
        Ok(CompletionResponse {
            engine_timings: None,
            content: r#"{"answer": "42"}"#.into(),
            tool_calls: vec![],
            usage: TokenUsage::default(),
            finish_reason: FinishReason::Stop,
            latency_ms: 1,
            ttft_ms: None,
        })
    }

    async fn stream(
        &self,
        _req: CompletionRequest,
    ) -> Result<Pin<Box<dyn futures::Stream<Item = Result<StreamChunk, LlmError>> + Send>>, LlmError>
    {
        Ok(Box::pin(futures::stream::empty()))
    }

    fn is_available(&self) -> bool {
        true
    }
    fn backend_name(&self) -> &str {
        "model"
    }
    fn model_id(&self) -> &str {
        "model"
    }
}

/// How many model calls a run of the fixture may make.
const MODEL_CALLS_PER_RUN: u32 = 2;

/// Answers every agent with `answer to <message>`, and remembers each call.
#[derive(Default)]
struct Agents {
    calls: Mutex<Vec<(String, String)>>,
}

impl AgentRunner for Agents {
    fn run(
        &self,
        agent: String,
        message: String,
        _nesting: Nesting,
    ) -> BoxFuture<'_, Result<String, String>> {
        self.calls
            .lock()
            .expect("calls lock")
            .push((agent, message.clone()));
        Box::pin(async move { Ok(format!("answer to {message}")) })
    }
}

struct Fixture {
    dir: tempfile::TempDir,
    engine: WorkflowHandle,
    repository: Arc<TaskRepository>,
    events: broadcast::Receiver<RuntimeEvent>,
    agents: Arc<Agents>,
    model: Arc<Model>,
}

impl Fixture {
    fn marker(&self) -> Option<String> {
        std::fs::read_to_string(self.dir.path().join("marker.txt")).ok()
    }
}

/// Puts the SDK of this tree first on `sys.path`.
fn use_tree_sdk() {
    let sdk = Path::new(env!("CARGO_MANIFEST_DIR")).join("../sdk");
    Python::with_gil(|py| -> PyResult<()> {
        py.import("sys")?
            .getattr("path")?
            .call_method1("insert", (0, sdk.to_string_lossy().as_ref()))?;
        Ok(())
    })
    .expect("sys.path accepts the SDK");
}

/// Stores the package `books` under `dir`, its six workflows linked.
fn store_books(dir: &Path) -> PackageRepository {
    let root = dir.join("packages").join("books");
    std::fs::create_dir_all(&root).expect("package dir");
    let marker = format!("{:?}", dir.join("marker.txt").to_string_lossy());
    std::fs::write(
        root.join("flows.py"),
        FLOWS_MODULE.replace("__MARKER__", &marker),
    )
    .expect("write the module");
    let manifest = apollia_aip::apollia_manifest::parse(BOOKS_MANIFEST).expect("valid manifest");
    let packages = PackageRepository::open(&dir.join("agents.db")).expect("open the store");
    packages
        .save(&InstalledPackage {
            name: "books".into(),
            version: "1.0.0".into(),
            root_path: root,
            manifest_json: serde_json::to_string(&manifest).expect("manifest as JSON"),
            installed_at: "2026-10-02T00:00:00Z".into(),
            updated_at: "2026-10-02T00:00:00Z".into(),
        })
        .expect("save the package");
    for workflow in ["close", "file", "remind", "stamp", "pay", "think"] {
        packages
            .link_item("books", PackageItemKind::Workflow, workflow)
            .expect("link the workflow");
    }
    packages
}

/// The package `books` stored with its five workflows linked, and the engine
/// attached to them.
async fn fixture() -> Fixture {
    use_tree_sdk();
    let dir = tempfile::tempdir().expect("temp dir");
    let packages = store_books(dir.path());
    let model = Arc::new(Model::default());
    let mut backends: std::collections::HashMap<String, Arc<dyn CompletionModel>> =
        std::collections::HashMap::new();
    backends.insert("model".into(), model.clone());
    let workflows = PackageWorkflows::read(&packages, dir.path().join("venvs"))
        .expect("read the workflows")
        .with_models(Some(WorkflowModels::new(
            Arc::new(LlmRouter::with_backends(backends, "model")),
            None,
            MODEL_CALLS_PER_RUN,
        )));
    assert_eq!(
        workflows.ids(),
        vec![
            "books/close",
            "books/file",
            "books/remind",
            "books/stamp",
            "books/pay",
            "books/think"
        ]
    );

    let repository = Arc::new(
        TaskRepository::open(&dir.path().join("hitl.db"))
            .await
            .expect("open hitl.db"),
    );
    let (bus, events) = broadcast::channel(256);
    let agents = Arc::new(Agents::default());
    let engine = WorkflowHandle::spawn(
        repository.clone(),
        bus,
        StepBudgetConfig::default(),
        Some(agents.clone() as Arc<dyn AgentRunner>),
    );
    engine
        .set_host(Arc::new(workflows))
        .await
        .expect("attach the host");
    Fixture {
        dir,
        engine,
        repository,
        events,
        agents,
        model,
    }
}

fn step_events(events: &mut broadcast::Receiver<RuntimeEvent>, workflow: &str) -> Vec<String> {
    let mut seen = Vec::new();
    while let Ok(event) = events.try_recv() {
        match event {
            RuntimeEvent::WorkflowStepStarted {
                workflow_id, step, ..
            } if workflow_id == workflow => seen.push(format!("started {step}")),
            RuntimeEvent::WorkflowStepCompleted {
                workflow_id, step, ..
            } if workflow_id == workflow => seen.push(format!("completed {step}")),
            RuntimeEvent::WorkflowStepFailed {
                workflow_id, step, ..
            } if workflow_id == workflow => seen.push(format!("failed {step}")),
            _ => {}
        }
    }
    seen
}

// GIVEN a stored package whose workflow `close` asks an agent, then runs the
//       workflow `file` of the same package by its bare name
// WHEN it runs on valid inputs
// THEN each Python step was reported through the private hook, published on
//      the bus and recorded; the agent was asked through `ctx.agents.run`; and
//      `file` ran nested at depth 2, its result returned to `close`
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn package_workflow_runs_its_steps_and_its_nested_calls() {
    // GIVEN
    let mut fx = fixture().await;

    // WHEN
    let run = fx
        .engine
        .run_to_end("books/close", json!({"folder": "march"}))
        .await
        .expect("the run ends");

    // THEN
    assert_eq!(run.status, "completed", "{:?}", run.error);
    assert_eq!(
        run.output,
        Some(json!({"answer": "answer to total of march", "filed": {"chars": 24}}))
    );
    let steps = fx
        .repository
        .workflow_steps(&run.run_id)
        .await
        .expect("read the steps");
    let statuses: Vec<&str> = steps.iter().map(|s| s.status.as_str()).collect();
    assert_eq!(statuses, vec!["completed", "completed", "completed"]);
    assert_eq!(steps[0].output.as_deref(), Some("answer to total of march"));
    assert_eq!(
        step_events(&mut fx.events, "books/close"),
        vec![
            "started ask",
            "completed ask",
            "started file",
            "completed file",
            "started notify",
            "completed notify"
        ]
    );
    assert_eq!(
        fx.agents.calls.lock().expect("calls lock").clone(),
        vec![("accountant".to_string(), "total of march".to_string())]
    );
    let nested = fx
        .engine
        .history("books/file", 10)
        .await
        .expect("read the history");
    assert_eq!(nested.len(), 1);
    assert_eq!(nested[0].depth, 2);
    assert_eq!(
        nested[0].parent_run_id.as_deref(),
        Some(run.run_id.as_str())
    );
}

// GIVEN a run whose first Python step is awaiting an hour-long sleep
// WHEN the run is stopped
// THEN the coroutine was cancelled inside that step, the step is canceled,
//      the two others were never started, and no agent was asked
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stopping_a_package_workflow_interrupts_its_python_step() {
    // GIVEN
    let fx = fixture().await;
    let run = fx
        .engine
        .launch("books/close", json!({"folder": "march", "hold": "true"}))
        .await
        .expect("the run starts");
    while fx.marker().as_deref() != Some("started") {
        tokio::task::yield_now().await;
    }

    // WHEN
    let stopped = fx.engine.cancel(&run.run_id).await.expect("the run stops");

    // THEN
    assert_eq!(stopped.status, "canceled");
    assert_eq!(fx.marker().as_deref(), Some("cancelled"));
    let steps = fx
        .repository
        .workflow_steps(&run.run_id)
        .await
        .expect("read the steps");
    let statuses: Vec<&str> = steps.iter().map(|s| s.status.as_str()).collect();
    assert_eq!(statuses, vec!["canceled", "not_started", "not_started"]);
    assert!(fx.agents.calls.lock().expect("calls lock").is_empty());
}

// GIVEN the stored package, whose `close` declares `folder` as a string
// WHEN it is launched with a number for `folder`, then without it
// THEN both launches are refused with the field named, before any run
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn package_workflow_inputs_are_checked_against_the_manifest_schema() {
    // GIVEN
    let fx = fixture().await;

    // WHEN
    let mistyped = fx
        .engine
        .launch("books/close", json!({"folder": 3}))
        .await
        .expect_err("a number is no folder");
    let missing = fx
        .engine
        .launch("books/close", json!({}))
        .await
        .expect_err("the folder is required");

    // THEN
    assert!(matches!(mistyped, WorkflowError::InvalidInput(ref e) if e.field == "folder"));
    assert!(matches!(missing, WorkflowError::InvalidInput(ref e) if e.field == "folder"));
    let history = fx
        .engine
        .history("books/close", 10)
        .await
        .expect("read the history");
    assert!(history.is_empty());
}

// GIVEN the stored package, whose `remind` takes a file, a date and a long
//       text, and hands the date to the workflow `stamp`
// WHEN it runs on a file that exists and a day of the calendar
// THEN the Python body read the file at the path it was given, received the
//      date as a `date` (its weekday was computed), handed it on and got it
//      back written `YYYY-MM-DD`
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_package_workflow_reads_its_file_and_its_date_inputs() {
    // GIVEN
    let fx = fixture().await;
    let invoice = fx.dir.path().join("f-12.txt");
    std::fs::write(&invoice, "invoice 12").expect("write the invoice");

    // WHEN
    let run = fx
        .engine
        .run_to_end(
            "books/remind",
            json!({
                "invoice": invoice.to_string_lossy(),
                "due": "2026-10-03",
                "note": "Hello,\nplease pay."
            }),
        )
        .await
        .expect("the run ends");

    // THEN
    assert_eq!(run.status, "completed", "{:?}", run.error);
    assert_eq!(
        run.output,
        Some(json!({
            "invoice": "invoice 12",
            "stamped": {"weekday": 6, "day": "2026-10-03"},
            "note": "Hello,\nplease pay."
        }))
    );
}

// GIVEN the stored package and a folder holding no `missing.pdf`
// WHEN `remind` is launched on a day that does not exist, then on that file
// THEN both launches are refused with their code, before any run
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_malformed_date_and_an_absent_file_never_start_a_package_workflow() {
    // GIVEN
    let fx = fixture().await;
    let invoice = fx.dir.path().join("f-12.txt");
    std::fs::write(&invoice, "invoice 12").expect("write the invoice");
    let absent = fx.dir.path().join("missing.pdf");

    // WHEN
    let date = fx
        .engine
        .launch(
            "books/remind",
            json!({"invoice": invoice.to_string_lossy(), "due": "2026-02-30"}),
        )
        .await
        .expect_err("February has no 30th");
    let file = fx
        .engine
        .launch(
            "books/remind",
            json!({"invoice": absent.to_string_lossy(), "due": "2026-10-03"}),
        )
        .await
        .expect_err("the file is absent");

    // THEN
    assert!(matches!(
        date,
        WorkflowError::InvalidInput(ref e) if e.code == InputErrorCode::MalformedDate
    ));
    assert!(matches!(
        file,
        WorkflowError::InvalidInput(ref e) if e.code == InputErrorCode::MissingFile
    ));
    let history = fx
        .engine
        .history("books/remind", 10)
        .await
        .expect("read the history");
    assert!(history.is_empty());
}

/// The run `run_id` once it is no longer running.
async fn wait_ended(fx: &Fixture, run_id: &str) -> apollia_tools::WorkflowRunRow {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        let detail = fx
            .engine
            .detail(run_id)
            .await
            .expect("read the run")
            .expect("the run exists");
        if detail.run.status != "running" {
            return detail.run;
        }
        assert!(std::time::Instant::now() < deadline, "the run never ended");
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
}

/// The question `books/pay` waits on once it asked `question`.
async fn asked(fx: &Fixture, question: &str) -> WorkflowQuestion {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        let waiting = fx.engine.pending_questions().await.expect("read questions");
        if let Some(found) = waiting.into_iter().find(|q| q.asked.question == question) {
            return found;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the workflow never asked {question:?}"
        );
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
}

// GIVEN a stored package whose workflow `pay` asks the user an amount in its
//       step `validate`, then asks to confirm it
// WHEN the user gives a free answer to the first, then answers the second
// THEN each `ctx.user.ask` stopped the Python step until its answer, the
//      second question was built from the first answer, and the run went on
//      to its next step with both
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn package_workflow_asks_the_user_and_goes_on_with_each_answer() {
    // GIVEN
    let fx = fixture().await;
    let run = fx
        .engine
        .launch("books/pay", json!({"supplier": "Acme"}))
        .await
        .expect("the run starts");

    // WHEN
    let first = asked(&fx, "Which amount for Acme?").await;
    let paused = fx
        .repository
        .workflow_steps(&run.run_id)
        .await
        .expect("read the steps");
    fx.engine
        .answer_question(&first.request_id, UserReply::Answer("180.50".into()))
        .await
        .expect("the first answer is taken");
    let second = asked(&fx, "Pay 180.50 to Acme?").await;
    fx.engine
        .answer_question(&second.request_id, UserReply::Answer("yes".into()))
        .await
        .expect("the second answer is taken");

    // THEN
    assert_eq!(first.run_id, run.run_id);
    assert_eq!(first.workflow_id, "books/pay");
    assert_eq!(first.step.as_deref(), Some("validate"));
    assert_eq!(first.asked.options, vec!["120.00", "240.00"]);
    assert_eq!(first.asked.hint.as_deref(), Some("an amount in euros"));
    let statuses: Vec<&str> = paused.iter().map(|s| s.status.as_str()).collect();
    assert_eq!(statuses, vec!["running", "pending"]);
    assert!(second.asked.options.is_empty());
    let ended = wait_ended(&fx, &run.run_id).await;
    assert_eq!(ended.status, "completed", "{:?}", ended.error);
    assert_eq!(
        ended.output,
        Some(json!({"paid": "180.50", "confirmed": "yes"}))
    );
}

// GIVEN the workflow `pay` waiting on its confirmation, which it asks under a
//       `try` for `QuestionDeclined`, then a second run waiting on its amount,
//       asked with no `try`
// WHEN the user declines each, with a reason
// THEN the first run received the typed refusal with the question and the
//      reason and completed without paying; the second failed in its step
//      with the refusal
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_declined_question_reaches_the_python_step_as_a_typed_refusal() {
    // GIVEN
    let fx = fixture().await;
    let caught = fx
        .engine
        .launch("books/pay", json!({"supplier": "Acme"}))
        .await
        .expect("the run starts");
    let amount = asked(&fx, "Which amount for Acme?").await;
    fx.engine
        .answer_question(&amount.request_id, UserReply::Answer("120.00".into()))
        .await
        .expect("the amount is taken");
    let confirmation = asked(&fx, "Pay 120.00 to Acme?").await;

    // WHEN
    fx.engine
        .answer_question(
            &confirmation.request_id,
            UserReply::Declined("not this month".into()),
        )
        .await
        .expect("the refusal is taken");
    let caught = wait_ended(&fx, &caught.run_id).await;
    let uncaught = fx
        .engine
        .launch("books/pay", json!({"supplier": "Globex"}))
        .await
        .expect("the run starts");
    let amount = asked(&fx, "Which amount for Globex?").await;
    fx.engine
        .answer_question(&amount.request_id, UserReply::Declined("no budget".into()))
        .await
        .expect("the refusal is taken");
    let uncaught = wait_ended(&fx, &uncaught.run_id).await;

    // THEN
    assert_eq!(caught.status, "completed", "{:?}", caught.error);
    assert_eq!(
        caught.output,
        Some(json!({
            "paid": null,
            "question": "Pay 120.00 to Acme?",
            "reason": "not this month"
        }))
    );
    assert_eq!(uncaught.status, "failed");
    let error = uncaught.error.unwrap_or_default();
    assert!(error.contains("QuestionDeclined"), "{error}");
    assert!(error.contains("no budget"), "{error}");
    let steps = fx
        .repository
        .workflow_steps(&uncaught.run_id)
        .await
        .expect("read the steps");
    assert_eq!(steps[0].status, "failed");
    assert_eq!(steps[1].status, "not_started");
}

// GIVEN the workflow `pay` asked to propose a choice that is not text
// WHEN it runs
// THEN `ctx.user.ask` refuses the call in the step, nothing is listed for the
//      user, and the run fails naming the field
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_question_whose_choices_are_not_text_is_refused_before_it_is_asked() {
    // GIVEN
    let fx = fixture().await;

    // WHEN
    let run = fx
        .engine
        .run_to_end("books/pay", json!({"supplier": "Acme", "choices": 3}))
        .await
        .expect("the run ends");

    // THEN
    assert_eq!(run.status, "failed");
    let error = run.error.unwrap_or_default();
    assert!(error.contains("options must be a list of text"), "{error}");
    assert!(fx
        .engine
        .pending_questions()
        .await
        .expect("read questions")
        .is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_package_workflow_calls_the_model_through_ctx_llm() {
    // GIVEN the package `books`, its host given the models
    let fx = fixture().await;

    // WHEN the workflow `think` runs, one constrained call made without
    // reasoning
    let run = fx
        .engine
        .run_to_end("books/think", json!({"calls": 1}))
        .await
        .expect("the run ends");

    // THEN the run returns the validated value, and the model was asked once,
    // with the cap, the schema and the reasoning switch of the call
    assert_eq!(run.status, "completed", "{:?}", run.error);
    assert_eq!(run.output, Some(json!({"answer": "42"})));
    let requests = fx.model.requests.lock().expect("requests");
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].max_tokens, Some(64));
    assert_eq!(requests[0].thinking, Some(false));
    assert!(requests[0].response_schema.is_some());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_package_workflow_is_stopped_at_its_budget_of_model_calls() {
    // GIVEN a host whose runs may call the model twice
    let fx = fixture().await;

    // WHEN the workflow `think` asks for three calls
    let run = fx
        .engine
        .run_to_end("books/think", json!({"calls": 3}))
        .await
        .expect("the run ends");

    // THEN the run fails on its third call, which never reaches the model
    assert_eq!(run.status, "failed");
    let reason = run.error.unwrap_or_default();
    assert!(reason.contains("step budget exhausted"), "{reason}");
    assert_eq!(
        fx.model.requests.lock().expect("requests").len(),
        MODEL_CALLS_PER_RUN as usize
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_package_installed_while_the_runtime_runs_brings_its_workflows_at_once() {
    // GIVEN a runtime started on an empty package store, its workflow host
    // attached with nothing to run
    use_tree_sdk();
    let dir = tempfile::tempdir().expect("temp dir");
    drop(PackageRepository::open(&dir.path().join("agents.db")).expect("open the store"));
    let repository = Arc::new(
        TaskRepository::open(&dir.path().join("hitl.db"))
            .await
            .expect("open hitl.db"),
    );
    let (bus, _events) = broadcast::channel(256);
    let engine = WorkflowHandle::spawn(repository, bus, StepBudgetConfig::default(), None);
    let attached =
        apollia_aip::package_tools::workflows::attach(dir.path(), Some(&engine), None).await;
    let before = engine
        .run_to_end("books/stamp", json!({"day": "2026-10-09"}))
        .await;
    let registry = apollia_tools::ToolRegistryHandle::start();

    // WHEN a package is stored, then the hook the host leaves the runtime
    // re-reads the package code
    drop(store_books(dir.path()));
    let reload = apollia_aip::package_tools::reload_hook(
        dir.path().to_path_buf(),
        registry.clone(),
        None,
        Vec::new(),
        Some(engine.clone()),
        None,
    );
    reload().await;
    let after = engine
        .run_to_end("books/stamp", json!({"day": "2026-10-09"}))
        .await
        .expect("the run ends");

    // THEN the workflow that did not exist before runs, with no restart
    assert_eq!(attached, 0);
    assert!(before.is_err(), "no workflow before the package is stored");
    assert_eq!(after.status, "completed", "{:?}", after.error);
    assert_eq!(
        after.output,
        Some(json!({"weekday": 5, "day": "2026-10-09"}))
    );
    registry.shutdown().await;
}
