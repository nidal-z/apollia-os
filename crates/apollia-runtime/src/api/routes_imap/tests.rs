//! Tests of the mailbox routes: what is answered, stored and journaled, with a
//! probe standing in for the server.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use apollia_auth::{AuthError, SecretStore};
use apollia_connectors::imap::IMAP_INDEX_FILE;

use super::*;
use crate::governance::{ConnectionAdmission, PolicyReadError};

const PASSWORD: &str = "abcd efgh ijkl mnop";

#[derive(Default, Clone)]
struct MemorySecrets(Arc<Mutex<HashMap<(String, String), String>>>);

impl SecretStore for MemorySecrets {
    fn set(&self, service: &str, user: &str, value: &str) -> Result<(), AuthError> {
        self.0
            .lock()
            .unwrap()
            .insert((service.into(), user.into()), value.into());
        Ok(())
    }
    fn get(&self, service: &str, user: &str) -> Result<Option<String>, AuthError> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .get(&(service.into(), user.into()))
            .cloned())
    }
    fn delete(&self, service: &str, user: &str) -> Result<(), AuthError> {
        self.0
            .lock()
            .unwrap()
            .remove(&(service.into(), user.into()));
        Ok(())
    }
    fn backend_id(&self) -> &'static str {
        "memory"
    }
}

/// A typed failure a probe answers.
type Failure = fn() -> ImapError;

struct Probe(Option<Failure>);

impl ConnectionProbe for Probe {
    async fn verify(&self, _account: &ImapAccount, _password: &str) -> Result<(), ImapError> {
        match self.0 {
            None => Ok(()),
            Some(failure) => Err(failure()),
        }
    }
}

struct RefuseRemote;

impl ConnectionAdmission for RefuseRemote {
    fn read_decision(
        &self,
        request: &AdmissionRequest,
    ) -> Result<AdmissionDecision, PolicyReadError> {
        Ok(if request.remote {
            AdmissionDecision::Refused {
                reason: "remote connections are not allowed".into(),
            }
        } else {
            AdmissionDecision::Admitted
        })
    }
}

fn body(email: &str, password: &str) -> ImapAccountBody {
    ImapAccountBody {
        email: email.to_owned(),
        host: None,
        port: None,
        username: None,
        password: password.to_owned(),
    }
}

#[test]
fn test_known_provider_is_described_from_the_address() {
    // GIVEN a Gmail address and a password, nothing else
    // WHEN the body is read
    let (account, password) = described(body("ana@gmail.com", PASSWORD)).unwrap();
    // THEN server, port and identifier are filled in
    assert_eq!(account.host, "imap.gmail.com");
    assert_eq!(account.port, 993);
    assert_eq!(account.username, "ana@gmail.com");
    assert_eq!(password, PASSWORD);
}

#[test]
fn test_unusable_bodies_are_bad_requests_with_their_code() {
    // GIVEN an empty password, then an unknown domain without a server
    // WHEN each body is read
    let (empty_status, Json(empty)) = described(body("ana@gmail.com", "")).err().unwrap();
    let (unknown_status, Json(unknown)) =
        described(body("ana@example.org", PASSWORD)).err().unwrap();
    // THEN each is a bad request carrying its own code
    assert_eq!(empty_status, StatusCode::BAD_REQUEST);
    assert_eq!(empty.code, "invalid_password");
    assert_eq!(unknown_status, StatusCode::BAD_REQUEST);
    assert_eq!(unknown.code, "unknown_provider");
}

#[tokio::test]
async fn test_each_verification_failure_has_its_status_and_code() {
    // GIVEN a Gmail account and probes failing in each typed way
    let (account, _) = described(body("ana@gmail.com", PASSWORD)).unwrap();
    let cases: [(Failure, StatusCode, &str); 3] = [
        (
            || ImapError::CredentialsRejected,
            StatusCode::UNPROCESSABLE_ENTITY,
            "credentials_rejected",
        ),
        (
            || ImapError::Unreachable {
                host: "imap.gmail.com".into(),
                port: 993,
                detail: "refused".into(),
            },
            StatusCode::BAD_GATEWAY,
            "unreachable",
        ),
        (
            || ImapError::Tls {
                host: "imap.gmail.com".into(),
                detail: "untrusted".into(),
            },
            StatusCode::BAD_GATEWAY,
            "tls",
        ),
    ];
    for (failure, status, code) in cases {
        // WHEN the mailbox is tested
        let (got, Json(err)) = verify(&account, PASSWORD, &Probe(Some(failure)))
            .await
            .unwrap_err();
        // THEN the status and the code say which failure it was
        assert_eq!(got, status, "{code}");
        assert_eq!(err.code, code);
    }
}

#[tokio::test]
async fn test_add_stores_the_mailbox_and_journals_it_without_the_password() {
    // GIVEN governance over a journal, an empty store, and a probe that accepts
    let dir = tempfile::TempDir::new().unwrap();
    let journal = crate::audit_journal::AuditJournalHandle::open(&dir.path().join("journal.db"))
        .await
        .unwrap();
    let governance = ConnectionGovernance::without_policy(Some(journal.clone()));
    let secrets = MemorySecrets::default();
    let store =
        ImapAccountStore::with_store(dir.path().join(IMAP_INDEX_FILE), Box::new(secrets.clone()));
    let (account, password) = described(body("ana@gmail.com", PASSWORD)).unwrap();

    // WHEN the mailbox is added
    let response = add(&governance, &store, account, &password, &Probe(None))
        .await
        .unwrap();

    // THEN it is answered and listed, the password in the secret store only
    assert_eq!(response.connection_id, "imap:ana@gmail.com");
    assert_eq!(
        ImapAccountStore::accounts_in(dir.path()).unwrap()[0].email,
        "ana@gmail.com"
    );
    assert_eq!(store.password("ana@gmail.com").unwrap(), PASSWORD);
    // AND its connection run records the addition, without the password
    let entries = journal
        .query_run(&crate::governance::connection_run_id("imap:ana@gmail.com"))
        .await;
    assert_eq!(entries.len(), 1);
    assert_eq!(
        entries[0].kind,
        crate::audit_journal::JournalEntryKind::ConnectionAdded
    );
    assert_eq!(entries[0].payload["kind"], "imap");
    assert_eq!(entries[0].payload["remote"], true);
    assert!(!entries[0].payload.to_string().contains("abcd"));
    journal.shutdown().await;
}

#[tokio::test]
async fn test_failed_add_stores_and_journals_nothing() {
    // GIVEN governance over a journal, an empty store, and a refusing server
    let dir = tempfile::TempDir::new().unwrap();
    let journal = crate::audit_journal::AuditJournalHandle::open(&dir.path().join("journal.db"))
        .await
        .unwrap();
    let governance = ConnectionGovernance::without_policy(Some(journal.clone()));
    let secrets = MemorySecrets::default();
    let store =
        ImapAccountStore::with_store(dir.path().join(IMAP_INDEX_FILE), Box::new(secrets.clone()));
    let (account, password) = described(body("ana@gmail.com", PASSWORD)).unwrap();

    // WHEN the mailbox is added
    let (status, _) = add(
        &governance,
        &store,
        account,
        &password,
        &Probe(Some(|| ImapError::CredentialsRejected)),
    )
    .await
    .unwrap_err();

    // THEN nothing is listed, kept or journaled
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(ImapAccountStore::accounts_in(dir.path())
        .unwrap()
        .is_empty());
    assert!(secrets.0.lock().unwrap().is_empty());
    let entries = journal
        .query_run(&crate::governance::connection_run_id("imap:ana@gmail.com"))
        .await;
    assert!(entries.is_empty());
    journal.shutdown().await;
}

#[test]
fn test_admission_sees_a_remote_mailbox_and_can_refuse_it() {
    // GIVEN a policy refusing remote connections
    let governance = ConnectionGovernance::new(Arc::new(RefuseRemote), None);
    let (gmail, _) = described(body("ana@gmail.com", PASSWORD)).unwrap();
    let local = ImapAccount {
        host: "127.0.0.1".into(),
        ..gmail.clone()
    };
    // WHEN a Gmail mailbox, then one on this machine, are admitted
    let (status, Json(err)) = admit(&governance, &gmail).unwrap_err();
    // THEN the remote one is refused with the policy's reason, the local one
    // admitted
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(err.code, "admission_refused");
    assert_eq!(err.error, "remote connections are not allowed");
    assert!(admit(&governance, &local).is_ok());
}

#[tokio::test]
async fn test_providers_route_lists_every_prefilled_provider() {
    // GIVEN the provider table the runtime prefills from
    let expected = KNOWN_PROVIDERS.len();

    // WHEN the read-only providers route answers
    let Json(answer) = list_imap_providers().await;

    // THEN every provider is listed with its domains, server and port, and
    // the port of any other server is the implicit TLS one
    assert_eq!(answer.providers.len(), expected);
    let gmail = answer.providers.iter().find(|p| p.name == "Gmail").unwrap();
    assert_eq!(gmail.host, "imap.gmail.com");
    assert_eq!(gmail.port, 993);
    assert!(gmail.domains.iter().any(|d| d == "gmail.com"));
    assert_eq!(answer.default_port, 993);
    let json = serde_json::to_value(&answer).unwrap();
    assert!(json.get("defaultPort").is_some());
}
