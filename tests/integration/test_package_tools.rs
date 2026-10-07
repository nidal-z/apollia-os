//! Integration test: the Python tools of a package run in the embedded
//! interpreter, for any agent granted them.
//!
//! The package is stored the way the package store records one: its row, its
//! `apollia.toml` as JSON, and a link per tool. The tools are read from that
//! manifest, entered in the registry, and executed through the dispatcher that
//! runs the native tools. The module of a tool is imported on its first call.
//!
//! The SDK of this repository is put first on `sys.path`, so the test reads the
//! `@tool` decorator of the tree it builds. Gated behind `python-tests`:
//!
//!   PYO3_PYTHON="$(command -v python3)" \
//!   cargo test -p apollia-e2e-tests --features python-tests --test test_package_tools

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use apollia_aip::package_tools::PackageTools;
use apollia_core::{AutonomyLevel, RunId, RuntimeEvent, StepBudgetConfig};
use apollia_llm::types::{
    CompletionModel, CompletionRequest, CompletionResponse, FinishReason, LlmError, StreamChunk,
    TokenUsage, ToolCall,
};
use apollia_llm::LlmRouter;
use apollia_mcp::session::LoadingMode;
use apollia_runtime::chat::{
    AgentRunSpec, AgentTaskRun, ChatSessionManagerHandle, ToolCallStatus, ToolDecision,
};
use apollia_runtime::coordinator::CancellationToken;
use apollia_runtime::eventbus::EventBus;
use apollia_runtime::package_tools::PackageToolHost;
use apollia_runtime::registry::AgentRegistry;
use apollia_tools::executor::{ToolDispatcher, ToolExecutionError};
use apollia_tools::{
    AgentAccess, AgentOrigin, AgentRepository, InstalledPackage, PackageItemKind,
    PackageRepository, StoredAgent, ToolRegistryHandle,
};
use pyo3::prelude::*;
use serde_json::json;
use tokio::sync::{broadcast, oneshot};

/// The package `calc`: three tools, one module. `add` declares a personal
/// read, a family that asks before it runs: a plain read runs unasked, and the
/// agent test below waits on that question. The module imports
/// `calc_vendor`, which only the package environment holds, and counts its own
/// imports in `IMPORTS`; `wait` sleeps an hour and writes how it ended;
/// `forbidden` leaves a file behind if it ever runs.
const CALC_MODULE: &str = r#"
import asyncio
import pathlib
from typing import Annotated

from apollia import tool

import calc_vendor

IMPORTS = pathlib.Path(__IMPORTS__)
MARKER = pathlib.Path(__MARKER__)
RAN = pathlib.Path(__RAN__)
IMPORTS.write_text(str(int(IMPORTS.read_text()) + 1) if IMPORTS.exists() else "1")


@tool("add", "Add two numbers.", action="read_personal")
async def add(ctx, a: Annotated[int, "first"], b: Annotated[int, "second"]) -> dict:
    return {"sum": a + b, "caller": ctx.agent_name, "vendor": calc_vendor.ORIGIN}


@tool("wait", "Wait for an hour.", action="read")
async def wait(ctx) -> str:
    MARKER.write_text("started")
    try:
        await asyncio.sleep(3600)
    except asyncio.CancelledError:
        MARKER.write_text("cancelled")
        raise
    MARKER.write_text("finished")
    return "done"


@tool("forbidden", "Never granted.", action="write_or_delete")
async def forbidden(ctx) -> str:
    RAN.write_text("ran")
    return "ran"
"#;

/// `apollia.toml` of `calc`, as `python -m apollia describe` writes it.
const CALC_MANIFEST: &str = r#"
[package]
name = "calc"
version = "1.0.0"
publisher = "Example"
sdk = ">=0.3, <0.4"

[[tools]]
name = "add"
module = "tools/calc.py"
description = "Add two numbers."
action = "read_personal"
inputs = { type = "object", properties = { a = { type = "integer" }, b = { type = "integer" } }, required = ["a", "b"] }

[[tools]]
name = "wait"
module = "tools/calc.py"
description = "Wait for an hour."
action = "read"
inputs = { type = "object" }

[[tools]]
name = "forbidden"
module = "tools/calc.py"
description = "Never granted."
action = "write_or_delete"
inputs = { type = "object" }
"#;

/// `calc_vendor`, the dependency the environment of `calc` installed.
const CALC_VENDOR: &str = "ORIGIN = \"package environment\"\n";

const AGENT: &str = "bookkeeper";

struct Fixture {
    dir: tempfile::TempDir,
    tools: PackageTools,
}

impl Fixture {
    fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    fn read(&self, name: &str) -> Option<String> {
        std::fs::read_to_string(self.path(name)).ok()
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

/// A package `calc` with its three tools linked, stored in `agents.db`, and its
/// environment at `venvs/calc/venv` holding `calc_vendor` in `site-packages`.
fn stored_package() -> Fixture {
    use_tree_sdk();
    let dir = tempfile::tempdir().expect("temp dir");
    let root = dir.path().join("packages").join("calc");
    std::fs::create_dir_all(root.join("tools")).expect("package dirs");
    let quoted = |name: &str| format!("{:?}", dir.path().join(name).to_string_lossy());
    let module = CALC_MODULE
        .replace("__IMPORTS__", &quoted("imports.txt"))
        .replace("__MARKER__", &quoted("marker.txt"))
        .replace("__RAN__", &quoted("ran.txt"));
    std::fs::write(root.join("tools/calc.py"), module).expect("write the module");
    let manifest = apollia_aip::apollia_manifest::parse(CALC_MANIFEST).expect("valid manifest");
    let repo = PackageRepository::open(&dir.path().join("agents.db")).expect("open the store");
    repo.save(&InstalledPackage {
        name: "calc".into(),
        version: "1.0.0".into(),
        root_path: root,
        manifest_json: serde_json::to_string(&manifest).expect("manifest as JSON"),
        installed_at: "2026-10-02T00:00:00Z".into(),
        updated_at: "2026-10-02T00:00:00Z".into(),
    })
    .expect("save the package");
    for tool in ["add", "wait", "forbidden"] {
        repo.link_item("calc", PackageItemKind::Tool, tool)
            .expect("link the tool");
    }
    let site_packages = dir
        .path()
        .join("venvs/calc/venv/lib/python3.12/site-packages");
    std::fs::create_dir_all(&site_packages).expect("package environment dirs");
    std::fs::write(site_packages.join("calc_vendor.py"), CALC_VENDOR)
        .expect("write the vendored module");
    let tools = PackageTools::read(&repo, dir.path().join("venvs")).expect("read the tools");
    Fixture { dir, tools }
}

// GIVEN a stored package whose tool `add` has never been called
// WHEN an agent calls it twice through the dispatcher of the native tools
// THEN both calls answer with the sum, the calling agent and a value read from
//      the package environment, and the module was imported once, at the
//      first call
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn package_tool_is_loaded_on_first_call_and_runs_through_the_dispatcher() {
    // GIVEN
    let fx = stored_package();
    let dispatcher = ToolDispatcher::new(fx.tools.executors(AGENT, &CancellationToken::new()));
    assert_eq!(fx.read("imports.txt"), None, "imported before any call");

    // WHEN
    let first = dispatcher
        .dispatch("pkg:calc/add", json!({"a": 2, "b": 3}))
        .await;
    let second = dispatcher
        .dispatch("pkg:calc/add", json!({"a": 4, "b": 5}))
        .await;

    // THEN
    assert_eq!(
        first.expect("first call"),
        json!({"sum": 5, "caller": AGENT, "vendor": "package environment"})
    );
    assert_eq!(
        second.expect("second call"),
        json!({"sum": 9, "caller": AGENT, "vendor": "package environment"})
    );
    assert_eq!(fx.read("imports.txt").as_deref(), Some("1"));
}

// GIVEN a package tool inside an hour-long await
// WHEN the run's cancellation token is cancelled
// THEN the call ends within two seconds as canceled, and the coroutine itself
//      received the cancellation
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn package_tool_in_progress_stops_on_the_cancellation_token() {
    // GIVEN
    let fx = stored_package();
    let cancel = CancellationToken::new();
    let dispatcher = Arc::new(ToolDispatcher::new(fx.tools.executors(AGENT, &cancel)));
    let running = {
        let dispatcher = Arc::clone(&dispatcher);
        tokio::spawn(async move { dispatcher.dispatch("pkg:calc/wait", json!({})).await })
    };
    let deadline = Instant::now() + Duration::from_secs(20);
    while fx.read("marker.txt").as_deref() != Some("started") {
        assert!(Instant::now() < deadline, "the tool never started");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    // WHEN
    let cancelled_at = Instant::now();
    cancel.cancel();
    let outcome = running.await.expect("the call task joins");

    // THEN
    assert!(
        cancelled_at.elapsed() < Duration::from_secs(2),
        "the call took {:?} to stop",
        cancelled_at.elapsed()
    );
    assert!(
        matches!(outcome, Err(ToolExecutionError::ExecutionFailed { ref code, .. }) if code == "canceled"),
        "{outcome:?}"
    );
    assert_eq!(fx.read("marker.txt").as_deref(), Some("cancelled"));
}

/// First calls `pkg:calc/forbidden`, then `pkg:calc/add`, then answers.
struct Scripted {
    turns: AtomicU32,
}

#[async_trait::async_trait]
impl CompletionModel for Scripted {
    async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        Ok(CompletionResponse {
            engine_timings: None,
            content: "The sum is 5.".into(),
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
        let chunk = match self.turns.fetch_add(1, Ordering::SeqCst) {
            0 => StreamChunk::ToolCall(ToolCall {
                id: "c1".into(),
                name: "pkg:calc/forbidden".into(),
                arguments: json!({}),
            }),
            1 => StreamChunk::ToolCall(ToolCall {
                id: "c2".into(),
                name: "pkg:calc/add".into(),
                arguments: json!({"a": 2, "b": 3}),
            }),
            _ => StreamChunk::Text("The sum is 5.".into()),
        };
        Ok(Box::pin(futures::stream::iter(vec![Ok(chunk)])))
    }

    fn is_available(&self) -> bool {
        true
    }
    fn backend_name(&self) -> &str {
        "scripted"
    }
    fn model_id(&self) -> &str {
        "scripted"
    }
}

/// An agent created in the interface, without code, granted `pkg:calc/add`
/// alone, as the store resolves it, with the access a run of it is given.
fn interface_agent(dir: &Path) -> (AgentRunSpec, AgentAccess) {
    let agents = AgentRepository::open(&dir.join("agents.db")).expect("open the agent store");
    agents
        .save_definition(&StoredAgent {
            name: AGENT.into(),
            origin: AgentOrigin::Interface,
            package: None,
            version: "1.0.0".into(),
            definition: json!({
                "id": AGENT,
                "name": "Bookkeeper",
                "mission": "Add up what you are asked to.",
                "defaults": {"access": ["pkg:calc/add"], "autonomy": "assisted"}
            }),
            enabled: true,
            installed_at: "2026-10-02T00:00:00Z".into(),
            updated_at: "2026-10-02T00:00:00Z".into(),
        })
        .expect("store the agent");
    let resolved = agents
        .resolved(AGENT, None, AutonomyLevel::Assisted)
        .expect("read the agent")
        .expect("the agent is stored");
    (
        AgentRunSpec::from_resolved(&resolved),
        AgentAccess::of(&resolved),
    )
}

// GIVEN a package without an agent, one of whose tools is granted to an agent
//       created in the interface, the tools entered in the registry and
//       handed to the agent engine
// WHEN that agent runs a task whose model first calls a package tool it is not
//      granted, then the one it is granted
// THEN the first call is refused before running, the granted one asks for
//      approval like any tool, runs once accepted, and the task completes
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn interface_agent_runs_the_package_tool_it_is_granted() {
    // GIVEN
    let fx = stored_package();
    let (spec, access) = interface_agent(fx.dir.path());
    assert_eq!(spec.granted_tools, vec!["pkg:calc/add"]);
    let (event_sender, _rx) = EventBus::new();
    let mut events: broadcast::Receiver<RuntimeEvent> = event_sender.subscribe();
    let registry = ToolRegistryHandle::start();
    assert_eq!(fx.tools.register(&registry).await, 3);
    let mut backends: HashMap<String, Arc<dyn CompletionModel>> = HashMap::new();
    backends.insert(
        "scripted".into(),
        Arc::new(Scripted {
            turns: AtomicU32::new(0),
        }),
    );
    let chat = ChatSessionManagerHandle::spawn(
        &fx.path("chat.db"),
        Some(Arc::new(LlmRouter::with_backends(backends, "scripted"))),
        registry.clone(),
        None,
        event_sender.clone(),
        StepBudgetConfig::default(),
        None,
        AgentRegistry::spawn(event_sender.clone()),
        None,
        None,
        None,
        LoadingMode::Eager,
        20,
        None,
        false,
    )
    .expect("spawn the chat manager");
    chat.set_package_tools(Arc::new(fx.tools.clone())).await;

    // WHEN
    let (done_tx, done_rx) = oneshot::channel();
    chat.run_agent_task(AgentTaskRun {
        task_id: "task-calc".into(),
        run_id: RunId::new(),
        spec,
        access,
        message: "What is 2 + 3?".into(),
        authorized_tools: Vec::new(),
        cancel: CancellationToken::new(),
        plan_cache: None,
        done: done_tx,
        budget: None,
        depth: 1,
        unattended: false,
    })
    .await
    .expect("the run starts");
    let approval = loop {
        match events.recv().await {
            Ok(RuntimeEvent::ChatApprovalRequired {
                session_id,
                message_id,
                tool_call_id,
                tool_name,
                ..
            }) => break (session_id, message_id, tool_call_id, tool_name),
            Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => {}
            Err(broadcast::error::RecvError::Closed) => panic!("event bus closed"),
        }
    };
    let (session_id, message_id, call_id, tool) = approval;
    assert_eq!(tool, "pkg:calc/add");
    chat.resolve_tool(session_id, message_id, call_id, tool, ToolDecision::Accept)
        .await
        .expect("the approval was waiting");
    let response = done_rx
        .await
        .expect("the run reports")
        .expect("the run succeeds");

    // THEN
    let calls: Vec<(&str, &ToolCallStatus, Option<&str>)> = response
        .tool_calls
        .iter()
        .map(|c| (c.tool_name.as_str(), &c.status, c.output.as_deref()))
        .collect();
    let forbidden = calls
        .iter()
        .find(|(name, _, _)| *name == "pkg:calc/forbidden")
        .expect("the refused call is recorded");
    assert_ne!(*forbidden.1, ToolCallStatus::Executed, "{calls:?}");
    assert_eq!(fx.read("ran.txt"), None, "the tool not granted ran");
    let add = calls
        .iter()
        .find(|(name, _, _)| *name == "pkg:calc/add")
        .expect("the granted call is recorded");
    assert_eq!(*add.1, ToolCallStatus::Executed, "{calls:?}");
    let output: serde_json::Value =
        serde_json::from_str(add.2.expect("the call has an output")).expect("JSON output");
    assert_eq!(
        output,
        json!({"sum": 5, "caller": AGENT, "vendor": "package environment"})
    );
    assert_eq!(response.content, "The sum is 5.");
}
