//! The OAuth clients a connector needs before a browser window opens: the
//! client id of each provider, kept in `oauth-clients.toml` under the data
//! directory, and its client secret and API key, kept in the secret store.
//!
//! The desktop running the runtime in its own process calls these functions
//! directly; a client of the daemon reaches the same functions through
//! `/api/v1/oauth/clients`. Both therefore read the same file, answer the same
//! rows and refuse with the same words.
//!
//! Every value resolves through the same chain as the connectors that spend
//! it: the runtime environment variable, then the file (client id) or the
//! secret store (client secret, API key), then the value compiled into the
//! build. A row reports which layer won, never a secret.

use std::path::{Path, PathBuf};

use apollia_auth::oauth_client_secrets::{self, OauthClientSecret};
use apollia_auth::oauth_clients_file::{self, OAUTH_CLIENTS_FILENAME};
use apollia_auth::{ConnectorProvider, SecretStore};
use serde::{Deserialize, Serialize};

use crate::inference_servers::SecretStoreOpener;

/// The providers whose OAuth client the operator configures, in the order the
/// settings list them.
const PROVIDERS: [ConnectorProvider; 2] = [ConnectorProvider::Google, ConnectorProvider::Microsoft];

/// Why an OAuth client operation did not complete.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum OauthClientsError {
    /// The provider id is not one Apollia connects to.
    #[error("unknown provider: {0}")]
    UnknownProvider(String),
    /// The credentials file is not a recognisable OAuth client export.
    #[error("invalid OAuth client file: {0}")]
    InvalidClientFile(String),
    /// The home directory, and therefore the file, cannot be resolved.
    #[error("cannot determine the home directory")]
    NoHome,
    /// The file could not be written.
    #[error("internal: {0}")]
    Write(#[source] std::io::Error),
    /// The secret store could not be opened or refused the value.
    #[error("secret store unavailable: {0}")]
    Secrets(String),
}

/// One provider's OAuth client, as the settings draw it.
///
/// Reports the **effective** client id of the resolution chain
/// (`env var → override file → compiled default`), where it came from, and
/// whether a client secret and an API key resolve. Never carries a secret or
/// a key, only their presence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct OauthClientIdStatus {
    /// Provider id (`"google"` / `"microsoft"`).
    pub provider: String,
    /// Effective client id used at runtime. Empty string when no source is configured.
    pub effective_client_id: String,
    /// Source that produced the effective value: `"env"`, `"file"`, `"builtin"`, or `"none"`.
    pub source: String,
    /// Override stored in `oauth-clients.toml`, if any.
    pub override_client_id: Option<String>,
    /// True when a client secret resolves through any source.
    pub has_client_secret: bool,
    /// Source of the secret: `"env"`, `"store"` (the secret store), `"builtin"`, or `"none"`.
    pub client_secret_source: String,
    /// True when this provider needs a client secret (Google), false for a
    /// public client (Microsoft).
    pub requires_client_secret: bool,
    /// True when an API key resolves through any source. Google Picker needs
    /// one; Microsoft entries leave this false.
    pub has_api_key: bool,
    /// Source for the resolved API key. Same semantics as `client_secret_source`.
    pub api_key_source: String,
    /// True when this provider needs an API key (Google, for Picker only).
    pub requires_api_key: bool,
}

/// The verdict of a non-interactive check of a provider's OAuth client.
///
/// A full validation (that the authorization server accepts the client) needs
/// an interactive consent and a token exchange, which cannot run headless.
/// This is the strongest safe check: credentials present, well-formed, and the
/// provider's authorization server reachable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct OauthClientTestResult {
    /// True when every check passed.
    pub ok: bool,
    /// Human-readable summary: what passed, or the first failing check.
    pub detail: String,
}

/// Which field of a provider's entry a write sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OauthClientField {
    /// The client id.
    ClientId,
    /// The client secret.
    ClientSecret,
    /// The public API key (Google Picker).
    ApiKey,
}

/// `oauth-clients.toml` under `data_dir`.
pub fn file_in(data_dir: &Path) -> PathBuf {
    data_dir.join(OAUTH_CLIENTS_FILENAME)
}

/// `oauth-clients.toml` under the data directory of the current home, the
/// file the connectors read.
///
/// # Errors
///
/// [`OauthClientsError::NoHome`] when the home directory cannot be resolved.
pub fn default_file() -> Result<PathBuf, OauthClientsError> {
    oauth_clients_file::oauth_clients_path().ok_or(OauthClientsError::NoHome)
}

/// The provider named by `id`.
///
/// # Errors
///
/// [`OauthClientsError::UnknownProvider`] for an id Apollia does not connect to.
pub fn provider(id: &str) -> Result<ConnectorProvider, OauthClientsError> {
    ConnectorProvider::from_id(id).ok_or_else(|| OauthClientsError::UnknownProvider(id.to_owned()))
}

/// Whether `provider` needs a client secret at its token endpoint (Google's
/// Installed App type does, Microsoft public clients do not).
pub const fn requires_client_secret(provider: ConnectorProvider) -> bool {
    matches!(provider, ConnectorProvider::Google)
}

const fn requires_api_key(provider: ConnectorProvider) -> bool {
    matches!(provider, ConnectorProvider::Google)
}

/// The process environment, as the resolution chain reads it.
fn process_env(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

/// One value of the resolution chain and the layer that produced it:
/// `stored` comes from `stored_layer`, the file or the secret store.
fn resolve(
    env: &dyn Fn(&str) -> Option<String>,
    env_var: &str,
    stored: Option<String>,
    stored_layer: &'static str,
    compiled: &str,
) -> (Option<String>, &'static str) {
    if let Some(value) = env(env_var).filter(|v| !v.is_empty()) {
        return (Some(value), "env");
    }
    if let Some(value) = stored {
        return (Some(value), stored_layer);
    }
    if !compiled.is_empty() {
        return (Some(compiled.to_owned()), "builtin");
    }
    (None, "none")
}

/// The stored `secret` of `provider`, or `None` when absent, when no store
/// was opened, or when the store fails (logged).
fn stored_secret(
    secrets: Option<&dyn SecretStore>,
    provider: ConnectorProvider,
    secret: OauthClientSecret,
) -> Option<String> {
    let store = secrets?;
    match oauth_client_secrets::load(store, provider.id(), secret) {
        Ok(value) => value,
        Err(e) => {
            tracing::event!(
                tracing::Level::WARN,
                provider = %provider.id(),
                error = %e,
                "runtime.oauth_clients.secret_unreadable"
            );
            None
        }
    }
}

/// The store `secrets` opens, or `None` (logged) when it cannot be opened.
fn open_secrets(secrets: &SecretStoreOpener) -> Option<Box<dyn SecretStore>> {
    match secrets() {
        Ok(store) => Some(store),
        Err(e) => {
            tracing::event!(
                tracing::Level::WARN,
                error = %e,
                "runtime.oauth_clients.secret_store_unavailable"
            );
            None
        }
    }
}

fn status_of(
    provider: ConnectorProvider,
    file: &Path,
    env: &dyn Fn(&str) -> Option<String>,
    secrets: Option<&dyn SecretStore>,
) -> OauthClientIdStatus {
    let id = provider.id();
    let override_client_id = oauth_clients_file::lookup_client_id_at(file, id);
    let (effective, source) = resolve(
        env,
        provider.client_id_env_var(),
        override_client_id.clone(),
        "file",
        provider.default_client_id(),
    );
    let (secret, secret_source) = resolve(
        env,
        provider.client_secret_env_var(),
        stored_secret(secrets, provider, OauthClientSecret::ClientSecret),
        "store",
        provider.default_client_secret(),
    );
    let (api_key, api_key_source) = resolve(
        env,
        provider.api_key_env_var(),
        stored_secret(secrets, provider, OauthClientSecret::ApiKey),
        "store",
        provider.default_api_key(),
    );
    OauthClientIdStatus {
        provider: id.to_owned(),
        effective_client_id: effective.unwrap_or_default(),
        source: source.to_owned(),
        override_client_id,
        has_client_secret: secret.is_some(),
        client_secret_source: secret_source.to_owned(),
        requires_client_secret: requires_client_secret(provider),
        has_api_key: api_key.is_some(),
        api_key_source: api_key_source.to_owned(),
        requires_api_key: requires_api_key(provider),
    }
}

/// The OAuth client of every provider, resolved against `file` and the
/// secret store `secrets` opens.
pub fn list_client_ids(file: &Path, secrets: &SecretStoreOpener) -> Vec<OauthClientIdStatus> {
    let store = open_secrets(secrets);
    list_client_ids_with(file, &process_env, store.as_deref())
}

fn list_client_ids_with(
    file: &Path,
    env: &dyn Fn(&str) -> Option<String>,
    secrets: Option<&dyn SecretStore>,
) -> Vec<OauthClientIdStatus> {
    PROVIDERS
        .iter()
        .map(|p| status_of(*p, file, env, secrets))
        .collect()
}

/// Writes one field of `provider_id`'s client, trimmed: the client id into
/// `file`, the client secret and the API key into the secret store `secrets`
/// opens, never into the file. An empty value clears it, so the chain falls
/// back to the environment or the build. The file is created on demand,
/// `0o600` on Unix.
///
/// # Errors
///
/// [`OauthClientsError::UnknownProvider`] for an unknown provider,
/// [`OauthClientsError::Write`] when the file cannot be written,
/// [`OauthClientsError::Secrets`] when the secret store fails.
pub fn set_field(
    file: &Path,
    provider_id: &str,
    field: OauthClientField,
    value: &str,
    secrets: &SecretStoreOpener,
) -> Result<(), OauthClientsError> {
    let id = provider(provider_id)?.id();
    let value = value.trim();
    let secret = match field {
        OauthClientField::ClientId => {
            oauth_clients_file::set_client_id_at(file, id, value)
                .map_err(OauthClientsError::Write)?;
            None
        }
        OauthClientField::ClientSecret => Some(OauthClientSecret::ClientSecret),
        OauthClientField::ApiKey => Some(OauthClientSecret::ApiKey),
    };
    if let Some(secret) = secret {
        let store = secrets().map_err(OauthClientsError::Secrets)?;
        oauth_client_secrets::save(store.as_ref(), id, secret, value)
            .map_err(|e| OauthClientsError::Secrets(e.to_string()))?;
    }
    tracing::event!(
        tracing::Level::INFO,
        provider = %id,
        field = ?field,
        cleared = value.is_empty(),
        "runtime.oauth_clients.field_written"
    );
    Ok(())
}

/// Returns `Some(reason)` when `client_id` is malformed for `provider`.
pub fn client_id_shape_error(provider: ConnectorProvider, client_id: &str) -> Option<String> {
    match provider {
        ConnectorProvider::Google if !client_id.ends_with(".apps.googleusercontent.com") => {
            Some("Google client id should end with .apps.googleusercontent.com".to_string())
        }
        ConnectorProvider::Microsoft if !is_guid_like(client_id) => {
            Some("Microsoft client id should be a GUID (Azure application id)".to_string())
        }
        _ => None,
    }
}

/// Whether `s` looks like a canonical GUID (`8-4-4-4-12` hex groups).
fn is_guid_like(s: &str) -> bool {
    let groups: Vec<&str> = s.split('-').collect();
    groups.len() == 5
        && groups.iter().map(|g| g.len()).eq([8usize, 4, 4, 4, 12])
        && groups
            .iter()
            .all(|g| g.chars().all(|c| c.is_ascii_hexdigit()))
}

/// The provider's OIDC discovery document, reachable without credentials. A
/// successful GET proves the authorization server is up and DNS/TLS resolve.
const fn discovery_url(provider: ConnectorProvider) -> &'static str {
    match provider {
        ConnectorProvider::Google => "https://accounts.google.com/.well-known/openid-configuration",
        ConnectorProvider::Microsoft => {
            "https://login.microsoftonline.com/common/v2.0/.well-known/openid-configuration"
        }
    }
}

/// The first local check that fails for `provider`, before any network call.
fn local_check(
    provider: ConnectorProvider,
    file: &Path,
    env: &dyn Fn(&str) -> Option<String>,
    secrets: Option<&dyn SecretStore>,
) -> Option<String> {
    let status = status_of(provider, file, env, secrets);
    if status.effective_client_id.trim().is_empty() {
        return Some("OAuth client id is not configured".to_string());
    }
    if let Some(reason) = client_id_shape_error(provider, &status.effective_client_id) {
        return Some(reason);
    }
    if status.requires_client_secret && !status.has_client_secret {
        return Some("client secret is required for this provider but not configured".to_string());
    }
    None
}

/// Checks `provider_id`'s OAuth client non-interactively, stopping at the
/// first failure: a client id resolves, it is well-formed, a client secret
/// resolves where the provider needs one, and the provider's discovery
/// endpoint answers. It inspects only the presence of the credentials, never
/// sends them, and calls a public, credential-less URL.
///
/// # Errors
///
/// [`OauthClientsError::UnknownProvider`] for an unknown provider. A failing
/// check is a verdict, not an error.
pub async fn test_client(
    file: &Path,
    provider_id: &str,
    secrets: &SecretStoreOpener,
) -> Result<OauthClientTestResult, OauthClientsError> {
    let provider = provider(provider_id)?;
    let local = {
        let store = open_secrets(secrets);
        local_check(provider, file, &process_env, store.as_deref())
    };
    if let Some(detail) = local {
        return Ok(OauthClientTestResult { ok: false, detail });
    }
    let client = match apollia_core::net::safe_client() {
        Ok(client) => client,
        Err(e) => {
            return Ok(OauthClientTestResult {
                ok: false,
                detail: format!("failed to build the HTTP client: {e}"),
            })
        }
    };
    let verdict = match client.get(discovery_url(provider)).send().await {
        Ok(resp) if resp.status().is_success() => OauthClientTestResult {
            ok: true,
            detail: format!(
                "client id present and well-formed; {} authorization server reachable",
                provider.id()
            ),
        },
        Ok(resp) => OauthClientTestResult {
            ok: false,
            detail: format!("authorization server returned HTTP {}", resp.status()),
        },
        Err(e) => OauthClientTestResult {
            ok: false,
            detail: format!("authorization server unreachable: {e}"),
        },
    };
    Ok(verdict)
}

/// Extract the client id and optional secret from a Google Cloud OAuth client
/// export.
///
/// The console hands the operator a JSON file rather than two strings on a
/// page, so reading the file directly removes the step where they hunt for
/// `client_id` inside it and paste the wrong half. Desktop clients nest under
/// `installed`; `web` is accepted too because the console produces that shape
/// for the other client type and the fields are identical.
fn parse_google_client_json(raw: &str) -> Result<(String, Option<String>), OauthClientsError> {
    let root: serde_json::Value = serde_json::from_str(raw)
        .map_err(|e| OauthClientsError::InvalidClientFile(format!("not valid JSON: {e}")))?;
    let section = root
        .get("installed")
        .or_else(|| root.get("web"))
        .ok_or_else(|| {
            OauthClientsError::InvalidClientFile(
                "expected an \"installed\" or \"web\" object at the root".to_owned(),
            )
        })?;
    let field = |name: &str| {
        section
            .get(name)
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
    };
    let client_id = field("client_id")
        .ok_or_else(|| OauthClientsError::InvalidClientFile("no client_id field".to_owned()))?;
    Ok((client_id, field("client_secret")))
}

/// Imports an OAuth client from the JSON text the provider's console exports:
/// the client id into `file`, the secret into the secret store.
///
/// Google only: Microsoft's portal shows the application id on screen and
/// issues no secret for a public client, so there is no file to import. The
/// window reads the file the operator picked; this function receives its text,
/// so the import serves a daemon as well as the runtime of this process.
///
/// # Errors
///
/// [`OauthClientsError::UnknownProvider`] for an unknown provider,
/// [`OauthClientsError::InvalidClientFile`] for Microsoft or a text that is
/// not a Google client export, [`OauthClientsError::Write`] when the file
/// cannot be written, [`OauthClientsError::Secrets`] when the secret store
/// fails.
pub fn import_client_json(
    file: &Path,
    provider_id: &str,
    client_json: &str,
    secrets: &SecretStoreOpener,
) -> Result<(), OauthClientsError> {
    let provider = provider(provider_id)?;
    if provider != ConnectorProvider::Google {
        return Err(OauthClientsError::InvalidClientFile(format!(
            "{} does not publish a credentials file; paste the application id instead",
            provider.id()
        )));
    }
    let (client_id, client_secret) = parse_google_client_json(client_json)?;
    if let Some(reason) = client_id_shape_error(provider, &client_id) {
        return Err(OauthClientsError::InvalidClientFile(reason));
    }
    if let Some(secret) = client_secret {
        set_field(
            file,
            provider.id(),
            OauthClientField::ClientSecret,
            &secret,
            secrets,
        )?;
    }
    set_field(
        file,
        provider.id(),
        OauthClientField::ClientId,
        &client_id,
        secrets,
    )?;
    Ok(())
}

/// Startup step: moves the client secrets and API keys an earlier version
/// wrote in clear in `file` into the secret store `secrets` opens, and
/// rewrites the file with the client ids only. Returns how many values moved.
///
/// The store is opened only when the file holds a secret. A failure is
/// logged and leaves the file as it was, so the next start replays it.
pub(crate) fn migrate_file_secrets_at_startup(file: &Path, secrets: &SecretStoreOpener) -> usize {
    let holds_secret = match oauth_clients_file::load_from(file) {
        Ok(Some(f)) => [&f.google, &f.microsoft]
            .iter()
            .any(|e| !e.client_secret.trim().is_empty() || !e.api_key.trim().is_empty()),
        Ok(None) => false,
        Err(e) => {
            tracing::event!(
                tracing::Level::WARN,
                error = %e,
                "runtime.oauth_clients.migration.unreadable"
            );
            false
        }
    };
    if !holds_secret {
        return 0;
    }
    let moved = secrets().map_err(|e| e.to_string()).and_then(|store| {
        oauth_client_secrets::migrate_file_secrets(file, store.as_ref()).map_err(|e| e.to_string())
    });
    match moved {
        Ok(moved) => moved,
        Err(e) => {
            tracing::event!(
                tracing::Level::WARN,
                error = %e,
                detail = "left in the file, retried at the next start",
                "runtime.oauth_clients.migration.failed"
            );
            0
        }
    }
}

#[cfg(test)]
mod tests;
