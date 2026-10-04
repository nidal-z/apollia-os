//! REST routes for a mailbox connected over IMAP, under
//! `/api/v1/connectors/imap/`.
//!
//! - `GET /api/v1/connectors/imap/providers`: the providers whose server and
//!   port are filled from the address, read-only, so a form can show them
//!   before anything is sent.
//! - `POST /api/v1/connectors/imap/test`: log in and open the inbox
//!   read-only, store nothing.
//! - `POST /api/v1/connectors/imap/accounts`: the same verification, then the
//!   connection is stored: the description in the mailbox list, the password
//!   in the secret store.
//!
//! The server and port may be left out for a known provider; they are filled
//! from the address. A failed verification answers its typed code
//! (`unreachable`, `tls`, `credentials_rejected`, ...) and stores nothing. An
//! addition goes through the connection admission first and is journaled.
//! The password is never answered, never journaled, never traced.

use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use apollia_connectors::imap::providers::{KnownProvider, DEFAULT_IMAP_PORT, KNOWN_PROVIDERS};
use apollia_connectors::imap::{
    ConnectionProbe, ImapAccount, ImapAccountInput, ImapAccountStore, ImapClient, ImapError,
};
use apollia_mcp::Provenance;

use crate::api::server::AppState;
use crate::connections::{imap_connection_id, is_remote_host, ConnectionKind};
use crate::coordinator::ExecutionBackend;
use crate::governance::{
    AdmissionDecision, AdmissionRequest, ConnectionEvent, ConnectionGovernance,
};

/// Who the connection journal names for a mailbox added through these routes.
const ROUTE_ACTOR: &str = "api";

/// Longest password accepted. Application passwords are a few dozen bytes.
const MAX_PASSWORD_LEN: usize = 1024;

/// Build the mailbox router.
pub fn imap_router<B: ExecutionBackend + Clone>() -> Router<AppState<B>> {
    Router::new()
        .route(
            "/api/v1/connectors/imap/providers",
            get(list_imap_providers),
        )
        .route("/api/v1/connectors/imap/test", post(test_imap_mailbox))
        .route(
            "/api/v1/connectors/imap/accounts",
            post(add_imap_mailbox::<B>),
        )
}

/// Body of both mailbox routes. Deliberately without `Debug`: it carries the
/// password.
#[derive(Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ImapAccountBody {
    /// Address of the mailbox.
    pub email: String,
    /// IMAP server; filled from the address for a known provider.
    #[serde(default)]
    pub host: Option<String>,
    /// IMAP port, implicit TLS; the provider's, else 993.
    #[serde(default)]
    pub port: Option<u16>,
    /// Identifier sent at login; the address when absent.
    #[serde(default)]
    pub username: Option<String>,
    /// Application password. Kept in the secret store, never answered.
    pub password: String,
}

/// A verified mailbox, as both routes answer it.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ImapAccountResponse {
    /// Identifier of the connection, `imap:<address>`.
    pub connection_id: String,
    /// Address of the mailbox.
    pub email: String,
    /// IMAP server.
    pub host: String,
    /// IMAP port.
    pub port: u16,
    /// Identifier sent at login.
    pub username: String,
}

/// A provider whose IMAP server is known from the address.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ImapProviderResponse {
    /// Name of the provider.
    pub name: String,
    /// Address domains it serves, lowercase.
    pub domains: Vec<String>,
    /// IMAP server.
    pub host: String,
    /// IMAP port, implicit TLS.
    pub port: u16,
}

/// The providers known from the address, and the port of any other server.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ImapProvidersResponse {
    /// Every provider prefilled from the address.
    pub providers: Vec<ImapProviderResponse>,
    /// Port of a server outside the list, implicit TLS.
    pub default_port: u16,
}

impl From<&KnownProvider> for ImapProviderResponse {
    fn from(provider: &KnownProvider) -> Self {
        Self {
            name: provider.name.to_owned(),
            domains: provider.domains.iter().map(|d| (*d).to_owned()).collect(),
            host: provider.host.to_owned(),
            port: provider.port,
        }
    }
}

/// `GET /api/v1/connectors/imap/providers`: the providers whose server and
/// port are filled from the address. Reads a table compiled into the runtime;
/// nothing is reached, nothing is stored.
#[utoipa::path(
    get,
    path = "/api/v1/connectors/imap/providers",
    tag = "connectors",
    responses(
        (status = 200, description = "The providers known from the address", body = ImapProvidersResponse),
    )
)]
pub(crate) async fn list_imap_providers() -> Json<ImapProvidersResponse> {
    Json(ImapProvidersResponse {
        providers: KNOWN_PROVIDERS
            .iter()
            .map(ImapProviderResponse::from)
            .collect(),
        default_port: DEFAULT_IMAP_PORT,
    })
}

/// A refused mailbox: the message and its stable code.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ImapErrorBody {
    /// What went wrong, for a person.
    pub error: String,
    /// Stable code: `invalid_email`, `unknown_provider`, `invalid_host`,
    /// `invalid_port`, `invalid_password`, `unreachable`, `tls`,
    /// `credentials_rejected`, `protocol`, `admission_refused`,
    /// `secret_store`, `index`.
    pub code: String,
}

type ImapRouteError = (StatusCode, Json<ImapErrorBody>);

fn refused(status: StatusCode, code: &str, error: impl std::fmt::Display) -> ImapRouteError {
    (
        status,
        Json(ImapErrorBody {
            error: error.to_string(),
            code: code.to_owned(),
        }),
    )
}

fn imap_err(err: &ImapError) -> ImapRouteError {
    let status = match err {
        ImapError::InvalidEmail { .. }
        | ImapError::UnknownProvider { .. }
        | ImapError::InvalidHost { .. }
        | ImapError::InvalidPort => StatusCode::BAD_REQUEST,
        ImapError::CredentialsRejected => StatusCode::UNPROCESSABLE_ENTITY,
        ImapError::Unreachable { .. } | ImapError::Tls { .. } | ImapError::Protocol { .. } => {
            StatusCode::BAD_GATEWAY
        }
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    refused(status, err.code(), err)
}

fn answer(account: &ImapAccount) -> ImapAccountResponse {
    ImapAccountResponse {
        connection_id: imap_connection_id(&account.email),
        email: account.email.clone(),
        host: account.host.clone(),
        port: account.port,
        username: account.username.clone(),
    }
}

/// The account the body describes, and its password once checked for size.
fn described(body: ImapAccountBody) -> Result<(ImapAccount, String), ImapRouteError> {
    if body.password.is_empty() || body.password.len() > MAX_PASSWORD_LEN {
        return Err(refused(
            StatusCode::BAD_REQUEST,
            "invalid_password",
            "the password is empty or too long",
        ));
    }
    let account = ImapAccount::from_input(&ImapAccountInput {
        email: body.email,
        host: body.host,
        port: body.port,
        username: body.username,
    })
    .map_err(|e| imap_err(&e))?;
    Ok((account, body.password))
}

fn client() -> Result<ImapClient, ImapRouteError> {
    ImapClient::with_system_roots().map_err(|e| imap_err(&e))
}

/// `POST /api/v1/connectors/imap/test`: verify a mailbox without storing it.
#[utoipa::path(
    post,
    path = "/api/v1/connectors/imap/test",
    tag = "connectors",
    request_body = ImapAccountBody,
    responses(
        (status = 200, description = "The mailbox accepted the identifier and password", body = ImapAccountResponse),
        (status = 400, description = "Unusable address, server, port or password", body = ImapErrorBody),
        (status = 422, description = "The server refused the identifier or the password", body = ImapErrorBody),
        (status = 502, description = "The server is unreachable, its TLS failed, or it answered outside the protocol", body = ImapErrorBody),
    )
)]
pub(crate) async fn test_imap_mailbox(
    Json(body): Json<ImapAccountBody>,
) -> Result<Json<ImapAccountResponse>, ImapRouteError> {
    let (account, password) = described(body)?;
    verify(&account, &password, &client()?).await.map(Json)
}

async fn verify<P: ConnectionProbe>(
    account: &ImapAccount,
    password: &str,
    probe: &P,
) -> Result<ImapAccountResponse, ImapRouteError> {
    probe
        .verify(account, password)
        .await
        .map_err(|e| imap_err(&e))?;
    Ok(answer(account))
}

/// `POST /api/v1/connectors/imap/accounts`: verify a mailbox, then store it.
#[utoipa::path(
    post,
    path = "/api/v1/connectors/imap/accounts",
    tag = "connectors",
    request_body = ImapAccountBody,
    responses(
        (status = 201, description = "Mailbox verified and connected", body = ImapAccountResponse),
        (status = 400, description = "Unusable address, server, port or password", body = ImapErrorBody),
        (status = 403, description = "Refused by the connection admission", body = ImapErrorBody),
        (status = 422, description = "The server refused the identifier or the password", body = ImapErrorBody),
        (status = 500, description = "The secret store or the mailbox list could not be written", body = ImapErrorBody),
        (status = 502, description = "The server is unreachable, its TLS failed, or it answered outside the protocol", body = ImapErrorBody),
    )
)]
pub(crate) async fn add_imap_mailbox<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Json(body): Json<ImapAccountBody>,
) -> Result<(StatusCode, Json<ImapAccountResponse>), ImapRouteError> {
    let (account, password) = described(body)?;
    admit(&state.connection_governance, &account)?;
    let store = ImapAccountStore::open(&state.data_dir).map_err(|e| imap_err(&e))?;
    let response = add(
        &state.connection_governance,
        &store,
        account,
        &password,
        &client()?,
    )
    .await?;
    Ok((StatusCode::CREATED, Json(response)))
}

fn admit(governance: &ConnectionGovernance, account: &ImapAccount) -> Result<(), ImapRouteError> {
    match governance.admit(&AdmissionRequest::for_imap_account(account)) {
        AdmissionDecision::Admitted => Ok(()),
        AdmissionDecision::Refused { reason } => {
            Err(refused(StatusCode::FORBIDDEN, "admission_refused", reason))
        }
    }
}

/// Verify with `probe`, store in `store`, journal the addition.
async fn add<P: ConnectionProbe>(
    governance: &ConnectionGovernance,
    store: &ImapAccountStore,
    account: ImapAccount,
    password: &str,
    probe: &P,
) -> Result<ImapAccountResponse, ImapRouteError> {
    let account = store
        .add_verified(account, password, probe)
        .await
        .map_err(|e| imap_err(&e))?;
    governance.record_or_warn(&ConnectionEvent::Added {
        connection_id: imap_connection_id(&account.email),
        kind: ConnectionKind::Imap,
        remote: is_remote_host(&account.host),
        provenance: Provenance::Selection,
        actor: ROUTE_ACTOR.to_owned(),
    });
    Ok(answer(&account))
}

#[cfg(test)]
mod tests;
