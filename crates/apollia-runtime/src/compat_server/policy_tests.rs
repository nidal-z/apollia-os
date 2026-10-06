//! The organisation policy over the compatible server: what it closes when
//! the runtime starts, what the journal keeps of it, and what the state of
//! the server says.

use apollia_core::config::PolicySettings;
use serde_json::json;
use tokio::net::TcpStream;

use super::lan_tests::lan_ip;
use super::policy::{closed_against_the_file, ServerPolicyLocks};
use super::tests::{free_port, get, settings_file, spawn_server, spawn_server_under};
use crate::audit_journal::{AuditJournalHandle, JournalEntryKind};
use crate::eventbus::EventBus;
use crate::governance::POLICY_RUN_ID;

const KEY: &[u8] = b"server-policy-journal-key-for-tests";

/// A policy that closes the three settings of the server.
fn closing_policy() -> PolicySettings {
    PolicySettings::none()
        .impose("server.serve_on_lan", json!(false))
        .impose("server.require_auth", json!(true))
        .impose("server.allow_request_mcp", json!(false))
}

/// A `[server]` section turned on and left open on the three settings.
fn open_server(port: u16) -> String {
    format!(
        "[server]\nenabled = true\nport = {port}\nserve_on_lan = true\n\
         require_auth = false\nallow_request_mcp = true\n"
    )
}

async fn signed_journal(dir: &std::path::Path) -> AuditJournalHandle {
    AuditJournalHandle::open_with_key_bytes(&dir.join("audit_journal.db"), KEY.to_vec())
        .await
        .unwrap()
}

/// The settings the journal says the policy closed, in the order written.
async fn closed_settings(journal: &AuditJournalHandle) -> Vec<String> {
    journal
        .query_run(POLICY_RUN_ID)
        .await
        .into_iter()
        .filter(|entry| entry.kind == JournalEntryKind::PolicySettingEnforced)
        .map(|entry| entry.payload["key"].as_str().unwrap().to_owned())
        .collect()
}

#[tokio::test]
async fn test_a_server_left_open_is_closed_when_the_policy_is_read_and_the_journal_names_each_setting(
) {
    // GIVEN two machines whose file left the server on and open: to the local
    // network, without a token, to the MCP servers a request brings. The
    // second has a policy that closes the three.
    let bus = EventBus::new().0;
    let free_machine_port = free_port();
    let (_free_dir, free_path) = settings_file(&open_server(free_machine_port));
    let port = free_port();
    let (dir, path) = settings_file(&open_server(port));
    let journal = signed_journal(dir.path()).await;

    // WHEN each runtime starts, the second reading its policy
    let free_handle = spawn_server(&free_path, &bus);
    let free = free_handle.status().await.unwrap();
    let handle = spawn_server_under(&path, &bus, closing_policy(), Some(journal.clone()));
    let status = handle.status().await.unwrap();

    // THEN the first is open as its file says
    assert!(free.running && free.serve_on_lan);
    let (code, _) = get(free_machine_port, "/v1/models").await;
    assert_eq!(code, 200);

    // AND the second runs closed: on the machine alone, behind a token
    assert!(status.running);
    assert!(!status.serve_on_lan);
    assert_eq!(status.address, format!("http://127.0.0.1:{port}"));
    if let Some(ip) = lan_ip() {
        assert!(
            TcpStream::connect((ip, port)).await.is_err(),
            "the server still answers the local network"
        );
    }
    let (code, body) = get(port, "/v1/models").await;
    assert_eq!(code, 401, "{body}");

    // AND the journal names each setting the policy closed, and nothing else
    assert_eq!(
        closed_settings(&journal).await,
        [
            "server.serve_on_lan",
            "server.require_auth",
            "server.allow_request_mcp"
        ]
    );
    let entries = journal.query_run(POLICY_RUN_ID).await;
    assert_eq!(
        entries[0].payload,
        json!({"event": "policy_setting_enforced", "key": "server.serve_on_lan"})
    );
}

#[tokio::test]
async fn test_a_request_cannot_bring_a_server_once_the_policy_refuses_them() {
    // GIVEN a file that lets a request bring an MCP server
    let (dir, path) = settings_file("[server]\nallow_request_mcp = true\n");
    let models =
        super::LocalModels::default().with_files(path.clone(), dir.path().join("system.db"));
    assert!(models.allows_request_mcp());

    // WHEN the same file is read under a policy that refuses them
    let governed = models.under_policy(closing_policy());

    // THEN they are refused, and the refusal is the policy's
    assert!(!governed.allows_request_mcp());
    assert!(governed.request_mcp_fixed_by_policy());
}

#[tokio::test]
async fn test_a_file_that_agrees_with_the_policy_leaves_no_entry() {
    // GIVEN a server turned on that stays on the machine, as its file says
    // nothing else, and a policy that fixes exactly that
    let bus = EventBus::new().0;
    let port = free_port();
    let (dir, path) = settings_file(&format!("[server]\nenabled = true\nport = {port}\n"));
    let journal = signed_journal(dir.path()).await;
    let policy = PolicySettings::none().impose("server.serve_on_lan", json!(false));

    // WHEN the runtime starts
    let handle = spawn_server_under(&path, &bus, policy, Some(journal.clone()));
    let status = handle.status().await.unwrap();

    // THEN the server runs, nothing was closed, and the journal says nothing
    assert!(status.running);
    assert!(closed_settings(&journal).await.is_empty());
}

#[tokio::test]
async fn test_a_server_that_is_off_is_not_said_closed() {
    // GIVEN a server the file leaves off, with settings the policy fixes
    // otherwise
    let bus = EventBus::new().0;
    let port = free_port();
    let content = open_server(port).replace("enabled = true", "enabled = false");
    let (dir, path) = settings_file(&content);
    let journal = signed_journal(dir.path()).await;

    // WHEN the runtime starts
    let handle = spawn_server_under(&path, &bus, closing_policy(), Some(journal.clone()));
    let status = handle.status().await.unwrap();

    // THEN nothing listens, the journal says nothing, and the state already
    // gives the settings as the policy fixes them
    assert!(!status.running);
    assert!(!status.serve_on_lan);
    assert!(closed_settings(&journal).await.is_empty());
}

#[tokio::test]
async fn test_a_server_started_by_hand_under_the_policy_opens_closed() {
    // GIVEN a server left off, whose file would open it to the local network
    let bus = EventBus::new().0;
    let port = free_port();
    let content = open_server(port).replace("enabled = true", "enabled = false");
    let (_dir, path) = settings_file(&content);
    let handle = spawn_server_under(&path, &bus, closing_policy(), None);

    // WHEN the user starts it
    let status = handle.start().await.unwrap();

    // THEN it runs on the machine alone, behind a token
    assert!(status.running);
    assert!(!status.serve_on_lan);
    let (code, _) = get(port, "/v1/models").await;
    assert_eq!(code, 401);
}

#[tokio::test]
async fn test_the_state_says_which_settings_the_policy_imposes() {
    // GIVEN a policy that keeps the server on the machine and requires its
    // token, and says nothing of the MCP servers a request brings
    let bus = EventBus::new().0;
    let (_dir, path) = settings_file(&format!("[server]\nport = {}\n", free_port()));
    let policy = PolicySettings::none()
        .impose("server.serve_on_lan", json!(false))
        .impose("server.require_auth", json!(true));

    // WHEN the state is read without the policy, then under it
    let free = spawn_server(&path, &bus).status().await.unwrap();
    let governed = spawn_server_under(&path, &bus, policy, None)
        .status()
        .await
        .unwrap();

    // THEN each setting says whether the policy imposes it
    assert_eq!(free.imposed_by_policy, ServerPolicyLocks::default());
    assert_eq!(
        governed.imposed_by_policy,
        ServerPolicyLocks {
            serve_on_lan: true,
            require_auth: true,
            allow_request_mcp: false,
        }
    );
}

#[test]
fn test_only_a_setting_fixed_against_the_file_counts_as_closed() {
    // GIVEN a file that opens the local network and already requires a token
    let (_dir, path) = settings_file("[server]\nserve_on_lan = true\nrequire_auth = true\n");

    // WHEN it is compared with nothing, then with a policy closing the three
    let free = closed_against_the_file(&path, &PolicySettings::none());
    let closed = closed_against_the_file(&path, &closing_policy());

    // THEN only the local network is closed: the token was already required,
    // and the MCP servers of a request were already refused by default
    assert!(free.is_empty());
    assert_eq!(closed, ["server.serve_on_lan"]);
}
