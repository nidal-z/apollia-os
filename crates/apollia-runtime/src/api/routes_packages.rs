//! `.apo` packages: preview, installation, and the keyring of trusted
//! publisher keys.
//!
//! - `POST   /api/v1/packages/preview`                        , what a package contains and asks for; nothing is written
//! - `POST   /api/v1/packages/install`                        , install or update a package
//! - `GET    /api/v1/packages/trusted-keys`                   , list the trusted publisher keys
//! - `POST   /api/v1/packages/trusted-keys`                   , import a publisher key
//! - `DELETE /api/v1/packages/trusted-keys/{fingerprint}`     , remove a publisher key
//! - `POST   /api/v1/packages/{name}/uninstall`               , uninstall a package
//! - `POST   /api/v1/agents/installed/{name}/uninstall`       , uninstall an agent installed on its own
//!
//! The archive is named by its path on this machine: the runtime and its
//! callers share the file system. Both package routes answer `503` without
//! an audit journal: an installation is never left unrecorded, and the
//! preview reads the same governance the installation does.

use std::path::PathBuf;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use apollia_mcp::config::McpServerConfig;
use apollia_tools::apo_archive::{ApoArchive, ApoError};
use apollia_tools::{
    AgentRepository, PackageRepository, ToolDescriptor, TrustedKey, TrustedKeyError,
    TrustedKeyOrigin, TrustedKeyRepository,
};

pub(crate) mod uninstall;

pub use uninstall::UninstallRequest;

use crate::api::server::AppState;
use crate::connections::Connection;
use crate::coordinator::ExecutionBackend;
use crate::governance::NoPolicyGovernance;
use crate::packages::{
    commit, prepare, preview, CommitTargets, InstallOptions, InstallRefusal, InstallReport,
    McpServerAdder, PackageError, PackageOrigin, PackagePreview, PreviewSources,
};

/// Who the journal names for an installation made through the API.
const API_ACTOR: &str = "api";

/// Error body of the package routes: the message, and a stable code a client
/// can act on.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct PackageErrorBody {
    /// Human-readable description.
    pub error: String,
    /// Stable code: `archive`, `invalid_signature`, `signature_required`,
    /// `downgrade`, `agent_taken`, `deps_confirmation_required`,
    /// `changes_confirmation_required`, `mcp_unavailable`, `mcp_server`,
    /// `environment`, `invalid_key`, `not_found`, `built_in`,
    /// `owned_by_package`, `journal_unavailable`, `internal`.
    pub code: String,
}

type ApiError = (StatusCode, Json<PackageErrorBody>);

fn api_err(status: StatusCode, code: &str, msg: impl std::fmt::Display) -> ApiError {
    (
        status,
        Json(PackageErrorBody {
            error: msg.to_string(),
            code: code.to_owned(),
        }),
    )
}

/// Body of the preview and of the installation: the archive.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PackagePreviewRequest {
    /// Path of the `.apo` file on this machine.
    pub path: String,
}

/// Body of `POST /api/v1/packages/install`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PackageInstallRequest {
    /// Path of the `.apo` file on this machine.
    pub path: String,
    /// The user accepted the Python dependencies the package installs.
    #[serde(default)]
    pub deps_confirmed: bool,
    /// The user accepted what an update adds.
    #[serde(default)]
    pub changes_confirmed: bool,
    /// Install a version older than the installed one.
    #[serde(default)]
    pub allow_downgrade: bool,
}

/// Body of `POST /api/v1/packages/trusted-keys`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TrustKeyRequest {
    /// The publisher's public key: 64 hexadecimal characters.
    pub public_key: String,
    /// Name to show for the publisher.
    pub label: String,
}

/// A trusted publisher key.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TrustedKeyView {
    /// SHA-256 fingerprint of the key.
    pub fingerprint: String,
    /// The public key, 64 hexadecimal characters.
    pub public_key: String,
    /// Name shown for the publisher.
    pub label: String,
    /// Who put the key there: `user` or `policy`.
    pub origin: String,
    /// When it was added.
    pub added_at: String,
}

impl From<TrustedKey> for TrustedKeyView {
    fn from(key: TrustedKey) -> Self {
        Self {
            fingerprint: key.fingerprint,
            public_key: key.public_key,
            label: key.label,
            origin: key.origin.as_str().to_owned(),
            added_at: key.added_at,
        }
    }
}

/// The package routes.
pub fn package_routes<B: ExecutionBackend + Clone>() -> Router<AppState<B>> {
    Router::new()
        .route("/api/v1/packages/preview", post(preview_package::<B>))
        .route("/api/v1/packages/install", post(install_package::<B>))
        .route(
            "/api/v1/packages/trusted-keys",
            get(list_trusted_keys::<B>).post(trust_key::<B>),
        )
        .route(
            "/api/v1/packages/trusted-keys/:fingerprint",
            delete(remove_trusted_key::<B>),
        )
        .route(
            "/api/v1/packages/:name/uninstall",
            post(uninstall::uninstall_package::<B>),
        )
        .route(
            "/api/v1/agents/installed/:name/uninstall",
            post(uninstall::uninstall_agent::<B>),
        )
}

/// The stores a preview and an installation read, owned so that the work on
/// the archive runs off the async runtime.
struct Stores {
    governance: NoPolicyGovernance,
    packages: PackageRepository,
    agents: AgentRepository,
    trusted_keys: TrustedKeyRepository,
    connections: Vec<Connection>,
    tools: Vec<ToolDescriptor>,
}

impl Stores {
    fn sources(&self) -> PreviewSources<'_> {
        PreviewSources {
            governance: &self.governance,
            packages: &self.packages,
            agents: &self.agents,
            trusted_keys: &self.trusted_keys,
            connections: &self.connections,
            tools: &self.tools,
        }
    }
}

fn internal(e: impl std::fmt::Display) -> ApiError {
    api_err(StatusCode::INTERNAL_SERVER_ERROR, "internal", e)
}

fn agents_db<B: ExecutionBackend + Clone>(state: &AppState<B>) -> PathBuf {
    apollia_core::paths::DataFile::Agents.path(&state.data_dir)
}

async fn gather<B: ExecutionBackend + Clone>(
    state: &AppState<B>,
    archive: &std::path::Path,
) -> Result<Stores, ApiError> {
    let journal = state.audit_journal.clone().ok_or_else(|| {
        api_err(
            StatusCode::SERVICE_UNAVAILABLE,
            "journal_unavailable",
            "audit journal not available: a package is not installed without its record",
        )
    })?;
    let db = agents_db(state);
    let packages = PackageRepository::open(&db).map_err(internal)?;
    let agents = AgentRepository::open(&db).map_err(internal)?;
    let trusted_keys = TrustedKeyRepository::open(&db).map_err(internal)?;

    // The native accounts are read only for a package that requires a
    // connection: reading them opens the account index.
    let requires_connections = ApoArchive::open(archive)
        .map(|a| !a.manifest().requires.is_empty())
        .unwrap_or(false);
    let accounts = if requires_connections {
        match crate::api::routes_connections::native_accounts().await {
            Ok(accounts) => accounts,
            Err((_, body)) => {
                tracing::event!(
                    tracing::Level::WARN,
                    error = %body.0.error,
                    detail = "native accounts are reported missing",
                    "packages.accounts.unreadable"
                );
                Vec::new()
            }
        }
    } else {
        Vec::new()
    };
    let connections = crate::api::routes_connections::collect_connections(state, accounts)
        .await
        .map_err(|(_, body)| internal(&body.0.error))?;
    let tools = match &state.tool_registry_handle {
        Some(registry) => registry.list().await.map_err(internal)?,
        None => Vec::new(),
    };
    Ok(Stores {
        governance: NoPolicyGovernance::new(journal),
        packages,
        agents,
        trusted_keys,
        connections,
        tools,
    })
}

fn package_error(error: PackageError) -> ApiError {
    let message = error.to_string();
    match error {
        PackageError::Archive(ApoError::Io { .. }) => {
            api_err(StatusCode::BAD_REQUEST, "archive", message)
        }
        PackageError::Archive(_) => api_err(StatusCode::UNPROCESSABLE_ENTITY, "archive", message),
        PackageError::Refused(refusal) => match refusal {
            InstallRefusal::InvalidSignature { .. } => api_err(
                StatusCode::UNPROCESSABLE_ENTITY,
                "invalid_signature",
                message,
            ),
            InstallRefusal::SignatureRequired => {
                api_err(StatusCode::FORBIDDEN, "signature_required", message)
            }
            InstallRefusal::Downgrade { .. } => api_err(StatusCode::CONFLICT, "downgrade", message),
            InstallRefusal::AgentTaken { .. } => {
                api_err(StatusCode::CONFLICT, "agent_taken", message)
            }
        },
        PackageError::DepsConfirmationRequired { .. } => api_err(
            StatusCode::PRECONDITION_REQUIRED,
            "deps_confirmation_required",
            message,
        ),
        PackageError::ChangesConfirmationRequired => api_err(
            StatusCode::PRECONDITION_REQUIRED,
            "changes_confirmation_required",
            message,
        ),
        PackageError::McpUnavailable => {
            api_err(StatusCode::SERVICE_UNAVAILABLE, "mcp_unavailable", message)
        }
        PackageError::McpServer { .. } => {
            api_err(StatusCode::UNPROCESSABLE_ENTITY, "mcp_server", message)
        }
        PackageError::Environment(_) => {
            api_err(StatusCode::INTERNAL_SERVER_ERROR, "environment", message)
        }
        _ => internal(message),
    }
}

/// `POST /api/v1/packages/preview`: what the package contains, who signed
/// it, what it asks for, which connections it requires and brings, and what
/// an update adds. Nothing is written.
#[utoipa::path(
    post,
    path = "/api/v1/packages/preview",
    tag = "packages",
    request_body = PackagePreviewRequest,
    responses(
        (status = 200, description = "The preview", body = PackagePreview),
        (status = 400, description = "The file cannot be read", body = PackageErrorBody),
        (status = 422, description = "Not a valid package", body = PackageErrorBody),
        (status = 503, description = "No audit journal", body = PackageErrorBody),
    )
)]
pub(crate) async fn preview_package<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Json(request): Json<PackagePreviewRequest>,
) -> Result<Json<PackagePreview>, ApiError> {
    let path = PathBuf::from(&request.path);
    let stores = gather(&state, &path).await?;
    let result = tokio::task::spawn_blocking(move || preview(&path, &stores.sources()))
        .await
        .map_err(internal)?;
    result.map(Json).map_err(package_error)
}

/// The MCP manager of the runtime, as the installer reaches it.
struct StateMcp<'a, B: ExecutionBackend + Clone> {
    state: &'a AppState<B>,
}

impl<B: ExecutionBackend + Clone> McpServerAdder for StateMcp<'_, B> {
    async fn add(&self, config: McpServerConfig) -> Result<(), String> {
        crate::api::routes_mcp::add_server_to(self.state, config)
            .await
            .map(|_| ())
            .map_err(|(_, body)| mcp_message(&body.0))
    }

    async fn remove(&self, name: &str) -> Result<(), String> {
        crate::api::routes_mcp::remove_server_from(self.state, name)
            .await
            .map_err(|(_, body)| mcp_message(&body.0))
    }
}

fn mcp_message(body: &serde_json::Value) -> String {
    body.get("error")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("refused")
        .to_owned()
}

/// `POST /api/v1/packages/install`: install a package, or update it.
///
/// The preview is computed again and its refusal applies: an invalid
/// signature, an unsigned package when governance requires a signature, an
/// older version without `allowDowngrade`, an agent another package owns.
/// Python dependencies need `depsConfirmed`; an update that adds accesses,
/// required connections or MCP servers needs `changesConfirmed`. The MCP
/// servers the package brings are added like `POST /api/v1/mcp/servers`
/// adds one. The installation is recorded in the audit journal.
#[utoipa::path(
    post,
    path = "/api/v1/packages/install",
    tag = "packages",
    request_body = PackageInstallRequest,
    responses(
        (status = 201, description = "Installed", body = InstallReport),
        (status = 400, description = "The file cannot be read", body = PackageErrorBody),
        (status = 403, description = "A signed package is required", body = PackageErrorBody),
        (status = 409, description = "Older version, or agent owned elsewhere", body = PackageErrorBody),
        (status = 422, description = "Invalid package or signature, or MCP server refused", body = PackageErrorBody),
        (status = 428, description = "A confirmation is missing", body = PackageErrorBody),
        (status = 500, description = "Store, environment or journal failure", body = PackageErrorBody),
        (status = 503, description = "No audit journal, or no MCP manager for the servers it brings", body = PackageErrorBody),
    )
)]
pub(crate) async fn install_package<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Json(request): Json<PackageInstallRequest>,
) -> Result<(StatusCode, Json<InstallReport>), ApiError> {
    let path = PathBuf::from(&request.path);
    let stores = gather(&state, &path).await?;
    let options = InstallOptions {
        deps_confirmed: request.deps_confirmed,
        changes_confirmed: request.changes_confirmed,
        allow_downgrade: request.allow_downgrade,
        actor: API_ACTOR.to_owned(),
        origin: PackageOrigin::File,
    };
    let data_dir = state.data_dir.clone();
    let (stores, staged) = tokio::task::spawn_blocking(move || {
        let staged = prepare(&path, &options, &stores.sources(), &data_dir);
        (stores, staged)
    })
    .await
    .map_err(internal)?;
    let staged = staged.map_err(package_error)?;

    let mcp = StateMcp { state: &state };
    let venvs = state.data_dir.join("venvs");
    let targets = CommitTargets {
        packages: &stores.packages,
        agents: &stores.agents,
        governance: &stores.governance,
        mcp: state.mcp_handle.as_ref().map(|_| &mcp),
        venvs: &venvs,
    };
    let report = commit(staged, &targets, API_ACTOR)
        .await
        .map_err(package_error)?;
    Ok((StatusCode::CREATED, Json(report)))
}

fn open_keyring<B: ExecutionBackend + Clone>(
    state: &AppState<B>,
) -> Result<TrustedKeyRepository, ApiError> {
    TrustedKeyRepository::open(&agents_db(state)).map_err(internal)
}

/// `GET /api/v1/packages/trusted-keys`: the publisher keys the user trusts.
#[utoipa::path(
    get,
    path = "/api/v1/packages/trusted-keys",
    tag = "packages",
    responses(
        (status = 200, description = "The trusted keys", body = [TrustedKeyView]),
        (status = 500, description = "The keyring cannot be read", body = PackageErrorBody),
    )
)]
pub(crate) async fn list_trusted_keys<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Result<Json<Vec<TrustedKeyView>>, ApiError> {
    let keys = open_keyring(&state)?.list().map_err(internal)?;
    Ok(Json(keys.into_iter().map(Into::into).collect()))
}

/// `POST /api/v1/packages/trusted-keys`: trust a publisher key. A package
/// signed by it is then shown under the label given here.
#[utoipa::path(
    post,
    path = "/api/v1/packages/trusted-keys",
    tag = "packages",
    request_body = TrustKeyRequest,
    responses(
        (status = 201, description = "The key is trusted", body = TrustedKeyView),
        (status = 422, description = "Not a public key, or an empty label", body = PackageErrorBody),
        (status = 500, description = "The keyring cannot be written", body = PackageErrorBody),
    )
)]
pub(crate) async fn trust_key<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Json(request): Json<TrustKeyRequest>,
) -> Result<(StatusCode, Json<TrustedKeyView>), ApiError> {
    let key = open_keyring(&state)?
        .import(&request.public_key, &request.label, TrustedKeyOrigin::User)
        .map_err(|e| match e {
            TrustedKeyError::InvalidKey(_) | TrustedKeyError::EmptyLabel => {
                api_err(StatusCode::UNPROCESSABLE_ENTITY, "invalid_key", e)
            }
            other => internal(other),
        })?;
    tracing::event!(
        tracing::Level::INFO,
        fingerprint = %key.fingerprint,
        "packages.key.trusted"
    );
    Ok((StatusCode::CREATED, Json(key.into())))
}

/// `DELETE /api/v1/packages/trusted-keys/{fingerprint}`: stop trusting a
/// publisher key.
#[utoipa::path(
    delete,
    path = "/api/v1/packages/trusted-keys/{fingerprint}",
    tag = "packages",
    params(("fingerprint" = String, Path, description = "SHA-256 fingerprint of the key")),
    responses(
        (status = 200, description = "The key is removed"),
        (status = 404, description = "No such key", body = PackageErrorBody),
        (status = 500, description = "The keyring cannot be written", body = PackageErrorBody),
    )
)]
pub(crate) async fn remove_trusted_key<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(fingerprint): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if !open_keyring(&state)?
        .remove(&fingerprint)
        .map_err(internal)?
    {
        return Err(api_err(
            StatusCode::NOT_FOUND,
            "not_found",
            format!("no trusted key of fingerprint {fingerprint}"),
        ));
    }
    tracing::event!(
        tracing::Level::INFO,
        fingerprint = %fingerprint,
        "packages.key.removed"
    );
    Ok(Json(serde_json::json!({ "removed": fingerprint })))
}

#[cfg(test)]
mod tests;
