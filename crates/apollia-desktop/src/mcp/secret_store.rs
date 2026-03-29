//! OS keychain secret store for MCP server credentials.
//!
//! Wraps the `keyring` crate to provide a typed interface for storing and
//! retrieving API tokens and other secrets required by MCP servers. Secrets
//! never leave the local machine — they are delegated entirely to the native
//! OS keychain (macOS Keychain, Linux Secret Service, Windows Credential Manager).

use apollia_mcp::config::SecretResolver;

/// Local secret store backed by the OS keychain.
///
/// All secrets are stored under the service name `apollia-mcp`. Keys follow
/// the format `{server_name}:{env_var_name}` (e.g. `notion:NOTION_API_KEY`),
/// produced by [`SecretStore::key_for`].
///
/// The struct is cheap to clone — the underlying `keyring` crate holds no
/// persistent connection.
#[derive(Debug, Clone)]
pub struct SecretStore {
    service_name: String,
}

/// Errors that can occur when interacting with the OS keychain.
#[derive(Debug, thiserror::Error)]
pub enum SecretStoreError {
    /// The keyring backend is not available on this system (e.g. no D-Bus on headless Linux).
    #[error("keyring backend unavailable: {0}")]
    BackendUnavailable(String),

    /// The secret could not be persisted.
    #[error("failed to store secret: {0}")]
    StoreFailed(String),

    /// The secret exists but could not be read.
    #[error("failed to retrieve secret: {0}")]
    RetrieveFailed(String),

    /// No secret is stored for the given key.
    #[error("secret not found for key: {0}")]
    NotFound(String),

    /// The secret could not be removed from the keychain.
    #[error("failed to delete secret: {0}")]
    DeleteFailed(String),
}

impl SecretStore {
    /// Creates a new `SecretStore` bound to the `apollia-mcp` keychain service.
    pub fn new() -> Self {
        Self {
            service_name: "apollia-mcp".to_string(),
        }
    }

    /// Builds the keychain key for an MCP server environment variable.
    ///
    /// The resulting string is `"{server_name}:{env_var}"`, e.g.
    /// `"notion:NOTION_API_KEY"`.
    pub fn key_for(server_name: &str, env_var: &str) -> String {
        format!("{}:{}", server_name, env_var)
    }

    /// Stores a secret in the OS keychain under the given key.
    pub fn store(&self, key: &str, value: &str) -> Result<(), SecretStoreError> {
        let entry = keyring::Entry::new(&self.service_name, key)
            .map_err(|e| SecretStoreError::BackendUnavailable(e.to_string()))?;
        entry
            .set_password(value)
            .map_err(|e| SecretStoreError::StoreFailed(e.to_string()))
    }

    /// Retrieves a secret from the OS keychain.
    ///
    /// Returns [`SecretStoreError::NotFound`] when no secret is stored for
    /// `key`, and [`SecretStoreError::RetrieveFailed`] for other keychain
    /// errors.
    pub fn retrieve(&self, key: &str) -> Result<String, SecretStoreError> {
        let entry = keyring::Entry::new(&self.service_name, key)
            .map_err(|e| SecretStoreError::BackendUnavailable(e.to_string()))?;
        entry.get_password().map_err(|e| match e {
            keyring::Error::NoEntry => SecretStoreError::NotFound(key.to_string()),
            other => SecretStoreError::RetrieveFailed(other.to_string()),
        })
    }

    /// Removes a secret from the OS keychain.
    pub fn delete(&self, key: &str) -> Result<(), SecretStoreError> {
        let entry = keyring::Entry::new(&self.service_name, key)
            .map_err(|e| SecretStoreError::BackendUnavailable(e.to_string()))?;
        entry
            .delete_credential()
            .map_err(|e| SecretStoreError::DeleteFailed(e.to_string()))
    }

    /// Lists stored key names for a given MCP server.
    ///
    /// The `keyring` crate does not expose a listing API, so keys are tracked
    /// externally (in `mcp.toml`). This method always returns an empty `Vec`
    /// and is provided as a stable interface for future backends that support
    /// enumeration.
    pub fn list_keys_for_server(&self, _server_name: &str) -> Vec<String> {
        Vec::new()
    }
}

impl Default for SecretStore {
    fn default() -> Self {
        Self::new()
    }
}

impl SecretResolver for SecretStore {
    /// Retrieve a secret from the OS keychain by its composite key.
    ///
    /// Returns `Ok(value)` when found, or `Err(message)` for any keychain
    /// error including [`SecretStoreError::NotFound`].
    fn get_secret(&self, key: &str) -> Result<String, String> {
        self.retrieve(key).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_for_formats_correctly() {
        assert_eq!(
            SecretStore::key_for("notion", "NOTION_API_KEY"),
            "notion:NOTION_API_KEY"
        );
    }

    #[test]
    fn key_for_handles_empty_parts() {
        assert_eq!(SecretStore::key_for("", "KEY"), ":KEY");
        assert_eq!(SecretStore::key_for("svc", ""), "svc:");
    }

    #[test]
    fn list_keys_for_server_always_empty() {
        let store = SecretStore::new();
        assert!(store.list_keys_for_server("notion").is_empty());
        assert!(store.list_keys_for_server("slack").is_empty());
    }

    /// Full round-trip: store → retrieve → delete.
    ///
    /// Requires a live OS keychain (skipped in headless CI).
    #[test]
    #[ignore = "requires live OS keychain"]
    fn store_retrieve_delete_round_trip() {
        let store = SecretStore::new();
        let key = SecretStore::key_for("test-server", "TEST_API_KEY");
        let value = "test-secret-value";

        // GIVEN a fresh secret store
        // WHEN we store a value
        store.store(&key, value).expect("store failed");

        // THEN retrieve returns the same value
        let retrieved = store.retrieve(&key).expect("retrieve failed");
        assert_eq!(retrieved, value);

        // WHEN we delete the secret
        store.delete(&key).expect("delete failed");

        // THEN a subsequent retrieve returns NotFound
        let result = store.retrieve(&key);
        assert!(
            matches!(result, Err(SecretStoreError::NotFound(_))),
            "expected NotFound, got: {result:?}"
        );
    }

    /// Retrieving a key that has never been stored returns NotFound.
    #[test]
    #[ignore = "requires live OS keychain"]
    fn retrieve_missing_secret_returns_not_found() {
        let store = SecretStore::new();
        let key = "nonexistent-server:NONEXISTENT_KEY_apollia_test_ephemeral";

        // GIVEN a key that was never stored
        // WHEN we try to retrieve it
        let result = store.retrieve(key);

        // THEN NotFound is returned
        assert!(
            matches!(result, Err(SecretStoreError::NotFound(_))),
            "expected NotFound, got: {result:?}"
        );
    }
}
