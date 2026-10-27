//! The starts a resource listing makes, driven on the actor itself so each
//! start is awaited where it reports, without a real `sleep`.

use std::collections::HashMap;
use std::path::Path;

use apollia_tools::ToolRegistryHandle;
use tempfile::TempDir;
use tokio::sync::oneshot;

use crate::config::McpServerConfig;
use crate::manager::on_demand::Dormant;
use crate::manager::McpClientManager;
use crate::server_repository::{StoredResource, StoredTool};
use crate::session::LoadingMode;
use crate::tool_store::ToolIndexStore;
use crate::McpServerRepository;

fn config(name: &str, command: &str, args: Vec<String>) -> McpServerConfig {
    McpServerConfig {
        format_version: 1,
        name: name.to_string(),
        command: command.to_string(),
        args,
        env: HashMap::new(),
        transport: "stdio".to_string(),
        url: None,
        init_timeout_secs: 10,
        call_timeout_secs: 10,
        max_response_bytes: 8 * 1024 * 1024,
        max_tools: 256,
        tags: vec![],
        tool_families: Default::default(),
        operator_families: Default::default(),
        origin: None,
    }
}

/// A server out of reach: each start appends a line to `starts`, then the
/// process exits before any handshake.
fn unreachable_config(starts: &Path) -> McpServerConfig {
    config(
        "down",
        "sh",
        vec![
            "-c".to_string(),
            format!("echo start >> '{}'; exit 1", starts.display()),
        ],
    )
}

fn echo_tool() -> StoredTool {
    StoredTool {
        name: "echo".to_string(),
        title: None,
        description: Some("Echo the input".to_string()),
        input_schema: serde_json::json!({"type": "object"}),
        annotations: None,
        fetched_at: "2026-10-27T08:00:00Z".to_string(),
    }
}

fn starts_of(path: &Path) -> usize {
    std::fs::read_to_string(path)
        .map(|s| s.lines().count())
        .unwrap_or(0)
}

impl McpClientManager {
    /// Let the start in flight for `name`, if any, report and be handled.
    async fn settle_start(&mut self, name: &str) {
        if self.dormant.get(name).is_some_and(|d| d.starting) {
            let cmd = self.rx.recv().await.expect("the start reports");
            assert!(self.handle(cmd).await);
        }
    }
}

#[tokio::test]
async fn test_a_server_out_of_reach_is_started_once_for_five_listings() {
    // GIVEN a dormant server, its tools kept but never listed, whose every
    //       start fails at once and is counted
    let dir = TempDir::new().unwrap();
    let starts = dir.path().join("starts");
    let registry = ToolRegistryHandle::start();
    let (mut actor, _handle) = McpClientManager::new(&registry, None, LoadingMode::Deferred, None);
    actor.dormant.insert(
        "down".to_string(),
        Dormant::new(unreachable_config(&starts), vec![echo_tool()]),
    );

    // WHEN the resources are listed five times, as five openings of the
    //      @ picker would, each start awaited until it reports
    for _ in 0..5 {
        assert!(actor.handle_list_resources().await.is_empty());
        actor.settle_start("down").await;
    }

    // THEN only the first listing started it
    assert_eq!(starts_of(&starts), 1);

    // WHEN one of its resources is read explicitly
    let (reply, answer) = oneshot::channel();
    actor
        .handle_read_resource(Some("down".to_string()), "memo://x".to_string(), reply)
        .await;
    actor.settle_start("down").await;

    // THEN the read still starts it, and answers the failure
    assert!(answer.await.expect("answered").is_err());
    assert_eq!(starts_of(&starts), 2);

    // WHEN it is checked explicitly, then listed again
    assert!(actor.start_dormant_now("down").await.is_err());
    assert_eq!(starts_of(&starts), 3);
    assert!(actor.handle_list_resources().await.is_empty());
    actor.settle_start("down").await;

    // THEN the listing after the check tries once more
    assert_eq!(starts_of(&starts), 4);
}

#[tokio::test]
async fn test_the_list_answered_at_a_start_on_demand_is_kept_at_once() {
    // GIVEN a dormant server never listed, which offers two resources, and a
    //       manager that keeps what its servers answer in mcp.db
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("mcp.db");
    let registry = ToolRegistryHandle::start();
    let (mut actor, _handle) = McpClientManager::new(
        &registry,
        None,
        LoadingMode::Deferred,
        Some(ToolIndexStore::new(&db)),
    );
    let docs = config(
        "docs",
        "python3",
        vec![format!(
            "{}/tests/mock_mcp_server_resources.py",
            env!("CARGO_MANIFEST_DIR")
        )],
    );
    actor
        .dormant
        .insert("docs".to_string(), Dormant::new(docs, vec![echo_tool()]));

    // WHEN a listing starts it and the start reports, nothing listing again
    assert!(actor.handle_list_resources().await.is_empty());
    actor.settle_start("docs").await;

    // THEN the list its start answered is kept, in the actor and in mcp.db
    assert!(actor.sessions.contains_key("docs"));
    let uris = |list: &[StoredResource]| list.iter().map(|r| r.uri.clone()).collect::<Vec<_>>();
    let expected = vec!["memo://alpha".to_string(), "memo://beta".to_string()];
    assert_eq!(
        actor.kept_resources.get("docs").map(|l| uris(l)),
        Some(expected.clone())
    );
    let stored = McpServerRepository::open(&db)
        .unwrap()
        .stored_resources()
        .unwrap();
    assert_eq!(stored.get("docs").map(|l| uris(l)), Some(expected));
    if let Some(session) = actor.sessions.remove("docs") {
        session.shutdown().await;
    }
}
