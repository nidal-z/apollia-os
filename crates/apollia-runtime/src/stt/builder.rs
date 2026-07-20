//! Reusable constructor for the STT engine actor.
//!
//! Shared between boot ([`Supervisor::start_stt_engine`](crate::supervisor))
//! and the runtime reload path, so a model enabled or downloaded mid-session
//! can be brought online without restarting the daemon.

use std::path::Path;
use std::sync::{Arc, Mutex};

use apollia_core::SttConfigRow;
use tracing::{error, info, warn};

use crate::eventbus::EventBusSender;
use crate::runner_supervisor::{RunnerProxy, RunnerSttBackend};
use crate::stt::SttEngineHandle;

/// Build a fresh STT engine actor from the persisted configuration.
///
/// The engine (first tuple element) is `None` when STT is disabled in config,
/// the model file is absent, the runner sidecar is unavailable, or the store
/// cannot be opened. The repository (second element) is opened independently of
/// the model, so transcription history stays available even when live dictation
/// cannot start; it is `None` only when STT is disabled in config or the store
/// itself fails to open. `data_dir` locates `stt_transcriptions.db`;
/// `runner_proxy` is the sidecar handle STT inference routes through.
///
/// STT always routes through the runner sidecar; without a proxy it stays
/// disabled.
#[allow(clippy::type_complexity)]
pub async fn build_stt_engine(
    data_dir: &Path,
    stt_cfg: Option<&SttConfigRow>,
    runner_proxy: Option<RunnerProxy>,
    event_sender: &EventBusSender,
) -> (
    Option<SttEngineHandle>,
    Option<Arc<Mutex<apollia_stt::SttRepository>>>,
) {
    let Some(cfg) = stt_cfg.filter(|c| c.enabled) else {
        info!("STT disabled in config - engine not started");
        return (None, None);
    };
    info!("starting SttEngine");

    // The transcription repository (history) is independent of the whisper
    // model: past transcriptions stay viewable even when the model file is
    // absent and live dictation cannot start. Open it before the model check so
    // the history read path works in that degraded state.
    let repo_path = data_dir.join("stt_transcriptions.db");
    let api_repo = apollia_stt::SttRepository::open(&repo_path)
        .map(|r| Arc::new(Mutex::new(r)))
        .ok();

    let model_path = crate::supervisor::resolve_home(Path::new(&cfg.model_path));
    if !model_path.exists() {
        error!(
            path = %model_path.display(),
            "STT model file not found - dictation disabled, transcription history still available"
        );
        return (None, api_repo);
    }

    let repository = match apollia_stt::SttRepository::open(&repo_path) {
        Ok(repository) => repository,
        Err(e) => {
            error!(error = %e, "SttRepository failed to open - SttEngine disabled");
            return (None, api_repo);
        }
    };

    let model_id = model_path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "whisper".to_string());

    let Some(proxy) = runner_proxy else {
        warn!("STT engine disabled (runner sidecar unavailable)");
        return (None, api_repo);
    };
    let backend: Box<dyn apollia_stt::SttBackend> =
        Box::new(RunnerSttBackend::new(proxy, model_id));

    let handle = SttEngineHandle::start(backend, repository, cfg.clone(), event_sender.clone());
    info!("SttEngine ready");
    (Some(handle), api_repo)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eventbus::EventBus;
    use apollia_core::SttConfigRow;

    // GIVEN STT enabled but the whisper model file is absent
    // WHEN the engine is built
    // THEN dictation stays off (engine None) but the transcription repository is
    //      still opened so history remains viewable.
    #[tokio::test]
    async fn build_without_model_keeps_repository_for_history() {
        // GIVEN
        let dir = tempfile::tempdir().expect("tempdir");
        let cfg = SttConfigRow {
            enabled: true,
            model_path: dir.path().join("does-not-exist.bin").display().to_string(),
            ..Default::default()
        };
        let (event_tx, _rx) = EventBus::new();

        // WHEN
        let (engine, repository) = build_stt_engine(dir.path(), Some(&cfg), None, &event_tx).await;

        // THEN
        assert!(engine.is_none(), "engine must be disabled without a model");
        assert!(
            repository.is_some(),
            "history repository must stay available without a model"
        );
    }

    // GIVEN STT disabled in config
    // WHEN the engine is built
    // THEN neither engine nor repository is created.
    #[tokio::test]
    async fn build_disabled_returns_nothing() {
        // GIVEN
        let dir = tempfile::tempdir().expect("tempdir");
        let cfg = SttConfigRow {
            enabled: false,
            ..Default::default()
        };
        let (event_tx, _rx) = EventBus::new();

        // WHEN
        let (engine, repository) = build_stt_engine(dir.path(), Some(&cfg), None, &event_tx).await;

        // THEN
        assert!(engine.is_none());
        assert!(repository.is_none());
    }
}
