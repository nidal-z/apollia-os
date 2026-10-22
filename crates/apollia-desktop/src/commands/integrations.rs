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
//! ## Organisation policy
//!
//! Starting a consent is submitted to the runtime's connection admission
//! before the loopback port is bound, so a policy that denies remote
//! connections refuses the flow with [`IntegrationsError::RefusedByPolicy`].
//! The check reads the policy the runtime loaded, never a value the renderer
//! sends.

//! ## Credentials
//!
//! Builds embed Apollia's own Microsoft client, and no Google client at all
//! (see `apollia_auth::connector_providers`). Either way every connect attempt
//! goes through [`credential_gate`] first, so an unresolved or half-resolved
//! credential is refused, by name, before a browser window opens.
//!
//! ## Governance
//!
//! An account is submitted to the runtime's connection admission before its
//! code is exchanged, and connecting or disconnecting one is journaled, the
//! same as through the runtime's HTTP consent routes.
//!
//! ## Attached mode
//!
//! Attached to the daemon `apollia-os start`, the consent, the accounts and
//! the Drive folders are the daemon's: the commands reach its routes through
//! [`attached`], and the window keeps only its loopback listener and the
//! browser it opens.

//! The credential surface lives in its own module: `credentials` holds the
//! client ids, secrets, API keys and Drive folders, and the checks that refuse
//! a half-resolved credential.

mod admin_approval;
mod attached;
pub mod credentials;

use credentials::ensure_credentials;

use std::sync::{Arc, OnceLock};

use apollia_auth::{
    AccountId, AuthManager, ConnectorProvider, ConsentError, OAuth2PkceFlow, ProviderConfig,
    StoredToken,
};
use apollia_runtime::governance::{
    AdmissionDecision, AdmissionRequest, ConnectionEvent, ConnectionGovernance,
};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tauri::State;
use tokio::sync::OnceCell;

use crate::attached::RuntimeLink;

/// Who the connection journal names for an account changed from the desktop.
const DESKTOP_ACTOR: &str = "desktop";

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

/// Lazy accessor for the process-wide [`AuthManager`]. The desktop keeps it for
/// the consent half alone, the part that opens a browser; the executors that
/// spend the resulting tokens live in `apollia_runtime::connectors_bridge` and
/// reach the same OS keychain through their own singleton.
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
    /// The connection admission refused the account; the detail is its reason.
    #[error("refused by the connection policy: {0}")]
    RefusedByPolicy(String),
    /// The provider answered that an administrator of the user's
    /// organisation must approve the application; the detail is the approval
    /// page to send them.
    #[error("an administrator must approve the application: {0}")]
    AdminApprovalRequired(String),
    /// Catch-all internal failure.
    #[error("internal: {0}")]
    Internal(String),
    /// The daemon the desktop is attached to refused or did not answer. The
    /// text is the one the interface shows, the daemon's own words when it
    /// gave some, so it crosses the bridge as a plain string.
    #[error("{0}")]
    #[serde(untagged)]
    Service(String),
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OauthCompletedAccount {
    /// Provider id (`google` / `microsoft`).
    pub provider: String,
    /// The connected account identifier (email or UPN).
    pub account_id: String,
    /// Scopes the AS actually granted.
    pub granted_scopes: Vec<String>,
}

/// Account metadata returned by `oauth_get_status`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OauthAccountInfo {
    /// Provider id.
    pub provider: String,
    /// Account id.
    pub account_id: String,
}

// ─── Provider + scope resolution ─────────────────────────────────────────────

fn provider_from_id(id: &str) -> Result<ConnectorProvider, IntegrationsError> {
    ConnectorProvider::from_id(id).ok_or_else(|| IntegrationsError::UnknownProvider(id.to_owned()))
}

/// The scope vocabulary is `apollia_auth`'s, shared with the runtime's HTTP
/// consent route, so a scope added there is offered here without a second
/// table to keep in step.
fn build_provider_with_scopes(
    provider: ConnectorProvider,
    scopes: &[String],
) -> Result<ProviderConfig, IntegrationsError> {
    apollia_auth::connector_consent::provider_config_for(provider, scopes).map_err(|e| match e {
        ConsentError::UnknownScope(name) => IntegrationsError::UnknownScope(name),
        other => IntegrationsError::Internal(other.to_string()),
    })
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
///
/// The account is submitted to the connection admission first, before any
/// port is bound or browser opened. Attached to the daemon, the consent is
/// started there, for the same loopback redirect, under the daemon's own
/// admission, and completed there by [`oauth_complete_flow`].
#[tauri::command]
pub async fn oauth_start_flow(
    app: tauri::AppHandle,
    link: State<'_, RuntimeLink>,
    provider: String,
    scopes: Vec<String>,
) -> Result<OauthStartFlow, IntegrationsError> {
    use tauri::Emitter;

    let sink = match link.inner() {
        RuntimeLink::Embedded(runtime) => {
            admit_start(&runtime.connection_governance, &provider)?;
            admin_approval::StateSink::embedded(&runtime.event_sender)
        }
        RuntimeLink::Attached(_) => None,
    };
    let provider_for_task = ConnectorProvider::from_id(&provider);
    let (listener, port) = apollia_auth::callback::bind_ephemeral_port()
        .await
        .map_err(|e| IntegrationsError::Internal(e.to_string()))?;
    let flow = OAuth2PkceFlow::new(port);
    let (auth_url, state) = match link.inner() {
        RuntimeLink::Embedded(_) => {
            let provider_id = provider_from_id(&provider)?;
            ensure_credentials(provider_id)?;
            let provider_config = build_provider_with_scopes(provider_id, &scopes)?;
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
            (auth_url, state)
        }
        RuntimeLink::Attached(remote) => {
            let started =
                attached::start_consent(remote, &provider, &scopes, &flow.redirect_uri).await?;
            (started.auth_url, started.state)
        }
    };

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
                // An organisation that requires its administrator's approval
                // is kept blocked with the approval page, which the dialog
                // shows to copy.
                let approval = matches!(err, apollia_auth::AuthError::AdminApprovalRequired(_));
                let address = admin_approval::approval_address(
                    provider_for_task,
                    approval,
                    apollia_auth::admin_approval::resolved_admin_consent_url(),
                );
                if let (Some(provider), Some(address)) = (provider_for_task, address.clone()) {
                    admin_approval::keep_blocked(sink, provider, err.to_string(), address).await;
                }
                let _ = app_handle.emit(
                    "oauth://error",
                    serde_json::json!({
                        "state": state_for_task,
                        "error": err.to_string(),
                        "admin_consent_url": address,
                    }),
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
///
/// The account is submitted to the connection admission before the code is
/// exchanged, and the connection is journaled once its token is stored.
/// Attached, the daemon that started the consent does all of it.
#[tauri::command]
pub async fn oauth_complete_flow(
    link: State<'_, RuntimeLink>,
    state: String,
    code: String,
) -> Result<OauthCompletedAccount, IntegrationsError> {
    let runtime = match link.inner() {
        RuntimeLink::Embedded(runtime) => runtime,
        RuntimeLink::Attached(remote) => {
            return attached::complete_consent(remote, &state, &code).await
        }
    };
    let governance = &runtime.connection_governance;
    let (provider, flow, provider_config) = {
        let entry = pending_flows()
            .remove(&state)
            .ok_or_else(|| IntegrationsError::FlowNotFound(state.clone()))?
            .1;
        (entry.provider, entry.flow, entry.provider_config)
    };
    admit_account(governance, provider)?;

    let sink = admin_approval::StateSink::embedded(&runtime.event_sender);
    let token: StoredToken =
        match apollia_auth::token::exchange_code(&provider_config, &flow, &code).await {
            Ok(token) => token,
            Err(e) => return Err(exchange_refused(sink, provider, e.to_string()).await),
        };

    let account_id = resolve_account_id(provider, &token.access_token).await?;
    let manager = auth_manager().await?;
    let granted_scopes = token.scopes.clone();
    store_and_record(governance, &manager, provider, &account_id, token).await?;
    admin_approval::lift_block(sink, provider).await;
    // Publish the tools of the account so it is usable without a restart. A
    // failure is logged: the account is connected, and its tools are
    // published at the next start.
    if let Err(error) = apollia_runtime::connectors_bridge::register_account_tools(
        &runtime.tool_registry_handle,
        provider,
        account_id.as_str(),
    )
    .await
    {
        tracing::event!(
            tracing::Level::WARN,
            provider = %provider.id(),
            error = %error,
            "integrations.account.tools.unpublished"
        );
    }

    Ok(OauthCompletedAccount {
        provider: provider.id().to_owned(),
        account_id: account_id.0,
        granted_scopes,
    })
}

/// The error of a token exchange `provider` refused with `text`: an
/// administrator approval is kept blocked and answered with its approval page,
/// anything else is an authentication error.
async fn exchange_refused(
    sink: Option<admin_approval::StateSink>,
    provider: ConnectorProvider,
    text: String,
) -> IntegrationsError {
    let address = admin_approval::approval_address(
        Some(provider),
        apollia_auth::admin_approval::requires_admin_approval(&text),
        apollia_auth::admin_approval::resolved_admin_consent_url(),
    );
    match address {
        Some(address) => {
            admin_approval::keep_blocked(sink, provider, text, address.clone()).await;
            IntegrationsError::AdminApprovalRequired(address)
        }
        None => IntegrationsError::Auth(text),
    }
}

/// Store the token of `account_id` in `manager`, then journal the connection.
async fn store_and_record(
    governance: &ConnectionGovernance,
    manager: &AuthManager,
    provider: ConnectorProvider,
    account_id: &AccountId,
    token: StoredToken,
) -> Result<(), IntegrationsError> {
    manager
        .put_token(provider, account_id, token)
        .await
        .map_err(|e| IntegrationsError::Auth(e.to_string()))?;
    governance.record_or_warn(&ConnectionEvent::account_connected(
        provider,
        &account_id.0,
        DESKTOP_ACTOR,
    ));
    Ok(())
}

/// Revoke (forget) the token for `(provider, account_id)`.
///
/// The local keyring entry is cleared. The upstream is not notified; call
/// the provider's revocation endpoint separately if a server-side revoke is
/// required. The disconnection is journaled.
#[tauri::command]
pub async fn oauth_disconnect(
    link: State<'_, RuntimeLink>,
    provider: String,
    account_id: String,
) -> Result<(), IntegrationsError> {
    let runtime = match link.inner() {
        RuntimeLink::Embedded(runtime) => runtime,
        RuntimeLink::Attached(remote) => {
            return attached::disconnect(remote, &provider, &account_id).await
        }
    };
    let provider_id = provider_from_id(&provider)?;
    let manager = auth_manager().await?;
    revoke_and_record(
        &runtime.connection_governance,
        &manager,
        provider_id,
        &account_id,
    )
    .await
}

/// Submit an account of `provider` to the connection admission.
/// Submit the provider a consent is about to start for to the connection
/// admission, answering the provider it names.
fn admit_start(
    governance: &ConnectionGovernance,
    provider: &str,
) -> Result<ConnectorProvider, IntegrationsError> {
    let provider = provider_from_id(provider)?;
    admit_account(governance, provider)?;
    Ok(provider)
}

fn admit_account(
    governance: &ConnectionGovernance,
    provider: ConnectorProvider,
) -> Result<(), IntegrationsError> {
    match governance.admit(&AdmissionRequest::for_native_account(provider)) {
        AdmissionDecision::Admitted => Ok(()),
        AdmissionDecision::Refused { reason } => Err(IntegrationsError::RefusedByPolicy(reason)),
    }
}

/// Forget the token of `account_id`, then journal the disconnection.
async fn revoke_and_record(
    governance: &ConnectionGovernance,
    manager: &AuthManager,
    provider: ConnectorProvider,
    account_id: &str,
) -> Result<(), IntegrationsError> {
    manager
        .revoke(provider, &AccountId::new(account_id))
        .await
        .map_err(|e| IntegrationsError::Auth(e.to_string()))?;
    governance.record_or_warn(&ConnectionEvent::account_disconnected(
        provider,
        account_id,
        DESKTOP_ACTOR,
    ));
    Ok(())
}

/// List connected accounts across all providers.
#[tauri::command]
pub async fn oauth_get_status(
    link: State<'_, RuntimeLink>,
) -> Result<Vec<OauthAccountInfo>, IntegrationsError> {
    if let RuntimeLink::Attached(remote) = link.inner() {
        return attached::accounts(remote).await;
    }
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
    apollia_auth::connector_consent::resolve_account_id(provider, bearer)
        .await
        .map_err(|e| match e {
            ConsentError::AccountResolution(msg) => IntegrationsError::Internal(msg),
            other => IntegrationsError::Internal(other.to_string()),
        })
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Token store held in memory, so no test touches the OS keychain.
    #[derive(Default)]
    struct MemoryStore(std::sync::Mutex<std::collections::HashMap<(String, String), String>>);

    impl apollia_auth::SecretStore for MemoryStore {
        fn set(
            &self,
            service: &str,
            user: &str,
            value: &str,
        ) -> Result<(), apollia_auth::AuthError> {
            self.0
                .lock()
                .unwrap()
                .insert((service.to_owned(), user.to_owned()), value.to_owned());
            Ok(())
        }
        fn get(
            &self,
            service: &str,
            user: &str,
        ) -> Result<Option<String>, apollia_auth::AuthError> {
            Ok(self
                .0
                .lock()
                .unwrap()
                .get(&(service.to_owned(), user.to_owned()))
                .cloned())
        }
        fn delete(&self, service: &str, user: &str) -> Result<(), apollia_auth::AuthError> {
            self.0
                .lock()
                .unwrap()
                .remove(&(service.to_owned(), user.to_owned()));
            Ok(())
        }
        fn backend_id(&self) -> &'static str {
            "memory"
        }
    }

    struct Unreadable;

    impl apollia_runtime::governance::ConnectionAdmission for Unreadable {
        fn read_decision(
            &self,
            _request: &AdmissionRequest,
        ) -> Result<AdmissionDecision, apollia_runtime::governance::PolicyReadError> {
            Err(apollia_runtime::governance::PolicyReadError::Unreadable(
                "policy.toml: invalid".into(),
            ))
        }
    }

    // An account the policy cannot be read for is refused before any exchange.
    #[test]
    fn test_account_refused_when_the_policy_cannot_be_read() {
        // GIVEN governance whose policy read fails
        let governance = ConnectionGovernance::new(std::sync::Arc::new(Unreadable), None);

        // WHEN a Google account is submitted
        let err = admit_account(&governance, ConnectorProvider::Google).unwrap_err();

        // THEN it is refused by the policy, naming why
        assert!(
            matches!(&err, IntegrationsError::RefusedByPolicy(reason) if reason.contains("policy.toml")),
            "{err:?}"
        );
        // AND the default governance admits it
        assert!(admit_account(&ConnectionGovernance::default(), ConnectorProvider::Google).is_ok());
    }

    // Connecting from the desktop stores the token and journals it.
    #[tokio::test]
    async fn test_desktop_connect_journals_the_account() {
        // GIVEN an empty isolated store, and governance over a journal
        let dir = tempfile::TempDir::new().unwrap();
        let storage = apollia_auth::MultiAccountStorage::with_store(
            dir.path().join("connectors-index.json"),
            Box::new(MemoryStore::default()),
        );
        let manager = AuthManager::with_storage(storage);
        let journal = apollia_runtime::audit_journal::AuditJournalHandle::open(
            &dir.path().join("journal.db"),
        )
        .await
        .unwrap();
        let governance = ConnectionGovernance::without_policy(Some(journal.clone()));

        // WHEN the token of a Microsoft account is stored
        store_and_record(
            &governance,
            &manager,
            ConnectorProvider::Microsoft,
            &AccountId::new("ada@contoso.test"),
            StoredToken {
                access_token: "at".into(),
                refresh_token: None,
                expires_at: None,
                scopes: vec![],
            },
        )
        .await
        .unwrap();

        // THEN the account is listed and the connection is journaled by the desktop
        let accounts = manager
            .list_accounts(ConnectorProvider::Microsoft)
            .await
            .unwrap();
        assert_eq!(accounts, vec![AccountId::new("ada@contoso.test")]);
        let entries = journal
            .query_run(&apollia_runtime::governance::connection_run_id(
                "native:microsoft:ada@contoso.test",
            ))
            .await;
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0].kind,
            apollia_runtime::audit_journal::JournalEntryKind::AccountConnected
        );
        assert_eq!(entries[0].payload["actor"], "desktop");
        assert_eq!(entries[0].payload["account"], "ada@contoso.test");
        journal.shutdown().await;
    }

    // Disconnecting from the desktop forgets the token and journals it.
    #[tokio::test]
    async fn test_desktop_disconnect_journals_the_account() {
        // GIVEN a connected Microsoft account in an isolated store, and
        // governance over a journal
        let dir = tempfile::TempDir::new().unwrap();
        let storage = apollia_auth::MultiAccountStorage::with_store(
            dir.path().join("connectors-index.json"),
            Box::new(MemoryStore::default()),
        );
        let manager = AuthManager::with_storage(storage);
        manager
            .put_token(
                ConnectorProvider::Microsoft,
                &AccountId::new("ada@contoso.test"),
                StoredToken {
                    access_token: "at".into(),
                    refresh_token: None,
                    expires_at: None,
                    scopes: vec![],
                },
            )
            .await
            .unwrap();
        let journal = apollia_runtime::audit_journal::AuditJournalHandle::open(
            &dir.path().join("journal.db"),
        )
        .await
        .unwrap();
        let governance = ConnectionGovernance::without_policy(Some(journal.clone()));

        // WHEN the account is disconnected
        revoke_and_record(
            &governance,
            &manager,
            ConnectorProvider::Microsoft,
            "ada@contoso.test",
        )
        .await
        .unwrap();

        // THEN the token is gone and the disconnection is journaled by the desktop
        assert!(manager
            .list_accounts(ConnectorProvider::Microsoft)
            .await
            .unwrap()
            .is_empty());
        let entries = journal
            .query_run(&apollia_runtime::governance::connection_run_id(
                "native:microsoft:ada@contoso.test",
            ))
            .await;
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0].kind,
            apollia_runtime::audit_journal::JournalEntryKind::AccountDisconnected
        );
        assert_eq!(entries[0].payload["actor"], "desktop");
        journal.shutdown().await;
    }

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

    /// Governance over the organisation policy whose `[connections]` section
    /// is `connections`, as the runtime builds it at boot.
    fn governance_over_policy(connections: &str) -> ConnectionGovernance {
        let text = format!(
            "schema_version = 1\nsigned_at = 2026-10-01T09:00:00Z\n[connections]\n{connections}\n"
        );
        let policy = apollia_permissions::parse_policy(&text).unwrap();
        let verdict =
            apollia_permissions::PolicyVerdict::Valid(Box::new(apollia_permissions::ValidPolicy {
                signed_at: policy.signed_at,
                policy,
                key_fingerprint: "00".into(),
                policy_digest: "00".into(),
                publisher: "it".into(),
            }));
        ConnectionGovernance::new(
            std::sync::Arc::new(apollia_runtime::governance::PolicyAdmission::new(
                std::sync::Arc::new(verdict),
            )),
            None,
        )
    }

    // A policy denying remote connections refuses the start of a native consent.
    #[test]
    fn test_start_refused_by_a_policy_denying_remote_connections() {
        // GIVEN a loaded organisation policy carrying `connections.remote = "deny"`
        let governance = governance_over_policy("remote = \"deny\"");

        // WHEN the start of a Google consent is submitted
        let err = admit_start(&governance, "google").unwrap_err();

        // THEN it is refused by the policy, a kind the renderer recognises
        assert!(
            matches!(&err, IntegrationsError::RefusedByPolicy(reason)
                if reason == apollia_runtime::governance::REMOTE_DENIED_REASON),
            "{err:?}"
        );
        let json = serde_json::to_value(&err).unwrap();
        assert_eq!(json["kind"], "refused_by_policy");
    }

    // With no policy and no sovereignty key anywhere, a Google consent starts.
    #[test]
    fn test_start_admitted_without_a_policy() {
        // GIVEN no organisation policy (the command takes no profile value)
        let governance = ConnectionGovernance::new(
            std::sync::Arc::new(apollia_runtime::governance::PolicyAdmission::new(
                std::sync::Arc::new(apollia_permissions::PolicyVerdict::Absent),
            )),
            None,
        );

        // WHEN the start of a Google consent is submitted
        let provider = admit_start(&governance, "google").unwrap();

        // THEN it is admitted
        assert_eq!(provider, ConnectorProvider::Google);
        // AND a policy silent on remote connections admits it too
        assert!(admit_start(&governance_over_policy(""), "google").is_ok());
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
    fn test_a_refusal_of_the_daemon_crosses_the_bridge_as_its_own_words() {
        // GIVEN the daemon's refusal, as the attached arm carries it
        let err = IntegrationsError::Service("refused by the connection policy".into());

        // WHEN it crosses the IPC boundary
        let json = serde_json::to_value(&err).expect("serialize");

        // THEN the interface receives the daemon's text alone, no kind before it
        assert_eq!(json, serde_json::json!("refused by the connection policy"));
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
