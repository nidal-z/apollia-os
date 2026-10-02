//! GGUF model downloads from HuggingFace with progress tracking.
//!
//! - Streaming via `reqwest` with a `Range` header for resuming.
//! - The HTTP status is checked before anything is written, and the SHA-256 of
//!   the file is computed as it arrives and compared at the end with the one
//!   its publisher declares; a file that does not match is deleted.
//! - A [`DownloadManager`] manages a pool of concurrent downloads.
//! - Progress is reported via [`DownloadProgress`], to be routed to the
//!   desktop-side Tauri events.

#![cfg(feature = "cloud")]

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use futures::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::fs::OpenOptions;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

/// Errors of the download module.
#[derive(Debug, Error)]
#[non_exhaustive]
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
    /// The server answered with a non-success status; nothing was written.
    #[error("the server answered HTTP {status} for {url}")]
    HttpStatus {
        /// HTTP status code.
        status: u16,
        /// Requested URL.
        url: String,
    },
    /// The downloaded file does not match the SHA-256 its publisher
    /// declares. The file has been deleted.
    #[error(
        "integrity check failed for '{file}': expected sha256 {expected}, got {actual}; \
         the file was deleted"
    )]
    IntegrityMismatch {
        /// Destination filename.
        file: String,
        /// Published SHA-256.
        expected: String,
        /// SHA-256 of the bytes received.
        actual: String,
    },
    /// The destination filename is absolute or climbs out of the destination
    /// directory.
    #[error("invalid destination filename '{0}'")]
    InvalidFilename(String),
    /// A file published without a SHA-256 does not have the size its
    /// repository declares. The file has been deleted.
    #[error(
        "size check failed for '{file}': expected {expected} bytes, got {actual}; \
         the file was deleted"
    )]
    SizeMismatch {
        /// Destination filename.
        file: String,
        /// Declared size.
        expected: u64,
        /// Size received.
        actual: u64,
    },
    /// A file committed outside LFS does not match the git blob identifier
    /// its repository publishes. The file has been deleted.
    #[error(
        "integrity check failed for '{file}': expected git blob {expected}, got {actual}; \
         the file was deleted"
    )]
    GitOidMismatch {
        /// Destination filename.
        file: String,
        /// Published git blob identifier.
        expected: String,
        /// Git blob identifier of the bytes received.
        actual: String,
    },
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
    /// SHA-256 of the file on disk, lowercase hex. Set on `Completed` only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    /// True on `Completed` when `sha256` matched the reference the request
    /// carried in [`DownloadRequest::expected_sha256`].
    #[serde(default)]
    pub integrity_verified: bool,
    /// Why the download stopped. Set on `Failed` only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
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
    /// SHA-256 the finished file must have (for a HuggingFace file, its
    /// `lfs.oid`). When `Some`, a file whose hash differs is deleted and the
    /// download fails with [`DownloadError::IntegrityMismatch`]. When `None`,
    /// the hash is computed and reported but nothing is verified.
    pub expected_sha256: Option<String>,
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
        let client = apollia_core::net::safe_client_builder()
            .user_agent("Apollia-OS/1.0")
            // connect_timeout only, no total-request timeout so large model downloads
            // (multi-GB) are not killed mid-stream after 60 s.
            .connect_timeout(std::time::Duration::from_secs(30))
            .build()
            // SAFETY: the only failure `ClientBuilder::build` reports is a TLS
            // backend that will not initialise; every setting above it is a
            // literal fixed in this file.
            .expect("the TLS backend failed to initialise");
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
            let mut active = self.active.lock().unwrap_or_else(PoisonError::into_inner);
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
                let mut map = active.lock().unwrap_or_else(PoisonError::into_inner);
                map.remove(&task_id);
            }

            if let Err(e) = result {
                tracing::error!(id = %task_id, error = %e, "model.download.failed");
            }
        });

        id
    }

    /// Cancel an in-flight download.
    ///
    /// # Errors
    /// [`DownloadError::NotFound`] if the ID is unknown or already finished.
    pub fn cancel(&self, id: &str) -> Result<(), DownloadError> {
        let active = self.active.lock().unwrap_or_else(PoisonError::into_inner);
        match active.get(id) {
            Some(token) => {
                token.cancel();
                Ok(())
            }
            None => Err(DownloadError::NotFound(id.to_string())),
        }
    }
}

impl Default for DownloadManager {
    fn default() -> Self {
        Self::new()
    }
}

// ── Core download logic ──────────────────────────────────────────────────

/// What a finished download hands back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DownloadOutcome {
    /// Where the file was written.
    pub(crate) dest_path: PathBuf,
    /// Size of the file on disk.
    pub(crate) bytes: u64,
    /// SHA-256 of the file on disk, lowercase hex.
    pub(crate) sha256: String,
    /// True when `sha256` was checked against a published reference.
    pub(crate) integrity_verified: bool,
}

/// Run one download and report a failure through `on_progress`, so the
/// caller sees why it stopped and not only that it stopped.
async fn run_download(
    client: reqwest::Client,
    request: DownloadRequest,
    id: DownloadId,
    cancel: CancellationToken,
    on_progress: ProgressCallback,
) -> Result<DownloadOutcome, DownloadError> {
    let dest_path = request
        .dest_dir
        .join(request_filename(&request).unwrap_or_default());
    let result =
        download_and_verify(client, request, id.clone(), cancel, on_progress.clone()).await;
    match &result {
        Err(DownloadError::Cancelled) | Ok(_) => {}
        Err(e) => on_progress(DownloadProgress {
            id,
            downloaded_bytes: 0,
            total_bytes: None,
            speed_bps: 0.0,
            dest_path,
            status: DownloadStatus::Failed,
            sha256: None,
            integrity_verified: false,
            error: Some(e.to_string()),
        }),
    }
    result
}

/// The destination name of a request: the requested filename, or the last
/// segment of the URL.
fn request_filename(request: &DownloadRequest) -> Option<String> {
    request.filename.clone().or_else(|| {
        request
            .url
            .rsplit('/')
            .next()
            .filter(|s| !s.is_empty())
            .map(String::from)
    })
}

/// A destination name may hold subfolders, never an absolute path or a `..`
/// that would climb out of the destination directory.
fn checked_relative(filename: &str) -> Result<&Path, DownloadError> {
    let path = Path::new(filename);
    let all_normal = path
        .components()
        .all(|c| matches!(c, std::path::Component::Normal(_)));
    if filename.is_empty() || !all_normal {
        return Err(DownloadError::InvalidFilename(filename.to_string()));
    }
    Ok(path)
}

/// How the server answered the request, once its status is known.
enum Body {
    /// Content to write, appended to the partial file when `resume` is true.
    Stream {
        resp: reqwest::Response,
        resume: bool,
    },
    /// 416 on a resumed request: the partial file is already whole.
    AlreadyComplete,
}

async fn download_and_verify(
    client: reqwest::Client,
    request: DownloadRequest,
    id: DownloadId,
    cancel: CancellationToken,
    on_progress: ProgressCallback,
) -> Result<DownloadOutcome, DownloadError> {
    let repo_id = request.repo_id.clone();
    let hf_token = request.hf_token.clone();
    let filename = request_filename(&request).unwrap_or_else(|| "model.gguf".to_string());
    let dest_path = request.dest_dir.join(checked_relative(&filename)?);

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
    // The status decides before a single byte reaches the disk: an error page
    // is a body too, and writing it would leave a file that looks like a model.
    let body = match resp.status() {
        reqwest::StatusCode::PARTIAL_CONTENT if already_downloaded > 0 => {
            Body::Stream { resp, resume: true }
        }
        reqwest::StatusCode::RANGE_NOT_SATISFIABLE if already_downloaded > 0 => {
            Body::AlreadyComplete
        }
        s if s.is_success() => Body::Stream {
            resp,
            resume: false,
        },
        s => {
            return Err(DownloadError::HttpStatus {
                status: s.as_u16(),
                url: request.url.clone(),
            })
        }
    };

    if let Some(parent) = dest_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let mut hasher = Sha256::new();
    let downloaded = match body {
        Body::AlreadyComplete => {
            hash_existing(&dest_path, &mut hasher).await?;
            already_downloaded
        }
        Body::Stream { resp, resume } => {
            let start_at = if resume {
                hash_existing(&dest_path, &mut hasher).await?;
                already_downloaded
            } else {
                0
            };
            let listener = Listener {
                id: &id,
                cancel: &cancel,
                on_progress: &on_progress,
            };
            stream_to_file(resp, &dest_path, start_at, &mut hasher, &listener).await?
        }
    };

    let sha256 = format!("{:x}", hasher.finalize());
    let integrity_verified = match request.expected_sha256.as_deref() {
        None => false,
        Some(expected) if expected.trim().eq_ignore_ascii_case(&sha256) => true,
        Some(expected) => {
            let expected = expected.trim().to_ascii_lowercase();
            if let Err(e) = tokio::fs::remove_file(&dest_path).await {
                tracing::event!(
                    tracing::Level::WARN,
                    id = %id,
                    path = %dest_path.display(),
                    error = %e,
                    "llm.download.integrity_mismatch.remove_failed"
                );
            }
            tracing::event!(
                tracing::Level::ERROR,
                id = %id,
                file = %filename,
                expected = %expected,
                actual = %sha256,
                "llm.download.integrity_mismatch"
            );
            return Err(DownloadError::IntegrityMismatch {
                file: filename,
                expected,
                actual: sha256,
            });
        }
    };

    on_progress(DownloadProgress {
        id: id.clone(),
        downloaded_bytes: downloaded,
        total_bytes: Some(downloaded),
        speed_bps: 0.0,
        dest_path: dest_path.clone(),
        status: DownloadStatus::Completed,
        sha256: Some(sha256.clone()),
        integrity_verified,
        error: None,
    });

    tracing::info!(
        id = %id,
        path = %dest_path.display(),
        bytes = downloaded,
        sha256 = %sha256,
        integrity_verified,
        "model.download.completed"
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
                detail = "the download itself is unaffected",
                "model.sampling_defaults.persist.failed"
            );
        }
    }

    Ok(DownloadOutcome {
        dest_path,
        bytes: downloaded,
        sha256,
        integrity_verified,
    })
}

/// Feed the bytes already on disk to `hasher`, so a resumed download is
/// hashed over the whole file and not only over its tail.
async fn hash_existing(path: &Path, hasher: &mut Sha256) -> Result<(), DownloadError> {
    let mut file = tokio::fs::File::open(path).await?;
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = file.read(&mut buf).await?;
        if n == 0 {
            return Ok(());
        }
        hasher.update(&buf[..n]);
    }
}

/// Who a streaming download reports to, and what can stop it.
struct Listener<'a> {
    id: &'a DownloadId,
    cancel: &'a CancellationToken,
    on_progress: &'a ProgressCallback,
}

/// Write the response body to `dest_path`, hashing every chunk on the way.
/// Returns the size of the file once the body is exhausted.
async fn stream_to_file(
    resp: reqwest::Response,
    dest_path: &Path,
    start_at: u64,
    hasher: &mut Sha256,
    listener: &Listener<'_>,
) -> Result<u64, DownloadError> {
    let Listener {
        id,
        cancel,
        on_progress,
    } = *listener;
    let total_bytes = resp.content_length().map(|l| l + start_at);

    let mut file = OpenOptions::new()
        .create(true)
        .append(start_at > 0)
        .truncate(start_at == 0)
        .write(true)
        .open(dest_path)
        .await?;

    // SAFETY: a model download is a multi-gigabyte body written to disk as it
    // arrives, so it cannot go through `apollia_core::net::read_capped_*`,
    // which buffer in memory. Nothing is buffered here either: each chunk is
    // appended to the file and dropped.
    let mut stream = resp.bytes_stream();
    let mut downloaded = start_at;
    let start = std::time::Instant::now();

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                drop(file);
                if let Err(e) = tokio::fs::remove_file(dest_path).await {
                    tracing::warn!(
                        id = %id,
                        path = %dest_path.display(),
                        error = %e,
                        "model.download.partial.remove.failed"
                    );
                }
                on_progress(DownloadProgress {
                    id: id.clone(),
                    downloaded_bytes: 0,
                    total_bytes,
                    speed_bps: 0.0,
                    dest_path: dest_path.to_path_buf(),
                    status: DownloadStatus::Cancelled,
                    sha256: None,
                    integrity_verified: false,
                    error: None,
                });
                return Err(DownloadError::Cancelled);
            }
            chunk = stream.next() => {
                match chunk {
                    None => break,
                    Some(Err(e)) => return Err(DownloadError::Http(e)),
                    Some(Ok(bytes)) => {
                        file.write_all(&bytes).await?;
                        hasher.update(&bytes);
                        downloaded += bytes.len() as u64;
                        let elapsed = start.elapsed().as_secs_f64().max(0.001);
                        let speed_bps = downloaded as f64 / elapsed;
                        on_progress(DownloadProgress {
                            id: id.clone(),
                            downloaded_bytes: downloaded,
                            total_bytes,
                            speed_bps,
                            dest_path: dest_path.to_path_buf(),
                            status: DownloadStatus::InProgress,
                            sha256: None,
                            integrity_verified: false,
                            error: None,
                        });
                    }
                }
            }
        }
    }

    file.flush().await?;
    Ok(downloaded)
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
                return Err("generation_config.json absent, and no base_model declared".to_string());
            };
            tracing::info!(
                repo = %repo_id,
                base_model = %base,
                detail = "retrying on the base model",
                "model.generation_config.missing"
            );
            client.get_generation_config(&base).await.ok_or_else(|| {
                format!("generation_config.json absent on base_model {base} as well")
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
        ..crate::model_defaults::ModelDefaults::default()
    };

    if defaults.is_empty() {
        return Err("generation_config.json present but with no usable hyperparameter".to_string());
    }

    let path = crate::model_defaults::UserOverrides::default_path();
    crate::model_defaults::UserOverrides::upsert(&path, model_filename, defaults.clone())
        .map_err(|e| format!("writing {}: {e}", path.display()))?;

    tracing::info!(
        repo = %repo_id,
        file = %model_filename,
        path = %path.display(),
        "model.sampling_defaults.persisted"
    );
    Ok(())
}

mod group;

pub use group::VariantDownload;

#[cfg(test)]
mod tests;
