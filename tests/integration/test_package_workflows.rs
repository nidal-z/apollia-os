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

use apollia_aip::package_tools::workflows::PackageWorkflows;
use apollia_core::{RuntimeEvent, StepBudgetConfig};
use apollia_runtime::agents::task::Nesting;
use apollia_runtime::workflows::{
    AgentRunner, BoxFuture, InputErrorCode, WorkflowError, WorkflowHandle,
};
use apollia_tools::{InstalledPackage, PackageItemKind, PackageRepository, TaskRepository};
use pyo3::prelude::*;
use serde_json::json;
use tokio::sync::broadcast;

/// The module of `books`: `close` asks an agent, files the answer through the
/// workflow `file`, then notifies; with `hold`, its first step waits an hour
/// and writes how the wait ended. `remind` reads the file it is given and
/// hands its date to `stamp`, which returns the weekday and the date.
const FLOWS_MODULE: &str = r#"
import asyncio
import datetime
import pathlib
from typing import NotRequired, TypedDict

from apollia import FileInput, LongText, WorkflowCtx, workflow

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
"#;

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

/// The package `books` stored with its four workflows linked, and the engine
/// attached to them.
async fn fixture() -> Fixture {
    use_tree_sdk();
    let dir = tempfile::tempdir().expect("temp dir");
    let root = dir.path().join("packages").join("books");
    std::fs::create_dir_all(&root).expect("package dir");
    let marker = format!("{:?}", dir.path().join("marker.txt").to_string_lossy());
    std::fs::write(
        root.join("flows.py"),
        FLOWS_MODULE.replace("__MARKER__", &marker),
    )
    .expect("write the module");
    let manifest = apollia_aip::apollia_manifest::parse(BOOKS_MANIFEST).expect("valid manifest");
    let packages = PackageRepository::open(&dir.path().join("agents.db")).expect("open the store");
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
    for workflow in ["close", "file", "remind", "stamp"] {
        packages
            .link_item("books", PackageItemKind::Workflow, workflow)
            .expect("link the workflow");
    }
    let workflows =
        PackageWorkflows::read(&packages, dir.path().join("venvs")).expect("read the workflows");
    assert_eq!(
        workflows.ids(),
        vec!["books/close", "books/file", "books/remind", "books/stamp"]
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
