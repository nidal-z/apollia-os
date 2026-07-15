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
/// Returns `(None, None)` when STT is disabled, the model file is absent, the
/// runner sidecar is unavailable, or the transcription store cannot be opened.
/// `data_dir` locates `stt_transcriptions.db`; `runner_proxy` is the sidecar
/// handle STT inference routes through. The second tuple element is a separate
/// API-side repository handle over the same database.
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

    let model_path = crate::supervisor::resolve_home(Path::new(&cfg.model_path));
    if !model_path.exists() {
        error!(
            path = %model_path.display(),
            "STT model file not found - SttEngine disabled"
        );
        return (None, None);
    }

    let repo_path = data_dir.join("stt_transcriptions.db");
    let repository = match apollia_stt::SttRepository::open(&repo_path) {
        Ok(repository) => repository,
        Err(e) => {
            error!(error = %e, "SttRepository failed to open - SttEngine disabled");
            return (None, None);
        }
    };

    let model_id = model_path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "whisper".to_string());

    let Some(proxy) = runner_proxy else {
        warn!("STT engine disabled (runner sidecar unavailable)");
        return (None, None);
    };
    let backend: Box<dyn apollia_stt::SttBackend> =
        Box::new(RunnerSttBackend::new(proxy, model_id));

    let handle = SttEngineHandle::start(backend, repository, cfg.clone(), event_sender.clone());
    info!("SttEngine ready");
    let api_repo = apollia_stt::SttRepository::open(&repo_path)
        .map(|r| Arc::new(Mutex::new(r)))
        .ok();
    (Some(handle), api_repo)
}
