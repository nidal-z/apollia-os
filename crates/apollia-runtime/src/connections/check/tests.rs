use std::sync::Mutex;

use apollia_auth::{AccountId, ConnectorProvider, StoredToken, TokenRefresh};
use apollia_connectors::imap::{ConnectionProbe, ImapAccount, ImapAccountInput, ImapError};
use apollia_core::{
    ConnectionReasonKey, ConnectionStateStore, ConnectionStatus, McpHealth, RuntimeEvent,
};
use apollia_mcp::manager::McpConnectionTestResult;
use apollia_mcp::session::McpSessionError;

use super::*;
use crate::connectors_bridge::test_support::auth_with_tokens;

/// A token endpoint that refuses every refresh with `invalid_grant`, and the
/// address of every exchange it was asked for.
#[derive(Default)]
struct RefusingTokenEndpoint {
    asked: Mutex<Vec<String>>,
}

impl TokenRefresh for RefusingTokenEndpoint {
    async fn refresh(
        &self,
        provider: &ProviderConfig,
        _stored: &StoredToken,
    ) -> Result<StoredToken, AuthError> {
        self.asked
            .lock()
            .unwrap()
            .push(provider.token_url.to_owned());
        Err(AuthError::ProviderError(
            "invalid_grant: Token has been expired or revoked.".to_owned(),
        ))
    }
}

fn expired_token() -> StoredToken {
    StoredToken {
        access_token: "old".to_owned(),
        refresh_token: Some("refresh".to_owned()),
        expires_at: Some(chrono::Utc::now() - chrono::Duration::hours(1)),
        scopes: Vec::new(),
    }
}

#[tokio::test]
async fn test_a_refused_refresh_asks_to_sign_in_again_and_calls_the_token_endpoint_only() {
    // GIVEN a Google account whose token expired, and a token endpoint that
    // refuses the refresh
    let dir = tempfile::tempdir().unwrap();
    let auth = auth_with_tokens(dir.path(), ConnectorProvider::Google, &[]).await;
    auth.put_token(
        ConnectorProvider::Google,
        &AccountId::new("ada@example.com"),
        expired_token(),
    )
    .await
    .unwrap();
    let endpoint = RefusingTokenEndpoint::default();
    let config = provider_config(ConnectorProvider::Google);

    // WHEN the account is checked
    let found = check_native(
        "native:google:ada@example.com",
        &auth,
        ConnectorProvider::Google,
        "ada@example.com",
        &endpoint,
    )
    .await;

    // THEN it needs repair, to sign in again, and the one exchange asked for
    // is at Google's token endpoint
    assert_eq!(found.state, ConnectionStatus::NeedsRepair);
    assert_eq!(
        found.reason.map(|r| r.key),
        Some(ConnectionReasonKey::SignInAgain)
    );
    let asked = endpoint.asked.lock().unwrap().clone();
    assert_eq!(asked, vec![config.token_url.to_owned()]);
    assert_eq!(asked[0], "https://oauth2.googleapis.com/token");
}

#[tokio::test]
async fn test_a_valid_token_proves_the_account_without_the_network() {
    // GIVEN a Microsoft account whose token never expires
    let dir = tempfile::tempdir().unwrap();
    let auth = auth_with_tokens(
        dir.path(),
        ConnectorProvider::Microsoft,
        &[("ada@example.com", "valid")],
    )
    .await;
    let endpoint = RefusingTokenEndpoint::default();

    // WHEN the account is checked
    let found = check_native(
        "native:microsoft:ada@example.com",
        &auth,
        ConnectorProvider::Microsoft,
        "ada@example.com",
        &endpoint,
    )
    .await;

    // THEN it works, dated, and no exchange was asked for
    assert_eq!(found.state, ConnectionStatus::Working);
    assert!(found.reason.is_none());
    assert!(!found.checked_at.is_empty());
    assert!(endpoint.asked.lock().unwrap().is_empty());
}

#[test]
fn test_a_network_failure_reads_unreachable_and_a_keychain_failure_failing() {
    // GIVEN the failures a token can meet
    let network = AuthError::HttpError("connection refused".into());
    let keychain = AuthError::Keyring("locked".into());
    let refused = AuthError::TokenExchangeFailed("invalid_grant".into());

    // WHEN each is read
    // THEN only the refusal asks to sign in again
    assert_eq!(native_reason(&network), ConnectionReasonKey::Unreachable);
    assert_eq!(native_reason(&keychain), ConnectionReasonKey::Failing);
    assert_eq!(native_reason(&refused), ConnectionReasonKey::SignInAgain);
    assert_eq!(
        native_reason(&AuthError::NoRefreshToken),
        ConnectionReasonKey::SignInAgain
    );
}

/// A mailbox server that answers `failure`, and the server each check was
/// sent to.
struct FakeMailServer {
    failure: Option<fn() -> ImapError>,
    reached: Mutex<Vec<String>>,
}

impl ConnectionProbe for FakeMailServer {
    async fn verify(&self, account: &ImapAccount, _password: &str) -> Result<(), ImapError> {
        self.reached
            .lock()
            .unwrap()
            .push(format!("{}:{}", account.host, account.port));
        self.failure.map_or(Ok(()), |failure| Err(failure()))
    }
}

fn mailbox() -> ImapAccount {
    ImapAccount::from_input(&ImapAccountInput {
        email: "ada@example.org".into(),
        host: Some("imap.example.org".into()),
        port: Some(993),
        username: None,
    })
    .unwrap()
}

#[tokio::test]
async fn test_a_refused_login_reads_as_a_refused_password_on_the_mailbox_server_only() {
    // GIVEN a mailbox whose server refuses the password
    let server = FakeMailServer {
        failure: Some(|| ImapError::CredentialsRejected),
        reached: Mutex::default(),
    };

    // WHEN the mailbox is checked
    let found = check_imap(
        "imap:ada@example.org",
        &mailbox(),
        Ok("wrong".into()),
        &server,
    )
    .await;

    // THEN it needs repair, its password refused, and only its own server
    // was reached
    assert_eq!(found.state, ConnectionStatus::NeedsRepair);
    assert_eq!(
        found.reason.map(|r| r.key),
        Some(ConnectionReasonKey::PasswordRefused)
    );
    assert_eq!(
        *server.reached.lock().unwrap(),
        vec!["imap.example.org:993".to_owned()]
    );
}

#[tokio::test]
async fn test_a_mailbox_that_logs_in_works_and_a_lost_password_reaches_nothing() {
    // GIVEN a server that accepts the login
    let server = FakeMailServer {
        failure: None,
        reached: Mutex::default(),
    };

    // WHEN the mailbox is checked with its password, then without one
    let working = check_imap("imap:a", &mailbox(), Ok("good".into()), &server).await;
    let lost = check_imap(
        "imap:a",
        &mailbox(),
        Err(ImapError::CredentialsRejected),
        &server,
    )
    .await;

    // THEN the first works, the second asks for the password, unreached
    assert_eq!(working.state, ConnectionStatus::Working);
    assert_eq!(
        lost.reason.map(|r| r.key),
        Some(ConnectionReasonKey::PasswordRefused)
    );
    assert_eq!(server.reached.lock().unwrap().len(), 1);
}

#[test]
fn test_a_folder_that_is_gone_reads_not_found() {
    // GIVEN a folder that exists and one that was moved away
    let dir = tempfile::tempdir().unwrap();
    let gone = dir.path().join("moved");

    // WHEN both are checked
    let here = check_folder("folder:a", dir.path());
    let missing = check_folder("folder:b", &gone);

    // THEN the first works, the second needs repair: folder not found
    assert_eq!(here.state, ConnectionStatus::Working);
    assert_eq!(missing.state, ConnectionStatus::NeedsRepair);
    assert_eq!(
        missing.reason.map(|r| r.key),
        Some(ConnectionReasonKey::FolderNotFound)
    );
}

fn tested(live_health: Option<McpHealth>) -> Result<McpConnectionTestResult, McpSessionError> {
    Ok(McpConnectionTestResult {
        server_info: "mock".into(),
        protocol_version: "2025-06-18".into(),
        tools: Vec::new(),
        test_duration_ms: 1,
        live_health,
    })
}

#[test]
fn test_an_answered_mcp_test_works_unless_its_operations_fail() {
    // GIVEN tests that answered with a healthy session, with failing
    // operations, and one the server refused for lack of a sign-in
    let healthy = tested(Some(McpHealth::Healthy { verified: false }));
    let failing = tested(Some(McpHealth::Degraded {
        category: apollia_core::ErrorCategory::ToolFailure,
        last_error: "403 forbidden".into(),
        since: "2026-10-21T09:00:00Z".into(),
        consecutive_failures: 1,
    }));
    let unauthorized: Result<McpConnectionTestResult, McpSessionError> =
        Err(McpSessionError::Unauthorized {
            server: "notion".into(),
            www_authenticate: String::new(),
        });

    // WHEN each is read
    let healthy = mcp_tested("mcp:a", &healthy);
    let failing = mcp_tested("mcp:a", &failing);
    let unauthorized = mcp_tested("mcp:a", &unauthorized);

    // THEN only the healthy one works
    assert_eq!(healthy.state, ConnectionStatus::Working);
    assert_eq!(
        failing.reason.map(|r| r.key),
        Some(ConnectionReasonKey::Failing)
    );
    assert_eq!(
        unauthorized.reason.map(|r| r.key),
        Some(ConnectionReasonKey::SignInAgain)
    );
}

#[tokio::test]
async fn test_a_check_is_kept_and_announced() {
    // GIVEN a bus with a reader, and a folder found missing
    let dir = tempfile::tempdir().unwrap();
    let (bus, _) = crate::eventbus::EventBus::new();
    let mut events = bus.subscribe();
    let found = check_folder("folder:b", &dir.path().join("moved"));

    // WHEN it is recorded
    record(dir.path(), &bus, &found).unwrap();

    // THEN the store keeps it and the bus announced it
    let store = ConnectionStateStore::open(&apollia_core::paths::DataFile::System.path(dir.path()))
        .unwrap();
    assert_eq!(store.get("folder:b").unwrap(), Some(found.clone()));
    match events.recv().await.unwrap() {
        RuntimeEvent::ConnectionStateChanged {
            id,
            state,
            reason,
            checked_at,
        } => {
            assert_eq!(id, "folder:b");
            assert_eq!(state, ConnectionStatus::NeedsRepair);
            assert_eq!(reason, found.reason);
            assert_eq!(checked_at, found.checked_at);
        }
        other => panic!("unexpected event {other:?}"),
    }
}
