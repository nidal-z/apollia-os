use std::collections::HashMap;
use std::sync::Arc;

use apollia_core::{ConnectionStateRecord, ConnectionStatus};
use apollia_tools::{AgentOrigin, AgentRepository, StoredAgent};
use serde_json::json;

use super::*;
use crate::api::routes_mcp::governance_tests::{harness, mock_server, persisted, Harness};
use crate::api::test_support::MemorySecrets;
use crate::audit_journal::JournalEntryKind;
use crate::governance::governance_run_id;

const SEARCH: &str = "mcp:notion/search";

fn package_agent(name: &str, access: &[&str]) -> StoredAgent {
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

/// A runtime with a live MCP manager and a journal; the server `notion`
/// stored and never started, naming the secret `TOKEN`, with files installed
/// for it, a sign-in token, the secret, and a state kept; the agent `triage`
/// declaring its search and the mailbox search; two mailboxes with their
/// passwords.
async fn runtime() -> (Harness, MemorySecrets) {
    let mut h = harness(Arc::new(crate::governance::NoPolicyAdmission)).await;
    let dir = h.dir.path().to_path_buf();
    let mut notion = mock_server("notion");
    notion.env = HashMap::from([("API_TOKEN".into(), "${APOLLIA_SECRET:TOKEN}".into())]);
    h.state
        .mcp_server_repo
        .as_ref()
        .unwrap()
        .lock()
        .unwrap()
        .save(&notion)
        .unwrap();
    std::fs::create_dir_all(installed_files(&dir, "notion").join("node_modules")).unwrap();
    let secrets = MemorySecrets::default();
    secrets.set(MCP_OAUTH_SERVICE, "notion", "{}").unwrap();
    secrets
        .set(MCP_STATIC_SECRET_SERVICE, "notion:TOKEN", "tok")
        .unwrap();
    for email in ["ada@example.org", "support@example.org"] {
        secrets.set(IMAP_SECRET_SERVICE, email, "pw").unwrap();
    }
    std::fs::write(
        dir.join(IMAP_INDEX_FILE),
        json!({"accounts": [
            {"email": "ada@example.org", "host": "imap.example.org", "port": 993, "username": "ada@example.org"},
            {"email": "support@example.org", "host": "imap.example.org", "port": 993, "username": "support@example.org"},
        ]})
        .to_string(),
    )
    .unwrap();
    ConnectionStateStore::open(&apollia_core::paths::DataFile::System.path(&dir))
        .unwrap()
        .put(&ConnectionStateRecord {
            id: "mcp:notion".into(),
            state: ConnectionStatus::NeedsRepair,
            reason: None,
            checked_at: "2026-10-21T09:00:00Z".into(),
        })
        .unwrap();
    AgentRepository::open(&apollia_core::paths::DataFile::Agents.path(&dir))
        .unwrap()
        .save_definition(&package_agent(
            "triage",
            &[SEARCH, "imap.search", "file_read"],
        ))
        .unwrap();
    h.state.secret_store = secrets.opener();
    h.state.governance = Some(crate::governance::GovernanceHandle::detached(
        crate::governance::GovernanceState::default(),
        Some(h.journal.clone()),
    ));
    (h, secrets)
}

fn kinds(removal: &ConnectionRemoval) -> Vec<RemovedKind> {
    removal.removed.iter().map(|i| i.kind).collect()
}

fn triage_access(h: &Harness) -> Vec<String> {
    let repo = definitions::open_agents(&h.state).unwrap();
    definitions::resolved(&h.state, &repo, "triage")
        .unwrap()
        .access
}

// GIVEN a stored server that never started, with its files, token, secret,
//       state, and an agent declaring one of its tools
// WHEN its removal is asked as a dry run
// THEN the answer lists each of them, and nothing is erased
#[tokio::test]
async fn test_a_dry_run_lists_everything_and_erases_nothing() {
    // GIVEN
    let (h, secrets) = runtime().await;

    // WHEN
    let removal = remove(&h.state, Vec::new(), "mcp:notion", true)
        .await
        .unwrap();

    // THEN
    assert!(removal.dry_run);
    assert_eq!(
        kinds(&removal),
        [
            RemovedKind::Configuration,
            RemovedKind::InstalledFiles,
            RemovedKind::Token,
            RemovedKind::Secret,
            RemovedKind::AgentAccess,
            RemovedKind::State,
        ]
    );
    assert_eq!(removal.removed[3].name, "TOKEN");
    assert_eq!(removal.removed[4].agent.as_deref(), Some("triage"));
    assert_eq!(removal.removed[4].name, SEARCH);
    assert_eq!(persisted(&h).len(), 1);
    assert!(installed_files(h.dir.path(), "notion").exists());
    assert!(secrets.value(MCP_OAUTH_SERVICE, "notion").is_some());
    assert!(secrets
        .value(MCP_STATIC_SECRET_SERVICE, "notion:TOKEN")
        .is_some());
    assert!(has_state(h.dir.path(), "mcp:notion"));
    assert!(triage_access(&h).contains(&SEARCH.to_owned()));
    h.manager.shutdown().await;
}

// GIVEN the same server
// WHEN it is removed, then a server is added again under the same name
// THEN its configuration, files, token, secret and state are gone, the
//      agent's access is revoked and journaled, the removal is announced,
//      and the server added again does not give the agent its tool back
#[tokio::test]
async fn test_a_removal_erases_what_it_lists_and_revokes_the_agents() {
    // GIVEN
    let (h, secrets) = runtime().await;
    let mut events = h.state.event_sender.subscribe();

    // WHEN
    let removal = remove(&h.state, Vec::new(), "mcp:notion", false)
        .await
        .unwrap();
    h.state
        .mcp_server_repo
        .as_ref()
        .unwrap()
        .lock()
        .unwrap()
        .save(&mock_server("notion"))
        .unwrap();

    // THEN
    assert!(!removal.dry_run);
    assert!(removal.left.is_empty(), "{:?}", removal.left);
    assert_eq!(removal.removed.len(), 6);
    assert!(!installed_files(h.dir.path(), "notion").exists());
    assert!(secrets.value(MCP_OAUTH_SERVICE, "notion").is_none());
    assert!(secrets
        .value(MCP_STATIC_SECRET_SERVICE, "notion:TOKEN")
        .is_none());
    assert!(!has_state(h.dir.path(), "mcp:notion"));
    let access = triage_access(&h);
    assert!(!access.contains(&SEARCH.to_owned()));
    assert!(access.contains(&"imap.search".to_owned()));
    let entries = h.journal.query_run(&governance_run_id("triage")).await;
    assert!(
        entries
            .iter()
            .any(|e| e.kind == JournalEntryKind::AccessChanged
                && e.payload["access_id"] == SEARCH
                && e.payload["change"] == "revoked"),
        "{entries:?}"
    );
    let announced = std::iter::from_fn(|| events.try_recv().ok())
        .any(|e| matches!(e, RuntimeEvent::ConnectionRemoved { ref id } if id == "mcp:notion"));
    assert!(announced);
    let again = super::super::agents::agents_of(&h.state, Vec::new(), "mcp:notion")
        .await
        .unwrap();
    assert!(again.agents.iter().all(|a| a.id != "triage"));
    h.manager.shutdown().await;
}

// GIVEN two mailboxes with their passwords, and an agent holding the
//       mailbox search
// WHEN one mailbox is removed, then the other
// THEN each leaves the list with its password; the search is kept while a
//      mailbox is left and revoked with the last one
#[tokio::test]
async fn test_a_mailbox_is_removed_and_its_tools_go_with_the_last() {
    // GIVEN
    let (h, secrets) = runtime().await;

    // WHEN
    let first = remove(&h.state, Vec::new(), "imap:ada@example.org", false)
        .await
        .unwrap();
    let kept = triage_access(&h);
    let last = remove(&h.state, Vec::new(), "imap:support@example.org", false)
        .await
        .unwrap();

    // THEN
    assert_eq!(
        kinds(&first),
        [RemovedKind::Configuration, RemovedKind::Secret]
    );
    assert!(kept.contains(&"imap.search".to_owned()));
    assert_eq!(
        kinds(&last),
        [
            RemovedKind::Configuration,
            RemovedKind::Secret,
            RemovedKind::AgentAccess
        ]
    );
    assert!(ImapAccountStore::accounts_in(h.dir.path())
        .unwrap()
        .is_empty());
    assert!(secrets
        .value(IMAP_SECRET_SERVICE, "ada@example.org")
        .is_none());
    assert!(secrets
        .value(IMAP_SECRET_SERVICE, "support@example.org")
        .is_none());
    assert!(!triage_access(&h).contains(&"imap.search".to_owned()));
    h.manager.shutdown().await;
}

// GIVEN no connection under an identifier
// WHEN its removal is asked
// THEN it answers 404
#[tokio::test]
async fn test_an_unknown_connection_is_not_found() {
    // GIVEN
    let (h, _) = runtime().await;

    // WHEN
    let answer = remove(&h.state, Vec::new(), "mcp:absent", false).await;

    // THEN
    assert_eq!(answer.unwrap_err().0, StatusCode::NOT_FOUND);
    h.manager.shutdown().await;
}
