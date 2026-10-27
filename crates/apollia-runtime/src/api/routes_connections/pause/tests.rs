use std::sync::Arc;

use apollia_core::ConnectionStatus;
use apollia_mcp::McpServerRepository;
use apollia_tools::{ActionFamily, AgentOrigin, AgentRepository, StoredAgent};
use axum::http::StatusCode;
use serde_json::json;

use super::*;
use crate::api::routes_llm::tests::{test_app_state_no_llm, MockBackend};
use crate::governance::{
    AccessCall, AccessDecision, DecisionReason, GovernanceContract, GovernanceHandle,
    GovernanceSources,
};

const NOTION_SEARCH: &str = "mcp:notion/search";

fn agent(name: &str, access: &[&str]) -> StoredAgent {
    StoredAgent {
        name: name.into(),
        origin: AgentOrigin::Package,
        package: Some("office".into()),
        version: "1.0.0".into(),
        definition: json!({"id": name, "name": name, "mission": "Work.", "defaults": {"access": access}}),
        enabled: true,
        installed_at: "2026-10-01T00:00:00Z".into(),
        updated_at: "2026-10-01T00:00:00Z".into(),
    }
}

/// A runtime over a data directory holding the MCP server `notion`, one
/// mailbox, an agent `scribe` holding a Notion tool, and a governance actor
/// over the directory's `governance.db`.
fn runtime(dir: &tempfile::TempDir) -> AppState<MockBackend> {
    AgentRepository::open(&apollia_core::paths::DataFile::Agents.path(dir.path()))
        .unwrap()
        .save_definition(&agent("scribe", &[NOTION_SEARCH, "file_read"]))
        .unwrap();
    std::fs::write(
        dir.path().join(apollia_connectors::imap::IMAP_INDEX_FILE),
        json!({"accounts": [
            {"email": "ada@example.org", "host": "imap.example.org", "port": 993, "username": "ada@example.org"},
        ]})
        .to_string(),
    )
    .unwrap();
    let mcp = McpServerRepository::open(&dir.path().join("mcp.db")).unwrap();
    mcp.save(&serde_json::from_value(json!({"name": "notion", "command": "npx"})).unwrap())
        .unwrap();
    let mut state = test_app_state_no_llm();
    state.data_dir = dir.path().to_path_buf();
    state.mcp_server_repo = Some(Arc::new(std::sync::Mutex::new(mcp)));
    state.governance = Some(GovernanceHandle::spawn(GovernanceSources {
        governance_db: Some(apollia_core::paths::DataFile::Governance.path(dir.path())),
        config_path: None,
        policy: Arc::new(apollia_permissions::PolicyVerdict::Absent),
        journal: None,
        events: None,
        forbidden_connections: Default::default(),
    }));
    state
}

fn search_call() -> AccessCall<'static> {
    AccessCall {
        agent_id: "scribe",
        access_id: NOTION_SEARCH,
        argument: None,
        reaches: &[],
        family: ActionFamily::Read,
        unattended: false,
        granted_for_run: false,
    }
}

fn enabled(state: &AppState<MockBackend>) -> Option<bool> {
    state
        .mcp_server_repo
        .as_ref()
        .unwrap()
        .lock()
        .unwrap()
        .is_server_enabled("notion")
        .unwrap()
}

// A server put in pause reads paused, is switched off in mcp.db, and its
// tools are refused at once; resumed, it is switched on and its tools run.
#[tokio::test]
async fn test_a_paused_server_is_switched_off_and_refused_then_resumed() {
    // GIVEN a runtime whose agent holds a tool of the Notion server
    let dir = tempfile::tempdir().unwrap();
    let state = runtime(&dir);
    let governance = state.governance.clone().unwrap();
    assert!(governance.decide(&search_call()).is_allowed());

    // WHEN the server is put in pause
    let paused = set_paused(&state, Vec::new(), "mcp:notion", true)
        .await
        .unwrap();

    // THEN it reads paused, its flag is off, and its tool is refused
    assert_eq!(paused.status, ConnectionStatus::Paused);
    assert_eq!(paused.reason, None);
    assert_eq!(enabled(&state), Some(false));
    assert_eq!(
        governance.decide(&search_call()),
        AccessDecision::Denied {
            reason: DecisionReason::Paused {
                connection_id: "mcp:notion".into()
            }
        }
    );

    // WHEN it is resumed
    let resumed = set_paused(&state, Vec::new(), "mcp:notion", false)
        .await
        .unwrap();

    // THEN it no longer reads paused, its flag is on, and its tool runs
    assert_ne!(resumed.status, ConnectionStatus::Paused);
    assert_eq!(enabled(&state), Some(true));
    assert!(governance.decide(&search_call()).is_allowed());
}

// The accesses of the agents do not change while a connection is in pause.
#[tokio::test]
async fn test_a_pause_keeps_the_accesses_of_the_agents() {
    // GIVEN an agent holding a tool of the Notion server
    let dir = tempfile::tempdir().unwrap();
    let state = runtime(&dir);
    let before = super::super::agents::agents_of(&state, Vec::new(), "mcp:notion")
        .await
        .unwrap();
    assert_eq!(before.agents.len(), 1);

    // WHEN the server is put in pause, then resumed
    set_paused(&state, Vec::new(), "mcp:notion", true)
        .await
        .unwrap();
    let during = super::super::agents::agents_of(&state, Vec::new(), "mcp:notion")
        .await
        .unwrap();
    set_paused(&state, Vec::new(), "mcp:notion", false)
        .await
        .unwrap();
    let after = super::super::agents::agents_of(&state, Vec::new(), "mcp:notion")
        .await
        .unwrap();

    // THEN the agent holds the same tools throughout, and no override was
    // written for it
    assert_eq!(during, before);
    assert_eq!(after, before);
    let overrides = AgentRepository::open(&apollia_core::paths::DataFile::Agents.path(dir.path()))
        .unwrap()
        .overrides("scribe")
        .unwrap();
    assert!(overrides.is_empty());
}

// A mailbox put in pause reads paused in the list.
#[tokio::test]
async fn test_a_paused_mailbox_reads_paused() {
    // GIVEN a runtime with one mailbox
    let dir = tempfile::tempdir().unwrap();
    let state = runtime(&dir);

    // WHEN it is put in pause
    let paused = set_paused(&state, Vec::new(), "imap:ada@example.org", true)
        .await
        .unwrap();

    // THEN it reads paused
    assert_eq!(paused.status, ConnectionStatus::Paused);
}

// A connection that does not exist is not paused, and nothing is written.
#[tokio::test]
async fn test_an_unknown_connection_is_not_found() {
    // GIVEN a runtime
    let dir = tempfile::tempdir().unwrap();
    let state = runtime(&dir);

    // WHEN an unknown connection is put in pause
    let refused = set_paused(&state, Vec::new(), "mcp:ghost", true).await;

    // THEN it is not found and governance holds no pause
    assert_eq!(refused.unwrap_err().0, StatusCode::NOT_FOUND);
    assert!(state.governance.unwrap().state().paused.is_empty());
}

// Without governance, nothing is paused.
#[tokio::test]
async fn test_without_governance_nothing_is_paused() {
    // GIVEN a runtime without governance
    let dir = tempfile::tempdir().unwrap();
    let mut state = runtime(&dir);
    state.governance = None;

    // WHEN the server is put in pause
    let refused = set_paused(&state, Vec::new(), "mcp:notion", true).await;

    // THEN it is refused as unavailable and the server stays on
    assert_eq!(refused.unwrap_err().0, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(enabled(&state), Some(true));
}

// A resumed connection is announced in its new state, as its pause was, so
// a page that shows it changes without reading the list again.
#[tokio::test]
async fn test_a_resume_is_announced_as_the_pause_is() {
    // GIVEN a mailbox in pause, and a reader of the bus
    let dir = tempfile::tempdir().unwrap();
    let state = runtime(&dir);
    set_paused(&state, Vec::new(), "imap:ada@example.org", true)
        .await
        .unwrap();
    let mut events = state.event_sender.subscribe();

    // WHEN it is resumed
    let resumed = set_paused(&state, Vec::new(), "imap:ada@example.org", false)
        .await
        .unwrap();

    // THEN the bus carries its new state, the one the list shows
    let announced = loop {
        match events.try_recv() {
            Ok(apollia_core::RuntimeEvent::ConnectionStateChanged {
                id, state, reason, ..
            }) => break Some((id, state, reason)),
            Ok(_) => continue,
            Err(_) => break None,
        }
    };
    assert_eq!(
        announced,
        Some((
            "imap:ada@example.org".to_string(),
            resumed.status,
            resumed.reason.clone()
        ))
    );
    assert_ne!(resumed.status, ConnectionStatus::Paused);
}
