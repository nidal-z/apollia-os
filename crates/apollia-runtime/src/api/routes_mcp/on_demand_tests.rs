//! The routes over a manager that starts its servers on demand: an addition
//! starts the server once, which tries it and fills the index, and a server
//! that sleeps is listed, dormant, with the tools it last published.

use std::sync::Arc;

use apollia_mcp::manager::{McpClientManagerHandle, OnDemand};
use apollia_mcp::session::LoadingMode;
use apollia_mcp::tool_store::ToolIndexStore;
use apollia_mcp::McpServerRepository;
use apollia_tools::ToolRegistryHandle;

use super::governance_tests::{harness, mock_server};
use super::{add_server_to, list_servers};

#[tokio::test]
async fn test_an_addition_starts_the_server_once_and_fills_the_index() {
    // GIVEN the route's state over a deferred manager that keeps tools in
    //       mcp.db and starts no server by itself
    let mut h = harness(Arc::new(crate::governance::NoPolicyAdmission)).await;
    let db = h.dir.path().join("mcp.db");
    let registry = ToolRegistryHandle::start();
    let manager = McpClientManagerHandle::start_keeping_tools(
        vec![],
        &registry,
        None,
        LoadingMode::Deferred,
        OnDemand {
            tool_store: ToolIndexStore::new(&db),
            idle_stop: Some(std::time::Duration::from_secs(900)),
        },
    )
    .await
    .expect("manager");
    h.state.mcp_handle = Some(manager.clone());

    // WHEN a server is added through the route
    let status = add_server_to(&h.state, mock_server("calc"))
        .await
        .expect("added");

    // THEN its handshake answered, its session runs, the index lists its
    //      tools and mcp.db keeps them
    assert_eq!(status.name, "calc");
    assert_eq!(status.tools_count, 2);
    let statuses = manager.status().await;
    assert_eq!(statuses.len(), 1);
    let mut indexed: Vec<String> = manager
        .get_tool_index()
        .await
        .into_iter()
        .map(|e| e.tool_name)
        .collect();
    indexed.sort();
    assert_eq!(indexed, vec!["add".to_string(), "echo".to_string()]);
    let kept = McpServerRepository::open(&db)
        .expect("repo")
        .stored_tools()
        .expect("kept");
    assert_eq!(kept["calc"].len(), 2);
    manager.shutdown().await;
    h.manager.shutdown().await;
}

#[tokio::test]
async fn test_an_addition_that_fails_its_trial_keeps_nothing() {
    // GIVEN the same state
    let mut h = harness(Arc::new(crate::governance::NoPolicyAdmission)).await;
    let db = h.dir.path().join("mcp.db");
    let registry = ToolRegistryHandle::start();
    let manager = McpClientManagerHandle::start_keeping_tools(
        vec![],
        &registry,
        None,
        LoadingMode::Deferred,
        OnDemand {
            tool_store: ToolIndexStore::new(&db),
            idle_stop: None,
        },
    )
    .await
    .expect("manager");
    h.state.mcp_handle = Some(manager.clone());

    // WHEN a server whose process cannot start is added
    let mut broken = mock_server("broken");
    broken.command = "/nonexistent/apollia-mcp-server".to_string();
    broken.args = vec![];
    let added = add_server_to(&h.state, broken).await;

    // THEN the route refuses it, and neither mcp.db nor the index holds it
    assert!(added.is_err());
    assert!(manager.status().await.is_empty());
    assert!(manager.get_tool_index().await.is_empty());
    let repo = McpServerRepository::open(&db).expect("repo");
    assert!(repo.list().expect("list").is_empty());
    manager.shutdown().await;
    h.manager.shutdown().await;
}

#[tokio::test]
async fn test_the_list_of_servers_shows_a_dormant_server_with_its_tools() {
    // GIVEN a server added once, so mcp.db keeps its tools, then a new
    //       manager over it, which starts no server by itself
    let mut h = harness(Arc::new(crate::governance::NoPolicyAdmission)).await;
    let db = h.dir.path().join("mcp.db");
    let on_demand = || OnDemand {
        tool_store: ToolIndexStore::new(&db),
        idle_stop: None,
    };
    let registry = ToolRegistryHandle::start();
    let first = McpClientManagerHandle::start_keeping_tools(
        vec![],
        &registry,
        None,
        LoadingMode::Deferred,
        on_demand(),
    )
    .await
    .expect("manager");
    h.state.mcp_handle = Some(first.clone());
    add_server_to(&h.state, mock_server("calc"))
        .await
        .expect("added");
    first.shutdown().await;
    let manager = McpClientManagerHandle::start_keeping_tools(
        vec![mock_server("calc")],
        &registry,
        None,
        LoadingMode::Deferred,
        on_demand(),
    )
    .await
    .expect("manager");
    h.state.mcp_handle = Some(manager.clone());

    // WHEN `GET /api/v1/mcp/servers` is read
    let axum::Json(listed) = list_servers(axum::extract::State(h.state.clone())).await;

    // THEN the server is in it, dormant and not connected, with the tools
    //      it last published, and no session started for the listing
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].name, "calc");
    assert!(listed[0].dormant);
    assert!(!listed[0].connected);
    assert_eq!(listed[0].tools_count, 2);
    let json = serde_json::to_value(&listed[0]).expect("json");
    assert_eq!(json["dormant"], true);
    assert_eq!(json["health"]["state"], "healthy");
    // AND its detail answers from what it kept, the way the page opens it
    let detail = manager.server_detail("calc").await.expect("detail");
    assert_eq!(detail.tools.len(), 2);
    assert!(manager.status().await.iter().all(|s| !s.connected));
    manager.shutdown().await;
    h.manager.shutdown().await;
}
