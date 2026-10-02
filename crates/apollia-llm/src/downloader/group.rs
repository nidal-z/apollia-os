//! Download every file of one variant as a single download.
//!
//! The files of a [`VariantPlan`] are fetched one after the other under one
//! identifier, with one cancellation token and one progress stream: the
//! listener sees the bytes of the whole variant against its total size, one
//! `Completed` when the last file is checked, and one `Failed` or `Cancelled`
//! otherwise. Each file is checked on the way: against its SHA-256 when the
//! repository stores it through LFS, and against its declared size and its git
//! blob identifier when it is a small file committed outside LFS.

use std::path::PathBuf;
use std::sync::{Arc, PoisonError};

use sha1::{Digest, Sha1};
use tokio::io::AsyncReadExt;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::{
    download_and_verify, DownloadError, DownloadId, DownloadManager, DownloadProgress,
    DownloadRequest, DownloadStatus, ProgressCallback,
};
use crate::hf_registry::VariantPlan;
use crate::model_index::ModelFormat;

/// Request to download one variant.
#[derive(Debug, Clone)]
pub struct VariantDownload {
    /// What to fetch.
    pub plan: VariantPlan,
    /// Models directory the plan's relative paths are resolved against.
    pub dest_dir: PathBuf,
    /// HF token for gated models (optional).
    pub hf_token: Option<String>,
}

impl VariantDownload {
    /// Where the model lands: the first weights file of a GGUF, the folder of
    /// an MLX model. This is the path the model index records.
    #[must_use]
    pub fn model_path(&self) -> PathBuf {
        self.dest_dir.join(&self.plan.model_path)
    }
}

impl DownloadManager {
    /// Start downloading every file of a variant, in the background, as one
    /// download.
    ///
    /// Returns the `DownloadId` immediately. `on_progress` receives the
    /// aggregated progress, its `dest_path` always the model path.
    pub fn start_variant(
        &self,
        request: VariantDownload,
        on_progress: ProgressCallback,
    ) -> DownloadId {
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
            let result = run_variant(client, request, task_id.clone(), cancel, on_progress).await;
            {
                let mut map = active.lock().unwrap_or_else(PoisonError::into_inner);
                map.remove(&task_id);
            }
            if let Err(e) = result {
                tracing::event!(
                    tracing::Level::ERROR,
                    id = %task_id,
                    error = %e,
                    "model.download.variant.failed"
                );
            }
        });
        id
    }
}

/// Run a variant download and report a failure once, through `on_progress`.
pub(super) async fn run_variant(
    client: reqwest::Client,
    request: VariantDownload,
    id: DownloadId,
    cancel: CancellationToken,
    on_progress: ProgressCallback,
) -> Result<DownloadProgress, DownloadError> {
    let model_path = request.model_path();
    let result = fetch_all(client, &request, &id, &cancel, &on_progress).await;
    match &result {
        Ok(done) => on_progress(done.clone()),
        Err(DownloadError::Cancelled) => {}
        Err(e) => on_progress(DownloadProgress {
            id: id.clone(),
            downloaded_bytes: 0,
            total_bytes: Some(request.plan.total_bytes),
            speed_bps: 0.0,
            dest_path: model_path,
            status: DownloadStatus::Failed,
            sha256: None,
            integrity_verified: false,
            error: Some(e.to_string()),
        }),
    }
    result
}

async fn fetch_all(
    client: reqwest::Client,
    request: &VariantDownload,
    id: &DownloadId,
    cancel: &CancellationToken,
    on_progress: &ProgressCallback,
) -> Result<DownloadProgress, DownloadError> {
    let plan = &request.plan;
    let model_path = request.model_path();
    let total = plan.total_bytes;
    let mut done = 0u64;
    let mut all_sha_verified = true;
    let mut model_sha256 = None;

    for (i, file) in plan.files.iter().enumerate() {
        // The sampling defaults are read once, for the weights a GGUF model is
        // named after; an MLX folder carries its own generation config.
        let repo_id = (i == 0 && plan.format == ModelFormat::Gguf).then(|| plan.repo_id.clone());
        let file_request = DownloadRequest {
            url: file.url.clone(),
            dest_dir: request.dest_dir.clone(),
            filename: Some(file.dest_name.clone()),
            hf_token: request.hf_token.clone(),
            repo_id,
            expected_sha256: file.sha256.clone(),
        };
        let relay = aggregate(on_progress.clone(), done, total, model_path.clone());
        let outcome = download_and_verify(
            client.clone(),
            file_request,
            id.clone(),
            cancel.clone(),
            relay,
        )
        .await?;

        if file.sha256.is_none() && outcome.bytes != file.size_bytes {
            remove_quietly(id, &outcome.dest_path).await;
            return Err(DownloadError::SizeMismatch {
                file: file.dest_name.clone(),
                expected: file.size_bytes,
                actual: outcome.bytes,
            });
        }
        let git_oid_verified = match file.git_oid.as_deref() {
            Some(expected) if file.sha256.is_none() => {
                let actual = git_blob_oid(&outcome.dest_path).await?;
                if actual != expected {
                    remove_quietly(id, &outcome.dest_path).await;
                    return Err(DownloadError::GitOidMismatch {
                        file: file.dest_name.clone(),
                        expected: expected.to_owned(),
                        actual,
                    });
                }
                true
            }
            _ => false,
        };
        all_sha_verified &= outcome.integrity_verified || git_oid_verified;
        if i == 0 && plan.format == ModelFormat::Gguf {
            model_sha256 = Some(outcome.sha256.clone());
        }
        done += outcome.bytes;
    }

    tracing::event!(
        tracing::Level::INFO,
        id = %id,
        repo = %plan.repo_id,
        path = %model_path.display(),
        files = plan.files.len(),
        bytes = done,
        "model.download.variant.completed"
    );
    Ok(DownloadProgress {
        id: id.clone(),
        downloaded_bytes: done,
        total_bytes: Some(done),
        speed_bps: 0.0,
        dest_path: model_path,
        status: DownloadStatus::Completed,
        sha256: model_sha256,
        integrity_verified: all_sha_verified,
        error: None,
    })
}

/// A listener for one file that reports to the variant's listener: bytes
/// counted from `before`, against `total`, under the model path. A file's own
/// `Completed` is swallowed, the variant reports its own once at the end.
fn aggregate(
    on_progress: ProgressCallback,
    before: u64,
    total: u64,
    model_path: PathBuf,
) -> ProgressCallback {
    Arc::new(move |p: DownloadProgress| match p.status {
        DownloadStatus::InProgress => on_progress(DownloadProgress {
            downloaded_bytes: before + p.downloaded_bytes,
            total_bytes: Some(total),
            dest_path: model_path.clone(),
            ..p
        }),
        DownloadStatus::Cancelled => on_progress(DownloadProgress {
            downloaded_bytes: before,
            total_bytes: Some(total),
            dest_path: model_path.clone(),
            ..p
        }),
        DownloadStatus::Completed | DownloadStatus::Failed => {}
    })
}

/// The git blob identifier of the file at `path`: the SHA-1 of
/// `blob <size>\0` followed by the content, as git and HuggingFace compute it.
async fn git_blob_oid(path: &std::path::Path) -> Result<String, DownloadError> {
    let mut file = tokio::fs::File::open(path).await?;
    let size = file.metadata().await?.len();
    let mut hasher = Sha1::new();
    hasher.update(format!("blob {size}\0").as_bytes());
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

async fn remove_quietly(id: &DownloadId, path: &std::path::Path) {
    if let Err(e) = tokio::fs::remove_file(path).await {
        tracing::event!(
            tracing::Level::WARN,
            id = %id,
            path = %path.display(),
            error = %e,
            "llm.download.variant_file.remove_failed"
        );
    }
}

#[cfg(test)]
mod tests;
