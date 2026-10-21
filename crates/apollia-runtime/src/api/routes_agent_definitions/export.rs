//! `POST /api/v1/agent-definitions/{name}/export`: an agent created in the
//! interface, as a `.apo` package.
//!
//! The answer is the archive itself, `application/octet-stream`; the caller
//! writes the file where its user chose. What the export did on the way (the
//! publisher, the key that signed, the accesses removed, the connections
//! required) travels in the `X-Apollia-Export-Report` header, a JSON object in
//! ASCII, so the body stays the file and nothing else.
//!
//! The publisher is the name the request gives, else the `author.name`
//! setting of `apollia.toml`. A signed export signs with the author key kept
//! in the secret store, the key `package build --sign` uses; the first
//! signature generates it there.

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::Response;
use axum::Json;
use serde::{Deserialize, Serialize};

use apollia_signing::{
    load_signing_key, store_signing_key, SigningError, SigningKeyPair, AUTHOR_KEY_LABEL,
};
use apollia_tools::apo_archive::SignatureState;

use super::{open_agents, resolved};
use crate::api::routes_llm::servers::{api_err, ApiError};
use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;
use crate::packages::{export_agent, publisher_of, AgentExport, ExportError};

/// Name of the header that carries the report of an export.
pub const EXPORT_REPORT_HEADER: &str = "x-apollia-export-report";

/// Answer of an export: the bytes of the `.apo` archive.
///
/// The handler answers raw bytes, which carry no schema of their own.
pub enum ApoArchiveBody {}

impl utoipa::PartialSchema for ApoArchiveBody {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        use utoipa::openapi::schema::{KnownFormat, ObjectBuilder, SchemaFormat, Type};
        ObjectBuilder::new()
            .schema_type(Type::String)
            .format(Some(SchemaFormat::KnownFormat(KnownFormat::Binary)))
            .into()
    }
}

impl utoipa::ToSchema for ApoArchiveBody {}

/// Body of `POST /api/v1/agent-definitions/{name}/export`.
#[derive(Debug, Default, Deserialize, utoipa::ToSchema)]
pub struct ExportAgentRequest {
    /// Sign the package with the author key, generated on the first signature.
    #[serde(default)]
    pub sign: bool,
    /// Publisher name for this export only; the `author.name` setting when
    /// absent or blank.
    #[serde(default)]
    pub publisher: Option<String>,
}

/// What an export did, carried by the `X-Apollia-Export-Report` header.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ExportReport {
    /// File name the archive is offered under: `<id>-<version>.apo`.
    pub file_name: String,
    /// Publisher written in the package.
    pub publisher: String,
    /// Whether the package is signed.
    pub signed: bool,
    /// Fingerprint of the key that signed, 64 lowercase hex characters.
    pub key_fingerprint: Option<String>,
    /// Accesses left out: those into another package, agent or workflow.
    pub removed_access: Vec<String>,
    /// Connection types the package requires of whoever installs it.
    pub required_connections: Vec<String>,
}

impl From<&AgentExport> for ExportReport {
    fn from(export: &AgentExport) -> Self {
        let key_fingerprint = match &export.signature {
            SignatureState::Signed {
                key_fingerprint, ..
            } => Some(key_fingerprint.clone()),
            SignatureState::Unsigned => None,
        };
        Self {
            file_name: export.file_name.clone(),
            publisher: export.publisher.clone(),
            signed: key_fingerprint.is_some(),
            key_fingerprint,
            removed_access: export.removed_access.clone(),
            required_connections: export.required_connections.clone(),
        }
    }
}

/// JSON of `value` with every character outside ASCII escaped, so it fits in
/// a header.
fn ascii_json(value: &impl Serialize) -> String {
    let text = serde_json::to_string(value).unwrap_or_default();
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if c.is_ascii() {
            out.push(c);
        } else {
            let mut units = [0u16; 2];
            for unit in c.encode_utf16(&mut units) {
                out.push_str(&format!("\\u{unit:04x}"));
            }
        }
    }
    out
}

/// The `author.name` setting of the `apollia.toml` the runtime reads; empty
/// without a file or a name.
fn author_setting<B: ExecutionBackend + Clone>(state: &AppState<B>) -> String {
    let Some(path) = state.config_path.as_ref() else {
        return String::new();
    };
    match apollia_core::config::resolve_setting(
        path,
        apollia_core::config::AUTHOR_NAME_KEY,
        &apollia_core::config::PolicySettings::none(),
    ) {
        Ok(setting) => setting.value.as_str().unwrap_or_default().to_owned(),
        Err(error) => {
            tracing::event!(
                tracing::Level::WARN,
                error = %error,
                "agents.export.author_setting_unreadable"
            );
            String::new()
        }
    }
}

/// The author key in the secret store, generated and stored on first use.
fn author_key<B: ExecutionBackend + Clone>(
    state: &AppState<B>,
) -> Result<SigningKeyPair, ApiError> {
    let store = (state.secret_store)().map_err(|e| {
        api_err(
            StatusCode::SERVICE_UNAVAILABLE,
            format!("secret store unavailable: {e}"),
        )
    })?;
    match load_signing_key(store.as_ref(), AUTHOR_KEY_LABEL) {
        Ok(key) => Ok(key),
        Err(SigningError::KeyNotFound { .. }) => {
            let key = SigningKeyPair::generate()
                .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e))?;
            store_signing_key(store.as_ref(), AUTHOR_KEY_LABEL, &key)
                .map_err(|e| api_err(StatusCode::SERVICE_UNAVAILABLE, e))?;
            tracing::event!(
                tracing::Level::INFO,
                fingerprint = %key.public_key().fingerprint(),
                "agents.export.author_key_generated"
            );
            Ok(key)
        }
        Err(e) => Err(api_err(StatusCode::SERVICE_UNAVAILABLE, e)),
    }
}

fn export_error(err: ExportError) -> ApiError {
    match err {
        ExportError::NotInterface { .. } | ExportError::BuiltIn { .. } => {
            api_err(StatusCode::CONFLICT, err)
        }
        ExportError::InvalidIdentifier { .. }
        | ExportError::PublisherRequired
        | ExportError::InvalidPublisher(_) => api_err(StatusCode::UNPROCESSABLE_ENTITY, err),
        other => api_err(StatusCode::INTERNAL_SERVER_ERROR, other),
    }
}

/// `POST /api/v1/agent-definitions/{name}/export`: the agent as a `.apo`
/// package, signed on request.
#[utoipa::path(
    post,
    path = "/api/v1/agent-definitions/{name}/export",
    tag = "agents",
    params(("name" = String, Path, description = "Agent identifier")),
    request_body = ExportAgentRequest,
    responses(
        (status = 200, description = "The `.apo` archive. The `X-Apollia-Export-Report` header carries an `ExportReport` as ASCII JSON", content_type = "application/octet-stream", body = inline(ApoArchiveBody),
            headers(
                ("Content-Disposition" = String, description = "attachment; filename=\"<id>-<version>.apo\""),
                ("X-Apollia-Export-Report" = String, description = "The ExportReport of this export, ASCII JSON"),
            )),
        (status = 404, description = "Agent not found", body = crate::api::openapi::ApiErrorBody),
        (status = 409, description = "The agent came with a package, or is the built-in agent", body = crate::api::openapi::ApiErrorBody),
        (status = 422, description = "No publisher name, an invalid one, or an identifier that cannot name a package", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Secret store unavailable for a signed export", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn export_agent_definition<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(name): Path<String>,
    Json(req): Json<ExportAgentRequest>,
) -> Result<Response, ApiError> {
    let repo = open_agents(&state)?;
    let agent = resolved(&state, &repo, &name)?;
    drop(repo);
    // The card lists the default access of an agent nobody chose for; the
    // package grants what the card listed.
    let agent = match state.tool_registry_handle.as_ref() {
        Some(registry) => {
            apollia_tools::with_default_access(
                agent,
                registry,
                &super::default_access_setting(&state),
                &super::disabled_tools(&state),
            )
            .await
        }
        None => agent,
    };
    let publisher =
        publisher_of(req.publisher.as_deref(), &author_setting(&state)).map_err(export_error)?;
    let key = if req.sign {
        Some(author_key(&state)?)
    } else {
        None
    };
    let work_dir = state.data_dir.clone();
    let export = tokio::task::spawn_blocking(move || {
        export_agent(&agent, &publisher, key.as_ref(), &work_dir)
    })
    .await
    .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e))?
    .map_err(export_error)?;

    let report = ExportReport::from(&export);
    tracing::event!(
        tracing::Level::INFO,
        agent = %name,
        signed = report.signed,
        removed = report.removed_access.len(),
        required = report.required_connections.len(),
        "agents.export.done"
    );
    let disposition = format!("attachment; filename=\"{}\"", export.file_name);
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(
            header::CONTENT_DISPOSITION,
            HeaderValue::from_str(&disposition)
                .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e))?,
        )
        .header(
            EXPORT_REPORT_HEADER,
            HeaderValue::from_str(&ascii_json(&report))
                .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e))?,
        )
        .body(Body::from(export.bytes))
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e))
}
