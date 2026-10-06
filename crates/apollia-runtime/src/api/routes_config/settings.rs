//! Settings of `apollia.toml` the settings pages read and write key by key,
//! and the isolation level of the machine the runtime runs on:
//!
//! - `GET` and `PUT /api/v1/config/observability`: the capture and retention
//!   policy, `[observability]` and `[llm.observability] debug_log_prompt`;
//! - `GET` and `PUT /api/v1/config/python-interpreter`: `[tools]
//!   python_interpreter`, with the interpreter this runtime ships;
//! - `GET /api/v1/security-posture`: the confinement active where the tools
//!   of this runtime run.
//!
//! The writes go through the one writer of the file, held to the organisation
//! policy, and are shown to the configuration reloader, which applies what
//! the running engine can apply and journals the change as made from the
//! settings.

use std::path::Path;

use apollia_core::config::writer::{update_config, ConfigEdit, KeyChange};
use apollia_core::config::PolicySettings;
use apollia_core::ObservabilityConfig;
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::{config_path, failure, refusal, ConfigErrorBody, RouteError};
use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;

/// Builds the router of the settings routes.
pub fn settings_router<B: ExecutionBackend + Clone>() -> Router<AppState<B>> {
    Router::new()
        .route(
            "/api/v1/config/observability",
            get(read_observability::<B>).put(save_observability::<B>),
        )
        .route(
            "/api/v1/config/python-interpreter",
            get(read_python_interpreter::<B>).put(save_python_interpreter::<B>),
        )
        .route("/api/v1/security-posture", get(security_posture))
}

/// The observability policy, one form over the two sections that hold it.
///
/// Every field but `debug_log_prompt` is a key of `[observability]`;
/// `debug_log_prompt` is the key of `[llm.observability]`, read by a different
/// type, and is written there: under `[observability]` nothing would read it.
/// An absent field reads as its default.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(default)]
pub struct ObservabilitySettings {
    /// Max size of task and step inputs, in bytes.
    pub max_input_bytes: usize,
    /// Max size of task, step and completion outputs, in bytes.
    pub max_output_bytes: usize,
    /// Max size of a tool's stdout and stderr, in bytes.
    pub max_tool_output_bytes: usize,
    /// Whether the reasoning of an agent is kept on its trace.
    pub capture_thoughts: bool,
    /// Whether the arguments of tool calls are kept.
    pub capture_tool_args: bool,
    /// Whether the outputs of tool calls are kept.
    pub capture_tool_outputs: bool,
    /// Whether the logs an agent writes are kept on its trace.
    pub capture_agent_logs: bool,
    /// Days the trace is kept before it is purged.
    pub retention_days: u32,
    /// Logs the whole prompt at `TRACE`, and persists nothing.
    pub debug_log_prompt: bool,
}

impl Default for ObservabilitySettings {
    fn default() -> Self {
        Self::from_parts(ObservabilityConfig::default(), false)
    }
}

impl ObservabilitySettings {
    fn from_parts(c: ObservabilityConfig, debug_log_prompt: bool) -> Self {
        Self {
            max_input_bytes: c.max_input_bytes,
            max_output_bytes: c.max_output_bytes,
            max_tool_output_bytes: c.max_tool_output_bytes,
            capture_thoughts: c.capture_thoughts,
            capture_tool_args: c.capture_tool_args,
            capture_tool_outputs: c.capture_tool_outputs,
            capture_agent_logs: c.capture_agent_logs,
            retention_days: c.retention_days,
            debug_log_prompt,
        }
    }

    /// Each field as the key of its section.
    fn changes(&self) -> Vec<KeyChange> {
        vec![
            KeyChange::set("observability.max_input_bytes", json!(self.max_input_bytes)),
            KeyChange::set(
                "observability.max_output_bytes",
                json!(self.max_output_bytes),
            ),
            KeyChange::set(
                "observability.max_tool_output_bytes",
                json!(self.max_tool_output_bytes),
            ),
            KeyChange::set(
                "observability.capture_thoughts",
                json!(self.capture_thoughts),
            ),
            KeyChange::set(
                "observability.capture_tool_args",
                json!(self.capture_tool_args),
            ),
            KeyChange::set(
                "observability.capture_tool_outputs",
                json!(self.capture_tool_outputs),
            ),
            KeyChange::set(
                "observability.capture_agent_logs",
                json!(self.capture_agent_logs),
            ),
            KeyChange::set("observability.retention_days", json!(self.retention_days)),
            KeyChange::set(
                "llm.observability.debug_log_prompt",
                json!(self.debug_log_prompt),
            ),
        ]
    }
}

/// The text of the file at `path`, or `None` when it is absent.
fn read_text(path: &Path) -> Result<Option<toml::Value>, String> {
    if !path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read {}: {e}", path.display()))?;
    content
        .parse()
        .map(Some)
        .map_err(|e| format!("failed to parse {}: {e}", path.display()))
}

/// Reads the observability policy of the file at `path`; an absent file or
/// section reads as the defaults.
///
/// # Errors
///
/// The sentence to show when the file cannot be read or parsed, or when its
/// `[observability]` section does not hold the policy.
pub fn read_observability_at(path: &Path) -> Result<ObservabilitySettings, String> {
    let Some(doc) = read_text(path)? else {
        return Ok(ObservabilitySettings::default());
    };
    let capture = match doc.get("observability") {
        Some(section) => section
            .clone()
            .try_into()
            .map_err(|e| format!("invalid [observability] section: {e}"))?,
        None => ObservabilityConfig::default(),
    };
    let debug_log_prompt = doc
        .get("llm")
        .and_then(|llm| llm.get("observability"))
        .and_then(|obs| obs.get("debug_log_prompt"))
        .and_then(toml::Value::as_bool)
        .unwrap_or(false);
    Ok(ObservabilitySettings::from_parts(capture, debug_log_prompt))
}

/// The Python interpreter setting.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct PythonInterpreterSetting {
    /// The absolute path in `[tools] python_interpreter`, or `None` when the
    /// bundled interpreter is in use.
    pub chosen: Option<String>,
    /// Absolute path of the interpreter this runtime ships, when it knows it;
    /// `None` in a development tree, where no bundle is staged.
    pub bundled: Option<String>,
    /// Minor version the bundled standard library requires of any chosen
    /// interpreter.
    pub required_minor: u32,
}

/// The interpreter to use: an absolute path, or none to return to the bundled
/// one.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct PythonInterpreterRequest {
    /// Absolute path of an interpreter; `null` or blank returns to the bundled
    /// interpreter.
    #[serde(default)]
    pub path: Option<String>,
}

/// `[tools] python_interpreter` of the file at `path`; `None` when the file
/// is absent, unreadable or does not carry the key, which all mean the bundled
/// interpreter is in use.
pub fn read_python_interpreter_at(path: &Path) -> Option<String> {
    read_text(path)
        .ok()
        .flatten()?
        .get("tools")?
        .get("python_interpreter")?
        .as_str()
        .map(str::to_string)
        .filter(|s| !s.trim().is_empty())
}

fn python_setting(path: &Path) -> PythonInterpreterSetting {
    PythonInterpreterSetting {
        chosen: read_python_interpreter_at(path),
        bundled: apollia_tools::tools::python_discovery::bundled_interpreter()
            .map(|p| p.display().to_string()),
        required_minor: apollia_tools::tools::python_discovery::BUNDLED_PYTHON_MINOR,
    }
}

/// Writes `[tools] python_interpreter` through the one writer of the file.
/// `None` removes the key: an absent key and the bundled interpreter are the
/// same thing, and a blank string in the file reads as a setting somebody
/// meant.
///
/// # Errors
///
/// The writer's error.
pub fn write_python_interpreter_at(
    path: &Path,
    chosen: Option<&str>,
    policy: &PolicySettings,
) -> Result<(), apollia_core::config::writer::ConfigWriteError> {
    const KEY: &str = "tools.python_interpreter";
    let change = match chosen {
        Some(candidate) => KeyChange::set(KEY, json!(candidate)),
        None => KeyChange::remove(KEY),
    };
    update_config(path, ConfigEdit::Keys(vec![change]), policy).map(|_| ())
}

fn internal(e: impl ToString) -> RouteError {
    failure(StatusCode::INTERNAL_SERVER_ERROR, "internal", e)
}

/// Shows the reloader the file a settings route has just written, so the
/// change is applied now and journaled as made from the settings rather than
/// found on disk by the watcher.
async fn declare_settings_write<B: ExecutionBackend + Clone>(state: &AppState<B>, path: &Path) {
    let Some(reload) = &state.config_reload else {
        return;
    };
    let path = path.to_path_buf();
    let read =
        tokio::task::spawn_blocking(move || apollia_core::config::writer::read_snapshot(&path))
            .await;
    if let Ok(Ok(snapshot)) = read {
        reload
            .saved(snapshot.text, crate::config_reload::ConfigOrigin::Settings)
            .await;
    }
}

/// `GET /api/v1/config/observability`: the capture and retention policy.
#[utoipa::path(
    get,
    path = "/api/v1/config/observability",
    tag = "config",
    responses(
        (status = 200, description = "The observability policy", body = ObservabilitySettings),
        (status = 500, description = "The file cannot be read or parsed", body = ConfigErrorBody),
        (status = 503, description = "No configuration file in this runtime", body = ConfigErrorBody),
    )
)]
pub async fn read_observability<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Result<Json<ObservabilitySettings>, RouteError> {
    let path = config_path(&state)?;
    let read = tokio::task::spawn_blocking(move || read_observability_at(&path))
        .await
        .map_err(internal)?
        .map_err(internal)?;
    Ok(Json(read))
}

/// `PUT /api/v1/config/observability`: writes the policy, applied at the next
/// start of the runtime.
#[utoipa::path(
    put,
    path = "/api/v1/config/observability",
    tag = "config",
    request_body = ObservabilitySettings,
    responses(
        (status = 200, description = "The policy as saved", body = ObservabilitySettings),
        (status = 403, description = "The policy fixes one of the keys (`locked`)", body = ConfigErrorBody),
        (status = 422, description = "A value is refused (`invalid`)", body = ConfigErrorBody),
        (status = 500, description = "The file cannot be written", body = ConfigErrorBody),
        (status = 503, description = "No configuration file in this runtime", body = ConfigErrorBody),
    )
)]
pub async fn save_observability<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Json(settings): Json<ObservabilitySettings>,
) -> Result<Json<ObservabilitySettings>, RouteError> {
    let path = config_path(&state)?;
    let policy = state.inference_governance.imposed_settings();
    let changes = settings.changes();
    let written = path.clone();
    tokio::task::spawn_blocking(move || update_config(&path, ConfigEdit::Keys(changes), &policy))
        .await
        .map_err(internal)?
        .map_err(refusal)?;
    declare_settings_write(&state, &written).await;
    tracing::event!(
        tracing::Level::INFO,
        max_input_bytes = settings.max_input_bytes,
        retention_days = settings.retention_days,
        debug_log_prompt = settings.debug_log_prompt,
        "observability.config_saved"
    );
    Ok(Json(settings))
}

/// `GET /api/v1/config/python-interpreter`: the chosen interpreter and the
/// one this runtime ships.
#[utoipa::path(
    get,
    path = "/api/v1/config/python-interpreter",
    tag = "config",
    responses(
        (status = 200, description = "The Python interpreter setting", body = PythonInterpreterSetting),
        (status = 503, description = "No configuration file in this runtime", body = ConfigErrorBody),
    )
)]
pub async fn read_python_interpreter<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Result<Json<PythonInterpreterSetting>, RouteError> {
    let path = config_path(&state)?;
    let setting = tokio::task::spawn_blocking(move || python_setting(&path))
        .await
        .map_err(internal)?;
    Ok(Json(setting))
}

/// `PUT /api/v1/config/python-interpreter`: chooses an interpreter, checked
/// before it is written (it exists, starts, and has the minor version of the
/// bundled standard library), or returns to the bundled one. Applies to the
/// agents started after it.
#[utoipa::path(
    put,
    path = "/api/v1/config/python-interpreter",
    tag = "config",
    request_body = PythonInterpreterRequest,
    responses(
        (status = 200, description = "The setting as saved", body = PythonInterpreterSetting),
        (status = 403, description = "The policy fixes the key (`locked`)", body = ConfigErrorBody),
        (status = 422, description = "The interpreter is refused, with the condition it failed (`invalid`)", body = ConfigErrorBody),
        (status = 500, description = "The file cannot be written", body = ConfigErrorBody),
        (status = 503, description = "No configuration file in this runtime", body = ConfigErrorBody),
    )
)]
pub async fn save_python_interpreter<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Json(request): Json<PythonInterpreterRequest>,
) -> Result<Json<PythonInterpreterSetting>, RouteError> {
    let path = config_path(&state)?;
    let policy = state.inference_governance.imposed_settings();
    let chosen = request
        .path
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty());
    let written = chosen.clone();
    let file = path.clone();
    let setting = tokio::task::spawn_blocking(move || {
        if let Some(candidate) = written.as_deref() {
            apollia_tools::tools::python_discovery::validate_chosen_interpreter(Path::new(
                candidate,
            ))
            .map_err(|e| failure(StatusCode::UNPROCESSABLE_ENTITY, "invalid", e))?;
        }
        write_python_interpreter_at(&path, written.as_deref(), &policy).map_err(refusal)?;
        Ok::<_, RouteError>(python_setting(&path))
    })
    .await
    .map_err(internal)??;
    declare_settings_write(&state, &file).await;
    tracing::event!(
        tracing::Level::INFO,
        chosen = chosen.as_deref().unwrap_or("<bundled>"),
        "python.interpreter.setting_saved"
    );
    Ok(Json(setting))
}

/// `GET /api/v1/security-posture`: the confinement of native tools and the
/// trust model of agent code where this runtime runs them.
#[utoipa::path(
    get,
    path = "/api/v1/security-posture",
    tag = "governance",
    responses(
        (status = 200, description = "The active security posture", body = Object),
        (status = 500, description = "The probe did not finish", body = ConfigErrorBody),
    )
)]
pub async fn security_posture() -> Result<Json<apollia_core::SecurityPosture>, RouteError> {
    let posture = tokio::task::spawn_blocking(apollia_core::SecurityPosture::detect)
        .await
        .map_err(internal)?;
    Ok(Json(posture))
}

#[cfg(test)]
mod tests;
