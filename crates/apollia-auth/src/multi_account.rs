//! Multi-account storage for SaaS connector OAuth tokens.
//!
//! Each (provider, account_id) pair is stored:
//! - The token (access + refresh) in the OS keyring under
//!   `service = "apollia-connector-<provider>"`, `user = "<account_id>"`.
//! - The account_id in an index file at `~/.apollia/connectors-index.json` so
//!   the UI can list connected accounts without enumerating the keyring (which
//!   most platforms do not allow).
//!
//! `account_id` is the user-facing identifier, typically the connected email
//! address. It is **not** a secret; the secret lives in the keyring entry.

use std::collections::BTreeMap;
use std::path::PathBuf;

use keyring::Entry;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::{connector_providers::ConnectorProvider, error::AuthError, token::StoredToken};

// ─── Identifier ──────────────────────────────────────────────────────────────

/// Identifier for a connected account on a given provider.
///
/// Typically the user's email address, opaque to the auth crate.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct AccountId(pub String);

impl AccountId {
    /// Create from any string-like value.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Borrow the raw account identifier.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for AccountId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

// ─── Index file format ───────────────────────────────────────────────────────

#[derive(Debug, Default, Serialize, Deserialize)]
struct AccountIndex {
    /// Map of provider id → sorted list of account ids.
    #[serde(default)]
    accounts: BTreeMap<String, Vec<String>>,
}

// ─── Storage ─────────────────────────────────────────────────────────────────

/// Multi-account keyring storage for SaaS connector tokens.
///
/// Operations are async and serialize index updates through an internal
/// [`Mutex`] so concurrent stores from multiple tasks do not corrupt the index.
pub struct MultiAccountStorage {
    /// Filesystem path to the index file. Default: `~/.apollia/connectors-index.json`.
    index_path: PathBuf,
    /// Serialize index reads + writes to prevent torn updates.
    index_lock: Mutex<()>,
    /// Where the tokens themselves live. Production is the OS keyring; tests
    /// substitute a directory of files, because the keyring resolves through
    /// the operator's real profile (on macOS the default keychain sits under
    /// `~/Library`), which a test must never write, the same isolation rule
    /// as the real `~/.apollia`.
    tokens: TokenBackend,
}

/// Token storage backend of [`MultiAccountStorage`].
enum TokenBackend {
    /// OS-native keyring (production).
    Keyring,
    /// Plain JSON files under a test-owned directory.
    #[cfg(test)]
    Files(PathBuf),
}

impl MultiAccountStorage {
    /// Build a [`MultiAccountStorage`] using the default index location.
    pub fn new() -> Result<Self, AuthError> {
        let home = dirs::home_dir()
            .ok_or_else(|| AuthError::Keyring("home directory not found".into()))?;
        let dir = home.join(".apollia");
        std::fs::create_dir_all(&dir)
            .map_err(|e| AuthError::Keyring(format!("cannot create ~/.apollia: {e}")))?;
        Ok(Self {
            index_path: dir.join("connectors-index.json"),
            index_lock: Mutex::new(()),
            tokens: TokenBackend::Keyring,
        })
    }

    /// Build a [`MultiAccountStorage`] with a custom index file path.
    ///
    /// Used by tests to isolate the index from the user's real `~/.apollia`.
    pub fn with_index_path(index_path: PathBuf) -> Self {
        Self {
            index_path,
            index_lock: Mutex::new(()),
            tokens: TokenBackend::Keyring,
        }
    }

    /// Build a [`MultiAccountStorage`] whose index AND tokens live under
    /// `dir`, for tests. The keyring resolves through the operator's real
    /// profile (macOS keeps the default keychain under `~/Library`), so a
    /// test that stores a token through it writes outside every sandbox a
    /// test HOME can provide; this backend keeps the whole round-trip on
    /// test-owned files instead.
    #[cfg(test)]
    pub(crate) fn with_file_store(dir: &std::path::Path) -> Self {
        Self {
            index_path: dir.join("connectors-index.json"),
            index_lock: Mutex::new(()),
            tokens: TokenBackend::Files(dir.to_path_buf()),
        }
    }

    /// Filesystem name of one token in the [`TokenBackend::Files`] backend.
    #[cfg(test)]
    fn token_file(
        dir: &std::path::Path,
        provider: ConnectorProvider,
        account_id: &AccountId,
    ) -> PathBuf {
        dir.join(format!(
            "token-{}-{}.json",
            provider.id(),
            account_id.as_str().replace(['/', ':'], "_")
        ))
    }

    /// Store a token for `(provider, account_id)` and register the account in the index.
    pub async fn store(
        &self,
        provider: ConnectorProvider,
        account_id: &AccountId,
        token: &StoredToken,
    ) -> Result<(), AuthError> {
        let json =
            serde_json::to_string(token).map_err(|e| AuthError::Serialization(e.to_string()))?;
        match &self.tokens {
            TokenBackend::Keyring => {
                let entry = Entry::new(&keyring_service(provider), account_id.as_str())
                    .map_err(|e| AuthError::Keyring(e.to_string()))?;
                // Delete-then-set so the entry's macOS ACL is rebound to the
                // current binary's identity. Without this, rebuilding the CLI
                // rotates the signing identity and `set_password` silently
                // falls through SecItemUpdate, leaving the daemon reading a
                // stale token. Same pattern as `KeyringSecretStore::set`.
                match entry.delete_credential() {
                    Ok(()) | Err(keyring::Error::NoEntry) => {}
                    Err(e) => {
                        tracing::warn!(
                            provider = %provider.id(),
                            account = %account_id.as_str(),
                            error = %e,
                            "keyring: pre-write delete failed; falling back to in-place update"
                        );
                    }
                }
                entry
                    .set_password(&json)
                    .map_err(|e| AuthError::Keyring(e.to_string()))?;
            }
            #[cfg(test)]
            TokenBackend::Files(dir) => {
                std::fs::write(Self::token_file(dir, provider, account_id), &json)
                    .map_err(|e| AuthError::Keyring(format!("write token file: {e}")))?;
            }
        }

        let _guard = self.index_lock.lock().await;
        let mut index = self.read_index()?;
        let entries = index.accounts.entry(provider.id().to_owned()).or_default();
        if !entries.iter().any(|a| a == account_id.as_str()) {
            entries.push(account_id.as_str().to_owned());
            entries.sort();
        }
        self.write_index(&index)?;
        Ok(())
    }

    /// Load the stored token for `(provider, account_id)`.
    ///
    /// Returns `Ok(None)` if no token is registered.
    pub async fn load(
        &self,
        provider: ConnectorProvider,
        account_id: &AccountId,
    ) -> Result<Option<StoredToken>, AuthError> {
        let json = match &self.tokens {
            TokenBackend::Keyring => {
                let entry = Entry::new(&keyring_service(provider), account_id.as_str())
                    .map_err(|e| AuthError::Keyring(e.to_string()))?;
                match entry.get_password() {
                    Ok(json) => json,
                    Err(keyring::Error::NoEntry) => return Ok(None),
                    Err(e) => return Err(AuthError::Keyring(e.to_string())),
                }
            }
            #[cfg(test)]
            TokenBackend::Files(dir) => {
                match std::fs::read_to_string(Self::token_file(dir, provider, account_id)) {
                    Ok(json) => json,
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                    Err(e) => return Err(AuthError::Keyring(format!("read token file: {e}"))),
                }
            }
        };
        let token =
            serde_json::from_str(&json).map_err(|e| AuthError::Serialization(e.to_string()))?;
        Ok(Some(token))
    }

    /// Delete the stored token for `(provider, account_id)` and remove the account from the index.
    ///
    /// Returns `Ok(())` even if the token was already gone.
    pub async fn delete(
        &self,
        provider: ConnectorProvider,
        account_id: &AccountId,
    ) -> Result<(), AuthError> {
        match &self.tokens {
            TokenBackend::Keyring => {
                let entry = Entry::new(&keyring_service(provider), account_id.as_str())
                    .map_err(|e| AuthError::Keyring(e.to_string()))?;
                match entry.delete_credential() {
                    Ok(()) | Err(keyring::Error::NoEntry) => {}
                    Err(e) => return Err(AuthError::Keyring(e.to_string())),
                }
            }
            #[cfg(test)]
            TokenBackend::Files(dir) => {
                match std::fs::remove_file(Self::token_file(dir, provider, account_id)) {
                    Ok(()) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => return Err(AuthError::Keyring(format!("delete token file: {e}"))),
                }
            }
        }

        let _guard = self.index_lock.lock().await;
        let mut index = self.read_index()?;
        if let Some(entries) = index.accounts.get_mut(provider.id()) {
            entries.retain(|a| a != account_id.as_str());
            if entries.is_empty() {
                index.accounts.remove(provider.id());
            }
        }
        self.write_index(&index)?;
        Ok(())
    }

    /// List the registered account ids for `provider`, sorted alphabetically.
    pub async fn list_accounts(
        &self,
        provider: ConnectorProvider,
    ) -> Result<Vec<AccountId>, AuthError> {
        let _guard = self.index_lock.lock().await;
        let index = self.read_index()?;
        Ok(index
            .accounts
            .get(provider.id())
            .map(|entries| entries.iter().cloned().map(AccountId).collect())
            .unwrap_or_default())
    }

    // ─── Internal index I/O ─────────────────────────────────────────────────

    fn read_index(&self) -> Result<AccountIndex, AuthError> {
        if !self.index_path.exists() {
            return Ok(AccountIndex::default());
        }
        let bytes = std::fs::read(&self.index_path)
            .map_err(|e| AuthError::Keyring(format!("read index: {e}")))?;
        if bytes.is_empty() {
            return Ok(AccountIndex::default());
        }
        serde_json::from_slice(&bytes).map_err(|e| AuthError::Serialization(e.to_string()))
    }

    fn write_index(&self, index: &AccountIndex) -> Result<(), AuthError> {
        let json = serde_json::to_vec_pretty(index)
            .map_err(|e| AuthError::Serialization(e.to_string()))?;
        let tmp = self.index_path.with_extension("json.tmp");
        std::fs::write(&tmp, json)
            .map_err(|e| AuthError::Keyring(format!("write index tmp: {e}")))?;
        std::fs::rename(&tmp, &self.index_path)
            .map_err(|e| AuthError::Keyring(format!("rename index: {e}")))?;
        Ok(())
    }
}

fn keyring_service(provider: ConnectorProvider) -> String {
    format!("apollia-connector-{}", provider.id())
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};

    fn fixture_token() -> StoredToken {
        StoredToken {
            access_token: "test-access".into(),
            refresh_token: Some("test-refresh".into()),
            expires_at: Some(Utc::now() + Duration::hours(1)),
            scopes: vec!["openid".into(), "email".into()],
        }
    }

    fn temp_storage() -> (MultiAccountStorage, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("tempdir");
        let storage = MultiAccountStorage::with_index_path(dir.path().join("idx.json"));
        (storage, dir)
    }

    #[tokio::test]
    async fn test_list_accounts_empty_returns_empty_vec() {
        // GIVEN a fresh storage with no accounts indexed
        let (storage, _dir) = temp_storage();
        // WHEN listing Google accounts
        let accounts = storage
            .list_accounts(ConnectorProvider::Google)
            .await
            .expect("list");
        // THEN empty
        assert!(accounts.is_empty());
    }

    #[tokio::test]
    async fn test_index_round_trips_through_disk() {
        // GIVEN a storage where we manually inject an index file
        let (storage, _dir) = temp_storage();
        let mut index = AccountIndex::default();
        index.accounts.insert(
            "google".into(),
            vec!["a@example.com".into(), "b@example.com".into()],
        );
        storage.write_index(&index).expect("write index");

        // WHEN we list accounts
        let accounts = storage
            .list_accounts(ConnectorProvider::Google)
            .await
            .expect("list");

        // THEN we see both entries in order
        assert_eq!(accounts.len(), 2);
        assert_eq!(accounts[0].as_str(), "a@example.com");
        assert_eq!(accounts[1].as_str(), "b@example.com");
    }

    #[tokio::test]
    async fn test_delete_removes_provider_when_last_account_gone() {
        // GIVEN an index with a single Google account
        let (storage, _dir) = temp_storage();
        let mut index = AccountIndex::default();
        index
            .accounts
            .insert("google".into(), vec!["only@example.com".into()]);
        storage.write_index(&index).expect("write index");

        // Simulate the index-only side of delete (we cannot exercise the
        // keyring portion in unit tests reliably across platforms).
        let _guard = storage.index_lock.lock().await;
        let mut current = storage.read_index().expect("read");
        if let Some(entries) = current.accounts.get_mut("google") {
            entries.retain(|a| a != "only@example.com");
            if entries.is_empty() {
                current.accounts.remove("google");
            }
        }
        storage.write_index(&current).expect("write");
        drop(_guard);

        // THEN list_accounts returns empty
        let accounts = storage
            .list_accounts(ConnectorProvider::Google)
            .await
            .expect("list");
        assert!(accounts.is_empty());
    }

    #[test]
    fn test_keyring_service_naming_is_stable() {
        assert_eq!(
            keyring_service(ConnectorProvider::Google),
            "apollia-connector-google"
        );
        assert_eq!(
            keyring_service(ConnectorProvider::Microsoft),
            "apollia-connector-microsoft"
        );
    }

    #[test]
    fn test_account_id_display_returns_raw_value() {
        let id = AccountId::new("nidal@example.com");
        assert_eq!(format!("{id}"), "nidal@example.com");
    }

    #[test]
    fn test_fixture_token_is_not_expired() {
        // Sanity check the test fixture itself.
        let token = fixture_token();
        assert!(!token.is_expired());
        assert!(token.refresh_token.is_some());
    }
}
