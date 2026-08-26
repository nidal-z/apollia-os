//! Tauri IPC commands for the Model Hub.
//!
//! - [`get_hardware_profile`]: detects the machine's RAM, CPU and GPU
//! - [`search_hf_models`]: searches HuggingFace GGUF models
//! - [`get_hf_model`]: full metadata for a HuggingFace model
//! - [`start_model_download`]: downloads a GGUF file
//! - [`cancel_model_download`]: cancels an in-progress download

use std::path::{Path, PathBuf};
use std::sync::Arc;

use apollia_core::LlmBackendRepository;
use apollia_llm::hardware::detect as detect_hardware;
use apollia_llm::{AcceleratorProfile, HardwareProfile};
use serde::{Deserialize, Serialize};
use tauri::{Emitter, State};
use tokio::sync::Mutex;

// ─────────────────────────────────────────────
// Shared download manager state
// ─────────────────────────────────────────────

/// Shared download manager, managed as Tauri state.
pub type SharedDownloadManager = Arc<Mutex<apollia_llm::DownloadManager>>;

// ─────────────────────────────────────────────
// Response types
// ─────────────────────────────────────────────

/// Serializable view of the hardware profile.
#[derive(Debug, Clone, Serialize)]
pub struct HardwareProfileView {
    pub total_ram_gb: f64,
    pub available_ram_gb: f64,
    pub cpu_model: String,
    pub cpu_cores: u32,
    pub accelerator: AcceleratorProfile,
    pub memory_budget_gb: f64,
}

impl From<HardwareProfile> for HardwareProfileView {
    fn from(p: HardwareProfile) -> Self {
        Self {
            total_ram_gb: p.total_ram_gb,
            available_ram_gb: p.available_ram_gb,
            cpu_model: p.cpu_model,
            cpu_cores: p.cpu_cores,
            accelerator: p.accelerator,
            memory_budget_gb: p.memory_budget_gb,
        }
    }
}

/// HuggingFace search parameters.
#[derive(Debug, Deserialize)]
pub struct HfSearchParams {
    pub query: String,
    pub limit: Option<u32>,
    pub sort: Option<String>,
    /// Filter by HF task (defaults to `"text-generation"`).
    pub pipeline_tag: Option<String>,
    /// Filter by ISO 639-1 language (e.g. `"fr"`, `"en"`).
    pub language: Option<String>,
    /// Pagination cursor: full `rel="next"` URL returned by the previous page.
    pub next_cursor: Option<String>,
    /// Optional HF token (for gated models).
    pub hf_token: Option<String>,
}

/// Model download request.
#[derive(Debug, Deserialize)]
pub struct DownloadModelRequest {
    /// Direct URL of the GGUF file.
    pub url: String,
    /// Destination filename (optional; derived from the URL when absent).
    pub filename: Option<String>,
    /// HF token for gated models.
    pub hf_token: Option<String>,
    /// Destination directory (defaults to `~/.apollia/models/`).
    pub dest_dir: Option<String>,
    /// Source HF repo (`org/name`). Lets the downloader fetch
    /// `generation_config.json` after the download and persist the
    /// official sampling defaults in `~/.apollia/models/sampling-defaults.json`.
    pub repo_id: Option<String>,
}

// ─────────────────────────────────────────────
// Commands
// ─────────────────────────────────────────────

/// Detects the machine's hardware profile (RAM, CPU, GPU/Metal/CUDA).
#[tauri::command]
pub async fn get_hardware_profile() -> Result<HardwareProfileView, String> {
    let profile = tokio::task::spawn_blocking(detect_hardware)
        .await
        .map_err(|e| format!("hardware detection failed: {e}"))?;
    Ok(HardwareProfileView::from(profile))
}

/// Searches GGUF models on HuggingFace.
///
/// Returns a list of [`HfModelCard`] with hardware-compatibility badges.
#[tauri::command]
pub async fn search_hf_models(params: HfSearchParams) -> Result<serde_json::Value, String> {
    use apollia_llm::{HfRegistryClient, HfSearchFilter};

    let hardware = tokio::task::spawn_blocking(detect_hardware).await.ok();

    let client = HfRegistryClient::new(params.hf_token);
    let filter = HfSearchFilter {
        filter: Some("gguf".to_string()),
        sort: params.sort,
        limit: params.limit,
        pipeline_tag: Some(
            params
                .pipeline_tag
                .unwrap_or_else(|| "text-generation".to_string()),
        ),
        language: params.language,
        next_cursor: params.next_cursor,
    };

    let page = client
        .search(&params.query, filter, hardware.as_ref())
        .await
        .map_err(|e| format!("HuggingFace search failed: {e}"))?;

    serde_json::to_value(page).map_err(|e| format!("serialization error: {e}"))
}

/// Fetches the full metadata for a HuggingFace model.
///
/// Includes the list of GGUF files with size and compatibility badge, the
/// recommended generation parameters from `generation_config.json`, and the
/// architecture type from `config.json` (with a 24h session cache).
#[tauri::command]
pub async fn get_hf_model(
    cache: State<'_, Arc<apollia_llm::HfModelTypeCache>>,
    repo_id: String,
    hf_token: Option<String>,
) -> Result<serde_json::Value, String> {
    use apollia_llm::HfRegistryClient;

    let hardware = tokio::task::spawn_blocking(detect_hardware).await.ok();

    let client = HfRegistryClient::new(hf_token);
    let mut card = client
        .get_model(&repo_id, hardware.as_ref(), Some(&cache))
        .await
        .map_err(|e| format!("failed to fetch model {repo_id}: {e}"))?;

    // Best-effort fetch of generation_config.json (separate from config.json)
    if let Some(gen_config) = client.get_generation_config(&repo_id).await {
        card.generation_config = Some(gen_config);
    }

    serde_json::to_value(card).map_err(|e| format!("serialization error: {e}"))
}

/// Starts downloading a GGUF file from HuggingFace.
///
/// Returns a unique `download_id`. Progress is emitted via the Tauri
/// `model-download-progress` event with an [`apollia_llm::DownloadProgress`] payload.
#[tauri::command]
pub async fn start_model_download(
    app: tauri::AppHandle,
    manager: State<'_, SharedDownloadManager>,
    request: DownloadModelRequest,
) -> Result<String, String> {
    use apollia_llm::DownloadRequest;

    let dest_dir = request
        .dest_dir
        .as_deref()
        .map(|d| {
            if d.starts_with("~/") {
                let home = apollia_core::paths::home_string().unwrap_or_default();
                PathBuf::from(format!("{}{}", home, &d[1..]))
            } else {
                PathBuf::from(d)
            }
        })
        .unwrap_or_else(|| {
            let home = apollia_core::paths::home_dir_or_temp();
            apollia_core::paths::data_dir_under(home).join("models")
        });

    // Ensure destination directory exists.
    tokio::fs::create_dir_all(&dest_dir)
        .await
        .map_err(|e| format!("failed to create model directory: {e}"))?;

    let dl_request = DownloadRequest {
        url: request.url,
        dest_dir,
        filename: request.filename,
        hf_token: request.hf_token,
        repo_id: request.repo_id,
    };

    let on_progress = Arc::new(move |progress: apollia_llm::DownloadProgress| {
        let _ = app.emit("model-download-progress", &progress);
    });

    let id = manager.lock().await.start(dl_request, on_progress);
    Ok(id)
}

/// Cancels an in-progress download.
#[tauri::command]
pub async fn cancel_model_download(
    manager: State<'_, SharedDownloadManager>,
    download_id: String,
) -> Result<(), String> {
    manager
        .lock()
        .await
        .cancel(&download_id)
        .map_err(|e| format!("cancel failed: {e}"))
}

// ─────────────────────────────────────────────
// Installed models (list + guarded delete)
// ─────────────────────────────────────────────

/// Classification of an installed model file by extension.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum InstalledModelKind {
    /// GGUF weights for local LLM inference.
    Llm,
    /// GGML weights for local speech-to-text.
    Stt,
}

/// A model file installed under `~/.apollia/models/`.
#[derive(Debug, Clone, Serialize)]
pub struct InstalledModelView {
    /// Absolute path of the file on disk.
    pub path: String,
    /// File name (basename).
    pub name: String,
    /// Size in bytes.
    pub size_bytes: u64,
    /// LLM or STT, inferred from the extension.
    pub kind: InstalledModelKind,
    /// True when an LLM backend or the STT config references this file.
    pub in_use: bool,
    /// Human-readable owner when `in_use` (e.g. `LLM backend 'local-code'`).
    pub used_by: Option<String>,
}

/// A model file referenced by the active configuration.
struct ModelRef {
    /// Basename of the referenced file, used for a conservative match.
    name: String,
    /// Owner label surfaced to the operator.
    label: String,
}

fn home_dir() -> PathBuf {
    apollia_core::paths::home_dir_or_temp()
}

fn models_dir() -> PathBuf {
    apollia_core::paths::data_dir_under(home_dir()).join("models")
}

fn expand_tilde(raw: &str) -> PathBuf {
    if let Some(rest) = raw.strip_prefix("~/") {
        home_dir().join(rest)
    } else {
        PathBuf::from(raw)
    }
}

/// Collect every model file referenced by an LLM backend or the STT config.
///
/// Read-only over `~/.apollia/system.db`. A backend whose `model` is a cloud
/// model name (not a path) simply never matches a local `.gguf`, so it is
/// harmless here.
fn collect_in_use_refs() -> Vec<ModelRef> {
    let mut refs = Vec::new();
    let db_path = apollia_core::paths::DataFile::System
        .path(&apollia_core::paths::data_dir_under(home_dir()));

    let mut push = |raw: &str, label: String| {
        if let Some(name) = expand_tilde(raw).file_name().map(|n| n.to_owned()) {
            refs.push(ModelRef {
                name: name.to_string_lossy().to_string(),
                label,
            });
        }
    };

    if let Ok(repo) = LlmBackendRepository::open(&db_path) {
        if let Ok(list) = repo.list() {
            for cfg in list {
                let label = format!("LLM backend '{}'", cfg.name);
                push(&cfg.model, label.clone());
                if let Some(mp) = cfg.config_json.get("model_path").and_then(|v| v.as_str()) {
                    push(mp, label);
                }
            }
        }
    }

    if let Ok(repo) = apollia_core::SttConfigRepository::open(&db_path) {
        if let Ok(row) = repo.get_or_default() {
            push(&row.model_path, "STT engine".to_string());
        }
    }

    refs
}

/// Conservative match: same file name as a referenced model. Prefers a false
/// "in use" (blocks a delete) over an accidental removal of a live model.
fn used_by(file: &Path, refs: &[ModelRef]) -> Option<String> {
    let name = file.file_name()?.to_string_lossy().to_string();
    refs.iter()
        .find(|r| r.name == name)
        .map(|r| r.label.clone())
}

fn collect_installed_models() -> Result<Vec<InstalledModelView>, String> {
    let dir = models_dir();
    let refs = collect_in_use_refs();
    let mut out = Vec::new();

    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        // No models directory yet is not an error: nothing installed.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
        Err(e) => return Err(format!("failed to read models directory: {e}")),
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let kind = match path.extension().and_then(|e| e.to_str()) {
            Some("gguf") => InstalledModelKind::Llm,
            Some("bin") => InstalledModelKind::Stt,
            _ => continue,
        };
        let size_bytes = entry.metadata().map(|m| m.len()).unwrap_or(0);
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let owner = used_by(&path, &refs);
        out.push(InstalledModelView {
            path: path.to_string_lossy().to_string(),
            name,
            size_bytes,
            kind,
            in_use: owner.is_some(),
            used_by: owner,
        });
    }

    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

fn delete_installed_model_inner(path: &str) -> Result<(), String> {
    let target = PathBuf::from(path);

    // The file must exist and resolve to a real path inside the managed models
    // directory: reject traversal, symlinks pointing outside, and stray paths.
    let canonical_target = target
        .canonicalize()
        .map_err(|e| format!("model file not found: {e}"))?;
    let canonical_dir = models_dir()
        .canonicalize()
        .map_err(|e| format!("models directory not found: {e}"))?;
    if !canonical_target.starts_with(&canonical_dir) {
        return Err("refusing to delete a file outside the models directory".to_string());
    }

    // In-use guard: never delete a model referenced by a live backend or STT.
    if let Some(owner) = used_by(&canonical_target, &collect_in_use_refs()) {
        return Err(format!(
            "model is in use by {owner}; change or remove that configuration first"
        ));
    }

    std::fs::remove_file(&canonical_target).map_err(|e| format!("failed to delete model file: {e}"))
}

/// Lists model files installed under `~/.apollia/models/` with size and usage.
#[tauri::command]
pub async fn list_installed_models() -> Result<Vec<InstalledModelView>, String> {
    tokio::task::spawn_blocking(collect_installed_models)
        .await
        .map_err(|e| format!("spawn_blocking failed: {e}"))?
}

/// Deletes an installed model file after a path and in-use safety check.
///
/// Refuses any path outside `~/.apollia/models/` and any file still referenced
/// by an LLM backend or the STT configuration.
#[tauri::command]
pub async fn delete_installed_model(path: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || delete_installed_model_inner(&path))
        .await
        .map_err(|e| format!("spawn_blocking failed: {e}"))?
}

/// Error raised while importing a user-selected model file into the managed
/// models directory.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ImportModelError {
    /// The chosen source path does not point to a regular file.
    #[error("source file not found: {0}")]
    NotFound(String),
    /// The file extension is not among the accepted ones.
    #[error("unsupported file type '{name}' (expected: {expected})")]
    BadExtension {
        /// Basename of the rejected file.
        name: String,
        /// Comma-separated list of accepted extensions.
        expected: String,
    },
    /// The source path has no usable file name component.
    #[error("invalid source file name")]
    BadFileName,
    /// The models directory could not be created.
    #[error("failed to create models directory: {0}")]
    CreateDir(#[source] std::io::Error),
    /// The file copy into the models directory failed.
    #[error("failed to copy model file: {0}")]
    Copy(#[source] std::io::Error),
}

/// Copies a user-selected model file into `~/.apollia/models/`.
///
/// The native file picker lives on the frontend (Tauri dialog plugin); this
/// command receives the chosen `source_path` and the `allowed_extensions` the
/// caller expects (`gguf` for an LLM, `bin`/`gguf` for a Whisper model),
/// validates the file, then copies it into the managed models directory
/// (created if missing). The copy is deliberately non-destructive: the source
/// is left in place. A source that already resides in the models directory is a
/// no-op. Returns the destination path so the caller can select it after a
/// re-scan.
#[tauri::command]
pub async fn import_model_file(
    source_path: String,
    allowed_extensions: Vec<String>,
) -> Result<String, String> {
    tokio::task::spawn_blocking(move || import_model_file_inner(&source_path, &allowed_extensions))
        .await
        .map_err(|e| format!("spawn_blocking failed: {e}"))?
        .map_err(|e| e.to_string())
}

fn import_model_file_inner(
    source_path: &str,
    allowed_extensions: &[String],
) -> Result<String, ImportModelError> {
    let source = PathBuf::from(source_path);
    if !source.is_file() {
        return Err(ImportModelError::NotFound(source_path.to_string()));
    }

    let ext = source
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();
    if !allowed_extensions
        .iter()
        .any(|a| a.eq_ignore_ascii_case(&ext))
    {
        let name = source
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        return Err(ImportModelError::BadExtension {
            name,
            expected: allowed_extensions.join(", "),
        });
    }

    let file_name = source.file_name().ok_or(ImportModelError::BadFileName)?;
    let dir = models_dir();
    std::fs::create_dir_all(&dir).map_err(ImportModelError::CreateDir)?;
    let dest = dir.join(file_name);

    // `std::fs::copy` streams the file (it does not buffer the whole payload in
    // memory), which matters for multi-gigabyte GGUF weights.
    if dest != source {
        std::fs::copy(&source, &dest).map_err(ImportModelError::Copy)?;
    }

    tracing::info!(dest = %dest.display(), "model.file.imported");
    Ok(dest.display().to_string())
}
