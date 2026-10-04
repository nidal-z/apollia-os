//! `apollia.toml` over the native API: `GET` and `PUT /api/v1/config`,
//! `POST /api/v1/config/validate` and `GET /api/v1/config/default`.
//!
//! The routes read and write the file the runtime was started on, through the
//! one writer of the file ([`apollia_core::config::writer`]): a save presents
//! the fingerprint of the text it read, is validated, is held to the
//! organisation policy, and replaces the file atomically. The keys the policy
//! fixes are listed with their value so an editor can lock their lines.
//!
//! Nothing is reloaded here: a saved file applies at the next start of the
//! runtime, and the list of keys waiting for that restart stays empty until
//! the runtime reloads its configuration.

use std::path::PathBuf;

use apollia_core::config::writer::{
    check_text, default_text_under, locked_keys, read_snapshot, write_config, ConfigEdit,
    ConfigSnapshot, ConfigWriteError,
};
use apollia_core::config::{ConfigDiagnostic, PolicySettings};
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;

/// Builds the router of the configuration routes.
pub fn config_router<B: ExecutionBackend + Clone>() -> Router<AppState<B>> {
    Router::new()
        .route(
            "/api/v1/config",
            get(read_config::<B>).put(save_config::<B>),
        )
        .route("/api/v1/config/validate", post(validate_config::<B>))
        .route("/api/v1/config/default", get(default_config::<B>))
}

/// One fault of a configuration text, at its line.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConfigDiagnosticBody {
    /// 1-based line.
    pub line: usize,
    /// 1-based column, in characters.
    pub column: usize,
    /// Dotted path of the key at fault; empty at the root.
    pub path: String,
    /// What is wrong, in English.
    pub message: String,
}

impl From<ConfigDiagnostic> for ConfigDiagnosticBody {
    fn from(d: ConfigDiagnostic) -> Self {
        Self {
            line: d.line,
            column: d.column,
            path: d.path,
            message: d.message,
        }
    }
}

/// A key the organisation policy fixes, with its value.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LockedKeyBody {
    /// Dotted key.
    pub key: String,
    /// The value the policy imposes.
    #[schema(value_type = Object)]
    pub value: serde_json::Value,
}

/// `apollia.toml` as the runtime reads it.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConfigView {
    /// Where the file lives.
    pub path: String,
    /// Whether the file exists; an absent file reads as an empty text.
    pub exists: bool,
    /// The text of the file.
    pub text: String,
    /// SHA-256 of `text`, to present when saving.
    pub sha256: String,
    /// The faults of the text, each at its line; empty when it is valid.
    pub diagnostics: Vec<ConfigDiagnosticBody>,
    /// The keys the organisation policy fixes, with their value.
    pub locked: Vec<LockedKeyBody>,
    /// Keys changed in the file that wait for a restart of the runtime.
    pub pending_restart: Vec<String>,
}

/// A text to check.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct ConfigValidateRequest {
    /// The whole text of a candidate `apollia.toml`.
    pub text: String,
}

/// The verdict on a text.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ConfigValidation {
    /// Whether the writer would accept the text.
    pub valid: bool,
    /// Every fault, each at its line.
    pub diagnostics: Vec<ConfigDiagnosticBody>,
}

/// A text to save, with the fingerprint of the text it was made from.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct ConfigSaveRequest {
    /// The whole text of the new `apollia.toml`.
    pub text: String,
    /// SHA-256 of the text the change was made from, as `GET` returned it.
    pub sha256: String,
}

/// The default text of the file.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ConfigDefaultText {
    /// Every key commented at its default, the keys the policy fixes written
    /// at their value.
    pub text: String,
}

/// The error of a configuration route.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ConfigErrorBody {
    /// What went wrong, for the user; the same sentence the command line
    /// prints for the same refusal.
    pub error: String,
    /// `modified`, `invalid`, `locked`, `unavailable` or `internal`.
    pub code: String,
    /// The faults of the text, for `invalid` and `locked`.
    pub diagnostics: Vec<ConfigDiagnosticBody>,
}

type RouteError = (StatusCode, Json<ConfigErrorBody>);

fn failure(status: StatusCode, code: &str, error: impl ToString) -> RouteError {
    (
        status,
        Json(ConfigErrorBody {
            error: error.to_string(),
            code: code.to_owned(),
            diagnostics: Vec::new(),
        }),
    )
}

fn refusal(error: ConfigWriteError) -> RouteError {
    let message = error.to_string();
    match error {
        ConfigWriteError::Modified { .. } => failure(StatusCode::CONFLICT, "modified", message),
        ConfigWriteError::Invalid { diagnostics, .. } => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ConfigErrorBody {
                error: message,
                code: "invalid".to_owned(),
                diagnostics: diagnostics.into_iter().map(Into::into).collect(),
            }),
        ),
        ConfigWriteError::LockedByPolicy { line, key, .. } => (
            StatusCode::FORBIDDEN,
            Json(ConfigErrorBody {
                error: message.clone(),
                code: "locked".to_owned(),
                diagnostics: vec![ConfigDiagnosticBody {
                    line,
                    column: 1,
                    path: key,
                    message,
                }],
            }),
        ),
        ConfigWriteError::UnknownKey { .. }
        | ConfigWriteError::Withdrawn { .. }
        | ConfigWriteError::InvalidValue { .. } => {
            failure(StatusCode::UNPROCESSABLE_ENTITY, "invalid", message)
        }
        _ => failure(StatusCode::INTERNAL_SERVER_ERROR, "internal", message),
    }
}

fn config_path<B: ExecutionBackend + Clone>(state: &AppState<B>) -> Result<PathBuf, RouteError> {
    state.config_path.clone().ok_or_else(|| {
        failure(
            StatusCode::SERVICE_UNAVAILABLE,
            "unavailable",
            "this runtime was started without a configuration file",
        )
    })
}

fn view(path: &std::path::Path, snapshot: ConfigSnapshot, policy: &PolicySettings) -> ConfigView {
    let diagnostics = if snapshot.text.is_empty() {
        Vec::new()
    } else {
        check_text(&snapshot.text, policy)
    };
    ConfigView {
        path: path.display().to_string(),
        exists: snapshot.exists,
        diagnostics: diagnostics.into_iter().map(Into::into).collect(),
        locked: locked_keys(policy)
            .into_iter()
            .map(|k| LockedKeyBody {
                key: k.key,
                value: k.value,
            })
            .collect(),
        pending_restart: Vec::new(),
        text: snapshot.text,
        sha256: snapshot.sha256,
    }
}

/// Reads the file and builds its view; for the route and for a caller that
/// holds the path and the policy, such as a test of a surface that writes.
///
/// # Errors
///
/// The writer's error when the file cannot be read.
pub fn read_view(
    path: &std::path::Path,
    policy: &PolicySettings,
) -> Result<ConfigView, ConfigWriteError> {
    let snapshot = read_snapshot(path)?;
    Ok(view(path, snapshot, policy))
}

/// `GET /api/v1/config`: the text of `apollia.toml`, its fingerprint, its
/// faults, the keys the policy fixes and the keys waiting for a restart.
#[utoipa::path(
    get,
    path = "/api/v1/config",
    tag = "config",
    responses(
        (status = 200, description = "The configuration file", body = ConfigView),
        (status = 500, description = "The file cannot be read", body = ConfigErrorBody),
        (status = 503, description = "No configuration file in this runtime", body = ConfigErrorBody),
    )
)]
pub async fn read_config<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Result<Json<ConfigView>, RouteError> {
    let path = config_path(&state)?;
    let policy = state.inference_governance.imposed_settings();
    let read = tokio::task::spawn_blocking(move || read_view(&path, &policy))
        .await
        .map_err(|e| failure(StatusCode::INTERNAL_SERVER_ERROR, "internal", e))?
        .map_err(refusal)?;
    Ok(Json(read))
}

/// `POST /api/v1/config/validate`: what the writer would say of a text,
/// without writing it.
#[utoipa::path(
    post,
    path = "/api/v1/config/validate",
    tag = "config",
    request_body = ConfigValidateRequest,
    responses(
        (status = 200, description = "The verdict, with every fault at its line", body = ConfigValidation),
    )
)]
pub async fn validate_config<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Json(request): Json<ConfigValidateRequest>,
) -> Json<ConfigValidation> {
    let diagnostics = check_text(
        &request.text,
        &state.inference_governance.imposed_settings(),
    );
    Json(ConfigValidation {
        valid: diagnostics.is_empty(),
        diagnostics: diagnostics.into_iter().map(Into::into).collect(),
    })
}

/// `PUT /api/v1/config`: saves a whole text through the writer.
#[utoipa::path(
    put,
    path = "/api/v1/config",
    tag = "config",
    request_body = ConfigSaveRequest,
    responses(
        (status = 200, description = "The file as saved", body = ConfigView),
        (status = 403, description = "A line gives a key the policy fixes another value (`locked`)", body = ConfigErrorBody),
        (status = 409, description = "The file changed since it was read (`modified`)", body = ConfigErrorBody),
        (status = 422, description = "The text does not validate (`invalid`)", body = ConfigErrorBody),
        (status = 500, description = "The file cannot be read or written", body = ConfigErrorBody),
        (status = 503, description = "No configuration file in this runtime", body = ConfigErrorBody),
    )
)]
pub async fn save_config<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Json(request): Json<ConfigSaveRequest>,
) -> Result<Json<ConfigView>, RouteError> {
    let path = config_path(&state)?;
    let policy = state.inference_governance.imposed_settings();
    let saved = tokio::task::spawn_blocking(move || {
        let written = write_config(
            &path,
            ConfigEdit::Text(request.text),
            &request.sha256,
            &policy,
        )?;
        Ok::<_, ConfigWriteError>(view(&path, written, &policy))
    })
    .await
    .map_err(|e| failure(StatusCode::INTERNAL_SERVER_ERROR, "internal", e))?
    .map_err(refusal)?;
    tracing::event!(tracing::Level::INFO, "config.saved");
    Ok(Json(saved))
}

/// `GET /api/v1/config/default`: the default text of the file, the keys the
/// policy fixes written at their value.
#[utoipa::path(
    get,
    path = "/api/v1/config/default",
    tag = "config",
    responses(
        (status = 200, description = "The default text", body = ConfigDefaultText),
    )
)]
pub async fn default_config<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Json<ConfigDefaultText> {
    Json(ConfigDefaultText {
        text: default_text_under(&state.inference_governance.imposed_settings()),
    })
}

#[cfg(test)]
mod tests;
