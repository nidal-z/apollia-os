//! The secret half of an operator's OAuth client, kept in the [`SecretStore`].
//!
//! `oauth-clients.toml` carries the client id of each provider, which is not a
//! secret. The client secret and the Google API key live in the configured
//! secret store, under the service [`OAUTH_CLIENT_SECRETS_SERVICE`] and the
//! account `<provider>:<field>`, like every other secret of a connection.
//!
//! Earlier versions wrote both into the file in clear;
//! [`migrate_file_secrets`] moves them into the store once and rewrites the
//! file without them.

use std::path::Path;

use crate::error::AuthError;
use crate::oauth_clients_file;
use crate::secret_storage::SecretStore;

/// Secret-store service holding the OAuth client secrets and API keys.
pub const OAUTH_CLIENT_SECRETS_SERVICE: &str = "apollia-oauth-clients";

/// Which secret of a provider's OAuth client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OauthClientSecret {
    /// The client secret Google's Installed App type requires at the token
    /// endpoint.
    ClientSecret,
    /// The public API key Google Picker requires.
    ApiKey,
}

impl OauthClientSecret {
    const fn field(self) -> &'static str {
        match self {
            Self::ClientSecret => "client_secret",
            Self::ApiKey => "api_key",
        }
    }
}

/// Secret-store account of `secret` for `provider_id`.
pub fn secret_account(provider_id: &str, secret: OauthClientSecret) -> String {
    format!("{provider_id}:{}", secret.field())
}

/// The stored `secret` of `provider_id`; `None` when absent or empty.
///
/// # Errors
///
/// [`AuthError`] when the secret store fails.
pub fn load(
    store: &dyn SecretStore,
    provider_id: &str,
    secret: OauthClientSecret,
) -> Result<Option<String>, AuthError> {
    Ok(store
        .get(
            OAUTH_CLIENT_SECRETS_SERVICE,
            &secret_account(provider_id, secret),
        )?
        .filter(|v| !v.is_empty()))
}

/// Stores `value` as the `secret` of `provider_id`, trimmed; an empty value
/// removes it, so the resolution chain falls back to the environment or the
/// build.
///
/// # Errors
///
/// [`AuthError`] when the secret store fails.
pub fn save(
    store: &dyn SecretStore,
    provider_id: &str,
    secret: OauthClientSecret,
    value: &str,
) -> Result<(), AuthError> {
    let account = secret_account(provider_id, secret);
    let value = value.trim();
    if value.is_empty() {
        store.delete(OAUTH_CLIENT_SECRETS_SERVICE, &account)
    } else {
        store.set(OAUTH_CLIENT_SECRETS_SERVICE, &account, value)
    }
}

/// The `secret` of `provider_id` from the store selected by
/// `APOLLIA_TOKEN_STORAGE`. A store that cannot be opened or read is logged
/// and reads as absent, so the caller names what is missing.
pub fn load_default(provider_id: &str, secret: OauthClientSecret) -> Option<String> {
    let read = crate::secret_storage::select_default()
        .and_then(|store| load(store.as_ref(), provider_id, secret));
    match read {
        Ok(value) => value,
        Err(e) => {
            tracing::event!(
                tracing::Level::WARN,
                provider = %provider_id,
                secret = secret.field(),
                error = %e,
                "auth.oauth_client.secret_unreadable"
            );
            None
        }
    }
}

/// Why the secrets of `oauth-clients.toml` could not be moved.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum FileSecretsError {
    /// The file could not be read or rewritten.
    #[error("oauth-clients.toml: {0}")]
    File(#[source] std::io::Error),
    /// The secret store refused a value; the file is left untouched.
    #[error("secret store: {0}")]
    Store(#[from] AuthError),
}

/// Moves the client secrets and API keys an earlier version wrote in clear in
/// the file at `path` into `store`, then rewrites the file with the client ids
/// only. Returns how many values moved; zero, and nothing written, when the
/// file is absent or holds no secret.
///
/// A value the store refuses stops the move before the file is rewritten, so
/// nothing is lost and the next start replays it.
///
/// # Errors
///
/// [`FileSecretsError`] when the file cannot be read or rewritten, or the
/// store refuses a value.
pub fn migrate_file_secrets(
    path: &Path,
    store: &dyn SecretStore,
) -> Result<usize, FileSecretsError> {
    let Some(file) = oauth_clients_file::load_from(path).map_err(FileSecretsError::File)? else {
        return Ok(0);
    };
    let entries = [("google", &file.google), ("microsoft", &file.microsoft)];
    let mut moved = 0usize;
    for (provider, entry) in entries {
        for (secret, value) in [
            (OauthClientSecret::ClientSecret, &entry.client_secret),
            (OauthClientSecret::ApiKey, &entry.api_key),
        ] {
            if !value.trim().is_empty() {
                save(store, provider, secret, value)?;
                moved += 1;
            }
        }
    }
    if moved > 0 {
        oauth_clients_file::strip_secrets_at(path).map_err(FileSecretsError::File)?;
        tracing::event!(
            tracing::Level::INFO,
            moved,
            "auth.oauth_client.file_secrets_moved"
        );
    }
    Ok(moved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secret_storage::MemorySecretStore;

    fn file_with(dir: &tempfile::TempDir, toml: &str) -> std::path::PathBuf {
        let path = oauth_clients_file::oauth_clients_path_in(dir.path());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, toml).unwrap();
        path
    }

    #[test]
    fn test_a_file_holding_secrets_gives_them_to_the_store_and_keeps_its_ids() {
        // GIVEN an oauth-clients.toml an earlier version wrote, secret and API
        // key in clear beside the client id
        let dir = tempfile::tempdir().unwrap();
        let path = file_with(
            &dir,
            "[google]\nclient_id = \"1.apps.googleusercontent.com\"\n\
             client_secret = \"GOCSPX-in-clear\"\napi_key = \"AIza-in-clear\"\n",
        );
        let store = MemorySecretStore::default();

        // WHEN the move runs, as it does at start
        let moved = migrate_file_secrets(&path, &store).unwrap();

        // THEN the store holds both, the file no longer carries either, and the
        // client id is still there
        assert_eq!(moved, 2);
        assert_eq!(
            load(&store, "google", OauthClientSecret::ClientSecret)
                .unwrap()
                .as_deref(),
            Some("GOCSPX-in-clear")
        );
        assert_eq!(
            load(&store, "google", OauthClientSecret::ApiKey)
                .unwrap()
                .as_deref(),
            Some("AIza-in-clear")
        );
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.contains("in-clear"), "{text}");
        assert_eq!(
            oauth_clients_file::lookup_client_id_at(&path, "google").as_deref(),
            Some("1.apps.googleusercontent.com")
        );
    }

    #[test]
    fn test_a_file_without_secrets_is_left_as_it_is() {
        // GIVEN a file holding a client id only
        let dir = tempfile::tempdir().unwrap();
        let raw = "[google]\nclient_id = \"1.apps.googleusercontent.com\"\n";
        let path = file_with(&dir, raw);

        // WHEN the move runs
        let moved = migrate_file_secrets(&path, &MemorySecretStore::default()).unwrap();

        // THEN nothing moves and the file is byte for byte the same
        assert_eq!(moved, 0);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), raw);
    }

    #[test]
    fn test_an_empty_value_removes_the_stored_secret() {
        // GIVEN a stored client secret
        let store = MemorySecretStore::default();
        save(
            &store,
            "google",
            OauthClientSecret::ClientSecret,
            "GOCSPX-1",
        )
        .unwrap();

        // WHEN an empty value is saved over it
        save(&store, "google", OauthClientSecret::ClientSecret, "  ").unwrap();

        // THEN it reads as absent
        assert_eq!(
            load(&store, "google", OauthClientSecret::ClientSecret).unwrap(),
            None
        );
    }
}
