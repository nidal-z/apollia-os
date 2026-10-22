use std::sync::Arc;

use apollia_auth::ConnectorProvider;
use apollia_mcp::McpServerRepository;
use apollia_tools::{ActionFamily, AgentOrigin, AgentRepository, StoredAgent, ToolRegistryHandle};
use axum::http::StatusCode;
use serde_json::{json, Value};

use super::*;
use crate::api::routes_llm::tests::{test_app_state_no_llm, MockBackend};
use crate::audit_journal::{AuditJournalHandle, JournalEntryKind};
use crate::governance::governance_run_id;

const KEY: &[u8] = b"connection-agents-journal-key-01";
const GMAIL_SEND: &str = "conn:google:ada@example.com/gmail.send";

fn stored(name: &str, origin: AgentOrigin, definition: Value) -> StoredAgent {
    StoredAgent {
        name: name.into(),
        origin,
        package: (origin == AgentOrigin::Package).then(|| "office".into()),
        version: "1.0.0".into(),
        definition,
        enabled: true,
        installed_at: "2026-10-01T00:00:00Z".into(),
        updated_at: "2026-10-01T00:00:00Z".into(),
    }
}

fn package_agent(name: &str, access: &[&str]) -> StoredAgent {
    stored(
        name,
        AgentOrigin::Package,
        json!({"id": name, "name": name, "mission": "Work.", "defaults": {"access": access}}),
    )
}

/// One Google account, connected.
fn google() -> Vec<NativeAccounts> {
    vec![(
        ConnectorProvider::Google,
        vec!["ada@example.com".to_owned()],
    )]
}

/// A runtime over a data directory holding two mailboxes, the MCP server
/// `notion` with a read tool registered, and three agents: `triage` holding
/// the mailbox search, `mailer` holding the bare `gmail.send` a package
/// declares, `writer` created without code and holding the default access.
async fn runtime(
    dir: &tempfile::TempDir,
    journal: bool,
) -> (AppState<MockBackend>, Option<AuditJournalHandle>) {
    let agents =
        AgentRepository::open(&apollia_core::paths::DataFile::Agents.path(dir.path())).unwrap();
    agents
        .save_definition(&package_agent("triage", &["imap.search", "file_read"]))
        .unwrap();
    agents
        .save_definition(&package_agent("mailer", &["gmail.send"]))
        .unwrap();
    agents
        .save_definition(&stored(
            "writer",
            AgentOrigin::Interface,
            json!({"id": "writer", "name": "Writer", "mission": "Write."}),
        ))
        .unwrap();
    std::fs::write(
        dir.path()
            .join(apollia_connectors::imap::IMAP_INDEX_FILE),
        json!({"accounts": [
            {"email": "ada@example.org", "host": "imap.example.org", "port": 993, "username": "ada@example.org"},
            {"email": "support@example.org", "host": "imap.example.org", "port": 993, "username": "support@example.org"},
        ]})
        .to_string(),
    )
    .unwrap();
    let mcp = McpServerRepository::open(&dir.path().join("mcp.db")).unwrap();
    mcp.save(&serde_json::from_value(json!({"name": "notion", "command": "npx"})).unwrap())
        .unwrap();
    let registry = ToolRegistryHandle::start();
    let mut search = crate::connectors_bridge::google_tool_descriptors()[0].clone();
    search.name = "mcp:notion/search".to_owned();
    search.family = ActionFamily::Read;
    registry.register(search).await.unwrap();

    let mut state = test_app_state_no_llm();
    state.data_dir = dir.path().to_path_buf();
    state.mcp_server_repo = Some(Arc::new(std::sync::Mutex::new(mcp)));
    state.tool_registry_handle = Some(registry);
    let handle = if journal {
        let handle =
            AuditJournalHandle::open_with_key_bytes(&dir.path().join("journal.db"), KEY.to_vec())
                .await
                .unwrap();
        state.audit_journal = Some(handle.clone());
        state.governance = Some(crate::governance::GovernanceHandle::detached(
            crate::governance::GovernanceState::default(),
            Some(handle.clone()),
        ));
        Some(handle)
    } else {
        None
    };
    (state, handle)
}

fn ids(answer: &ConnectionAgents) -> Vec<&str> {
    answer.agents.iter().map(|a| a.id.as_str()).collect()
}

fn overrides(dir: &tempfile::TempDir, agent: &str) -> usize {
    AgentRepository::open(&apollia_core::paths::DataFile::Agents.path(dir.path()))
        .unwrap()
        .overrides(agent)
        .unwrap()
        .len()
}

// GIVEN two mailboxes, a Google account, an MCP server, and agents holding
//       the mailbox search, the bare gmail.send of a package, and the default
//       access
// WHEN the agents of each connection are read, and of an unknown one
// THEN each mailbox lists the agent of the mailbox tools and names the other,
//      the account lists the agent of the bare operation resolved to it, the
//      server lists the agent on the default access as such, and the unknown
//      connection answers 404
#[tokio::test]
async fn test_agents_of_each_connection_are_resolved_by_the_runtime() {
    // GIVEN
    let dir = tempfile::tempdir().unwrap();
    let (state, _) = runtime(&dir, false).await;

    // WHEN
    let first = agents_of(&state, google(), "imap:ada@example.org")
        .await
        .unwrap();
    let second = agents_of(&state, google(), "imap:support@example.org")
        .await
        .unwrap();
    let account = agents_of(&state, google(), "native:google:ada@example.com")
        .await
        .unwrap();
    let server = agents_of(&state, google(), "mcp:notion").await.unwrap();
    let unknown = agents_of(&state, google(), "mcp:absent").await;

    // THEN
    assert_eq!(ids(&first), ["triage"]);
    assert_eq!(first.agents[0].tools, ["imap.search"]);
    assert_eq!(first.shared_with, ["imap:support@example.org"]);
    assert_eq!(ids(&second), ["triage"]);
    assert_eq!(second.shared_with, ["imap:ada@example.org"]);
    assert_eq!(ids(&account), ["mailer"]);
    assert_eq!(account.agents[0].tools, [GMAIL_SEND]);
    assert!(!account.agents[0].by_default);
    // Apollia, which every agent store holds, is on the default access too.
    assert_eq!(ids(&server), ["apollia:chat", "writer"]);
    assert_eq!(server.agents[1].tools, ["mcp:notion/search"]);
    assert!(server.agents[1].by_default);
    assert_eq!(unknown.unwrap_err().0, StatusCode::NOT_FOUND);
}

/// The tools the journal of `agent` records as granted, in order.
async fn granted(journal: &AuditJournalHandle, agent: &str) -> Vec<String> {
    journal
        .query_run(&governance_run_id(agent))
        .await
        .into_iter()
        .filter(|e| e.kind == JournalEntryKind::AccessChanged && e.payload["change"] == "granted")
        .filter_map(|e| e.payload["access_id"].as_str().map(str::to_owned))
        .collect()
}

/// The overrides of `agent`, by field.
fn override_fields(dir: &tempfile::TempDir, agent: &str) -> Vec<apollia_tools::OverrideField> {
    AgentRepository::open(&apollia_core::paths::DataFile::Agents.path(dir.path()))
        .unwrap()
        .overrides(agent)
        .unwrap()
        .into_iter()
        .map(|o| o.field)
        .collect()
}

// GIVEN the runtime above with a signed journal
// WHEN gmail.send of the account is granted to `triage` and to `writer`
// THEN both hold it in the answer; `triage` has it appended to its list,
//      `writer` keeps its default access and holds the tool on top of it; the
//      journal of each agent records its override and that tool alone
#[tokio::test]
async fn test_a_grant_to_several_agents_writes_and_journals_each() {
    // GIVEN
    let dir = tempfile::tempdir().unwrap();
    let (state, journal) = runtime(&dir, true).await;
    let journal = journal.unwrap();
    let body = GrantConnectionBody {
        agents: vec!["triage".into(), "writer".into()],
        tools: vec![GMAIL_SEND.into()],
    };

    // WHEN
    let answer = grant(&state, google(), "native:google:ada@example.com", &body)
        .await
        .unwrap();

    // THEN
    assert_eq!(ids(&answer), ["mailer", "triage", "writer"]);
    assert!(answer.agents.iter().all(|a| a.tools == [GMAIL_SEND]));
    assert!(answer.agents.iter().all(|a| !a.by_default));
    let server = agents_of(&state, google(), "mcp:notion").await.unwrap();
    assert_eq!(ids(&server), ["apollia:chat", "writer"]);
    assert_eq!(server.agents[1].tools, ["mcp:notion/search"]);
    assert!(server.agents[1].by_default);
    assert_eq!(
        override_fields(&dir, "triage"),
        [apollia_tools::OverrideField::Access]
    );
    assert_eq!(
        override_fields(&dir, "writer"),
        [apollia_tools::OverrideField::AccessAdded]
    );
    for agent in ["triage", "writer"] {
        let entries = journal.query_run(&governance_run_id(agent)).await;
        assert_eq!(
            entries.first().map(|e| e.kind.clone()),
            Some(JournalEntryKind::AgentOverrideChanged)
        );
        assert_eq!(granted(&journal, agent).await, [GMAIL_SEND], "{agent}");
    }
}

// GIVEN `writer` on the default access, given gmail.send of the account
// WHEN a reading tool of the server reaches the machine afterwards
// THEN `writer` holds it too, by default, beside the tool given
#[tokio::test]
async fn test_an_agent_given_a_connection_still_follows_the_default() {
    // GIVEN
    let dir = tempfile::tempdir().unwrap();
    let (state, _) = runtime(&dir, true).await;
    let body = GrantConnectionBody {
        agents: vec!["writer".into()],
        tools: vec![GMAIL_SEND.into()],
    };
    grant(&state, google(), "native:google:ada@example.com", &body)
        .await
        .unwrap();

    // WHEN
    let mut fetch = crate::connectors_bridge::google_tool_descriptors()[0].clone();
    fetch.name = "mcp:notion/fetch".to_owned();
    fetch.family = ActionFamily::Read;
    state
        .tool_registry_handle
        .as_ref()
        .unwrap()
        .register(fetch)
        .await
        .unwrap();
    let server = agents_of(&state, google(), "mcp:notion").await.unwrap();
    let account = agents_of(&state, google(), "native:google:ada@example.com")
        .await
        .unwrap();

    // THEN
    let writer = server.agents.iter().find(|a| a.id == "writer").unwrap();
    assert_eq!(writer.tools, ["mcp:notion/fetch", "mcp:notion/search"]);
    assert!(writer.by_default);
    let writer = account.agents.iter().find(|a| a.id == "writer").unwrap();
    assert_eq!(writer.tools, [GMAIL_SEND]);
}

// GIVEN the runtime above with a signed journal: `mailer` holds the bare
//      gmail.send its package declares, `writer` holds the server's reading
//      tool by default
// WHEN gmail.send of the account is granted to `mailer`, and the server's
//      reading tool to `writer`
// THEN neither agent receives an override nor a journal entry: each held the
//      tool already
#[tokio::test]
async fn test_a_tool_already_held_is_neither_written_nor_journaled() {
    // GIVEN
    let dir = tempfile::tempdir().unwrap();
    let (state, journal) = runtime(&dir, true).await;
    let journal = journal.unwrap();
    let to = |agent: &str, tool: &str| GrantConnectionBody {
        agents: vec![agent.to_owned()],
        tools: vec![tool.to_owned()],
    };

    // WHEN
    grant(
        &state,
        google(),
        "native:google:ada@example.com",
        &to("mailer", GMAIL_SEND),
    )
    .await
    .unwrap();
    grant(
        &state,
        google(),
        "mcp:notion",
        &to("writer", "mcp:notion/search"),
    )
    .await
    .unwrap();

    // THEN
    for agent in ["mailer", "writer"] {
        assert_eq!(overrides(&dir, agent), 0, "{agent}");
        assert!(
            journal
                .query_run(&governance_run_id(agent))
                .await
                .is_empty(),
            "{agent}"
        );
    }
}

// GIVEN the runtime above with a signed journal, and another without one
// WHEN a grant names a stored agent and an unknown one, a tool of another
//      connection, no agent, or is sent without a journal
// THEN they answer 404, 422, 422 and 503, and no access is written nor
//      recorded
#[tokio::test]
async fn test_a_refused_grant_writes_nothing() {
    // GIVEN
    let dir = tempfile::tempdir().unwrap();
    let (state, journal) = runtime(&dir, true).await;
    let journal = journal.unwrap();
    let other = tempfile::tempdir().unwrap();
    let (without_journal, _) = runtime(&other, false).await;
    let account = "native:google:ada@example.com";
    let body = |agents: &[&str], tools: &[&str]| GrantConnectionBody {
        agents: agents.iter().map(|a| (*a).to_owned()).collect(),
        tools: tools.iter().map(|t| (*t).to_owned()).collect(),
    };

    // WHEN
    let ghost = grant(
        &state,
        google(),
        account,
        &body(&["triage", "ghost"], &[GMAIL_SEND]),
    )
    .await;
    let foreign = grant(
        &state,
        google(),
        account,
        &body(&["triage"], &["imap.search"]),
    )
    .await;
    let nobody = grant(&state, google(), account, &body(&[], &[GMAIL_SEND])).await;
    let unrecorded = grant(
        &without_journal,
        google(),
        account,
        &body(&["triage"], &[GMAIL_SEND]),
    )
    .await;

    // THEN
    assert_eq!(ghost.unwrap_err().0, StatusCode::NOT_FOUND);
    assert_eq!(foreign.unwrap_err().0, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(nobody.unwrap_err().0, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(unrecorded.unwrap_err().0, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(overrides(&dir, "triage"), 0);
    assert_eq!(overrides(&other, "triage"), 0);
    assert!(journal
        .query_run(&governance_run_id("triage"))
        .await
        .is_empty());
}

// GIVEN the connections router
// WHEN it is built beside the folder routes
// THEN the two routes under a connection identifier do not conflict with them
#[test]
fn test_the_connection_routes_mount_beside_the_folder_routes() {
    // GIVEN / WHEN
    let built = std::panic::catch_unwind(super::super::connections_router::<MockBackend>);

    // THEN
    assert!(built.is_ok());
}
