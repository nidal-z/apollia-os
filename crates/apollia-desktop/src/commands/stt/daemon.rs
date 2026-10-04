//! The dictation settings served by the daemon, for the attached arm of the
//! commands of [`super`]: the configuration, the reload of the engine, its
//! status and the model files, over `/api/v1/stt/*`.

use serde::Deserialize;
use serde_json::{json, Value};

use super::{SttConfigView, SttModelInfo};
use crate::attached::{AttachedClient, AttachedError};

const CONFIG: &str = "/api/v1/stt/config";
const RELOAD: &str = "/api/v1/stt/reload";
const STATUS: &str = "/api/v1/stt/status";
const MODELS: &str = "/api/v1/stt/models";

/// One model file as the daemon lists it.
#[derive(Debug, Deserialize)]
struct DaemonModel {
    name: String,
    path: String,
    size_bytes: u64,
}

/// The answer of `GET /api/v1/stt/models`.
#[derive(Debug, Deserialize)]
struct DaemonModels {
    models: Vec<DaemonModel>,
}

/// Reads the configuration the daemon persisted.
pub(crate) async fn config_on(remote: &AttachedClient) -> Result<SttConfigView, AttachedError> {
    remote.get(CONFIG).await
}

/// Persists `config` on the daemon, then has it rebuild its engine.
///
/// As in the embedded arm, a reload that fails leaves the configuration saved
/// and is logged rather than returned.
pub(crate) async fn update_config_on(
    remote: &AttachedClient,
    config: &SttConfigView,
) -> Result<(), AttachedError> {
    let _: Value = remote.put(CONFIG, config).await?;
    tracing::event!(
        tracing::Level::INFO,
        enabled = config.enabled,
        model_path = %config.model_path,
        "stt.config.updated"
    );
    if let Err(e) = reload_on(remote).await {
        tracing::event!(
            tracing::Level::WARN,
            error = %e,
            detail = "the configuration is saved but the engine still runs the previous one",
            "stt.engine.reload.failed"
        );
    }
    Ok(())
}

/// Has the daemon rebuild its engine from the persisted configuration.
pub(crate) async fn reload_on(remote: &AttachedClient) -> Result<(), AttachedError> {
    let _: Value = remote.post(RELOAD, &json!({})).await?;
    Ok(())
}

/// Reads the status of the daemon's engine.
pub(crate) async fn status_on(remote: &AttachedClient) -> Result<Value, AttachedError> {
    remote.get(STATUS).await
}

/// Lists the model files of the daemon's models folder, described as the
/// embedded arm describes them.
pub(crate) async fn models_on(remote: &AttachedClient) -> Result<Vec<SttModelInfo>, AttachedError> {
    let answer: DaemonModels = remote.get(MODELS).await?;
    Ok(answer
        .models
        .into_iter()
        .map(|m| SttModelInfo::from_file(m.name, m.path, m.size_bytes))
        .collect())
}

#[cfg(test)]
mod tests;
