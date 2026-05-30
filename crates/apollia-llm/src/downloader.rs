//! GGUF model downloads from HuggingFace with progress tracking.
//!
//! - Streaming via `reqwest` with a `Range` header for resuming.
//! - A [`DownloadManager`] manages a pool of concurrent downloads.
//! - Progress is reported via [`DownloadProgress`], to be routed to the
//!   desktop-side Tauri events.

#![cfg(feature = "cloud")]

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use futures::StreamExt;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

/// Errors of the download module.
#[derive(Debug, Error)]
pub enum DownloadError {
    /// Network error.
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    /// I/O error (file write).
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    /// Download cancelled by the user.
    #[error("download cancelled")]
    Cancelled,
    /// Download not found.
    #[error("download '{0}' not found")]
    NotFound(String),
}

/// Unique identifier of a download.
pub type DownloadId = String;

/// Progress of an in-flight download.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadProgress {
    /// Download identifier.
    pub id: DownloadId,
    /// Bytes downloaded so far.
    pub downloaded_bytes: u64,
    /// Expected total size (if known via `Content-Length`).
    pub total_bytes: Option<u64>,
    /// Instantaneous speed in bytes/second.
    pub speed_bps: f64,
    /// Destination path.
    pub dest_path: PathBuf,
    /// Current status.
    pub status: DownloadStatus,
}

/// Status of a download.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DownloadStatus {
    /// Downloading.
    InProgress,
    /// Completed successfully.
    Completed,
    /// Cancelled by the user.
    Cancelled,
    /// Unrecoverable error.
    Failed,
}

/// Request to start a download.
#[derive(Debug, Clone)]
pub struct DownloadRequest {
    /// URL of the file to download.
    pub url: String,
    /// Destination directory (e.g. `~/.apollia/models/`).
    pub dest_dir: PathBuf,
    /// Destination filename (optional, derived from the URL if absent).
    pub filename: Option<String>,
    /// HF token for gated models (optional).
    pub hf_token: Option<String>,
    /// Source HF repo (`org/name`). When `Some`, the downloader fetches the
    /// repo's `generation_config.json` at the end and persists the recommended
    /// `temperature` / `top_p` / `top_k` / `repetition_penalty` into
    /// `~/.apollia/models/sampling-defaults.json` indexed by `filename`. This
    /// lets a local agent automatically benefit from the official
    /// hyperparameters published by the model author. No effect if `None`
    /// (non-HF downloads).
    pub repo_id: Option<String>,
}

type ProgressCallback = Arc<dyn Fn(DownloadProgress) + Send + Sync + 'static>;

/// Manager for concurrent downloads.
///
/// Each download gets a UUID and can be cancelled individually. Progress is
/// reported via an injectable callback (desktop-side Tauri events).
pub struct DownloadManager {
    client: reqwest::Client,
    /// Map of download_id to CancellationToken for cancellations.
    active: Arc<Mutex<HashMap<DownloadId, CancellationToken>>>,
}

impl DownloadManager {
    /// Create a new manager.
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .user_agent("Apollia-OS/1.0")
            // connect_timeout only, no total-request timeout so large model downloads
            // (multi-GB) are not killed mid-stream after 60 s.
            .connect_timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("reqwest client build never fails");
        Self {
            client,
            active: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Start a download in the background.
    ///
    /// Returns the `DownloadId` immediately. Progress is reported via
    /// `on_progress` on each received chunk.
    pub fn start(&self, request: DownloadRequest, on_progress: ProgressCallback) -> DownloadId {
        let id = Uuid::new_v4().to_string();
        let cancel = CancellationToken::new();

        {
            let mut active = self.active.lock().expect("active lock not poisoned");
            active.insert(id.clone(), cancel.clone());
        }

        let client = self.client.clone();
        let active = self.active.clone();
        let task_id = id.clone();

        tokio::spawn(async move {
            let result = run_download(
                client,
                request,
                task_id.clone(),
                cancel,
                on_progress.clone(),
            )
            .await;

            // Clean up active map
            {
                let mut map = active.lock().expect("active lock");
                map.remove(&task_id);
            }

            if let Err(e) = result {
                tracing::error!(id = %task_id, error = %e, "download failed");
            }
        });

        id
    }

    /// Cancel an in-flight download.
    ///
    /// # Errors
    /// [`DownloadError::NotFound`] if the ID is unknown or already finished.
    pub fn cancel(&self, id: &str) -> Result<(), DownloadError> {
        let active = self.active.lock().expect("active lock");
        match active.get(id) {
            Some(token) => {
                token.cancel();
                Ok(())
            }
            None => Err(DownloadError::NotFound(id.to_string())),
        }
    }

    /// Return the list of active download IDs.
    pub fn active_ids(&self) -> Vec<DownloadId> {
        self.active
            .lock()
            .expect("active lock")
            .keys()
            .cloned()
            .collect()
    }
}

impl Default for DownloadManager {
    fn default() -> Self {
        Self::new()
    }
}

// ── Core download logic ──────────────────────────────────────────────────

async fn run_download(
    client: reqwest::Client,
    request: DownloadRequest,
    id: DownloadId,
    cancel: CancellationToken,
    on_progress: ProgressCallback,
) -> Result<(), DownloadError> {
    let repo_id = request.repo_id.clone();
    let hf_token = request.hf_token.clone();
    let filename = request.filename.clone().unwrap_or_else(|| {
        request
            .url
            .rsplit('/')
            .next()
            .unwrap_or("model.gguf")
            .to_string()
    });

    let dest_path = request.dest_dir.join(&filename);

    // Check for partial download to resume.
    let already_downloaded = if dest_path.exists() {
        tokio::fs::metadata(&dest_path).await?.len()
    } else {
        0
    };

    let mut req = client.get(&request.url);
    if let Some(token) = &request.hf_token {
        req = req.header(reqwest::header::AUTHORIZATION, format!("Bearer {token}"));
    }
    if already_downloaded > 0 {
        req = req.header(
            reqwest::header::RANGE,
            format!("bytes={already_downloaded}-"),
        );
    }

    let resp = req.send().await?;
    let total_bytes = resp.content_length().map(|l| l + already_downloaded);

    let mut file = OpenOptions::new()
        .create(true)
        .append(already_downloaded > 0)
        .write(true)
        .open(&dest_path)
        .await?;

    let mut stream = resp.bytes_stream();
    let mut downloaded = already_downloaded;
    let start = std::time::Instant::now();

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                drop(file);
                if let Err(e) = tokio::fs::remove_file(&dest_path).await {
                    tracing::warn!(
                        id = %id,
                        path = %dest_path.display(),
                        error = %e,
                        "failed to remove partial file after cancel"
                    );
                }
                on_progress(DownloadProgress {
                    id: id.clone(),
                    downloaded_bytes: 0,
                    total_bytes,
                    speed_bps: 0.0,
                    dest_path: dest_path.clone(),
                    status: DownloadStatus::Cancelled,
                });
                return Err(DownloadError::Cancelled);
            }
            chunk = stream.next() => {
                match chunk {
                    None => break,
                    Some(Err(e)) => return Err(DownloadError::Http(e)),
                    Some(Ok(bytes)) => {
                        file.write_all(&bytes).await?;
                        downloaded += bytes.len() as u64;
                        let elapsed = start.elapsed().as_secs_f64().max(0.001);
                        let speed_bps = downloaded as f64 / elapsed;
                        on_progress(DownloadProgress {
                            id: id.clone(),
                            downloaded_bytes: downloaded,
                            total_bytes,
                            speed_bps,
                            dest_path: dest_path.clone(),
                            status: DownloadStatus::InProgress,
                        });
                    }
                }
            }
        }
    }

    file.flush().await?;

    on_progress(DownloadProgress {
        id: id.clone(),
        downloaded_bytes: downloaded,
        total_bytes: Some(downloaded),
        speed_bps: 0.0,
        dest_path: dest_path.clone(),
        status: DownloadStatus::Completed,
    });

    tracing::info!(
        id = %id,
        path = %dest_path.display(),
        bytes = downloaded,
        "model download completed"
    );

    // Opportunistically persist the official HF sampling defaults. Never fail
    // the download over this: it is a convenience, not a contract. If HF is
    // down or the `generation_config.json` file is absent, log at `info` and
    // continue.
    if let Some(ref repo) = repo_id {
        if let Err(e) = persist_sampling_defaults(repo, &filename, hf_token.as_deref()).await {
            tracing::info!(
                repo = %repo,
                file = %filename,
                error = %e,
                "sampling defaults non persistés (ignorable)"
            );
        }
    }

    Ok(())
}

/// Fetch the HF repo's `generation_config.json` and write the recommended
/// parameters into `~/.apollia/models/sampling-defaults.json`, indexed by
/// `model_filename` (the `.gguf` name without its path).
///
/// Indexing by filename rather than repo_id covers the multi-quant case (one
/// repo can ship Q4_K_M, Q5_K_M, Q8_0, etc.): each quantization can be named
/// and resolved independently, but all share the same official
/// hyperparameters, so the entry is duplicated per file.
///
/// Idempotent: a re-download overwrites the entry (useful if the author has
/// published new recommendations).
async fn persist_sampling_defaults(
    repo_id: &str,
    model_filename: &str,
    hf_token: Option<&str>,
) -> Result<(), String> {
    let client = crate::hf_registry::HfRegistryClient::new(hf_token.map(String::from));

    // Direct attempt: works for official repos (Qwen, Meta, etc.).
    let gen_config = match client.get_generation_config(repo_id).await {
        Some(c) => c,
        None => {
            // Quantizers (Bartowski, Unsloth, mradermacher) republish the GGUF
            // WITHOUT `generation_config.json`. The upstream repo, declared via
            // `cardData.base_model` or a `base_model:...` tag, does contain it.
            // Retry once against that.
            let Some(base) = client.resolve_base_model(repo_id).await else {
                return Err("generation_config.json absent et aucun base_model déclaré".to_string());
            };
            tracing::info!(
                repo = %repo_id,
                base_model = %base,
                "generation_config.json absent du repo dérivé, retry sur base_model"
            );
            client.get_generation_config(&base).await.ok_or_else(|| {
                format!("generation_config.json absent aussi sur base_model {base}")
            })?
        }
    };

    // f64 to f32 conversion; HF values fit comfortably in single precision
    // (never more than 4 significant digits).
    let defaults = crate::model_defaults::ModelDefaults {
        temperature: gen_config.temperature.map(|v| v as f32),
        top_p: gen_config.top_p.map(|v| v as f32),
        top_k: gen_config.top_k,
        repetition_penalty: gen_config.repetition_penalty.map(|v| v as f32),
    };

    if defaults.is_empty() {
        return Err(
            "generation_config.json présent mais sans hyperparamètre exploitable".to_string(),
        );
    }

    let path = crate::model_defaults::UserOverrides::default_path();
    crate::model_defaults::UserOverrides::upsert(&path, model_filename, defaults.clone())
        .map_err(|e| format!("écriture {}: {e}", path.display()))?;

    tracing::info!(
        repo = %repo_id,
        file = %model_filename,
        path = %path.display(),
        "sampling defaults HF persistés"
    );
    Ok(())
}
