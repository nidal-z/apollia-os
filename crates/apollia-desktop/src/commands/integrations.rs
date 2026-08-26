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

//! The credential surface lives in its own module: `credentials` holds the
//! client ids, secrets, API keys and Drive folders, and the checks that refuse
//! a half-resolved credential.

pub mod credentials;

use credentials::ensure_credentials;

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
                detail = "the manual link stays available",
                "oauth.browser.open.failed"
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
        // GIVEN the two connector identifiers the settings page offers
        // WHEN each is resolved to a provider
        // THEN both resolve
        assert!(provider_from_id("google").is_ok());
        assert!(provider_from_id("microsoft").is_ok());
    }

    #[test]
    fn test_provider_from_id_unknown_returns_err() {
        // GIVEN an identifier no connector claims
        // WHEN it is resolved to a provider
        let err = provider_from_id("github").unwrap_err();
        // THEN the refusal names the offending identifier
        match err {
            IntegrationsError::UnknownProvider(name) => assert_eq!(name, "github"),
            other => panic!("expected UnknownProvider, got: {other:?}"),
        }
    }

    #[test]
    fn test_build_google_provider_with_known_scopes() {
        // GIVEN two Apollia scope names for Google
        // WHEN the provider configuration is built from them
        let cfg = build_provider_with_scopes(
            ConnectorProvider::Google,
            &["mail.send".to_string(), "calendar.read".to_string()],
        )
        .expect("build");
        // THEN they are translated into the OAuth scopes Google expects
        assert!(cfg.scopes.iter().any(|s| s.contains("gmail.send")));
        assert!(cfg.scopes.iter().any(|s| s.contains("calendar.readonly")));
    }

    #[test]
    fn test_build_microsoft_provider_with_known_scopes() {
        // GIVEN two Apollia scope names for Microsoft
        // WHEN the provider configuration is built from them
        let cfg = build_provider_with_scopes(
            ConnectorProvider::Microsoft,
            &["mail.read".to_string(), "calendar.write".to_string()],
        )
        .expect("build");
        // THEN they are translated into the Graph permissions Microsoft expects
        assert!(cfg.scopes.contains(&"Mail.Read"));
        assert!(cfg.scopes.contains(&"Calendars.ReadWrite"));
    }

    #[test]
    fn test_unknown_scope_returns_err() {
        // GIVEN a scope name no provider declares
        // WHEN the provider configuration is built from it
        let err =
            build_provider_with_scopes(ConnectorProvider::Google, &["drive.read_all".to_string()])
                .unwrap_err();
        // THEN the refusal names the offending scope rather than asking for it silently
        match err {
            IntegrationsError::UnknownScope(name) => assert_eq!(name, "drive.read_all"),
            other => panic!("expected UnknownScope, got: {other:?}"),
        }
    }

    #[test]
    fn test_sovereignty_gate_allows_cloud_when_cloud_allowed() {
        // GIVEN a profile that allows the cloud
        // WHEN the sovereignty gate is asked
        // THEN the call goes through
        assert!(ensure_cloud_allowed(SovereigntyProfile::CloudAllowed).is_ok());
    }

    #[test]
    fn test_sovereignty_gate_blocks_when_local_only() {
        // GIVEN a local-only profile
        // WHEN the sovereignty gate is asked
        let err = ensure_cloud_allowed(SovereigntyProfile::LocalOnly).unwrap_err();
        // THEN the call is blocked, which is what keeps an OAuth round trip from starting
        assert!(matches!(err, IntegrationsError::SovereigntyBlocked));
    }

    #[test]
    fn test_integrations_error_serializes_with_kind_tag() {
        // GIVEN an integrations error
        let err = IntegrationsError::UnknownProvider("foo".into());
        // WHEN it crosses the bridge as JSON
        let json = serde_json::to_value(&err).expect("serialize");
        // THEN it carries a kind tag the front end can branch on
        assert_eq!(json["kind"], "unknown_provider");
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
}
