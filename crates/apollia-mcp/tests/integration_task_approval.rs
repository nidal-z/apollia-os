#![allow(clippy::unwrap_used, clippy::expect_used)]
//! A server declared `requires_approval` gates its calls on the task path,
//! against a real stdio MCP server.
//!
//! The flag was documented as gating every call and gated none. These tests
//! run the production assembly of the task path, `build_task_tool_executors`,
//! and prove the three outcomes: no approval pauses, an approval of that very
//! call runs it once, a refusal of it is a typed error and the call does not
//! run.

use std::collections::HashMap;

use apollia_core::InputResponseData;
use apollia_mcp::config::McpServerConfig;
use apollia_mcp::executor::{build_agent_tool_executors, build_task_tool_executors};
use apollia_mcp::manager::McpClientManagerHandle;
use apollia_mcp::session::LoadingMode;
use apollia_mcp::task_approval::{approval_payload, TaskApproval};
use apollia_tools::executor::{ToolDispatcher, ToolExecutionError};
use apollia_tools::ToolRegistryHandle;
use serde_json::json;

fn gated_server(name: &str) -> McpServerConfig {
    McpServerConfig {
        format_version: 1,
        name: name.to_string(),
        command: "python3".to_string(),
        args: vec![format!(
            "{}/tests/mock_mcp_server.py",
            env!("CARGO_MANIFEST_DIR")
        )],
        env: HashMap::new(),
        transport: "stdio".to_string(),
        url: None,
        requires_approval: true,
        init_timeout_secs: 10,
        call_timeout_secs: 10,
        max_response_bytes: 8 * 1024 * 1024,
        max_tools: 256,
        tags: vec![],
    }
}

fn resumed(approved: bool, payload: serde_json::Value) -> TaskApproval {
    TaskApproval::new(Some(InputResponseData {
        approved,
        reason: Some("not this one".into()),
        context: json!({}),
        responded_at: "2026-09-16T10:00:00Z".into(),
        answer: None,
        payload: Some(payload),
    }))
}

async fn start(registry: &ToolRegistryHandle) -> McpClientManagerHandle {
    McpClientManagerHandle::start(
        vec![gated_server("calc")],
        registry,
        None,
        None,
        LoadingMode::Eager,
    )
    .await
    .expect("manager start failed")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_gated_call_pauses_then_runs_once_approved() {
    // GIVEN a server declared requires_approval, on the task path
    let registry = ToolRegistryHandle::start();
    let manager = start(&registry).await;
    let input = json!({"a": 2, "b": 3});

    // WHEN a first run calls it
    let first = ToolDispatcher::new(
        build_task_tool_executors(&manager, TaskApproval::new(None), Vec::new()).await,
    );
    let err = first
        .dispatch("mcp:calc/add", input.clone())
        .await
        .expect_err("a gated call must not run unapproved");

    // THEN it asks for an approval naming the gesture and its arguments
    let payload = match err {
        ToolExecutionError::ApprovalRequired {
            gesture, payload, ..
        } => {
            assert_eq!(gesture, "calc/add");
            assert_eq!(payload["geste"], "calc/add");
            assert_eq!(payload["detail"], json!(["a: 2", "b: 3"]));
            payload
        }
        other => panic!("expected ApprovalRequired, got {other:?}"),
    };

    // WHEN the task resumes with that call approved
    let resumed_run = ToolDispatcher::new(
        build_task_tool_executors(&manager, resumed(true, payload), Vec::new()).await,
    );
    let out = resumed_run
        .dispatch("mcp:calc/add", input)
        .await
        .expect("the approved call runs");

    // THEN the real server executed it
    assert_eq!(out, json!({"content": "5"}));

    manager.shutdown().await;
    registry.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_agent_requiring_a_tool_of_a_gated_server_resolves() {
    // GIVEN a server declared requires_approval, and a manifest requiring one of
    // its tools without dangerous_tools_allowed, the shape the SDK produces
    let registry = ToolRegistryHandle::start();
    let manager = start(&registry).await;
    let manifest: apollia_core::AgentManifest = serde_json::from_value(json!({
        "name": "espace-writer",
        "version": "1.0.0",
        "description": "writes through a gated server",
        "tools_required": ["mcp:calc/add"],
    }))
    .expect("a minimal manifest deserializes");

    // WHEN the install-time resolution runs
    let report = apollia_tools::resolve(&manifest, &registry, &Default::default()).await;

    // THEN it resolves: the approval is held per call by the executor, not by a
    // refusal to install
    let report = report.expect("a gated tool must not block the install");
    assert_eq!(report.resolved, vec!["mcp:calc/add".to_string()]);

    manager.shutdown().await;
    registry.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_declined_call_is_a_typed_refusal_and_does_not_run() {
    // GIVEN a task resumed with the gated call declined
    let registry = ToolRegistryHandle::start();
    let manager = start(&registry).await;
    let input = json!({"a": 2, "b": 3});
    let dispatcher = ToolDispatcher::new(
        build_task_tool_executors(
            &manager,
            resumed(false, approval_payload("calc/add", &input)),
            Vec::new(),
        )
        .await,
    );

    // WHEN the resumed run makes the call again
    let err = dispatcher
        .dispatch("mcp:calc/add", input)
        .await
        .expect_err("a declined call must not run");

    // THEN it is refused with the typed error and the operator's reason
    match err {
        ToolExecutionError::ApprovalDenied { gesture, reason } => {
            assert_eq!(gesture, "calc/add");
            assert_eq!(reason.as_deref(), Some("not this one"));
        }
        other => panic!("expected ApprovalDenied, got {other:?}"),
    }

    manager.shutdown().await;
    registry.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_chat_assembly_stays_ungated() {
    // GIVEN the same gated server, assembled the way the chat path assembles it
    let registry = ToolRegistryHandle::start();
    let manager = start(&registry).await;
    let dispatcher = ToolDispatcher::new(build_agent_tool_executors(&manager).await);

    // WHEN a call is made
    let out = dispatcher
        .dispatch("mcp:calc/add", json!({"a": 1, "b": 1}))
        .await;

    // THEN it runs as before: the chat path keeps its own approval flow, and
    // has no task pause an approval could become
    assert_eq!(out.expect("ungated"), json!({"content": "2"}));

    manager.shutdown().await;
    registry.shutdown().await;
}
