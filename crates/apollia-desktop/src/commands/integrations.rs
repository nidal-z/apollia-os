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
//! Cf. plan §8.0.

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

async fn auth_manager() -> Result<Arc<AuthManager>, IntegrationsError> {
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
pub enum IntegrationsError {
    /// The active sovereignty profile (`local_only`) bans cloud connectors.
    #[error("sovereignty profile blocks cloud connectors")]
    SovereigntyBlocked,
    /// The provider has no OAuth client id configured.
    ///
    /// Means the build was made without `APOLLIA_BUILD_*_CLIENT_ID` and the
    /// user has not set the runtime override either. The UI surfaces a clear
    /// "OAuth client not configured" message instead of letting the flow
    /// fail mid-handshake with an opaque AS error.
    #[error("OAuth client not configured for {0}")]
    OauthClientNotConfigured(String),
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

/// Result of `oauth_start_flow` — the URL the desktop should open in the
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

/// Result of `oauth_complete_flow` — minimal account info to confirm success.
#[derive(Debug, Clone, Serialize)]
pub struct OauthCompletedAccount {
    /// Provider id (`google` / `microsoft`).
    pub provider: String,
    /// The connected account identifier (email or UPN).
    pub account_id: String,
    /// Scopes the AS actually granted.
    pub granted_scopes: Vec<String>,
}

/// Account metadata returned by `oauth_list_accounts`.
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
                    "mail.compose" => GoogleScope::MailCompose,
                    "calendar.read" => GoogleScope::CalendarRead,
                    "calendar.write" => GoogleScope::CalendarWrite,
                    "drive.workspace" => GoogleScope::DriveWorkspace,
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
    /// Local-only — block all cloud OAuth flows.
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
/// stores the pending entry, and returns the URL the renderer should open in
/// the system browser.
#[tauri::command]
pub async fn oauth_start_flow(
    provider: String,
    scopes: Vec<String>,
    sovereignty: SovereigntyProfile,
) -> Result<OauthStartFlow, IntegrationsError> {
    ensure_cloud_allowed(sovereignty)?;
    let provider_id = provider_from_id(&provider)?;
    if provider_id.resolve_client_id().is_none() {
        return Err(IntegrationsError::OauthClientNotConfigured(
            provider_id.id().to_owned(),
        ));
    }
    let provider_config = build_provider_with_scopes(provider_id, &scopes)?;

    let (_listener, port) = apollia_auth::callback::bind_ephemeral_port()
        .await
        .map_err(|e| IntegrationsError::Internal(e.to_string()))?;
    let flow = OAuth2PkceFlow::new(port);
    let auth_url = apollia_auth::build_auth_url(&provider_config, &flow);
    let state = flow.state.clone();
    pending_flows().insert(
        state.clone(),
        FlowEntry {
            provider: provider_id,
            flow,
            provider_config,
        },
    );
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

/// List the connected accounts for `provider`.
#[tauri::command]
pub async fn oauth_list_accounts(provider: String) -> Result<Vec<OauthAccountInfo>, IntegrationsError> {
    let provider_id = provider_from_id(&provider)?;
    let manager = auth_manager().await?;
    let accounts = manager
        .list_accounts(provider_id)
        .await
        .map_err(|e| IntegrationsError::Auth(e.to_string()))?;
    Ok(accounts
        .into_iter()
        .map(|a| OauthAccountInfo {
            provider: provider_id.id().to_owned(),
            account_id: a.0,
        })
        .collect())
}

/// Revoke (forget) the token for `(provider, account_id)`.
///
/// The local keyring entry is cleared. The upstream is not notified — call
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
/// file contains a per-provider entry.
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

/// Snapshot the current OAuth client_id configuration per provider.
///
/// Used by the Settings → Integrations OAuth panel to render which client id
/// is active and where it came from. Safe to call without a sovereignty check
/// since the response contains no token material — only the public client id.
#[tauri::command]
pub async fn oauth_list_client_ids() -> Result<Vec<OauthClientIdStatus>, IntegrationsError> {
    let mut out = Vec::new();
    for provider in [ConnectorProvider::Google, ConnectorProvider::Microsoft] {
        let effective = provider.resolve_client_id().unwrap_or_default();
        let source = detect_source(provider).to_string();
        let override_value = apollia_auth::oauth_clients_file::lookup_client_id(provider.id());
        out.push(OauthClientIdStatus {
            provider: provider.id().to_owned(),
            effective_client_id: effective,
            source,
            override_client_id: override_value,
        });
    }
    Ok(out)
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

// ─── Userinfo probe ──────────────────────────────────────────────────────────

async fn resolve_account_id(
    provider: ConnectorProvider,
    bearer: &str,
) -> Result<AccountId, IntegrationsError> {
    let client = reqwest::Client::new();
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
    let json: serde_json::Value = response
        .json()
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
        let err = build_provider_with_scopes(
            ConnectorProvider::Google,
            &["drive.read_all".to_string()],
        )
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
    fn test_integrations_error_serializes_with_kind_tag() {
        let err = IntegrationsError::UnknownProvider("foo".into());
        let json = serde_json::to_value(&err).expect("serialize");
        assert_eq!(json["kind"], "unknown_provider");
    }
}
