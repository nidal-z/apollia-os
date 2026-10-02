//! Tests of the mailbox connection store: what is written where, and what is
//! never written.

use std::collections::HashMap;
use std::io::Write;
use std::sync::{Arc, Mutex};

use apollia_auth::{AuthError, SecretStore};

use super::*;

const PASSWORD: &str = "abcd efgh ijkl mnop";

/// Held by every test that emits the store's events. A callsite first reached
/// on another thread while no subscriber wanted it caches that answer, and the
/// test that captures the events would then see none of them.
pub(super) static EVENTS: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[derive(Default, Clone)]
pub(super) struct MemorySecrets(Arc<Mutex<HashMap<(String, String), String>>>);

impl MemorySecrets {
    pub(super) fn is_empty(&self) -> bool {
        self.0.lock().unwrap().is_empty()
    }
}

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

struct Probe(Option<fn() -> ImapError>);

impl ConnectionProbe for Probe {
    async fn verify(&self, _account: &ImapAccount, _password: &str) -> Result<(), ImapError> {
        match self.0 {
            None => Ok(()),
            Some(failure) => Err(failure()),
        }
    }
}

#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl Write for Captured {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn gmail() -> ImapAccount {
    ImapAccount::from_input(&ImapAccountInput {
        email: "ana@gmail.com".into(),
        ..ImapAccountInput::default()
    })
    .unwrap()
}

#[tokio::test(flavor = "current_thread")]
async fn test_password_goes_to_the_secret_store_never_to_the_list_or_a_log() {
    let _events = EVENTS.lock().await;
    // GIVEN an empty store, a probe that accepts, and every event captured
    let dir = tempfile::tempdir().unwrap();
    let secrets = MemorySecrets::default();
    let store =
        ImapAccountStore::with_store(dir.path().join(IMAP_INDEX_FILE), Box::new(secrets.clone()));
    let captured = Captured::default();
    let sink = captured.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_writer(move || sink.clone())
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);

    // WHEN a Gmail connection is added with its application password
    let added = store
        .add_verified(gmail(), PASSWORD, &Probe(None))
        .await
        .unwrap();

    // THEN the list on disk carries the connection and not the password
    let on_disk = std::fs::read_to_string(dir.path().join(IMAP_INDEX_FILE)).unwrap();
    assert!(on_disk.contains("imap.gmail.com"));
    assert!(on_disk.contains("ana@gmail.com"));
    assert!(!on_disk.contains(PASSWORD));
    assert!(!on_disk.contains("abcd"));
    // AND the secret store holds it under the service and the address
    assert_eq!(
        secrets.get(IMAP_SECRET_SERVICE, "ana@gmail.com").unwrap(),
        Some(PASSWORD.to_owned())
    );
    assert_eq!(store.password(&added.email).unwrap(), PASSWORD);
    // AND an event was emitted, without the password
    let logs = String::from_utf8(captured.0.lock().unwrap().clone()).unwrap();
    assert!(logs.contains("imap.account.added"), "{logs}");
    assert!(!logs.contains("abcd"), "{logs}");
    // AND the connection is listed
    assert_eq!(store.list().unwrap(), vec![added]);
}

#[tokio::test(flavor = "current_thread")]
async fn test_failed_verification_stores_nothing() {
    let _events = EVENTS.lock().await;
    // GIVEN an empty store and probes that fail in each typed way
    let failures: [fn() -> ImapError; 3] = [
        || ImapError::CredentialsRejected,
        || ImapError::Unreachable {
            host: "imap.gmail.com".into(),
            port: 993,
            detail: "refused".into(),
        },
        || ImapError::Tls {
            host: "imap.gmail.com".into(),
            detail: "untrusted".into(),
        },
    ];
    for failure in failures {
        let dir = tempfile::tempdir().unwrap();
        let secrets = MemorySecrets::default();
        let store = ImapAccountStore::with_store(
            dir.path().join(IMAP_INDEX_FILE),
            Box::new(secrets.clone()),
        );

        // WHEN the connection is added
        let err = store
            .add_verified(gmail(), PASSWORD, &Probe(Some(failure)))
            .await
            .unwrap_err();

        // THEN the typed failure comes back
        assert_eq!(err.code(), failure().code());
        // AND neither the list nor the secret store was written
        assert!(!dir.path().join(IMAP_INDEX_FILE).exists());
        assert!(secrets.0.lock().unwrap().is_empty());
    }
}

#[tokio::test(flavor = "current_thread")]
async fn test_unwritable_list_takes_the_password_back_out() {
    let _events = EVENTS.lock().await;
    // GIVEN a store whose list can be read but not written: the temporary
    // file it is written through is taken by a directory
    let dir = tempfile::tempdir().unwrap();
    let index = dir.path().join(IMAP_INDEX_FILE);
    std::fs::create_dir_all(index.with_extension("json.tmp").join("blocker")).unwrap();
    let secrets = MemorySecrets::default();
    let store = ImapAccountStore::with_store(index, Box::new(secrets.clone()));

    // WHEN a verified connection is added
    let err = store
        .add_verified(gmail(), PASSWORD, &Probe(None))
        .await
        .unwrap_err();

    // THEN the failure is the list's, on writing
    assert!(
        matches!(
            err,
            ImapError::Index {
                action: "written",
                ..
            }
        ),
        "{err:?}"
    );
    // AND no password is left behind in the secret store
    assert!(secrets.0.lock().unwrap().is_empty());
}

#[tokio::test(flavor = "current_thread")]
async fn test_adding_the_same_address_again_replaces_it() {
    let _events = EVENTS.lock().await;
    // GIVEN a store holding a Gmail connection
    let dir = tempfile::tempdir().unwrap();
    let store = ImapAccountStore::with_store(
        dir.path().join(IMAP_INDEX_FILE),
        Box::new(MemorySecrets::default()),
    );
    store
        .add_verified(gmail(), PASSWORD, &Probe(None))
        .await
        .unwrap();

    // WHEN the same address is added with another identifier and password
    let mut again = gmail();
    again.username = "ana".into();
    store
        .add_verified(again.clone(), "new password", &Probe(None))
        .await
        .unwrap();

    // THEN one connection remains, the latest
    assert_eq!(store.list().unwrap(), vec![again]);
    assert_eq!(store.password("ana@gmail.com").unwrap(), "new password");
}

#[test]
fn test_unknown_address_is_not_found() {
    // GIVEN an empty store
    let dir = tempfile::tempdir().unwrap();
    let store = ImapAccountStore::with_store(
        dir.path().join(IMAP_INDEX_FILE),
        Box::new(MemorySecrets::default()),
    );
    // WHEN a connection and a password are asked for
    // THEN both answer not found
    assert_eq!(store.get("ana@gmail.com").unwrap_err().code(), "not_found");
    assert_eq!(
        store.password("ana@gmail.com").unwrap_err().code(),
        "not_found"
    );
}
