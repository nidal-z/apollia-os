use std::collections::HashMap;

use apollia_core::{
    ConnectionReason, ConnectionReasonKey, ConnectionStateRecord, ConnectionStateStore,
    ConnectionStatus, McpHealth, RuntimeEvent,
};
use apollia_mcp::config::McpServerConfig;
use apollia_mcp::manager::McpServerStatus;

use super::*;
use crate::connections::{assemble, ConnectionSources, ConnectionState};

const NOW: &str = "2026-10-21T09:00:00+00:00";

fn kept(
    id: &str,
    state: ConnectionStatus,
    key: Option<ConnectionReasonKey>,
) -> ConnectionStateRecord {
    ConnectionStateRecord {
        id: id.to_owned(),
        state,
        reason: key.map(|k| ConnectionReason::new(k, None)),
        checked_at: NOW.to_owned(),
    }
}

fn server(name: &str) -> McpServerConfig {
    serde_json::from_value(serde_json::json!({
        "name": name,
        "command": "npx",
        "transport": "stdio",
    }))
    .unwrap()
}

fn live(name: &str, health: &McpHealth) -> McpServerStatus {
    serde_json::from_value(serde_json::json!({
        "name": name,
        "server_info": name,
        "tools_count": 0,
        "connected": true,
        "pid": null,
        "uptime_secs": null,
        "last_call_at": null,
        "error": null,
        "package": null,
        "transport": "stdio",
        "health": health,
    }))
    .unwrap()
}

fn system_db(dir: &tempfile::TempDir) -> ConnectionStateStore {
    ConnectionStateStore::open(&apollia_core::paths::DataFile::System.path(dir.path())).unwrap()
}

#[test]
fn test_a_session_reachable_but_unproven_is_not_working() {
    // GIVEN an MCP server whose handshake passed and whose operations nothing
    // has proven, with no state kept for it
    let health = McpHealth::Healthy { verified: false };
    let sources = ConnectionSources {
        mcp_configs: vec![server("notion")],
        mcp_statuses: vec![live("notion", &health)],
        ..ConnectionSources::default()
    };

    // WHEN the connections are listed
    let list = assemble(&sources);

    // THEN it needs a check, it does not read as working
    assert_eq!(list[0].status, ConnectionStatus::NeedsRepair);
    assert_eq!(
        list[0].reason.as_ref().map(|r| r.key),
        Some(ConnectionReasonKey::NotChecked)
    );
    assert_eq!(list[0].checked_at, None);
}

#[test]
fn test_an_unproven_session_keeps_the_working_state_found_before() {
    // GIVEN a server found working before, whose new session is unproven
    let stored = kept("mcp:notion", ConnectionStatus::Working, None);

    // WHEN its health is recorded
    let found = mcp_record(
        "mcp:notion",
        &McpHealth::Healthy { verified: false },
        NOW,
        Some(&stored),
    );

    // THEN nothing new is recorded, and a proven session records working
    assert_eq!(found, None);
    let proven = mcp_record(
        "mcp:notion",
        &McpHealth::Healthy { verified: true },
        NOW,
        None,
    );
    assert_eq!(proven.map(|r| r.state), Some(ConnectionStatus::Working));
}

#[test]
fn test_each_failing_health_reads_as_needing_repair_with_its_reason() {
    // GIVEN the three failing healths
    let cases = [
        (
            McpHealth::NeedsReauth {
                reason: "unauthorized".into(),
            },
            ConnectionReasonKey::SignInAgain,
        ),
        (
            McpHealth::Unavailable {
                reason: "process_exited".into(),
            },
            ConnectionReasonKey::Unreachable,
        ),
        (
            McpHealth::Degraded {
                category: apollia_core::ErrorCategory::ToolFailure,
                last_error: "object_not_found".into(),
                consecutive_failures: 1,
                since: NOW.into(),
            },
            ConnectionReasonKey::Failing,
        ),
    ];
    for (health, key) in cases {
        // WHEN each is recorded
        let found = mcp_record("mcp:x", &health, NOW, None).unwrap();
        // THEN it needs repair, with its reason, its detail and its date
        assert_eq!(found.state, ConnectionStatus::NeedsRepair);
        let reason = found.reason.unwrap();
        assert_eq!(reason.key, key);
        assert!(reason.detail.is_some());
        assert_eq!(found.checked_at, NOW);
    }
}

#[test]
fn test_a_failed_start_keeps_what_its_failure_said_instead_of_stopped() {
    // GIVEN a server with no live session, whose start failed on an expired
    // token, as the recorder kept it
    let sources = ConnectionSources {
        mcp_configs: vec![server("notion"), server("idle")],
        states: HashMap::from([(
            "mcp:notion".to_owned(),
            kept(
                "mcp:notion",
                ConnectionStatus::NeedsRepair,
                Some(ConnectionReasonKey::SignInAgain),
            ),
        )]),
        ..ConnectionSources::default()
    };

    // WHEN the connections are listed
    let list = assemble(&sources);

    // THEN it reads as needing to sign in again, dated, not as stopped; a
    // server nothing was ever found about stays stopped and needs a check
    let notion = list.iter().find(|c| c.id == "mcp:notion").unwrap();
    assert_eq!(notion.state, ConnectionState::NeedsReauth);
    assert_eq!(notion.status, ConnectionStatus::NeedsRepair);
    assert_eq!(
        notion.reason.as_ref().map(|r| r.key),
        Some(ConnectionReasonKey::SignInAgain)
    );
    assert_eq!(notion.checked_at.as_deref(), Some(NOW));
    let idle = list.iter().find(|c| c.id == "mcp:idle").unwrap();
    assert_eq!(idle.state, ConnectionState::Stopped);
    assert_eq!(
        idle.reason.as_ref().map(|r| r.key),
        Some(ConnectionReasonKey::NotChecked)
    );
}

#[test]
fn test_a_live_health_newer_than_the_kept_state_is_shown_undated() {
    // GIVEN a server kept working, whose live session now fails to sign in
    let states = HashMap::from([(
        "mcp:notion".to_owned(),
        kept("mcp:notion", ConnectionStatus::Working, None),
    )]);
    let health = McpHealth::NeedsReauth {
        reason: "unauthorized".into(),
    };

    // WHEN the state shown is read, then with the kept state equal to it
    let shown = shown_state("mcp:notion", &states, false, Some(&health));
    let same = shown_state(
        "mcp:notion",
        &states,
        false,
        Some(&McpHealth::Healthy { verified: true }),
    );

    // THEN the live health wins while unrecorded, and the kept state keeps
    // its date when they agree
    assert_eq!(shown.status, ConnectionStatus::NeedsRepair);
    assert_eq!(shown.checked_at, None);
    assert_eq!(same.status, ConnectionStatus::Working);
    assert_eq!(same.checked_at.as_deref(), Some(NOW));
}

#[tokio::test]
async fn test_the_recorder_keeps_and_announces_each_state_and_the_list_reads_it_after_a_restart() {
    // GIVEN a recorder on a bus, over an empty data directory
    let dir = tempfile::tempdir().unwrap();
    let (bus, _keep) = tokio::sync::broadcast::channel(64);
    let mut events = bus.subscribe();
    let recorder = spawn_state_recorder(bus.clone(), dir.path().to_path_buf());

    // WHEN a server is proven working, then a new session of it is unproven,
    // then its token is refused
    for health in [
        McpHealth::Healthy { verified: true },
        McpHealth::Healthy { verified: false },
        McpHealth::NeedsReauth {
            reason: "unauthorized".into(),
        },
    ] {
        bus.send(RuntimeEvent::McpServerHealthChanged {
            name: "notion".into(),
            health,
        })
        .unwrap();
    }
    let mut announced = Vec::new();
    while announced.len() < 2 {
        if let RuntimeEvent::ConnectionStateChanged {
            id, state, reason, ..
        } = events.recv().await.unwrap()
        {
            announced.push((id, state, reason.map(|r| r.key)));
        }
    }
    bus.send(RuntimeEvent::ShutdownRequested).unwrap();
    recorder.await.unwrap();

    // THEN two states were announced, in order, the unproven session adding
    // none: working, then needing to sign in again
    assert_eq!(
        announced,
        vec![
            ("mcp:notion".to_owned(), ConnectionStatus::Working, None),
            (
                "mcp:notion".to_owned(),
                ConnectionStatus::NeedsRepair,
                Some(ConnectionReasonKey::SignInAgain)
            ),
        ]
    );
    // AND the last one is what a list read from the file after a restart
    // shows, dated, with no live session
    let states = kept_states(dir.path());
    let list = assemble(&ConnectionSources {
        mcp_configs: vec![server("notion")],
        states,
        ..ConnectionSources::default()
    });
    assert_eq!(list[0].status, ConnectionStatus::NeedsRepair);
    assert!(list[0].checked_at.is_some());
    assert_eq!(system_db(&dir).list().unwrap().len(), 1);
}

const APPROVAL: &str =
    "https://login.microsoftonline.com/organizations/adminconsent?client_id=c4f95bc5";

#[test]
fn test_an_administrator_approval_is_kept_announced_and_listed_as_blocked_with_its_address() {
    // GIVEN an empty data directory, and a bus a reader listens on
    let dir = tempfile::tempdir().unwrap();
    let (bus, _keep) = tokio::sync::broadcast::channel(8);
    let mut events = bus.subscribe();

    // WHEN Microsoft answers that an administrator must approve the
    // application, before any account of it could connect
    record_admin_approval_required(
        dir.path(),
        &bus,
        ConnectorProvider::Microsoft,
        Some("consent_required: AADSTS90094".to_owned()),
        APPROVAL,
    )
    .unwrap();

    // THEN the state is announced, blocked, with the approval to send
    let Ok(RuntimeEvent::ConnectionStateChanged {
        id, state, reason, ..
    }) = events.try_recv()
    else {
        panic!("no state announced");
    };
    assert_eq!(id, "native:microsoft");
    assert_eq!(state, ConnectionStatus::Blocked);
    let reason = reason.unwrap();
    assert_eq!(reason.key, ConnectionReasonKey::AdminApprovalRequired);
    assert_eq!(reason.address.as_deref(), Some(APPROVAL));
    // AND the list, read from the file, shows a Microsoft connection blocked
    // by the organisation, dated, with no account and no tool
    let list = assemble(&ConnectionSources {
        states: kept_states(dir.path()),
        ..ConnectionSources::default()
    });
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, "native:microsoft");
    assert_eq!(list[0].label, "Microsoft");
    assert_eq!(list[0].account, None);
    assert!(list[0].tools.is_empty());
    assert_eq!(list[0].status, ConnectionStatus::Blocked);
    assert_eq!(
        list[0].reason.as_ref().and_then(|r| r.address.as_deref()),
        Some(APPROVAL)
    );
    assert!(list[0].checked_at.is_some());
}

#[test]
fn test_an_account_that_connects_lifts_the_block_of_its_provider() {
    // GIVEN Microsoft kept as blocked until an administrator approves
    let dir = tempfile::tempdir().unwrap();
    let (bus, _keep) = tokio::sync::broadcast::channel(8);
    record_admin_approval_required(
        dir.path(),
        &bus,
        ConnectorProvider::Microsoft,
        None,
        APPROVAL,
    )
    .unwrap();
    let mut events = bus.subscribe();

    // WHEN an account of it connects, once the administrator approved: first
    // listed alongside the kept block, then with the block lifted
    let accounts = vec![(
        ConnectorProvider::Microsoft,
        vec!["ada@example.com".to_owned()],
    )];
    let before_lift = assemble(&ConnectionSources {
        native_accounts: accounts.clone(),
        states: kept_states(dir.path()),
        ..ConnectionSources::default()
    });
    lift_provider_block(dir.path(), &bus, ConnectorProvider::Microsoft).unwrap();
    lift_provider_block(dir.path(), &bus, ConnectorProvider::Google).unwrap();

    // THEN the account is the only Microsoft connection listed, and the
    // provider reads working, announced once, Google untouched
    assert_eq!(before_lift.len(), 1);
    assert_eq!(before_lift[0].id, "native:microsoft:ada@example.com");
    let kept = system_db(&dir).get("native:microsoft").unwrap().unwrap();
    assert_eq!(kept.state, ConnectionStatus::Working);
    assert_eq!(kept.reason, None);
    assert!(matches!(
        events.try_recv(),
        Ok(RuntimeEvent::ConnectionStateChanged {
            state: ConnectionStatus::Working,
            ..
        })
    ));
    assert!(events.try_recv().is_err());
    assert!(system_db(&dir).get("native:google").unwrap().is_none());
    let after = assemble(&ConnectionSources {
        states: kept_states(dir.path()),
        ..ConnectionSources::default()
    });
    assert!(after.is_empty());
}
