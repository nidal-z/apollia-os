//! Tauri IPC commands for the Model Hub.
//!
//! - [`get_hardware_profile`]: detects the machine's RAM, CPU and GPU
//! - [`search_hf_models`]: searches HuggingFace GGUF models
//! - [`get_hf_model`]: full metadata for a HuggingFace model
//! - [`start_model_download`]: downloads a GGUF file
//! - [`cancel_model_download`]: cancels an in-progress download
//! - [`list_model_downloads`]: lists active downloads

use std::path::PathBuf;
use std::sync::Arc;

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

/// Status of an active download.
#[derive(Debug, Clone, Serialize)]
pub struct DownloadStatusView {
    pub id: String,
    pub active_ids: Vec<String>,
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
                let home = std::env::var("HOME").unwrap_or_default();
                PathBuf::from(format!("{}{}", home, &d[1..]))
            } else {
                PathBuf::from(d)
            }
        })
        .unwrap_or_else(|| {
            let home = std::env::var("HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|_| std::env::temp_dir());
            home.join(".apollia").join("models")
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

/// Returns the list of active download IDs.
#[tauri::command]
pub async fn list_model_downloads(
    manager: State<'_, SharedDownloadManager>,
) -> Result<Vec<String>, String> {
    Ok(manager.lock().await.active_ids())
}
