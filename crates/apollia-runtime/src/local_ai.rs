//! The installation of the local AI at onboarding: what the machine can run,
//! the models already on its disk, and the two choices that make them the
//! engine's, a GGUF file for the language model and a Whisper file for
//! dictation.
//!
//! The desktop running the runtime in its own process calls these functions
//! directly; a client of the daemon reaches the same functions through
//! `/api/v1/local-ai/...`. Both therefore read the same folders, write the same
//! rows of `system.db` and refuse with the same words.

pub mod scan;

use std::path::{Path, PathBuf};

use apollia_core::{LlmBackendConfig, LlmBackendRepository, LlmProvider, SttConfigRepository};
use apollia_memory::user_memory::UserMemoryRepository;
use serde::{Deserialize, Serialize};

use crate::profile::ProfileStore;

pub use scan::{GgufModelInfo, SystemInfo, WhisperModelInfo};

/// Why an installation step did not complete. The display text is what the
/// interface shows, the same whichever process ran the step.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum LocalAiError {
    /// The chosen model file does not exist.
    #[error("file not found: {0}")]
    NotFound(String),
    /// The chosen file is not a GGUF model.
    #[error("expected a .gguf file")]
    NotGguf,
    /// The chosen path names no file.
    #[error("invalid file name")]
    InvalidFileName,
    /// The models folder could not be created.
    #[error("failed to create models directory: {0}")]
    ModelsDir(#[source] std::io::Error),
    /// The model could not be copied into the models folder.
    #[error("failed to copy model: {0}")]
    Copy(#[source] std::io::Error),
    /// `system.db` refused a read or a write; the text names which.
    #[error("{0}")]
    Store(String),
    /// `user_memory.db`, which holds the onboarding progress, did not open.
    #[error("UserMemory database not initialized")]
    OnboardingUnavailable,
    /// The onboarding progress could not be written.
    #[error("persistence error: {0}")]
    Persistence(String),
    /// The blocking task running the step did not complete.
    #[error("{what} failed: {source}")]
    Task {
        /// The step, as the interface names it.
        what: &'static str,
        /// The join failure.
        #[source]
        source: tokio::task::JoinError,
    },
}

impl LocalAiError {
    /// Whether the request itself is at fault rather than this machine.
    pub fn is_invalid_request(&self) -> bool {
        matches!(
            self,
            Self::NotFound(_) | Self::NotGguf | Self::InvalidFileName
        )
    }
}

/// What the local language model setup did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SetupLlmResult {
    /// Absolute path where the model is stored.
    pub model_path: String,
    /// Inferred quantization from the filename (e.g. `"q8_0"`, `"q4_k_m"`).
    pub quantization: String,
}

/// What the dictation setup did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DictationConfigured {
    /// The Whisper model file dictation now reads.
    pub model_path: String,
    /// The transcription language stored, `None` when none is set.
    pub language: Option<String>,
}

/// Returns system information for the model recommendations.
///
/// # Errors
///
/// [`LocalAiError::Task`] when the blocking read does not complete.
pub async fn system_info() -> Result<SystemInfo, LocalAiError> {
    tokio::task::spawn_blocking(scan::system_info)
        .await
        .map_err(|source| LocalAiError::Task {
            what: "system info query",
            source,
        })
}

/// The GGUF models under `home` and in the models folder in force, largest first.
///
/// # Errors
///
/// [`LocalAiError::Task`] when the blocking scan does not complete.
pub async fn gguf_models(
    home: PathBuf,
    data_dir: PathBuf,
) -> Result<Vec<GgufModelInfo>, LocalAiError> {
    tokio::task::spawn_blocking(move || scan::scan_gguf_models(&home, &data_dir))
        .await
        .map_err(|source| LocalAiError::Task {
            what: "GGUF scan",
            source,
        })
}

/// The Whisper models under `home` and in `<data_dir>/models`.
///
/// # Errors
///
/// [`LocalAiError::Task`] when the blocking scan does not complete.
pub async fn whisper_models(
    home: PathBuf,
    data_dir: PathBuf,
) -> Result<Vec<WhisperModelInfo>, LocalAiError> {
    tokio::task::spawn_blocking(move || scan::scan_whisper_models(&home, &data_dir))
        .await
        .map_err(|source| LocalAiError::Task {
            what: "Whisper scan",
            source,
        })
}

/// Sets up a local embedded LLM from a user-selected GGUF file.
///
/// Copies the model into `<data_dir>/models/`, registers it as a backend in
/// `<data_dir>/system.db`, and returns the path for confirmation.
///
/// The backend is saved as `"local"`: inserted as the default when no backend
/// has that name, otherwise pointed at the new file with its other settings and
/// default flag kept, so choosing another model during onboarding really
/// switches engines.
///
/// `context_window` is the window chosen at onboarding, stored as
/// `config_json.context_window`, the key the engine is launched with and the
/// router sizes compaction against. `None` leaves the stored value, or the
/// runtime default, in place. Reloading the router is the caller's next step.
///
/// # Errors
///
/// [`LocalAiError::NotFound`] or [`LocalAiError::NotGguf`] for a path that is
/// not a GGUF file, [`LocalAiError::Copy`] when the copy fails,
/// [`LocalAiError::Store`] when `system.db` refuses the backend.
pub async fn setup_local_llm(
    data_dir: &Path,
    gguf_path: &str,
    context_window: Option<u32>,
) -> Result<SetupLlmResult, LocalAiError> {
    let source = PathBuf::from(gguf_path);
    if !source.exists() {
        return Err(LocalAiError::NotFound(gguf_path.to_owned()));
    }
    if source.extension().and_then(|e| e.to_str()) != Some("gguf") {
        return Err(LocalAiError::NotGguf);
    }

    let file_stem = source
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("model");
    let quantization = infer_quantization(file_stem);

    let models_dir = apollia_core::config::models_dir(data_dir);
    tokio::fs::create_dir_all(&models_dir)
        .await
        .map_err(LocalAiError::ModelsDir)?;

    let file_name = source
        .file_name()
        .ok_or(LocalAiError::InvalidFileName)?
        .to_string_lossy()
        .to_string();
    // A file already under the models folder stays where it is: a download
    // puts a model that comes with a projector, or in several parts, in a
    // folder of its own, and copying its weights out of it would double them
    // on disk and leave the projector behind.
    let (dest, within_models) = match source.strip_prefix(&models_dir) {
        Ok(relative) => (
            source.clone(),
            relative.to_string_lossy().replace('\\', "/"),
        ),
        Err(_) => {
            let dest = models_dir.join(&file_name);
            tokio::fs::copy(&source, &dest)
                .await
                .map_err(LocalAiError::Copy)?;
            (dest, file_name.clone())
        }
    };

    let model_path_str = format!("~/.apollia/models/{within_models}");
    let db_path = data_dir.join(apollia_core::paths::DataFile::System.file_name());
    let device = if cfg!(target_os = "macos") {
        "metal"
    } else {
        "cpu"
    };
    let model_for_db = model_path_str.clone();
    let quant_for_db = quantization.clone();
    tokio::task::spawn_blocking(move || {
        let repo = LlmBackendRepository::open(&db_path)
            .map_err(|e| LocalAiError::Store(format!("failed to open system.db: {e}")))?;
        let existing = repo
            .find_by_name("local")
            .map_err(|e| LocalAiError::Store(format!("failed to query system.db: {e}")))?;
        let config = local_backend(
            existing,
            &model_for_db,
            device,
            &quant_for_db,
            context_window,
        );
        repo.save(&config).map_err(|e| {
            LocalAiError::Store(format!("failed to save LLM backend to system.db: {e}"))
        })
    })
    .await
    .map_err(|source| LocalAiError::Task {
        what: "spawn_blocking",
        source,
    })??;

    tracing::event!(
        tracing::Level::INFO,
        model = %model_path_str,
        quantization = %quantization,
        "llm.local.configured"
    );
    Ok(SetupLlmResult {
        model_path: dest.display().to_string(),
        quantization,
    })
}

/// Configures dictation with the chosen Whisper model and marks the onboarding
/// progress as dictation-ready.
///
/// Persists `enabled = true` and `model_path` in `<data_dir>/system.db`, and
/// the transcription language when none is set yet, so dictation transcribes
/// in the user's language without overriding an explicit choice. Then sets the
/// `stt_configured` and `voice_enabled` marks of the onboarding progress.
/// Loading the model into the dictation engine is the caller's next step.
///
/// # Errors
///
/// [`LocalAiError::Store`] when `system.db` refuses the row,
/// [`LocalAiError::OnboardingUnavailable`] when `user_memory.db` did not open,
/// [`LocalAiError::Persistence`] when the marks cannot be written.
pub async fn setup_dictation(
    data_dir: &Path,
    onboarding: Option<ProfileStore<'_>>,
    model_path: String,
    language: Option<String>,
) -> Result<DictationConfigured, LocalAiError> {
    let repo = onboarding
        .map(|store| std::sync::Arc::clone(store.0))
        .ok_or(LocalAiError::OnboardingUnavailable)?;
    let db_path = data_dir.join(apollia_core::paths::DataFile::System.file_name());
    let row_path = model_path.clone();
    let stored = tokio::task::spawn_blocking(move || {
        let stt = SttConfigRepository::open(&db_path)
            .map_err(|e| LocalAiError::Store(format!("open system.db: {e}")))?;
        let mut row = stt
            .get_or_default()
            .map_err(|e| LocalAiError::Store(format!("read STT config: {e}")))?;
        row.model_path = row_path;
        row.enabled = true;
        if row.language.is_none() {
            if let Some(lang) = language.filter(|l| !l.trim().is_empty()) {
                row.language = Some(lang);
            }
        }
        stt.upsert(&row)
            .map_err(|e| LocalAiError::Store(format!("persist STT config: {e}")))?;

        let guard = repo
            .lock()
            .map_err(|e| LocalAiError::Persistence(format!("mutex poisoned: {e}")))?;
        mark_dictation_ready(&guard)?;
        Ok::<_, LocalAiError>(row.language)
    })
    .await
    .map_err(|source| LocalAiError::Task {
        what: "spawn_blocking",
        source,
    })??;

    tracing::event!(
        tracing::Level::INFO,
        model_path = %model_path,
        "stt.model.configured"
    );
    Ok(DictationConfigured {
        model_path,
        language: stored,
    })
}

/// The first phase of the onboarding, as the progress stores it.
const FIRST_ONBOARDING_PHASE: &str = "welcome";

/// Sets the dictation marks of the onboarding progress, the internal keys the
/// desktop's onboarding state reads. A progress never started is started at
/// its first phase, so the marks are read back rather than masked by the
/// default state.
fn mark_dictation_ready(repo: &UserMemoryRepository) -> Result<(), LocalAiError> {
    let persistence =
        |e: apollia_memory::user_memory::UserMemoryError| LocalAiError::Persistence(e.to_string());
    if repo
        .get_internal("onboarding_phase")
        .map_err(persistence)?
        .is_none()
    {
        repo.set_internal("onboarding_phase", FIRST_ONBOARDING_PHASE)
            .map_err(persistence)?;
    }
    repo.set_internal("onboarding_stt_configured", "true")
        .map_err(persistence)?;
    repo.set_internal("onboarding_voice_enabled", "true")
        .map_err(persistence)
}

/// The `"local"` backend row for a model chosen at onboarding.
///
/// A fresh row is the default. An existing one keeps its flags and every
/// setting the operator added (sampling, device), and takes the new model, its
/// quantisation and, when one was chosen, the new window. A row stored with the
/// legacy `context_size` key converges on `context_window`.
fn local_backend(
    existing: Option<LlmBackendConfig>,
    model_path: &str,
    device: &str,
    quantization: &str,
    context_window: Option<u32>,
) -> LlmBackendConfig {
    let mut config = existing.unwrap_or_else(|| LlmBackendConfig {
        name: "local".to_string(),
        provider: LlmProvider::LlamaCpp,
        model: String::new(),
        config_json: serde_json::json!({ "device": device }),
        enabled: true,
        is_default: true,
    });
    config.provider = LlmProvider::LlamaCpp;
    config.model = model_path.to_owned();
    if !config.config_json.is_object() {
        config.config_json = serde_json::json!({ "device": device });
    }
    if let Some(obj) = config.config_json.as_object_mut() {
        obj.insert("model_path".into(), model_path.into());
        obj.insert("quantization".into(), quantization.into());
        obj.remove("model_paths");
        if let Some(n) = context_window.filter(|n| *n > 0) {
            obj.insert("context_window".into(), n.into());
            obj.remove("context_size");
        }
    }
    config
}

/// Infers the quantization type from a GGUF filename.
///
/// Looks for common patterns like `Q8_0`, `Q4_K_M`, `Q5_K_S`, etc.
/// Returns `"q4_k_m"` as a safe default if nothing is detected.
fn infer_quantization(stem: &str) -> String {
    let upper = stem.to_uppercase();
    // Ordered by specificity.
    let patterns = [
        "Q8_0", "Q6_K", "Q5_K_M", "Q5_K_S", "Q5_0", "Q4_K_M", "Q4_K_S", "Q4_0", "Q3_K_M", "Q3_K_S",
        "Q2_K", "IQ4_XS", "IQ3_M", "IQ2_S", "F16", "F32",
    ];
    for p in &patterns {
        if upper.contains(p) {
            return p.to_lowercase();
        }
    }
    "q4_k_m".to_string()
}

#[cfg(test)]
mod tests;
