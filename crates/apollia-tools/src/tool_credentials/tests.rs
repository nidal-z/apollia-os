use std::path::PathBuf;

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use apollia_auth::{AuthError, SecretStore};
use rusqlite::{params, Connection};
use tempfile::TempDir;

use super::test_support::MemorySecrets;
use super::*;
use crate::governance_db::{GovernanceDb, GOVERNANCE_DB_FILENAME};

const LEGACY_KEY: [u8; 32] = [7; 32];
const LEGACY_NONCE: [u8; 12] = [1; 12];

fn fresh(dir: &TempDir) -> PathBuf {
    GovernanceDb::open(dir.path()).expect("init governance.db");
    dir.path().join(GOVERNANCE_DB_FILENAME)
}

/// A `governance.db` as version 2 wrote it, with `value` encrypted under
/// [`LEGACY_KEY`] for `web_search/brave.api_key`, and the key file beside it.
fn legacy_v2(dir: &TempDir, value: &str, with_keyfile: bool) -> PathBuf {
    let db = dir.path().join(GOVERNANCE_DB_FILENAME);
    let conn = Connection::open(&db).expect("open");
    (apollia_permissions::GOVERNANCE_MIGRATIONS[0])(&conn).expect("v1");
    (apollia_permissions::GOVERNANCE_MIGRATIONS[1])(&conn).expect("v2");
    conn.pragma_update(None, "user_version", 2)
        .expect("stamp v2");
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&LEGACY_KEY));
    let mut blob = LEGACY_NONCE.to_vec();
    blob.extend(
        cipher
            .encrypt(Nonce::from_slice(&LEGACY_NONCE), value.as_bytes())
            .expect("encrypt"),
    );
    conn.execute(
        "INSERT INTO tool_credentials (tool_name, key_name, value_encrypted, created_at)
         VALUES ('web_search', 'brave.api_key', ?1, 1700000000)",
        params![blob],
    )
    .expect("legacy row");
    if with_keyfile {
        std::fs::write(dir.path().join(".keyfile"), LEGACY_KEY).expect("keyfile");
    }
    db
}

fn table_exists(db: &PathBuf, name: &str) -> bool {
    Connection::open(db)
        .expect("open")
        .query_row(
            "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type = 'table' AND name = ?1",
            params![name],
            |row| row.get(0),
        )
        .expect("probe")
}

fn upgrade_done(db: &PathBuf) -> bool {
    Connection::open(db)
        .expect("open")
        .query_row(
            "SELECT done_at IS NOT NULL FROM governance_upgrades WHERE name = ?1",
            params![apollia_permissions::TOOL_CREDENTIALS_UPGRADE],
            |row| row.get(0),
        )
        .expect("upgrade row")
}

/// A secret store that refuses every write.
struct RefusingSecrets;

impl SecretStore for RefusingSecrets {
    fn set(&self, _: &str, _: &str, _: &str) -> Result<(), AuthError> {
        Err(AuthError::Keyring("locked".into()))
    }
    fn get(&self, _: &str, _: &str) -> Result<Option<String>, AuthError> {
        Ok(None)
    }
    fn delete(&self, _: &str, _: &str) -> Result<(), AuthError> {
        Ok(())
    }
    fn backend_id(&self) -> &'static str {
        "refusing"
    }
}

#[test]
fn test_a_stored_value_lives_in_the_secret_store_and_only_its_name_in_the_database() {
    // GIVEN a fresh store over an in-memory secret store
    let dir = TempDir::new().expect("tempdir");
    let db = fresh(&dir);
    let secrets = MemorySecrets::default();
    let mut store = ToolCredentialStore::new(&db, Box::new(secrets.clone())).expect("open");

    // WHEN a value is stored then read back
    store
        .set("web_search", "brave.api_key", "PLAINTEXT-MARKER-XYZ")
        .expect("set");
    let read = store.get("web_search", "brave.api_key").expect("get");

    // THEN it reads back, the secret store holds it under the tool account,
    // and no byte of governance.db carries it
    assert_eq!(read.as_deref(), Some("PLAINTEXT-MARKER-XYZ"));
    assert_eq!(
        secrets
            .value(TOOL_CREDENTIALS_SERVICE, "web_search/brave.api_key")
            .as_deref(),
        Some("PLAINTEXT-MARKER-XYZ")
    );
    drop(store);
    let raw = std::fs::read(&db).expect("read db");
    assert!(!String::from_utf8_lossy(&raw).contains("PLAINTEXT-MARKER-XYZ"));
}

#[test]
fn test_a_value_the_index_does_not_name_is_absent() {
    // GIVEN a secret store holding a value the index of this database never named
    let dir = TempDir::new().expect("tempdir");
    let db = fresh(&dir);
    let secrets = MemorySecrets::default();
    secrets
        .set(
            TOOL_CREDENTIALS_SERVICE,
            "web_search/brave.api_key",
            "orphan",
        )
        .expect("seed");
    let store = ToolCredentialStore::new(&db, Box::new(secrets)).expect("open");

    // WHEN it is read
    let read = store.get("web_search", "brave.api_key").expect("get");

    // THEN it is absent
    assert_eq!(read, None);
}

#[test]
fn test_delete_removes_value_and_entry_and_list_filters_by_tool() {
    // GIVEN two stored credentials
    let dir = TempDir::new().expect("tempdir");
    let db = fresh(&dir);
    let secrets = MemorySecrets::default();
    let mut store = ToolCredentialStore::new(&db, Box::new(secrets.clone())).expect("open");
    store.set("web_search", "brave.api_key", "v1").expect("a");
    store.set("http_fetch", "auth.token", "v2").expect("b");

    // WHEN one tool is listed, then the other credential is deleted twice
    let only_search = store.list(Some("web_search")).expect("list filtered");
    let removed = store.delete("http_fetch", "auth.token").expect("delete");
    let missing = store
        .delete("http_fetch", "auth.token")
        .expect("delete again");

    // THEN the filter keeps its tool, the first delete reports the hit and
    // empties the secret store, the second reports nothing
    assert_eq!(only_search.len(), 1);
    assert_eq!(only_search[0].key_name, "brave.api_key");
    assert!(removed);
    assert!(!missing);
    assert_eq!(
        secrets.value(TOOL_CREDENTIALS_SERVICE, "http_fetch/auth.token"),
        None
    );
    assert_eq!(store.list(None).expect("list all").len(), 1);
}

#[test]
fn test_opening_a_v2_database_moves_its_encrypted_values_into_the_secret_store() {
    // GIVEN a governance.db written by version 2, one value encrypted with the
    // key file beside it
    let dir = TempDir::new().expect("tempdir");
    let db = legacy_v2(&dir, "BSA-legacy-key", true);
    let secrets = MemorySecrets::default();

    // WHEN the store opens it
    let store = ToolCredentialStore::new(&db, Box::new(secrets.clone())).expect("open");

    // THEN the value is in the secret store and still listed with its date,
    // the encrypted table and the key file are gone, and the move is recorded
    assert_eq!(
        secrets
            .value(TOOL_CREDENTIALS_SERVICE, "web_search/brave.api_key")
            .as_deref(),
        Some("BSA-legacy-key")
    );
    assert_eq!(
        store
            .get("web_search", "brave.api_key")
            .expect("get")
            .as_deref(),
        Some("BSA-legacy-key")
    );
    assert_eq!(store.list(None).expect("list")[0].created_at, 1_700_000_000);
    drop(store);
    assert!(!table_exists(&db, "tool_credentials_legacy"));
    assert!(!dir.path().join(".keyfile").exists());
    assert!(upgrade_done(&db));
}

#[test]
fn test_a_value_that_cannot_be_decrypted_stays_listed_without_a_value() {
    // GIVEN a version 2 database whose key file is gone
    let dir = TempDir::new().expect("tempdir");
    let db = legacy_v2(&dir, "BSA-lost", false);
    let secrets = MemorySecrets::default();

    // WHEN the store opens it
    let store = ToolCredentialStore::new(&db, Box::new(secrets.clone())).expect("open");

    // THEN the credential is still listed, reads as absent, and the move is
    // recorded rather than retried forever
    assert_eq!(store.list(None).expect("list").len(), 1);
    assert_eq!(store.get("web_search", "brave.api_key").expect("get"), None);
    drop(store);
    assert!(!table_exists(&db, "tool_credentials_legacy"));
    assert!(upgrade_done(&db));
}

#[test]
fn test_a_secret_store_that_refuses_leaves_the_move_pending() {
    // GIVEN a version 2 database and a secret store that refuses every write
    let dir = TempDir::new().expect("tempdir");
    let db = legacy_v2(&dir, "BSA-kept", true);

    // WHEN the store opens it
    let opened = ToolCredentialStore::new(&db, Box::new(RefusingSecrets));

    // THEN the open fails, and the encrypted table, the key file and the
    // pending mark are all kept for the next open
    assert!(matches!(opened, Err(ToolGovernanceError::Secrets(_))));
    assert!(table_exists(&db, "tool_credentials_legacy"));
    assert!(dir.path().join(".keyfile").exists());
    assert!(!upgrade_done(&db));
}
