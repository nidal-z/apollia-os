//! Tauri IPC commands for native SaaS connector integrations.
//!
//! Exposes the OAuth flow + multi-account management for the Google Workspace
//! and Microsoft 365 connectors (apollia-connectors + apollia-auth). The
//! commands are consumed by the desktop `/integrations` route.
//!
//! ## State management
//!
//! v0.1.0 keeps the `AuthManager` instance behind a lazily-initialized
//! `OnceCell<Arc<AuthManager>>`. This avoids touching `main.rs` to register
//! the manager as a Tauri-managed resource (deferred refactor) while still
//! preserving the in-memory token cache + singleflight refresh between
//! commands.
//!
//! Pending OAuth flows are tracked in a separate `DashMap<state, FlowEntry>`
//! so the `complete_flow` handler can pair the callback `state` with the
//! original PKCE verifier and provider configuration.
//!
//! ## Sovereignty profile
//!
//! When the user has selected the `local_only` sovereignty profile, every
//! OAuth-starting command returns [`IntegrationsError::SovereigntyBlocked`]
//! and the UI surfaces a static explanation panel instead of the wizards.
//!
//! ## Credentials
//!
//! Builds embed Apollia's own Microsoft client, and no Google client at all
//! (see `apollia_auth::connector_providers`). Either way every connect attempt
//! goes through [`credential_gate`] first, so an unresolved or half-resolved
//! credential is refused, by name, before a browser window opens.

use std::sync::{Arc, OnceLock};

use apollia_auth::{
    build_google_provider, build_microsoft_provider, AccountId, AuthManager, ConnectorProvider,
    GoogleScope, MicrosoftScope, OAuth2PkceFlow, ProviderConfig, StoredToken,
};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tokio::sync::OnceCell;

// ─── Shared state ────────────────────────────────────────────────────────────

/// Process-wide AuthManager instance. Lazily initialised on first command
/// call so the desktop binary boots fast even when no integration is in use.
/// Async OnceCell because AuthManager::new() is itself fallible and we want
/// concurrent callers to share the initialisation.
static AUTH_MANAGER: OnceCell<Arc<AuthManager>> = OnceCell::const_new();

/// Pending OAuth flows, keyed by the `state` parameter the user's browser
/// sends back to the local callback URL.
static PENDING_FLOWS: OnceLock<DashMap<String, FlowEntry>> = OnceLock::new();

struct FlowEntry {
    provider: ConnectorProvider,
    flow: OAuth2PkceFlow,
    provider_config: ProviderConfig,
}

/// Lazy accessor for the process-wide [`AuthManager`]. Exposed crate-internal
/// so the connector executor bridge (`apollia_desktop::connectors_bridge`)
/// can resolve bearer tokens on every tool call.
pub(crate) async fn auth_manager() -> Result<Arc<AuthManager>, IntegrationsError> {
    AUTH_MANAGER
        .get_or_try_init(|| async {
            AuthManager::new()
                .map(Arc::new)
                .map_err(|e| IntegrationsError::Internal(e.to_string()))
        })
        .await
        .map(Arc::clone)
}

fn pending_flows() -> &'static DashMap<String, FlowEntry> {
    PENDING_FLOWS.get_or_init(DashMap::new)
}

// ─── Error ───────────────────────────────────────────────────────────────────

/// Error surfaced to the JS frontend over the Tauri IPC boundary.
///
/// `Serialize` so Tauri's bincode IPC can ship the variant tag + payload to
/// the renderer for nice error messaging.
#[derive(Debug, thiserror::Error, Serialize)]
#[serde(tag = "kind", content = "detail", rename_all = "snake_case")]
#[non_exhaustive]
pub enum IntegrationsError {
    /// The active sovereignty profile (`local_only`) bans cloud connectors.
    #[error("sovereignty profile blocks cloud connectors")]
    SovereigntyBlocked,
    /// The provider has no OAuth client id configured.
    ///
    /// Google's normal state on a fresh install: no build embeds a Google
    /// client, so this stands until the operator supplies one through Settings
    /// → Integrations, `~/.apollia/oauth-clients.toml`, or the runtime env
    /// var. Microsoft resolves its shipped client instead and cannot reach
    /// this variant on a build that carries the constant: an override that is
    /// present but empty is skipped, not honoured, so it falls back to the
    /// shipped identifier rather than to nothing.
    /// The UI surfaces a clear "OAuth client not configured" message instead
    /// of letting the flow fail mid-handshake with an opaque AS error.
    #[error("OAuth client not configured for {0}")]
    OauthClientNotConfigured(String),
    /// The provider needs a client secret at the token endpoint and none is
    /// configured.
    ///
    /// Google's Installed App client type requires the secret even under
    /// PKCE. Without this guard the browser opens, the user grants consent,
    /// and only then does the token exchange fail with an opaque
    /// `invalid_client` from Google. Refusing up front costs the user nothing
    /// and names the missing piece.
    #[error("OAuth client secret not configured for {0}")]
    OauthClientSecretMissing(String),
    /// The credentials file the operator picked is not a recognisable OAuth
    /// client export.
    #[error("invalid OAuth client file: {0}")]
    InvalidClientFile(String),
    /// The provider id sent by the frontend is not recognised.
    #[error("unknown provider: {0}")]
    UnknownProvider(String),
    /// The scope group is not recognised for the provider.
    #[error("unknown scope: {0}")]
    UnknownScope(String),
    /// The pending flow with this `state` cannot be found (timeout, mismatch, …).
    #[error("no pending flow with state {0}")]
    FlowNotFound(String),
    /// A token store / keyring error.
    #[error("auth: {0}")]
    Auth(String),
    /// Catch-all internal failure.
    #[error("internal: {0}")]
    Internal(String),
}

// ─── DTOs ───────────────────────────────────────────────────────────────────

/// Result of `oauth_start_flow`: the URL the desktop should open in the
/// system browser, plus the opaque `state` the frontend will echo back.
#[derive(Debug, Clone, Serialize)]
pub struct OauthStartFlow {
    /// Full authorization URL with PKCE challenge + state + scope.
    pub auth_url: String,
    /// Opaque state value to be echoed back to `oauth_complete_flow`.
    pub state: String,
    /// Port of the local callback HTTP listener (already bound).
    pub callback_port: u16,
}

/// Result of `oauth_complete_flow`: minimal account info to confirm success.
#[derive(Debug, Clone, Serialize)]
pub struct OauthCompletedAccount {
    /// Provider id (`google` / `microsoft`).
    pub provider: String,
    /// The connected account identifier (email or UPN).
    pub account_id: String,
    /// Scopes the AS actually granted.
    pub granted_scopes: Vec<String>,
}

/// Account metadata returned by `oauth_get_status`.
#[derive(Debug, Clone, Serialize)]
pub struct OauthAccountInfo {
    /// Provider id.
    pub provider: String,
    /// Account id.
    pub account_id: String,
}

// ─── Provider + scope resolution ─────────────────────────────────────────────

fn provider_from_id(id: &str) -> Result<ConnectorProvider, IntegrationsError> {
    match id {
        "google" => Ok(ConnectorProvider::Google),
        "microsoft" => Ok(ConnectorProvider::Microsoft),
        other => Err(IntegrationsError::UnknownProvider(other.to_owned())),
    }
}

fn build_provider_with_scopes(
    provider: ConnectorProvider,
    scopes: &[String],
) -> Result<ProviderConfig, IntegrationsError> {
    match provider {
        ConnectorProvider::Google => {
            let mut typed = Vec::with_capacity(scopes.len());
            for s in scopes {
                let g = match s.as_str() {
                    "mail.send" => GoogleScope::MailSend,
                    "mail.drafts" => GoogleScope::MailDraftsCreate,
                    // Restricted `gmail.compose`: Expert Mode only, kept for
                    // power users who supply their own OAuth client.
                    "mail.compose" => GoogleScope::MailCompose,
                    "calendar.read" => GoogleScope::CalendarRead,
                    "calendar.write" => GoogleScope::CalendarWrite,
                    "drive.workspace" => GoogleScope::DriveWorkspace,
                    "sheets" => GoogleScope::SheetsReadWrite,
                    "docs" => GoogleScope::DocsReadWrite,
                    "slides" => GoogleScope::SlidesReadWrite,
                    "tasks" => GoogleScope::Tasks,
                    "forms" => GoogleScope::FormsReadWrite,
                    "youtube" => GoogleScope::YouTubeReadOnly,
                    "openid" => GoogleScope::OpenId,
                    "email" => GoogleScope::Email,
                    "profile" => GoogleScope::Profile,
                    other => return Err(IntegrationsError::UnknownScope(other.to_owned())),
                };
                typed.push(g);
            }
            Ok(build_google_provider(&typed))
        }
        ConnectorProvider::Microsoft => {
            let mut typed = Vec::with_capacity(scopes.len());
            for s in scopes {
                let m = match s.as_str() {
                    "mail.read" => MicrosoftScope::MailRead,
                    "mail.send" => MicrosoftScope::MailSend,
                    "calendar.read" => MicrosoftScope::CalendarRead,
                    "calendar.write" => MicrosoftScope::CalendarWrite,
                    "files.read" => MicrosoftScope::FilesRead,
                    "files.write" => MicrosoftScope::FilesWrite,
                    "profile" => MicrosoftScope::Profile,
                    "offline" => MicrosoftScope::Offline,
                    other => return Err(IntegrationsError::UnknownScope(other.to_owned())),
                };
                typed.push(m);
            }
            Ok(build_microsoft_provider(&typed))
        }
    }
}

// ─── Sovereignty gate ────────────────────────────────────────────────────────

/// Sovereignty profile (mirror of the desktop config field). When set to
/// `local_only`, cloud OAuth flows are blocked at the command boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SovereigntyProfile {
    /// All cloud connectors allowed.
    CloudAllowed,
    /// Local-only: block all cloud OAuth flows.
    LocalOnly,
}

/// Convenience guard returning [`IntegrationsError::SovereigntyBlocked`] when
/// the active profile forbids cloud connectors.
fn ensure_cloud_allowed(profile: SovereigntyProfile) -> Result<(), IntegrationsError> {
    match profile {
        SovereigntyProfile::CloudAllowed => Ok(()),
        SovereigntyProfile::LocalOnly => Err(IntegrationsError::SovereigntyBlocked),
    }
}

// ─── Tauri commands ──────────────────────────────────────────────────────────

/// Start an OAuth flow for the given provider + scopes.
///
/// Binds the local callback HTTP listener, builds the PKCE flow + auth URL,
/// **spawns a background task** that awaits the browser callback and emits
/// the captured `code` to the frontend via the `oauth://code-ready` Tauri
/// event, and returns the auth URL the renderer should open in the system
/// browser. The frontend listens for the event and finalises the flow with
/// [`oauth_complete_flow`]: no manual code paste needed on the happy path.
///
/// When the loopback can't fire (browser blocked the redirect, headless
/// environment, user closed the tab before granting), the frontend keeps a
/// manual paste fallback so the user can recover the code from the URL bar.
#[tauri::command]
pub async fn oauth_start_flow(
    app: tauri::AppHandle,
    provider: String,
    scopes: Vec<String>,
    sovereignty: SovereigntyProfile,
) -> Result<OauthStartFlow, IntegrationsError> {
    use tauri::Emitter;

    ensure_cloud_allowed(sovereignty)?;
    let provider_id = provider_from_id(&provider)?;
    ensure_credentials(provider_id)?;
    let provider_config = build_provider_with_scopes(provider_id, &scopes)?;

    let (listener, port) = apollia_auth::callback::bind_ephemeral_port()
        .await
        .map_err(|e| IntegrationsError::Internal(e.to_string()))?;
    let flow = OAuth2PkceFlow::new(port);
    let auth_url = apollia_auth::build_auth_url(&provider_config, &flow);
    let state = flow.state.clone();

    // Open the system browser via the Tauri opener plugin; `window.open`
    // from the renderer is unreliable in Tauri 2 webviews (browser sandboxing
    // blocks the popup silently on some platforms). The opener plugin goes
    // through the OS shell, same path as `xdg-open` / `open` / `start`.
    {
        use tauri_plugin_opener::OpenerExt;
        if let Err(err) = app.opener().open_url(&auth_url, None::<&str>) {
            tracing::warn!(
                error = %err,
                "failed to auto-open the system browser; user can still use the manual link"
            );
        }
    }

    pending_flows().insert(
        state.clone(),
        FlowEntry {
            provider: provider_id,
            flow,
            provider_config,
        },
    );

    // Spawn the loopback callback waiter; emit a Tauri event as soon as the
    // browser hits 127.0.0.1:port/callback. Frontend auto-finalises by
    // calling `oauth_complete_flow(state, code)`.
    let app_handle = app.clone();
    let state_for_task = state.clone();
    tokio::spawn(async move {
        match apollia_auth::callback::wait_for_callback(listener, &state_for_task).await {
            Ok(code) => {
                let _ = app_handle.emit(
                    "oauth://code-ready",
                    serde_json::json!({ "state": state_for_task, "code": code }),
                );
            }
            Err(err) => {
                let _ = app_handle.emit(
                    "oauth://error",
                    serde_json::json!({ "state": state_for_task, "error": err.to_string() }),
                );
            }
        }
    });

    Ok(OauthStartFlow {
        auth_url,
        state,
        callback_port: port,
    })
}

/// Complete the OAuth flow with the authorization code returned by the
/// browser callback.
#[tauri::command]
pub async fn oauth_complete_flow(
    state: String,
    code: String,
) -> Result<OauthCompletedAccount, IntegrationsError> {
    let (provider, flow, provider_config) = {
        let entry = pending_flows()
            .remove(&state)
            .ok_or_else(|| IntegrationsError::FlowNotFound(state.clone()))?
            .1;
        (entry.provider, entry.flow, entry.provider_config)
    };

    let token: StoredToken = apollia_auth::token::exchange_code(&provider_config, &flow, &code)
        .await
        .map_err(|e| IntegrationsError::Auth(e.to_string()))?;

    let account_id = resolve_account_id(provider, &token.access_token).await?;
    let manager = auth_manager().await?;
    manager
        .put_token(provider, &account_id, token.clone())
        .await
        .map_err(|e| IntegrationsError::Auth(e.to_string()))?;

    Ok(OauthCompletedAccount {
        provider: provider.id().to_owned(),
        account_id: account_id.0,
        granted_scopes: token.scopes,
    })
}

/// Revoke (forget) the token for `(provider, account_id)`.
///
/// The local keyring entry is cleared. The upstream is not notified; call
/// the provider's revocation endpoint separately if a server-side revoke is
/// required.
#[tauri::command]
pub async fn oauth_disconnect(
    provider: String,
    account_id: String,
) -> Result<(), IntegrationsError> {
    let provider_id = provider_from_id(&provider)?;
    let manager = auth_manager().await?;
    manager
        .revoke(provider_id, &AccountId::new(account_id))
        .await
        .map_err(|e| IntegrationsError::Auth(e.to_string()))?;
    Ok(())
}

/// List connected accounts across all providers.
#[tauri::command]
pub async fn oauth_get_status() -> Result<Vec<OauthAccountInfo>, IntegrationsError> {
    let manager = auth_manager().await?;
    let mut out = Vec::new();
    for provider in [ConnectorProvider::Google, ConnectorProvider::Microsoft] {
        let accounts = manager
            .list_accounts(provider)
            .await
            .map_err(|e| IntegrationsError::Auth(e.to_string()))?;
        for a in accounts {
            out.push(OauthAccountInfo {
                provider: provider.id().to_owned(),
                account_id: a.0,
            });
        }
    }
    Ok(out)
}

// ─── OAuth client_id overrides (~/.apollia/oauth-clients.toml) ──────────────

/// One row of `oauth_list_client_ids`.
///
/// Reports the **effective** client id used by the resolution chain
/// (`env var → override file → compiled default`) so the UI can show the
/// user what is currently active per provider, and whether the override
/// file contains a per-provider entry. Also reports whether a client_secret
/// is configured (Google Installed App needs one, Microsoft does not).
#[derive(Debug, Clone, Serialize)]
pub struct OauthClientIdStatus {
    /// Provider id (`"google"` / `"microsoft"`).
    pub provider: String,
    /// Effective client id used at runtime. Empty string when no source is configured.
    pub effective_client_id: String,
    /// Source that produced the effective value: `"env"`, `"file"`, `"builtin"`, or `"none"`.
    pub source: String,
    /// Override stored in `~/.apollia/oauth-clients.toml`, if any.
    pub override_client_id: Option<String>,
    /// True when a client_secret is resolved through any source. Never
    /// returns the secret value itself; the UI only renders a presence dot.
    pub has_client_secret: bool,
    /// Same source semantics as `source` but for the secret.
    pub client_secret_source: String,
    /// True when this provider requires a client_secret to function. Hint for
    /// the UI to surface a "secret missing" warning on Google but not on
    /// Microsoft where it's intentional.
    pub requires_client_secret: bool,
    /// True when an API key is resolved through any source. Google Picker
    /// needs one; Microsoft entries leave this false.
    pub has_api_key: bool,
    /// Source for the resolved API key. Same semantics as `source`.
    pub api_key_source: String,
    /// True when this provider needs an API key (Google for Picker only).
    pub requires_api_key: bool,
}

fn detect_source(provider: ConnectorProvider) -> &'static str {
    if std::env::var(provider.client_id_env_var())
        .ok()
        .filter(|v| !v.is_empty())
        .is_some()
    {
        return "env";
    }
    if apollia_auth::oauth_clients_file::lookup_client_id(provider.id()).is_some() {
        return "file";
    }
    if !provider.default_client_id().is_empty() {
        return "builtin";
    }
    "none"
}

fn detect_secret_source(provider: ConnectorProvider) -> &'static str {
    if std::env::var(provider.client_secret_env_var())
        .ok()
        .filter(|v| !v.is_empty())
        .is_some()
    {
        return "env";
    }
    if apollia_auth::oauth_clients_file::lookup_client_secret(provider.id()).is_some() {
        return "file";
    }
    if !provider.default_client_secret().is_empty() {
        return "builtin";
    }
    "none"
}

fn detect_api_key_source(provider: ConnectorProvider) -> &'static str {
    if std::env::var(provider.api_key_env_var())
        .ok()
        .filter(|v| !v.is_empty())
        .is_some()
    {
        return "env";
    }
    if apollia_auth::oauth_clients_file::lookup_api_key(provider.id()).is_some() {
        return "file";
    }
    if !provider.default_api_key().is_empty() {
        return "builtin";
    }
    "none"
}

const fn provider_requires_secret(provider: ConnectorProvider) -> bool {
    matches!(provider, ConnectorProvider::Google)
}

/// Refuse an OAuth handshake whose credentials are incomplete, before any
/// browser window opens.
///
/// Checking the client id alone is not enough: a Google client id without its
/// paired secret passes every local check, opens the consent screen, and fails
/// only at the token endpoint, after the user has already granted access. The
/// resulting `invalid_client` names nothing the operator can act on.
///
/// Pure on purpose. The callers resolve the three-source chain
/// (`ConnectorProvider::resolve_*`) and hand the outcome in, which keeps this
/// testable without touching process environment.
fn credential_gate(
    provider: ConnectorProvider,
    client_id: Option<&str>,
    client_secret: Option<&str>,
) -> Result<(), IntegrationsError> {
    let has_value = |v: Option<&str>| v.is_some_and(|s| !s.trim().is_empty());

    if !has_value(client_id) {
        return Err(IntegrationsError::OauthClientNotConfigured(
            provider.id().to_owned(),
        ));
    }
    if provider_requires_secret(provider) && !has_value(client_secret) {
        return Err(IntegrationsError::OauthClientSecretMissing(
            provider.id().to_owned(),
        ));
    }
    Ok(())
}

/// Resolve a provider's credentials and run them through [`credential_gate`].
fn ensure_credentials(provider: ConnectorProvider) -> Result<(), IntegrationsError> {
    let client_id = provider.resolve_client_id();
    let client_secret = provider.resolve_client_secret();
    credential_gate(provider, client_id.as_deref(), client_secret.as_deref())
}

const fn provider_requires_api_key(provider: ConnectorProvider) -> bool {
    matches!(provider, ConnectorProvider::Google)
}

/// Snapshot the current OAuth client_id configuration per provider.
///
/// Used by the Settings → Integrations OAuth panel to render which client id
/// is active and where it came from. Safe to call without a sovereignty check
/// since the response contains no token material, only the public client id.
#[tauri::command]
pub async fn oauth_list_client_ids() -> Result<Vec<OauthClientIdStatus>, IntegrationsError> {
    let mut out = Vec::new();
    for provider in [ConnectorProvider::Google, ConnectorProvider::Microsoft] {
        let effective = provider.resolve_client_id().unwrap_or_default();
        let source = detect_source(provider).to_string();
        let override_value = apollia_auth::oauth_clients_file::lookup_client_id(provider.id());
        let secret_source = detect_secret_source(provider).to_string();
        let has_secret = provider.resolve_client_secret().is_some();
        let api_key_source = detect_api_key_source(provider).to_string();
        let has_api_key = provider.resolve_api_key().is_some();
        out.push(OauthClientIdStatus {
            provider: provider.id().to_owned(),
            effective_client_id: effective,
            source,
            override_client_id: override_value,
            has_client_secret: has_secret,
            client_secret_source: secret_source,
            requires_client_secret: provider_requires_secret(provider),
            has_api_key,
            api_key_source,
            requires_api_key: provider_requires_api_key(provider),
        });
    }
    Ok(out)
}

/// Result of `oauth_test_client`: a lightweight, non-interactive check of a
/// provider's configured OAuth client credentials.
///
/// A full validation (that the authorization server actually accepts the
/// client) requires an interactive user consent + token exchange, which cannot
/// run headless. This is the strongest safe check: credentials present +
/// well-formed + the provider's authorization server reachable.
#[derive(Debug, Clone, Serialize)]
pub struct OauthClientTestResult {
    /// True when every check passed.
    pub ok: bool,
    /// Human-readable summary: what passed, or the first failing check.
    pub detail: String,
}

/// The provider's OIDC discovery document, reachable without credentials. A
/// successful GET proves the authorization server is up and DNS/TLS resolve.
const fn provider_discovery_url(provider: ConnectorProvider) -> &'static str {
    match provider {
        ConnectorProvider::Google => "https://accounts.google.com/.well-known/openid-configuration",
        ConnectorProvider::Microsoft => {
            "https://login.microsoftonline.com/common/v2.0/.well-known/openid-configuration"
        }
    }
}

/// Returns `Some(reason)` when `client_id` is malformed for `provider`.
fn client_id_shape_error(provider: ConnectorProvider, client_id: &str) -> Option<String> {
    match provider {
        ConnectorProvider::Google => {
            if client_id.ends_with(".apps.googleusercontent.com") {
                None
            } else {
                Some("Google client id should end with .apps.googleusercontent.com".to_string())
            }
        }
        ConnectorProvider::Microsoft => {
            if is_guid_like(client_id) {
                None
            } else {
                Some("Microsoft client id should be a GUID (Azure application id)".to_string())
            }
        }
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

/// Validate a provider's configured OAuth client credentials non-interactively.
///
/// Runs three cheap checks in order and stops at the first failure:
/// 1. an effective client id is configured (env / file / build-time chain);
/// 2. the client id is well-formed for the provider;
/// 3. for providers that need a client secret (Google), one is configured;
/// 4. the provider's OIDC discovery endpoint is reachable over the network.
///
/// Returns a typed `{ ok, detail }`. Safe to call without a sovereignty check:
/// it inspects only local credential *presence* (never their values) and hits a
/// public, credential-less discovery URL.
#[tauri::command]
pub async fn oauth_test_client(
    provider: String,
) -> Result<OauthClientTestResult, IntegrationsError> {
    let provider_id = provider_from_id(&provider)?;

    let client_id = match provider_id.resolve_client_id() {
        Some(id) if !id.trim().is_empty() => id,
        _ => {
            return Ok(OauthClientTestResult {
                ok: false,
                detail: "OAuth client id is not configured".to_string(),
            })
        }
    };

    if let Some(reason) = client_id_shape_error(provider_id, &client_id) {
        return Ok(OauthClientTestResult {
            ok: false,
            detail: reason,
        });
    }

    if provider_requires_secret(provider_id) && provider_id.resolve_client_secret().is_none() {
        return Ok(OauthClientTestResult {
            ok: false,
            detail: "client secret is required for this provider but not configured".to_string(),
        });
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
    match client.get(provider_discovery_url(provider_id)).send().await {
        Ok(resp) if resp.status().is_success() => Ok(OauthClientTestResult {
            ok: true,
            detail: format!(
                "client id present and well-formed; {} authorization server reachable",
                provider_id.id()
            ),
        }),
        Ok(resp) => Ok(OauthClientTestResult {
            ok: false,
            detail: format!("authorization server returned HTTP {}", resp.status()),
        }),
        Err(e) => Ok(OauthClientTestResult {
            ok: false,
            detail: format!("authorization server unreachable: {e}"),
        }),
    }
}

/// Write the client_id override for `provider` into `~/.apollia/oauth-clients.toml`.
///
/// Passing an empty string clears the override (the resolution chain then falls
/// back to env var or compiled default). The file is created on demand with
/// 0o600 permissions on Unix.
#[tauri::command]
pub async fn oauth_set_client_id(
    provider: String,
    client_id: String,
) -> Result<(), IntegrationsError> {
    let provider_id = provider_from_id(&provider)?;
    apollia_auth::oauth_clients_file::set_client_id(provider_id.id(), client_id.trim())
        .map_err(|e| IntegrationsError::Internal(e.to_string()))?;
    Ok(())
}

// ─── Drive folder preferences (~/.apollia/drive-prefs.toml) ─────────────────

/// One row of `oauth_list_drive_folders`. Lists the user-configured Drive
/// root path per Google account, plus the effective fallback so the UI can
/// distinguish "explicit override" from "default Apollia folder".
#[derive(Debug, Clone, Serialize)]
pub struct DriveFolderStatus {
    /// Account id (typically the user's Google email).
    pub account_id: String,
    /// Explicit user-set path, when present (`None` means falling back to default).
    pub folder_path: Option<String>,
    /// Effective folder path used at runtime (override or default).
    pub effective_folder_path: String,
}

/// List the per-account Drive folder configuration for every connected
/// Google account. Used by the Settings → Integrations Google card so the
/// user can review/edit each account's folder.
#[tauri::command]
pub async fn oauth_list_drive_folders() -> Result<Vec<DriveFolderStatus>, IntegrationsError> {
    let manager = auth_manager().await?;
    let accounts = manager
        .list_accounts(ConnectorProvider::Google)
        .await
        .map_err(|e| IntegrationsError::Auth(e.to_string()))?;
    let mut out = Vec::with_capacity(accounts.len());
    for account in accounts {
        let override_path =
            apollia_auth::drive_prefs::lookup_folder_path("google", account.as_str());
        let effective =
            apollia_auth::drive_prefs::effective_folder_path("google", account.as_str());
        out.push(DriveFolderStatus {
            account_id: account.0,
            folder_path: override_path,
            effective_folder_path: effective,
        });
    }
    Ok(out)
}

/// Persist the Drive folder path for one Google account.
///
/// Tri-state semantics (mirrors `drive_prefs::AccountDrivePref::folder_path`):
/// - non-empty path → walk `Documents/.../X`, nest `<agent_slug>` inside.
/// - empty string → user *explicitly* chose My Drive root; agents create
///   `<agent_slug>` directly under root, no `Apollia` intermediate.
/// - to fall back to the default behaviour (legacy `Apollia/<slug>`), call
///   [`oauth_reset_drive_folder`] instead.
///
/// The path is sanitised (leading/trailing slashes stripped, repeated `/`
/// collapsed) before being written.
#[tauri::command]
pub async fn oauth_set_drive_folder(
    account_id: String,
    folder_path: String,
) -> Result<(), IntegrationsError> {
    apollia_auth::drive_prefs::set_folder_path("google", account_id.trim(), folder_path.trim())
        .map_err(|e| IntegrationsError::Internal(e.to_string()))?;
    Ok(())
}

/// Clear the Drive folder override for one Google account, restoring the
/// default `Apollia/<agent_slug>` placement. Different from
/// [`oauth_set_drive_folder`] called with an empty string; that means
/// "use Drive root". Picked folders (Google Picker) are preserved.
#[tauri::command]
pub async fn oauth_reset_drive_folder(account_id: String) -> Result<(), IntegrationsError> {
    apollia_auth::drive_prefs::reset_folder_path("google", account_id.trim())
        .map_err(|e| IntegrationsError::Internal(e.to_string()))?;
    Ok(())
}

/// Persist the public Google API key used by Google Picker. Stored
/// alongside the client_id / client_secret in `~/.apollia/oauth-clients.toml`.
/// Empty string clears the override.
#[tauri::command]
pub async fn oauth_set_api_key(provider: String, api_key: String) -> Result<(), IntegrationsError> {
    let provider_id = provider_from_id(&provider)?;
    apollia_auth::oauth_clients_file::set_api_key(provider_id.id(), api_key.trim())
        .map_err(|e| IntegrationsError::Internal(e.to_string()))?;
    Ok(())
}

/// Write the client_secret override for `provider`. Passing an empty string
/// clears the override. Same on-disk file as `oauth_set_client_id`.
///
/// Only meaningful for Google (Installed App requires a secret at the token
/// endpoint per their non-standard implementation). Microsoft public clients
/// don't use a secret; calling this for `"microsoft"` is allowed but pointless.
#[tauri::command]
pub async fn oauth_set_client_secret(
    provider: String,
    client_secret: String,
) -> Result<(), IntegrationsError> {
    let provider_id = provider_from_id(&provider)?;
    apollia_auth::oauth_clients_file::set_client_secret(provider_id.id(), client_secret.trim())
        .map_err(|e| IntegrationsError::Internal(e.to_string()))?;
    Ok(())
}

// ─── Credentials file import ────────────────────────────────────────────────

/// Extract the client id and optional secret from a Google Cloud OAuth client
/// export.
///
/// The console hands the operator a JSON file rather than two strings on a
/// page, so reading the file directly removes the step where they hunt for
/// `client_id` inside it and paste the wrong half. Desktop clients nest under
/// `installed`; `web` is accepted too because the console produces that shape
/// for the other client type and the fields are identical.
fn parse_google_client_json(raw: &str) -> Result<(String, Option<String>), IntegrationsError> {
    let root: serde_json::Value = serde_json::from_str(raw)
        .map_err(|e| IntegrationsError::InvalidClientFile(format!("not valid JSON: {e}")))?;

    let section = root
        .get("installed")
        .or_else(|| root.get("web"))
        .ok_or_else(|| {
            IntegrationsError::InvalidClientFile(
                "expected an \"installed\" or \"web\" object at the root".to_owned(),
            )
        })?;

    let client_id = section
        .get("client_id")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| IntegrationsError::InvalidClientFile("no client_id field".to_owned()))?
        .to_owned();

    let client_secret = section
        .get("client_secret")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned);

    Ok((client_id, client_secret))
}

/// Import an OAuth client from the JSON file the provider's console produces.
///
/// Google only for now: Microsoft's portal shows the application id on screen
/// and issues no secret for a public client, so there is no file to import.
/// Both fields land in `~/.apollia/oauth-clients.toml` through the same
/// writers the manual fields use, which already create the file `0o600`.
#[tauri::command]
pub async fn oauth_import_client_json(
    provider: String,
    path: String,
) -> Result<(), IntegrationsError> {
    let provider_id = provider_from_id(&provider)?;
    if provider_id != ConnectorProvider::Google {
        return Err(IntegrationsError::InvalidClientFile(format!(
            "{} does not publish a credentials file; paste the application id instead",
            provider_id.id()
        )));
    }

    let raw = std::fs::read_to_string(path.trim())
        .map_err(|e| IntegrationsError::InvalidClientFile(format!("cannot read the file: {e}")))?;
    let (client_id, client_secret) = parse_google_client_json(&raw)?;

    if let Some(reason) = client_id_shape_error(provider_id, &client_id) {
        return Err(IntegrationsError::InvalidClientFile(reason));
    }

    apollia_auth::oauth_clients_file::set_client_id(provider_id.id(), &client_id)
        .map_err(|e| IntegrationsError::Internal(e.to_string()))?;
    if let Some(secret) = client_secret {
        apollia_auth::oauth_clients_file::set_client_secret(provider_id.id(), &secret)
            .map_err(|e| IntegrationsError::Internal(e.to_string()))?;
    }
    Ok(())
}

// ─── Userinfo probe ──────────────────────────────────────────────────────────

async fn resolve_account_id(
    provider: ConnectorProvider,
    bearer: &str,
) -> Result<AccountId, IntegrationsError> {
    let client =
        apollia_core::net::safe_client().map_err(|e| IntegrationsError::Internal(e.to_string()))?;
    let response = client
        .get(provider.userinfo_url())
        .bearer_auth(bearer)
        .send()
        .await
        .map_err(|e| IntegrationsError::Internal(e.to_string()))?;
    if !response.status().is_success() {
        return Err(IntegrationsError::Internal(format!(
            "userinfo returned {}",
            response.status()
        )));
    }
    let json: serde_json::Value =
        apollia_core::net::read_capped_json(response, apollia_core::net::MAX_METADATA_BYTES)
            .await
            .map_err(|e| IntegrationsError::Internal(e.to_string()))?;
    let id = match provider {
        ConnectorProvider::Google => json
            .get("email")
            .and_then(|v| v.as_str())
            .ok_or_else(|| IntegrationsError::Internal("userinfo missing email".into()))?,
        ConnectorProvider::Microsoft => json
            .get("userPrincipalName")
            .and_then(|v| v.as_str())
            .or_else(|| json.get("mail").and_then(|v| v.as_str()))
            .ok_or_else(|| {
                IntegrationsError::Internal("Graph /me missing userPrincipalName".into())
            })?,
    };
    Ok(AccountId::new(id))
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_from_id_known_returns_ok() {
        assert!(provider_from_id("google").is_ok());
        assert!(provider_from_id("microsoft").is_ok());
    }

    #[test]
    fn test_provider_from_id_unknown_returns_err() {
        let err = provider_from_id("github").unwrap_err();
        match err {
            IntegrationsError::UnknownProvider(name) => assert_eq!(name, "github"),
            other => panic!("expected UnknownProvider, got: {other:?}"),
        }
    }

    #[test]
    fn test_build_google_provider_with_known_scopes() {
        let cfg = build_provider_with_scopes(
            ConnectorProvider::Google,
            &["mail.send".to_string(), "calendar.read".to_string()],
        )
        .expect("build");
        assert!(cfg.scopes.iter().any(|s| s.contains("gmail.send")));
        assert!(cfg.scopes.iter().any(|s| s.contains("calendar.readonly")));
    }

    #[test]
    fn test_build_microsoft_provider_with_known_scopes() {
        let cfg = build_provider_with_scopes(
            ConnectorProvider::Microsoft,
            &["mail.read".to_string(), "calendar.write".to_string()],
        )
        .expect("build");
        assert!(cfg.scopes.contains(&"Mail.Read"));
        assert!(cfg.scopes.contains(&"Calendars.ReadWrite"));
    }

    #[test]
    fn test_unknown_scope_returns_err() {
        let err =
            build_provider_with_scopes(ConnectorProvider::Google, &["drive.read_all".to_string()])
                .unwrap_err();
        match err {
            IntegrationsError::UnknownScope(name) => assert_eq!(name, "drive.read_all"),
            other => panic!("expected UnknownScope, got: {other:?}"),
        }
    }

    #[test]
    fn test_sovereignty_gate_allows_cloud_when_cloud_allowed() {
        assert!(ensure_cloud_allowed(SovereigntyProfile::CloudAllowed).is_ok());
    }

    #[test]
    fn test_sovereignty_gate_blocks_when_local_only() {
        let err = ensure_cloud_allowed(SovereigntyProfile::LocalOnly).unwrap_err();
        assert!(matches!(err, IntegrationsError::SovereigntyBlocked));
    }

    #[test]
    fn test_is_guid_like_accepts_canonical_guid() {
        assert!(is_guid_like("12345678-1234-1234-1234-123456789abc"));
        assert!(is_guid_like("DEADBEEF-0000-1111-2222-333344445555"));
    }

    #[test]
    fn test_is_guid_like_rejects_malformed() {
        assert!(!is_guid_like("not-a-guid"));
        assert!(!is_guid_like("12345678123412341234123456789abc"));
        assert!(!is_guid_like("12345678-1234-1234-1234-123456789abz"));
        assert!(!is_guid_like(""));
    }

    #[test]
    fn test_client_id_shape_error_google() {
        // GIVEN a well-formed Google client id
        assert!(client_id_shape_error(
            ConnectorProvider::Google,
            "123-abc.apps.googleusercontent.com"
        )
        .is_none());
        // AND a malformed one
        assert!(client_id_shape_error(ConnectorProvider::Google, "bogus").is_some());
    }

    #[test]
    fn test_client_id_shape_error_microsoft() {
        assert!(client_id_shape_error(
            ConnectorProvider::Microsoft,
            "12345678-1234-1234-1234-123456789abc"
        )
        .is_none());
        assert!(client_id_shape_error(ConnectorProvider::Microsoft, "not-a-guid").is_some());
    }

    #[test]
    fn test_oauth_client_test_result_serializes() {
        let result = OauthClientTestResult {
            ok: true,
            detail: "reachable".to_string(),
        };
        let json = serde_json::to_value(&result).expect("serialize");
        assert_eq!(json["ok"], true);
        assert_eq!(json["detail"], "reachable");
    }

    #[test]
    fn test_integrations_error_serializes_with_kind_tag() {
        let err = IntegrationsError::UnknownProvider("foo".into());
        let json = serde_json::to_value(&err).expect("serialize");
        assert_eq!(json["kind"], "unknown_provider");
    }

    #[test]
    fn test_credential_gate_google_without_secret_is_refused() {
        // GIVEN a Google client id configured but no paired secret
        let client_id = Some("123-abc.apps.googleusercontent.com");
        let client_secret = None;

        // WHEN the connect attempt is gated
        let err = credential_gate(ConnectorProvider::Google, client_id, client_secret)
            .expect_err("Google without a secret must not reach the browser");

        // THEN the refusal names the secret, before any consent screen opens
        match err {
            IntegrationsError::OauthClientSecretMissing(p) => assert_eq!(p, "google"),
            other => panic!("expected OauthClientSecretMissing, got: {other:?}"),
        }
    }

    #[test]
    fn test_credential_gate_google_blank_secret_counts_as_missing() {
        // GIVEN a secret entry that holds only whitespace
        let client_secret = Some("   ");

        // WHEN the connect attempt is gated
        let err = credential_gate(
            ConnectorProvider::Google,
            Some("123-abc.apps.googleusercontent.com"),
            client_secret,
        )
        .expect_err("a blank secret is not a secret");

        // THEN it is treated exactly like an absent one
        assert!(matches!(
            err,
            IntegrationsError::OauthClientSecretMissing(_)
        ));
    }

    #[test]
    fn test_credential_gate_microsoft_needs_no_secret() {
        // GIVEN a Microsoft public client, which the spec says carries no secret
        let client_id = Some("00000000-1111-2222-3333-444444444444");

        // WHEN the connect attempt is gated without one
        let result = credential_gate(ConnectorProvider::Microsoft, client_id, None);

        // THEN it is allowed through
        assert!(result.is_ok(), "Microsoft must connect without a secret");
    }

    #[test]
    fn test_credential_gate_without_client_id_is_refused_first() {
        // GIVEN no client id at all, for a provider that also wants a secret
        // WHEN the connect attempt is gated
        let err = credential_gate(ConnectorProvider::Google, None, None)
            .expect_err("no client id means nothing to connect with");

        // THEN the missing client id is reported, not the missing secret
        match err {
            IntegrationsError::OauthClientNotConfigured(p) => assert_eq!(p, "google"),
            other => panic!("expected OauthClientNotConfigured, got: {other:?}"),
        }
    }

    #[test]
    fn test_secret_missing_error_serializes_with_its_own_kind() {
        // GIVEN the new refusal
        let err = IntegrationsError::OauthClientSecretMissing("google".into());

        // WHEN it crosses the IPC boundary
        let json = serde_json::to_value(&err).expect("serialize");

        // THEN the frontend can tell it apart from a missing client id
        assert_eq!(json["kind"], "oauth_client_secret_missing");
        assert_eq!(json["detail"], "google");
    }

    #[test]
    fn test_parse_google_client_json_reads_installed_section() {
        // GIVEN the file a Google Cloud Desktop client download produces
        let raw = r#"{
            "installed": {
                "client_id": "123-abc.apps.googleusercontent.com",
                "project_id": "apollia-test",
                "client_secret": "GOCSPX-example",
                "redirect_uris": ["http://localhost"]
            }
        }"#;

        // WHEN it is parsed
        let (id, secret) = parse_google_client_json(raw).expect("parse");

        // THEN both halves come out, so the operator never reads the file
        assert_eq!(id, "123-abc.apps.googleusercontent.com");
        assert_eq!(secret.as_deref(), Some("GOCSPX-example"));
    }

    #[test]
    fn test_parse_google_client_json_accepts_web_section() {
        // GIVEN the other shape the console produces
        let raw = r#"{"web": {"client_id": "456-def.apps.googleusercontent.com"}}"#;

        // WHEN it is parsed
        let (id, secret) = parse_google_client_json(raw).expect("parse");

        // THEN the client id is read and the absent secret stays absent
        assert_eq!(id, "456-def.apps.googleusercontent.com");
        assert!(secret.is_none());
    }

    #[test]
    fn test_parse_google_client_json_rejects_unknown_shape() {
        // GIVEN a JSON file that is not an OAuth client export
        let raw = r#"{"type": "service_account", "private_key": "..."}"#;

        // WHEN it is parsed
        let err =
            parse_google_client_json(raw).expect_err("service accounts are not OAuth clients");

        // THEN the operator is told what was expected
        match err {
            IntegrationsError::InvalidClientFile(reason) => assert!(reason.contains("installed")),
            other => panic!("expected InvalidClientFile, got: {other:?}"),
        }
    }

    #[test]
    fn test_parse_google_client_json_rejects_malformed_input() {
        // GIVEN a file that is not JSON at all
        // WHEN it is parsed
        let err = parse_google_client_json("not json").expect_err("must not accept garbage");

        // THEN the failure is attributed to the file, not to Apollia
        assert!(matches!(err, IntegrationsError::InvalidClientFile(_)));
    }
}
