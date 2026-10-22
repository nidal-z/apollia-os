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

/// An auth manager over a store of `dir` holding no account, standing in
/// for the keychain.
fn auth_in(dir: &std::path::Path) -> AuthManager {
    AuthManager::with_storage(apollia_auth::MultiAccountStorage::with_store(
        dir.join("connectors-index.json"),
        Box::new(MemorySecrets::default()),
    ))
}

fn no_accounts(h: &Harness) -> AuthManager {
    auth_in(h.dir.path())
}

/// A secret store whose deletions fail.
#[derive(Clone)]
struct Undeletable(MemorySecrets);

impl SecretStore for Undeletable {
    fn set(&self, service: &str, user: &str, value: &str) -> Result<(), apollia_auth::AuthError> {
        self.0.set(service, user, value)
    }
    fn get(&self, service: &str, user: &str) -> Result<Option<String>, apollia_auth::AuthError> {
        self.0.get(service, user)
    }
    fn delete(&self, _service: &str, _user: &str) -> Result<(), apollia_auth::AuthError> {
        Err(apollia_auth::AuthError::Keyring("locked".into()))
    }
    fn backend_id(&self) -> &'static str {
        "undeletable"
    }
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
    let removal = remove(&h.state, &no_accounts(&h), "mcp:notion", true)
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
    let removal = remove(&h.state, &no_accounts(&h), "mcp:notion", false)
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
    let first = remove(&h.state, &no_accounts(&h), "imap:ada@example.org", false)
        .await
        .unwrap();
    let kept = triage_access(&h);
    let last = remove(
        &h.state,
        &no_accounts(&h),
        "imap:support@example.org",
        false,
    )
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
    let answer = remove(&h.state, &no_accounts(&h), "mcp:absent", false).await;

    // THEN
    assert_eq!(answer.unwrap_err().0, StatusCode::NOT_FOUND);
    h.manager.shutdown().await;
}

// GIVEN a connected Google account, and a package agent declaring one of its
//       operations
// WHEN its removal is asked as a dry run, then made
// THEN both list its token and the agent access, the account is gone from the
//      auth manager, and the agent no longer holds the operation
#[tokio::test]
async fn test_a_native_account_is_removed_as_its_dry_run_says() {
    // GIVEN
    let (h, _) = runtime().await;
    let send = "conn:google:ada@example.com/gmail.send";
    AgentRepository::open(&apollia_core::paths::DataFile::Agents.path(h.dir.path()))
        .unwrap()
        .save_definition(&package_agent("mailer", &[send, "file_read"]))
        .unwrap();
    let auth = no_accounts(&h);
    auth.put_token(
        apollia_auth::ConnectorProvider::Google,
        &apollia_auth::AccountId::new("ada@example.com"),
        apollia_auth::StoredToken {
            access_token: "at".into(),
            refresh_token: None,
            expires_at: None,
            scopes: vec![],
        },
    )
    .await
    .unwrap();
    let id = "native:google:ada@example.com";

    // WHEN
    let dry = remove(&h.state, &auth, id, true).await.unwrap();
    let real = remove(&h.state, &auth, id, false).await.unwrap();

    // THEN
    assert_eq!(kinds(&dry), [RemovedKind::Token, RemovedKind::AgentAccess]);
    assert_eq!(dry.removed, real.removed);
    assert!(real.left.is_empty());
    assert!(auth
        .list_accounts(apollia_auth::ConnectorProvider::Google)
        .await
        .unwrap()
        .is_empty());
    let repo = definitions::open_agents(&h.state).unwrap();
    let access = definitions::resolved(&h.state, &repo, "mailer")
        .unwrap()
        .access;
    assert_eq!(access, ["file_read"]);
    h.manager.shutdown().await;
}

// GIVEN a folder connection with an indexed source made on it
// WHEN its removal is asked as a dry run, then made
// THEN both list the folder and its source, and the folder and the source
//      are gone
#[tokio::test]
async fn test_a_folder_is_removed_as_its_dry_run_says() {
    // GIVEN
    let dir = tempfile::TempDir::new().unwrap();
    let folder = tempfile::TempDir::new().unwrap();
    let (mut state, folder_id, source_id, index_file) =
        crate::api::routes_connections::folders::tests::folder_with_source(&dir, &folder).await;
    state.secret_store = MemorySecrets::default().opener();
    let auth = auth_in(dir.path());
    let id = format!("folder:{folder_id}");

    // WHEN
    let dry = remove(&state, &auth, &id, true).await.unwrap();
    let real = remove(&state, &auth, &id, false).await.unwrap();

    // THEN
    assert_eq!(
        kinds(&dry),
        [RemovedKind::Configuration, RemovedKind::Source]
    );
    assert_eq!(dry.removed[1].name, source_id);
    assert_eq!(dry.removed, real.removed);
    assert!(real.left.is_empty());
    assert!(super::super::folders::store(dir.path())
        .unwrap()
        .get(&folder_id)
        .unwrap()
        .is_none());
    let registry = state.source_registry.clone().unwrap();
    assert!(registry.list().await.unwrap().is_empty());
    assert!(!index_file.exists());
}

// GIVEN a mailbox whose password the secret store refuses to delete
// WHEN it is removed
// THEN the mailbox leaves the list, and its password is answered under
//      `left`, not under `removed`
#[tokio::test]
async fn test_a_mailbox_password_that_stays_is_answered_left() {
    // GIVEN
    let (mut h, secrets) = runtime().await;
    let store = Undeletable(secrets.clone());
    h.state.secret_store =
        Arc::new(move || Ok(Box::new(store.clone()) as Box<dyn apollia_auth::SecretStore>));

    // WHEN
    let removal = remove(&h.state, &no_accounts(&h), "imap:ada@example.org", false)
        .await
        .unwrap();

    // THEN
    assert_eq!(kinds(&removal), [RemovedKind::Configuration]);
    assert_eq!(
        removal.left,
        [RemovedItem::new(RemovedKind::Secret, "ada@example.org")]
    );
    assert!(secrets
        .value(IMAP_SECRET_SERVICE, "ada@example.org")
        .is_some());
    assert_eq!(
        ImapAccountStore::accounts_in(h.dir.path()).unwrap().len(),
        1
    );
    h.manager.shutdown().await;
}

fn chosen(id: &str, path: &str) -> apollia_connectors::microsoft::onedrive_choices::OneDriveChoice {
    apollia_connectors::microsoft::onedrive_choices::OneDriveChoice {
        format_version: 1,
        id: id.into(),
        name: path.rsplit('/').next().unwrap_or_default().into(),
        kind: apollia_connectors::microsoft::onedrive_choices::ChoiceKind::Folder,
        path: path.into(),
        drive_id: None,
    }
}

/// A connected Microsoft account `ada@example.com` holding two OneDrive
/// choices, and another account holding one.
async fn microsoft_with_choices(h: &Harness) -> (AuthManager, std::path::PathBuf) {
    let auth = no_accounts(h);
    put_microsoft(&auth).await;
    let file = h.dir.path().join(ONEDRIVE_CHOICES_FILENAME);
    for (account, id, path) in [
        ("ada@example.com", "c1", "/drive/root:/Clients"),
        ("ada@example.com", "c2", "/drive/root:/Finance"),
        ("grace@example.com", "c3", "/drive/root:/Lab"),
    ] {
        onedrive_choices::record(&file, account, chosen(id, path)).unwrap();
    }
    (auth, file)
}

async fn put_microsoft(auth: &AuthManager) {
    auth.put_token(
        apollia_auth::ConnectorProvider::Microsoft,
        &apollia_auth::AccountId::new("ada@example.com"),
        apollia_auth::StoredToken {
            access_token: "at".into(),
            refresh_token: None,
            expires_at: None,
            scopes: vec![],
        },
    )
    .await
    .unwrap();
}

const ADA_MICROSOFT: &str = "native:microsoft:ada@example.com";

// GIVEN a connected Microsoft account with two OneDrive choices, and another
//       account with one
// WHEN its removal is asked as a dry run, then made, then the same account
//      is connected again
// THEN the dry run lists both choices by their path and erases nothing; the
//      removal erases them, keeps the other account's, and the account
//      connected again has nothing chosen
#[tokio::test]
async fn test_a_microsoft_account_removal_erases_its_onedrive_choices() {
    // GIVEN
    let (h, _) = runtime().await;
    let (auth, file) = microsoft_with_choices(&h).await;

    // WHEN
    let dry = remove(&h.state, &auth, ADA_MICROSOFT, true).await.unwrap();
    let kept_by_dry_run = onedrive_choices::choices_of(&file, "ada@example.com")
        .unwrap()
        .len();
    let real = remove(&h.state, &auth, ADA_MICROSOFT, false).await.unwrap();
    put_microsoft(&auth).await;
    let again = remove(&h.state, &auth, ADA_MICROSOFT, true).await.unwrap();

    // THEN
    assert_eq!(
        kinds(&dry),
        [
            RemovedKind::Token,
            RemovedKind::OneDriveChoice,
            RemovedKind::OneDriveChoice
        ]
    );
    assert_eq!(dry.removed[1].name, "/drive/root:/Clients");
    assert_eq!(dry.removed[2].name, "/drive/root:/Finance");
    assert_eq!(kept_by_dry_run, 2);
    assert_eq!(real.removed, dry.removed);
    assert!(real.left.is_empty(), "{:?}", real.left);
    assert!(onedrive_choices::choices_of(&file, "ada@example.com")
        .unwrap()
        .is_empty());
    assert_eq!(
        onedrive_choices::choices_of(&file, "grace@example.com")
            .unwrap()
            .len(),
        1
    );
    assert_eq!(kinds(&again), [RemovedKind::Token]);
    h.manager.shutdown().await;
}

// GIVEN a connected Microsoft account whose OneDrive choices file cannot be
//       written
// WHEN it is removed
// THEN its token goes, and its choices are answered under `left`, not under
//      `removed`, and are still in the file
#[cfg(unix)]
#[tokio::test]
async fn test_onedrive_choices_that_stay_are_answered_left() {
    use std::os::unix::fs::PermissionsExt;

    // GIVEN
    let (h, _) = runtime().await;
    let (auth, file) = microsoft_with_choices(&h).await;
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o400)).unwrap();

    // WHEN
    let removal = remove(&h.state, &auth, ADA_MICROSOFT, false).await.unwrap();

    // THEN
    assert_eq!(kinds(&removal), [RemovedKind::Token]);
    assert_eq!(
        removal.left,
        [
            RemovedItem::new(RemovedKind::OneDriveChoice, "/drive/root:/Clients"),
            RemovedItem::new(RemovedKind::OneDriveChoice, "/drive/root:/Finance"),
        ]
    );
    assert_eq!(
        onedrive_choices::choices_of(&file, "ada@example.com")
            .unwrap()
            .len(),
        2
    );
    h.manager.shutdown().await;
}
