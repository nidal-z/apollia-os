//! The consent half of a native connector (Google Workspace, Microsoft 365),
//! with no interface in it.
//!
//! Connecting an account takes two calls. The first builds the authorization
//! URL and remembers the PKCE verifier under the `state` it sent; the second
//! receives the `code` the provider handed back, exchanges it, names the
//! account through the provider's userinfo endpoint, and stores the token in
//! the [`AuthManager`] the connector executors read.
//!
//! Nothing here opens a browser or binds a port. Where the user is sent and
//! how the code comes back belong to the caller: the desktop binds a loopback
//! listener and opens the system browser, while a hosted product redirects
//! its own user to the URL and receives the code on a page of its own domain,
//! the one it registered with the provider. The redirect URI is therefore an
//! input, and the only thing this module asks of it is that it could be a
//! legitimate one.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::connector_providers::{
    build_google_provider, build_microsoft_provider, ConnectorProvider, GoogleScope, MicrosoftScope,
};
use crate::multi_account::AccountId;
use crate::pkce::{build_auth_url, OAuth2PkceFlow};
use crate::providers::ProviderConfig;
use crate::AuthManager;

/// How long a started consent waits for its code. A user who leaves the
/// provider's page open longer starts again.
const CONSENT_TTL: Duration = Duration::from_secs(10 * 60);

/// Ceiling on consents started and not yet completed. A caller that starts
/// flows in a loop is refused rather than allowed to grow the map unbounded.
const MAX_PENDING_CONSENTS: usize = 64;

/// Longest redirect URI accepted. Providers cap theirs well below this.
const MAX_REDIRECT_URI_LEN: usize = 2048;

// ─── Error ───────────────────────────────────────────────────────────────────

/// Why a consent could not be started or completed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ConsentError {
    /// The provider identifier is not one of [`ConnectorProvider::ALL`].
    #[error("unknown provider: {0}")]
    UnknownProvider(String),
    /// The scope name is not in the provider's catalogue.
    #[error("unknown scope: {0}")]
    UnknownScope(String),
    /// No OAuth client id resolves for the provider.
    #[error("OAuth client not configured for {0}")]
    ClientNotConfigured(String),
    /// The provider needs a client secret at its token endpoint and none
    /// resolves. Refused before the user is sent to consent, since the
    /// exchange would otherwise fail only after they granted access.
    #[error("OAuth client secret not configured for {0}")]
    ClientSecretMissing(String),
    /// The redirect URI cannot be a legitimate one.
    #[error("invalid redirect URI: {0}")]
    InvalidRedirectUri(String),
    /// No consent waits under this `state`: never started, already
    /// completed, or expired.
    #[error("no pending consent for this state")]
    FlowNotFound,
    /// Too many consents are started and not completed.
    #[error("too many consents are pending, retry once one completes or expires")]
    TooManyPending,
    /// The provider refused the code.
    #[error("token exchange failed: {0}")]
    Exchange(String),
    /// The token was granted but the account behind it could not be named.
    #[error("account resolution failed: {0}")]
    AccountResolution(String),
    /// The token could not be stored.
    #[error("token storage failed: {0}")]
    Storage(String),
}

// ─── Provider, scopes and credentials ────────────────────────────────────────

/// Resolve a provider identifier, refusing any other value by name.
pub fn provider_from_id(id: &str) -> Result<ConnectorProvider, ConsentError> {
    ConnectorProvider::from_id(id).ok_or_else(|| ConsentError::UnknownProvider(id.to_owned()))
}

/// Build the provider configuration for the scope names a caller asked for.
///
/// Every name must be in the provider's catalogue: a name that is not is
/// refused rather than dropped, so a caller never believes it asked for a
/// permission it did not.
pub fn provider_config_for(
    provider: ConnectorProvider,
    scope_names: &[String],
) -> Result<ProviderConfig, ConsentError> {
    let unknown = |name: &String| ConsentError::UnknownScope(name.clone());
    match provider {
        ConnectorProvider::Google => {
            let scopes = scope_names
                .iter()
                .map(|n| GoogleScope::from_name(n).ok_or_else(|| unknown(n)))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(build_google_provider(&scopes))
        }
        ConnectorProvider::Microsoft => {
            let scopes = scope_names
                .iter()
                .map(|n| MicrosoftScope::from_name(n).ok_or_else(|| unknown(n)))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(build_microsoft_provider(&scopes))
        }
    }
}

/// Refuse a consent whose client credentials are incomplete.
///
/// Pure on purpose: the caller resolves the three-source chain and hands the
/// outcome in, which keeps the rule testable without the process environment.
/// Google's installed-app and web clients both need their secret at the token
/// endpoint; Microsoft's shipped public client needs none.
pub fn credential_gate(
    provider: ConnectorProvider,
    client_id: Option<&str>,
    client_secret: Option<&str>,
) -> Result<(), ConsentError> {
    let has_value = |v: Option<&str>| v.is_some_and(|s| !s.trim().is_empty());
    if !has_value(client_id) {
        return Err(ConsentError::ClientNotConfigured(provider.id().to_owned()));
    }
    if matches!(provider, ConnectorProvider::Google) && !has_value(client_secret) {
        return Err(ConsentError::ClientSecretMissing(provider.id().to_owned()));
    }
    Ok(())
}

/// Resolve a provider's credentials and run them through [`credential_gate`].
pub fn ensure_credentials(provider: ConnectorProvider) -> Result<(), ConsentError> {
    let client_id = provider.resolve_client_id();
    let client_secret = provider.resolve_client_secret();
    credential_gate(provider, client_id.as_deref(), client_secret.as_deref())
}

/// Refuse a redirect URI that no provider should be sent.
///
/// `https` on any host, or `http` on a loopback host only, as RFC 8252 allows
/// for native clients. No fragment (RFC 6749, section 3.1.2) and no user
/// information. The provider still checks the URI against its registration;
/// this check exists so a malformed value fails here, by name, rather than on
/// the provider's error page after the user has been sent there.
pub fn validate_redirect_uri(uri: &str) -> Result<(), ConsentError> {
    let refuse = |why: &str| Err(ConsentError::InvalidRedirectUri(why.to_owned()));
    if uri.len() > MAX_REDIRECT_URI_LEN {
        return refuse("longer than 2048 bytes");
    }
    let Ok(parsed) = reqwest::Url::parse(uri) else {
        return refuse("not an absolute URL");
    };
    if parsed.fragment().is_some() {
        return refuse("carries a fragment");
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return refuse("carries user information");
    }
    let loopback = matches!(parsed.host_str(), Some("127.0.0.1" | "localhost" | "[::1]"));
    match parsed.scheme() {
        "https" if parsed.host_str().is_some_and(|h| !h.is_empty()) => Ok(()),
        "http" if loopback => Ok(()),
        "http" => refuse("plain http is accepted on a loopback host only"),
        _ => refuse("the scheme must be https"),
    }
}

// ─── Pending consents ────────────────────────────────────────────────────────

/// What the caller sends its user to, and the `state` it hands back with the
/// code.
#[derive(Debug, Clone)]
pub struct ConsentStart {
    /// Full authorization URL, PKCE challenge, state and scopes included.
    pub auth_url: String,
    /// Opaque value the provider returns alongside the code.
    pub state: String,
}

/// A consent started and waiting for its code.
#[derive(Debug)]
pub struct PendingConsent {
    /// Provider the consent was started with.
    pub provider: ConnectorProvider,
    /// PKCE parameters, the verifier among them.
    pub flow: OAuth2PkceFlow,
    /// Provider configuration, scopes included.
    pub provider_config: ProviderConfig,
    started_at: Instant,
}

/// Consents started and not yet completed, keyed by `state`.
///
/// Bounded twice: an entry older than its time to live is never handed back,
/// and past a ceiling a new start is refused. The lock is never held across an
/// await.
#[derive(Debug)]
pub struct PendingConsents {
    entries: Mutex<HashMap<String, PendingConsent>>,
    ttl: Duration,
    capacity: usize,
}

impl Default for PendingConsents {
    fn default() -> Self {
        Self::new()
    }
}

impl PendingConsents {
    /// A store with the production time to live and ceiling.
    pub fn new() -> Self {
        Self::with_limits(CONSENT_TTL, MAX_PENDING_CONSENTS)
    }

    /// A store with explicit limits.
    pub fn with_limits(ttl: Duration, capacity: usize) -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            ttl,
            capacity,
        }
    }

    /// Start a consent: build the authorization URL for `redirect_uri` and
    /// remember the verifier under the `state` it carries.
    pub fn begin(
        &self,
        provider: ConnectorProvider,
        provider_config: ProviderConfig,
        redirect_uri: &str,
    ) -> Result<ConsentStart, ConsentError> {
        validate_redirect_uri(redirect_uri)?;
        let flow = OAuth2PkceFlow::with_redirect_uri(redirect_uri);
        let auth_url = build_auth_url(&provider_config, &flow);
        let state = flow.state.clone();

        let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        let ttl = self.ttl;
        entries.retain(|_, pending| pending.started_at.elapsed() < ttl);
        if entries.len() >= self.capacity {
            return Err(ConsentError::TooManyPending);
        }
        entries.insert(
            state.clone(),
            PendingConsent {
                provider,
                flow,
                provider_config,
                started_at: Instant::now(),
            },
        );
        Ok(ConsentStart { auth_url, state })
    }

    /// Hand back, once, the consent waiting under `state`.
    ///
    /// The entry leaves the store whatever happens next, so a code can be
    /// tried against a given verifier once only.
    pub fn take(&self, state: &str) -> Result<PendingConsent, ConsentError> {
        let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        match entries.remove(state) {
            Some(pending) if pending.started_at.elapsed() < self.ttl => Ok(pending),
            _ => Err(ConsentError::FlowNotFound),
        }
    }
}

// ─── Completion ──────────────────────────────────────────────────────────────

/// The account a completed consent connected.
#[derive(Debug, Clone)]
pub struct ConnectedAccount {
    /// Provider the account belongs to.
    pub provider: ConnectorProvider,
    /// Account identifier: the Google email, or the Microsoft principal name.
    pub account_id: AccountId,
    /// Scopes the provider actually granted.
    pub granted_scopes: Vec<String>,
}

/// Complete a consent: exchange `code`, name the account, store the token.
pub async fn complete(
    pending: PendingConsent,
    code: &str,
    manager: &AuthManager,
) -> Result<ConnectedAccount, ConsentError> {
    let token = crate::token::exchange_code(&pending.provider_config, &pending.flow, code)
        .await
        .map_err(|e| ConsentError::Exchange(e.to_string()))?;
    let account_id = resolve_account_id(pending.provider, &token.access_token).await?;
    let granted_scopes = token.scopes.clone();
    manager
        .put_token(pending.provider, &account_id, token)
        .await
        .map_err(|e| ConsentError::Storage(e.to_string()))?;
    Ok(ConnectedAccount {
        provider: pending.provider,
        account_id,
        granted_scopes,
    })
}

/// Name the account behind `bearer` through the provider's userinfo endpoint.
pub async fn resolve_account_id(
    provider: ConnectorProvider,
    bearer: &str,
) -> Result<AccountId, ConsentError> {
    let fail = |e: String| ConsentError::AccountResolution(e);
    let client = apollia_core::net::safe_client().map_err(|e| fail(e.to_string()))?;
    let response = client
        .get(provider.userinfo_url())
        .bearer_auth(bearer)
        .send()
        .await
        .map_err(|e| fail(e.to_string()))?;
    if !response.status().is_success() {
        return Err(fail(format!("userinfo returned {}", response.status())));
    }
    let json: serde_json::Value =
        apollia_core::net::read_capped_json(response, apollia_core::net::MAX_METADATA_BYTES)
            .await
            .map_err(|e| fail(e.to_string()))?;
    account_id_from_userinfo(provider, &json)
}

/// Read the account identifier out of a userinfo document.
fn account_id_from_userinfo(
    provider: ConnectorProvider,
    json: &serde_json::Value,
) -> Result<AccountId, ConsentError> {
    let field = |name: &str| json.get(name).and_then(|v| v.as_str());
    let id = match provider {
        ConnectorProvider::Google => field("email")
            .ok_or_else(|| ConsentError::AccountResolution("userinfo missing email".into()))?,
        ConnectorProvider::Microsoft => field("userPrincipalName")
            .or_else(|| field("mail"))
            .ok_or_else(|| {
                ConsentError::AccountResolution("Graph /me missing userPrincipalName".into())
            })?,
    };
    Ok(AccountId::new(id))
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn microsoft_config() -> ProviderConfig {
        ProviderConfig {
            name: "microsoft",
            auth_url: "https://login.microsoftonline.com/common/oauth2/v2.0/authorize",
            token_url: "https://login.microsoftonline.com/common/oauth2/v2.0/token",
            client_id: "cid".to_string(),
            client_secret: None,
            scopes: vec!["User.Read", "offline_access"],
        }
    }

    #[test]
    fn test_redirect_uri_accepts_https_and_loopback_http() {
        // GIVEN a hosted callback page and the three loopback spellings
        // WHEN each is validated
        // THEN all four are accepted
        for uri in [
            "https://app.example.test/oauth/callback",
            "http://127.0.0.1:53682/callback",
            "http://localhost:8080/callback",
            "http://[::1]:8080/callback",
        ] {
            assert!(validate_redirect_uri(uri).is_ok(), "{uri} must be accepted");
        }
    }

    #[test]
    fn test_redirect_uri_refuses_what_no_provider_should_be_sent() {
        // GIVEN plain http off loopback, a fragment, user information, a
        // foreign scheme and a relative path
        // WHEN each is validated
        // THEN each is refused as an invalid redirect URI
        for uri in [
            "http://app.example.test/callback",
            "https://app.example.test/callback#frag",
            "https://user:pw@app.example.test/callback",
            "javascript:alert(1)",
            "/oauth/callback",
        ] {
            assert!(
                matches!(
                    validate_redirect_uri(uri),
                    Err(ConsentError::InvalidRedirectUri(_))
                ),
                "{uri} must be refused"
            );
        }
    }

    #[test]
    fn test_scope_names_outside_the_catalogue_are_refused_by_name() {
        // GIVEN one known Google scope and one no provider declares
        let names = ["mail.send".to_string(), "drive.read_all".to_string()];

        // WHEN the provider configuration is built from them
        let err = provider_config_for(ConnectorProvider::Google, &names).unwrap_err();

        // THEN the refusal names the stranger rather than dropping it
        assert!(matches!(err, ConsentError::UnknownScope(ref n) if n == "drive.read_all"));
    }

    #[test]
    fn test_credential_gate_refuses_a_google_client_without_its_secret() {
        // GIVEN a Google client id with no secret, and none at all
        // WHEN the gate is asked
        let no_secret = credential_gate(ConnectorProvider::Google, Some("cid"), None);
        let no_id = credential_gate(ConnectorProvider::Google, Some("  "), Some("sec"));

        // THEN each is refused for the piece it lacks
        assert!(matches!(
            no_secret,
            Err(ConsentError::ClientSecretMissing(_))
        ));
        assert!(matches!(no_id, Err(ConsentError::ClientNotConfigured(_))));
        // AND a Microsoft public client passes without a secret
        assert!(credential_gate(ConnectorProvider::Microsoft, Some("cid"), None).is_ok());
    }

    #[test]
    fn test_a_started_consent_carries_the_caller_redirect_and_is_taken_once() {
        // GIVEN a store and a consent started for a hosted callback page
        let store = PendingConsents::new();
        let redirect = "https://app.example.test/oauth/callback";
        let start = store
            .begin(ConnectorProvider::Microsoft, microsoft_config(), redirect)
            .expect("start");

        // WHEN the consent is taken back twice under its state
        let first = store.take(&start.state);
        let second = store.take(&start.state);

        // THEN the URL names the caller's page, the first take succeeds and the second finds nothing
        assert!(start
            .auth_url
            .contains(&urlencoding::encode(redirect).into_owned()));
        assert!(start.auth_url.contains("code_challenge_method=S256"));
        let pending = first.expect("first take");
        assert_eq!(pending.flow.redirect_uri, redirect);
        assert!(matches!(second, Err(ConsentError::FlowNotFound)));
    }

    #[test]
    fn test_an_expired_consent_is_not_handed_back() {
        // GIVEN a store whose consents expire at once
        let store = PendingConsents::with_limits(Duration::ZERO, 8);
        let start = store
            .begin(
                ConnectorProvider::Microsoft,
                microsoft_config(),
                "https://app.example.test/cb",
            )
            .expect("start");

        // WHEN the consent is taken back
        let taken = store.take(&start.state);

        // THEN it is treated as never started
        assert!(matches!(taken, Err(ConsentError::FlowNotFound)));
    }

    #[test]
    fn test_the_store_refuses_a_start_past_its_ceiling() {
        // GIVEN a store that holds one pending consent, already full
        let store = PendingConsents::with_limits(Duration::from_secs(60), 1);
        let redirect = "https://app.example.test/cb";
        store
            .begin(ConnectorProvider::Microsoft, microsoft_config(), redirect)
            .expect("first start");

        // WHEN a second consent is started
        let second = store.begin(ConnectorProvider::Microsoft, microsoft_config(), redirect);

        // THEN it is refused rather than let the store grow
        assert!(matches!(second, Err(ConsentError::TooManyPending)));
    }

    #[test]
    fn test_userinfo_names_the_account_per_provider() {
        // GIVEN a Google userinfo document and a Graph /me document with no principal name
        let google = serde_json::json!({ "email": "ada@example.test" });
        let graph = serde_json::json!({ "mail": "ada@contoso.test" });

        // WHEN the account is read out of each
        let g = account_id_from_userinfo(ConnectorProvider::Google, &google).expect("google");
        let m = account_id_from_userinfo(ConnectorProvider::Microsoft, &graph).expect("graph");

        // THEN Google names the email and Graph falls back to the mail field
        assert_eq!(g.0, "ada@example.test");
        assert_eq!(m.0, "ada@contoso.test");
        // AND a document with neither is refused
        assert!(account_id_from_userinfo(ConnectorProvider::Google, &graph).is_err());
    }
}
