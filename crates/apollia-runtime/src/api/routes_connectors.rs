//! REST routes for the consent half of the native connectors (Google
//! Workspace, Microsoft 365) under `/api/v1/connectors/`.
//!
//! The executors that spend a connector's token already live in the runtime
//! (`crate::connectors_bridge`); until these routes, connecting an account did
//! not, and only the desktop could do it. A caller with no desktop, a hosted
//! product among them, had no way to connect one.
//!
//! The routes do not open a browser and do not bind a port. `start` answers the
//! authorization URL for a redirect URI the caller owns, one it registered with
//! the provider, and the caller sends its own user there. The provider returns
//! the user to that page with a `code` and the `state`; the caller hands both to
//! `complete`, which exchanges the code and stores the token in the same
//! [`apollia_auth::AuthManager`] the executors read. The desktop keeps its own
//! loopback path, unchanged.
//!
//! The runtime is single-user by design; a connected account is the
//! instance's account, as it is under the desktop.
//!
//! An account is submitted to the connection admission before its code is
//! exchanged, and connecting or disconnecting one is journaled.

use std::sync::OnceLock;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use apollia_auth::connector_consent::{self, ConsentError, PendingConsents};
use apollia_auth::{AccountId, ConnectorProvider};

use crate::api::openapi::ApiErrorBody;
use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;
use crate::governance::{
    AdmissionDecision, AdmissionRequest, ConnectionEvent, ConnectionGovernance,
};

/// Who the connection journal names for an account changed through these routes.
const ROUTE_ACTOR: &str = "api";

/// Longest `state` accepted back. The runtime issues 43 characters.
const MAX_STATE_LEN: usize = 256;

/// Longest authorization code accepted. Providers issue a few hundred bytes.
const MAX_CODE_LEN: usize = 4096;

/// Consents started over HTTP and not yet completed. Separate from the
/// desktop's own map: a consent is completed by the surface that started it.
static CONSENTS: OnceLock<PendingConsents> = OnceLock::new();

fn consents() -> &'static PendingConsents {
    CONSENTS.get_or_init(PendingConsents::new)
}

/// Build the connector consent router.
///
/// Mounted under the root router by [`crate::api::server::build_router`].
pub fn connectors_router<B: ExecutionBackend + Clone>() -> Router<AppState<B>> {
    Router::new()
        .route(
            "/api/v1/connectors/oauth/start",
            post(start_connector_consent),
        )
        .route(
            "/api/v1/connectors/oauth/complete",
            post(complete_connector_consent::<B>),
        )
        .route("/api/v1/connectors/accounts", get(list_connector_accounts))
        .route(
            "/api/v1/connectors/accounts/:provider/:account_id",
            delete(disconnect_connector_account::<B>),
        )
}

type ApiError = (StatusCode, Json<ApiErrorBody>);

fn api_err(code: StatusCode, msg: impl std::fmt::Display) -> ApiError {
    (
        code,
        Json(ApiErrorBody {
            error: msg.to_string(),
        }),
    )
}

fn consent_err(err: ConsentError) -> ApiError {
    let code = match &err {
        ConsentError::UnknownProvider(_)
        | ConsentError::UnknownScope(_)
        | ConsentError::InvalidRedirectUri(_) => StatusCode::BAD_REQUEST,
        ConsentError::ClientNotConfigured(_) | ConsentError::ClientSecretMissing(_) => {
            StatusCode::SERVICE_UNAVAILABLE
        }
        ConsentError::FlowNotFound => StatusCode::NOT_FOUND,
        ConsentError::TooManyPending => StatusCode::TOO_MANY_REQUESTS,
        ConsentError::Exchange(_) | ConsentError::AccountResolution(_) => StatusCode::BAD_GATEWAY,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    api_err(code, err)
}

/// Submit an account of `provider` to the connection admission; a refusal is
/// `403 Forbidden` carrying the reason.
fn admit_account(
    governance: &ConnectionGovernance,
    provider: ConnectorProvider,
) -> Result<(), ApiError> {
    match governance.admit(&AdmissionRequest::for_native_account(provider)) {
        AdmissionDecision::Admitted => Ok(()),
        AdmissionDecision::Refused { reason } => Err(api_err(StatusCode::FORBIDDEN, reason)),
    }
}

async fn auth_manager() -> Result<std::sync::Arc<apollia_auth::AuthManager>, ApiError> {
    crate::connectors_bridge::get_auth()
        .await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e))
}

// ─── Wire types ──────────────────────────────────────────────────────────────

/// Body of `POST /api/v1/connectors/oauth/start`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct StartConsentBody {
    /// Provider identifier: `google` or `microsoft`.
    pub provider: String,
    /// Scope names from the provider's catalogue (`mail.send`,
    /// `calendar.read`, ...). The identity scopes are added implicitly.
    #[serde(default)]
    pub scopes: Vec<String>,
    /// Page the provider returns the user to, registered with the provider
    /// under the OAuth client the runtime resolves. `https`, or `http` on a
    /// loopback host.
    pub redirect_uri: String,
}

/// Answer of `POST /api/v1/connectors/oauth/start`.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct StartConsentResponse {
    /// URL to send the user to.
    pub auth_url: String,
    /// Opaque value the provider returns with the code, to hand back to
    /// `complete`. Valid ten minutes, once.
    pub state: String,
}

/// Body of `POST /api/v1/connectors/oauth/complete`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CompleteConsentBody {
    /// The `state` the provider returned, as `start` issued it.
    pub state: String,
    /// The authorization `code` the provider returned.
    pub code: String,
}

/// Answer of `POST /api/v1/connectors/oauth/complete`.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ConnectedAccountResponse {
    /// Provider identifier.
    pub provider: String,
    /// Account identifier: the Google email, or the Microsoft principal name.
    pub account_id: String,
    /// Scopes the provider actually granted.
    pub granted_scopes: Vec<String>,
}

/// One connected account, in `GET /api/v1/connectors/accounts`.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ConnectorAccountResponse {
    /// Provider identifier.
    pub provider: String,
    /// Account identifier.
    pub account_id: String,
}

// ─── Handlers ────────────────────────────────────────────────────────────────

/// `POST /api/v1/connectors/oauth/start`: start a consent and answer the URL
/// to send the user to. No browser opens.
#[utoipa::path(
    post,
    path = "/api/v1/connectors/oauth/start",
    tag = "connectors",
    request_body = StartConsentBody,
    responses(
        (status = 200, description = "Consent started", body = StartConsentResponse),
        (status = 400, description = "Unknown provider or scope, or invalid redirect URI", body = ApiErrorBody),
        (status = 429, description = "Too many consents pending", body = ApiErrorBody),
        (status = 503, description = "The provider's OAuth client is not configured", body = ApiErrorBody),
    )
)]
pub(crate) async fn start_connector_consent(
    Json(body): Json<StartConsentBody>,
) -> Result<Json<StartConsentResponse>, ApiError> {
    let provider = connector_consent::provider_from_id(&body.provider).map_err(consent_err)?;
    connector_consent::validate_redirect_uri(&body.redirect_uri).map_err(consent_err)?;
    let provider_config =
        connector_consent::provider_config_for(provider, &body.scopes).map_err(consent_err)?;
    connector_consent::ensure_credentials(provider).map_err(consent_err)?;
    let start = consents()
        .begin(provider, provider_config, &body.redirect_uri)
        .map_err(consent_err)?;
    tracing::info!(provider = %provider.id(), "connectors.consent.started");
    Ok(Json(StartConsentResponse {
        auth_url: start.auth_url,
        state: start.state,
    }))
}

/// `POST /api/v1/connectors/oauth/complete`: exchange the code the provider
/// returned and store the account's token.
#[utoipa::path(
    post,
    path = "/api/v1/connectors/oauth/complete",
    tag = "connectors",
    request_body = CompleteConsentBody,
    responses(
        (status = 200, description = "Account connected", body = ConnectedAccountResponse),
        (status = 400, description = "Empty or oversized state or code", body = ApiErrorBody),
        (status = 403, description = "Refused by the connection admission", body = ApiErrorBody),
        (status = 404, description = "No pending consent for this state", body = ApiErrorBody),
        (status = 502, description = "The provider refused the code or named no account", body = ApiErrorBody),
    )
)]
pub(crate) async fn complete_connector_consent<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    body: Json<CompleteConsentBody>,
) -> Result<Json<ConnectedAccountResponse>, ApiError> {
    complete_consent(&state.connection_governance, body).await
}

/// Body of [`complete_connector_consent`]: validate, admit, exchange, journal.
async fn complete_consent(
    governance: &ConnectionGovernance,
    Json(body): Json<CompleteConsentBody>,
) -> Result<Json<ConnectedAccountResponse>, ApiError> {
    if body.state.is_empty() || body.state.len() > MAX_STATE_LEN {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "state is empty or too long",
        ));
    }
    if body.code.is_empty() || body.code.len() > MAX_CODE_LEN {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "code is empty or too long",
        ));
    }
    let pending = consents().take(&body.state).map_err(consent_err)?;
    admit_account(governance, pending.provider)?;
    let manager = auth_manager().await?;
    let account = connector_consent::complete(pending, &body.code, &manager)
        .await
        .map_err(consent_err)?;
    Ok(Json(record_connected(governance, account)))
}

/// Journal the connection of `account`, whose token is already stored, and
/// answer it.
fn record_connected(
    governance: &ConnectionGovernance,
    account: connector_consent::ConnectedAccount,
) -> ConnectedAccountResponse {
    tracing::info!(provider = %account.provider.id(), "connectors.consent.completed");
    governance.record_or_warn(&ConnectionEvent::account_connected(
        account.provider,
        &account.account_id.0,
        ROUTE_ACTOR,
    ));
    ConnectedAccountResponse {
        provider: account.provider.id().to_owned(),
        account_id: account.account_id.0,
        granted_scopes: account.granted_scopes,
    }
}

/// `GET /api/v1/connectors/accounts`: every connected account, all providers.
#[utoipa::path(
    get,
    path = "/api/v1/connectors/accounts",
    tag = "connectors",
    responses(
        (status = 200, description = "Connected accounts", body = [ConnectorAccountResponse]),
        (status = 500, description = "The token store could not be read", body = ApiErrorBody),
    )
)]
pub(crate) async fn list_connector_accounts(
) -> Result<Json<Vec<ConnectorAccountResponse>>, ApiError> {
    let manager = auth_manager().await?;
    let mut out = Vec::new();
    for provider in ConnectorProvider::ALL {
        let accounts = manager
            .list_accounts(provider)
            .await
            .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e))?;
        out.extend(accounts.into_iter().map(|a| ConnectorAccountResponse {
            provider: provider.id().to_owned(),
            account_id: a.0,
        }));
    }
    Ok(Json(out))
}

/// `DELETE /api/v1/connectors/accounts/:provider/:account_id`: forget the
/// account's token. The provider is not notified.
#[utoipa::path(
    delete,
    path = "/api/v1/connectors/accounts/{provider}/{account_id}",
    tag = "connectors",
    params(
        ("provider" = String, Path, description = "Provider identifier"),
        ("account_id" = String, Path, description = "Account identifier"),
    ),
    responses(
        (status = 204, description = "Token forgotten"),
        (status = 400, description = "Unknown provider", body = ApiErrorBody),
        (status = 500, description = "The token store could not be written", body = ApiErrorBody),
    )
)]
pub(crate) async fn disconnect_connector_account<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    path: Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    disconnect_account(&state.connection_governance, path).await
}

/// Body of [`disconnect_connector_account`]: forget the token, then journal.
async fn disconnect_account(
    governance: &ConnectionGovernance,
    Path((provider, account_id)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    let provider = connector_consent::provider_from_id(&provider).map_err(consent_err)?;
    let manager = auth_manager().await?;
    revoke_and_record(governance, &manager, provider, &account_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Forget the token of `account_id` in `manager`, then journal the
/// disconnection.
async fn revoke_and_record(
    governance: &ConnectionGovernance,
    manager: &apollia_auth::AuthManager,
    provider: ConnectorProvider,
    account_id: &str,
) -> Result<(), ApiError> {
    manager
        .revoke(provider, &AccountId::new(account_id))
        .await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    tracing::info!(provider = %provider.id(), "connectors.account.disconnected");
    governance.record_or_warn(&ConnectionEvent::account_disconnected(
        provider,
        account_id,
        ROUTE_ACTOR,
    ));
    Ok(())
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn start_body(provider: &str, scopes: &[&str], redirect_uri: &str) -> Json<StartConsentBody> {
        Json(StartConsentBody {
            provider: provider.to_owned(),
            scopes: scopes.iter().map(|s| (*s).to_owned()).collect(),
            redirect_uri: redirect_uri.to_owned(),
        })
    }

    #[tokio::test]
    async fn test_start_refuses_an_unknown_provider_before_anything_else() {
        // GIVEN a provider no connector claims
        let body = start_body("github", &[], "https://app.example.test/cb");

        // WHEN a consent is started for it
        let (code, Json(err)) = start_connector_consent(body).await.unwrap_err();

        // THEN it is a bad request naming the provider
        assert_eq!(code, StatusCode::BAD_REQUEST);
        assert!(err.error.contains("github"));
    }

    #[tokio::test]
    async fn test_start_refuses_a_scope_outside_the_catalogue() {
        // GIVEN a Microsoft consent asking for a scope Microsoft does not declare
        let body = start_body(
            "microsoft",
            &["drive.workspace"],
            "https://app.example.test/cb",
        );

        // WHEN it is started
        let (code, Json(err)) = start_connector_consent(body).await.unwrap_err();

        // THEN it is a bad request naming the scope
        assert_eq!(code, StatusCode::BAD_REQUEST);
        assert!(err.error.contains("drive.workspace"));
    }

    #[tokio::test]
    async fn test_start_refuses_a_redirect_uri_off_https() {
        // GIVEN a plain http redirect on a public host
        let body = start_body("microsoft", &["mail.read"], "http://app.example.test/cb");

        // WHEN a consent is started with it
        let (code, Json(err)) = start_connector_consent(body).await.unwrap_err();

        // THEN it is refused before the user is sent anywhere
        assert_eq!(code, StatusCode::BAD_REQUEST);
        assert!(err.error.contains("redirect URI"));
    }

    #[tokio::test]
    async fn test_complete_refuses_a_state_it_never_issued() {
        // GIVEN a state no start issued
        let body = Json(CompleteConsentBody {
            state: "never-issued".to_owned(),
            code: "abc".to_owned(),
        });

        // WHEN the consent is completed with it
        let (code, _) = complete_consent(&ConnectionGovernance::default(), body)
            .await
            .unwrap_err();

        // THEN it is not found, and no token exchange is attempted
        assert_eq!(code, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_complete_refuses_an_empty_code() {
        // GIVEN a completion carrying no code
        let body = Json(CompleteConsentBody {
            state: "some-state".to_owned(),
            code: String::new(),
        });

        // WHEN it is sent
        let (code, _) = complete_consent(&ConnectionGovernance::default(), body)
            .await
            .unwrap_err();

        // THEN it is a bad request
        assert_eq!(code, StatusCode::BAD_REQUEST);
    }

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

    struct RefuseRemote;

    impl crate::governance::ConnectionAdmission for RefuseRemote {
        fn read_decision(
            &self,
            request: &AdmissionRequest,
        ) -> Result<AdmissionDecision, crate::governance::PolicyReadError> {
            Ok(if request.remote {
                AdmissionDecision::Refused {
                    reason: "remote connections are not allowed".into(),
                }
            } else {
                AdmissionDecision::Admitted
            })
        }
    }

    // A refused account stops the completion before any code is exchanged.
    #[tokio::test]
    async fn test_complete_refused_by_admission_exchanges_nothing() {
        // GIVEN a pending Google consent and a policy refusing remote connections
        let config = connector_consent::provider_config_for(ConnectorProvider::Google, &[])
            .expect("provider config");
        let start = consents()
            .begin(
                ConnectorProvider::Google,
                config,
                "https://app.example.test/cb",
            )
            .expect("begin");
        let governance = ConnectionGovernance::new(std::sync::Arc::new(RefuseRemote), None);
        let body = Json(CompleteConsentBody {
            state: start.state,
            code: "abc".to_owned(),
        });

        // WHEN the consent is completed
        let (code, Json(err)) = complete_consent(&governance, body).await.unwrap_err();

        // THEN it is forbidden with the policy's reason, before the exchange
        // (an exchange would have answered a bad gateway)
        assert_eq!(code, StatusCode::FORBIDDEN);
        assert_eq!(err.error, "remote connections are not allowed");
    }

    // A completed consent journals the account it connected.
    #[tokio::test]
    async fn test_connect_journals_the_account() {
        // GIVEN governance over a journal, and a Google account whose token
        // the consent stored
        let dir = tempfile::TempDir::new().unwrap();
        let journal =
            crate::audit_journal::AuditJournalHandle::open(&dir.path().join("journal.db"))
                .await
                .unwrap();
        let governance = ConnectionGovernance::without_policy(Some(journal.clone()));
        let account = connector_consent::ConnectedAccount {
            provider: ConnectorProvider::Google,
            account_id: AccountId::new("ada@example.com"),
            granted_scopes: vec!["openid".to_owned()],
        };

        // WHEN the connection is recorded
        let response = record_connected(&governance, account);

        // THEN the answer names the account and its connection run records
        // the connection by the route
        assert_eq!(response.provider, "google");
        assert_eq!(response.account_id, "ada@example.com");
        assert_eq!(response.granted_scopes, vec!["openid".to_owned()]);
        let entries = journal
            .query_run(&crate::governance::connection_run_id(
                "native:google:ada@example.com",
            ))
            .await;
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0].kind,
            crate::audit_journal::JournalEntryKind::AccountConnected
        );
        assert_eq!(entries[0].payload["account"], "ada@example.com");
        assert_eq!(entries[0].payload["actor"], ROUTE_ACTOR);
        journal.shutdown().await;
    }

    // Disconnecting an account forgets its token and journals the disconnection.
    #[tokio::test]
    async fn test_disconnect_journals_the_account() {
        // GIVEN a connected Google account in an isolated store, and governance
        // over a journal
        let dir = tempfile::TempDir::new().unwrap();
        let storage = apollia_auth::MultiAccountStorage::with_store(
            dir.path().join("connectors-index.json"),
            Box::new(MemoryStore::default()),
        );
        let manager = apollia_auth::AuthManager::with_storage(storage);
        manager
            .put_token(
                ConnectorProvider::Google,
                &AccountId::new("ada@example.com"),
                apollia_auth::StoredToken {
                    access_token: "at".into(),
                    refresh_token: None,
                    expires_at: None,
                    scopes: vec![],
                },
            )
            .await
            .unwrap();
        let journal =
            crate::audit_journal::AuditJournalHandle::open(&dir.path().join("journal.db"))
                .await
                .unwrap();
        let governance = ConnectionGovernance::without_policy(Some(journal.clone()));

        // WHEN the account is disconnected
        revoke_and_record(
            &governance,
            &manager,
            ConnectorProvider::Google,
            "ada@example.com",
        )
        .await
        .unwrap();

        // THEN the account is gone and its connection run records the disconnection
        let accounts = manager
            .list_accounts(ConnectorProvider::Google)
            .await
            .unwrap();
        assert!(accounts.is_empty());
        let entries = journal
            .query_run(&crate::governance::connection_run_id(
                "native:google:ada@example.com",
            ))
            .await;
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0].kind,
            crate::audit_journal::JournalEntryKind::AccountDisconnected
        );
        assert_eq!(entries[0].payload["account"], "ada@example.com");
        journal.shutdown().await;
    }

    #[tokio::test]
    async fn test_disconnect_refuses_an_unknown_provider() {
        // GIVEN a provider no connector claims
        let path = Path(("github".to_owned(), "ada@example.test".to_owned()));

        // WHEN one of its accounts is disconnected
        let (code, _) = disconnect_account(&ConnectionGovernance::default(), path)
            .await
            .unwrap_err();

        // THEN it is a bad request, and the token store is never opened
        assert_eq!(code, StatusCode::BAD_REQUEST);
    }
}
