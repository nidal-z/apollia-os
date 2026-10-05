//! Tauri IPC commands for the Model Hub.
//!
//! - [`get_hardware_profile`]: detects the machine's RAM, CPU and GPU
//! - [`search_hf_models`]: searches the official publishers and the recognised
//!   converters, one HuggingFace organisation at a time
//! - [`get_hf_model`]: full metadata for a HuggingFace model
//! - [`start_model_download`]: downloads a GGUF file
//! - [`start_variant_download`]: downloads a whole variant (parts, projector,
//!   MLX folder) as one download
//! - [`cancel_model_download`]: cancels an in-progress download
//! - [`delete_installed_model`]: removes an installed model, unless in use
//! - [`get_models_dir`]: the models folder in force
//!
//! A request that carries no Hugging Face token uses the one stored by the
//! General settings (`hf_token`).
//!
//! The import of a model from a disk lives in `model_import`.

use std::path::PathBuf;
use std::sync::Arc;

use apollia_llm::hardware::detect as detect_hardware;
use apollia_runtime::api::routes_model_hub::installed::DeletedInstalledModel;
use apollia_runtime::installed_models::delete_installed;
use serde::Deserialize;
use tauri::{Emitter, State};
use tokio::sync::Mutex;

use crate::attached::{segment, RuntimeLink};

mod hub_link;

pub(crate) use hub_link::recommend_on;

pub(crate) use apollia_runtime::api::routes_model_hub::HardwareResponse as HardwareProfileView;

// ─────────────────────────────────────────────
// Shared download manager state
// ─────────────────────────────────────────────

/// Shared download manager, managed as Tauri state.
pub type SharedDownloadManager = Arc<Mutex<apollia_llm::DownloadManager>>;

// ─────────────────────────────────────────────
// Response types
// ─────────────────────────────────────────────

/// Search parameters. The search only ever asks the organisations of the two
/// publisher lists; the query names a model, never an owner.
#[derive(Debug, Deserialize)]
pub struct HfSearchParams {
    /// Words of a model name.
    #[serde(default)]
    pub query: String,
    /// `gguf` (default) or `mlx`, the latter on Apple Silicon only.
    #[serde(default)]
    pub format: apollia_llm::SearchFormat,
    /// `best_match` (default), `likes`, `downloads` or `updated`.
    pub sort: Option<String>,
    /// Maximum number of results.
    pub limit: Option<u32>,
    /// Keep only the results one of whose variants is recommended here.
    #[serde(default)]
    pub fits_only: bool,
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

/// Request to download one variant of a HuggingFace repository.
#[derive(Debug, Deserialize)]
pub struct VariantDownloadRequest {
    /// Repository (`org/name`).
    pub repo_id: String,
    /// Which variant: a GGUF weights file of the repository, or the whole MLX
    /// repository.
    pub selector: apollia_llm::VariantSelector,
    /// HF token for gated models.
    pub hf_token: Option<String>,
    /// Destination directory (defaults to `~/.apollia/models/`).
    pub dest_dir: Option<String>,
}

// ─────────────────────────────────────────────
// Commands
// ─────────────────────────────────────────────

/// Detects the machine's hardware profile (RAM, CPU, GPU/Metal/CUDA).
///
/// Attached, the daemon probes it: it runs on this machine.
#[tauri::command]
pub async fn get_hardware_profile(
    link: State<'_, RuntimeLink>,
) -> Result<serde_json::Value, String> {
    hub_link::hardware_on(&link).await
}

/// The hardware profile of this machine, probed in this process.
pub(crate) async fn local_hardware_profile() -> Result<serde_json::Value, String> {
    // The devices held back are a setting, read again at every probe: a
    // switch flipped in Settings > Hardware shows in the very next read.
    let profile = tokio::task::spawn_blocking(|| {
        apollia_runtime::llama_server::devices::sync_selection(
            Some(&apollia_core::paths::config_file_under(
                apollia_core::paths::home_dir_or_temp(),
            )),
            &crate::commands::config::settings::policy_in_force(),
        );
        detect_hardware()
    })
    .await
    .map_err(|e| format!("hardware detection failed: {e}"))?;
    serde_json::to_value(HardwareProfileView::from(profile))
        .map_err(|e| format!("serialization error: {e}"))
}

/// Searches the official publishers and the recognised converters.
///
/// Returns the bounded page: each result with its generated card, the
/// estimated capacity of this machine, and the organisations that could not
/// be reached. Offline, the error says the search is unavailable.
#[tauri::command]
pub async fn search_hf_models(
    link: State<'_, RuntimeLink>,
    params: HfSearchParams,
) -> Result<serde_json::Value, String> {
    let mut value = hub_link::search_on(&link, params).await?;
    // The bounded search has no further page. The Hub screen still reads a
    // cursor, so it is answered as the last page.
    if let Some(object) = value.as_object_mut() {
        object.insert("next_cursor".to_owned(), serde_json::Value::Null);
    }
    Ok(value)
}

/// The bounded search, run in this process.
pub(crate) async fn local_search(params: HfSearchParams) -> Result<serde_json::Value, String> {
    use apollia_llm::model_cards::default_cache_dir;
    use apollia_llm::{BoundedSearch, HfRegistryClient, SearchInputs, SearchSort};

    let sort = match params.sort.as_deref() {
        None => SearchSort::default(),
        Some(key) => SearchSort::from_key(key).ok_or_else(|| format!("unknown sort: {key}"))?,
    };
    let search = BoundedSearch {
        query: params.query,
        format: params.format,
        sort,
        limit: params.limit,
        fits_only: params.fits_only,
    };
    let hardware = tokio::task::spawn_blocking(detect_hardware).await.ok();
    let cache_dir = default_cache_dir(&super::model_cards::data_dir());
    let inputs = tokio::task::spawn_blocking(move || SearchInputs::load(&cache_dir))
        .await
        .map_err(|e| format!("search inputs unavailable: {e}"))?;

    let client = HfRegistryClient::new(super::hf_token::passed_or_stored(params.hf_token));
    let page = client
        .search_bounded(&search, &inputs.context(hardware.as_ref()))
        .await
        .map_err(|e| e.to_string())?;
    serde_json::to_value(page).map_err(|e| format!("serialization error: {e}"))
}

/// Fetches the full metadata for a HuggingFace model.
///
/// Includes the list of GGUF files with size, quantisation and compatibility
/// badge, the capabilities of the model, its short list of variants (each
/// sized, graded, marked when fully offloaded or already downloaded, one of
/// them recommended), the recommended generation parameters from
/// `generation_config.json`, and the architecture type from `config.json`
/// (with a 24h session cache).
#[tauri::command]
pub async fn get_hf_model(
    link: State<'_, RuntimeLink>,
    cache: State<'_, Arc<apollia_llm::HfModelTypeCache>>,
    repo_id: String,
    hf_token: Option<String>,
) -> Result<serde_json::Value, String> {
    hub_link::model_on(&link, &cache, repo_id, hf_token).await
}

/// The card of a repository, fetched in this process.
pub(crate) async fn local_model(
    cache: &apollia_llm::HfModelTypeCache,
    repo_id: String,
    hf_token: Option<String>,
) -> Result<serde_json::Value, String> {
    use apollia_llm::HfRegistryClient;

    let hardware = tokio::task::spawn_blocking(detect_hardware).await.ok();

    let client = HfRegistryClient::new(super::hf_token::passed_or_stored(hf_token));
    let mut card = client
        .get_model(&repo_id, hardware.as_ref(), Some(cache))
        .await
        .map_err(|e| format!("failed to fetch model {repo_id}: {e}"))?;

    // Best-effort fetch of generation_config.json (separate from config.json)
    if let Some(gen_config) = client.get_generation_config(&repo_id).await {
        card.generation_config = Some(gen_config);
    }

    // Which variants are already on this machine: read from the model index,
    // best-effort, since a card is still worth showing without it.
    let dir = models_dir();
    if let Ok(Ok(index)) =
        tokio::task::spawn_blocking(move || super::model_index::current(&dir)).await
    {
        apollia_llm::recommend::mark_downloaded(&mut card.variants, &index);
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

    let mut request = request;
    request.hf_token = super::hf_token::passed_or_stored(request.hf_token.take());
    // A dictation file stays in the default folder; the models folder setting
    // covers the language models, their projectors and the embedding models.
    let pinned = pinned_digest(&request.url);
    let index_dir = if pinned.is_some() {
        apollia_core::config::default_models_dir(&data_dir())
    } else {
        models_dir()
    };
    let dest_dir = request
        .dest_dir
        .as_deref()
        .map(expand_tilde)
        .unwrap_or_else(|| index_dir.clone());

    // Ensure destination directory exists.
    tokio::fs::create_dir_all(&dest_dir)
        .await
        .map_err(|e| format!("failed to create model directory: {e}"))?;

    // A dictation file of the catalogue is checked against the digest the
    // table pins, with no request to the Hub. Any other file from a HuggingFace
    // repository is checked against the SHA-256 its tree publishes; when that
    // reference cannot be read, the download is refused rather than run
    // unverified.
    let expected_sha256 = match (pinned, request.repo_id.as_deref()) {
        (Some(sha), _) => Some(sha),
        (None, Some(repo_id)) => {
            let path = hf_repo_path(&request.url, request.filename.as_deref());
            let client = apollia_llm::HfRegistryClient::new(request.hf_token.clone());
            let sha = client
                .lfs_sha256(repo_id, &path)
                .await
                .map_err(|e| format!("cannot verify the download, refusing it: {e}"))?;
            Some(sha)
        }
        (None, None) => None,
    };

    let dl_request = DownloadRequest {
        url: request.url,
        dest_dir,
        filename: request.filename,
        hf_token: request.hf_token,
        repo_id: request.repo_id,
        expected_sha256,
    };

    let repo_id = dl_request.repo_id.clone();
    let on_progress = Arc::new(move |progress: apollia_llm::DownloadProgress| {
        if progress.status == apollia_llm::DownloadStatus::Completed {
            let path = progress.dest_path.clone();
            let repo = repo_id.clone();
            let file = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let digest = progress.sha256.clone();
            let verified = progress.integrity_verified;
            let index_dir = index_dir.clone();
            tauri::async_runtime::spawn_blocking(move || {
                super::model_cards::copy_card_for_download(&path, repo.as_deref());
                super::model_index::record_download(
                    &index_dir,
                    &path,
                    repo,
                    file,
                    super::model_index::DownloadDigest {
                        sha256: digest,
                        verified,
                    },
                );
            });
        }
        let _ = app.emit("model-download-progress", &progress);
    });

    let id = manager.lock().await.start(dl_request, on_progress);
    Ok(id)
}

/// The digest the catalogue pins for the file at `url`, when `url` is the
/// address of one of its dictation entries.
fn pinned_digest(url: &str) -> Option<String> {
    apollia_llm::recommend::DictationCatalogue::embedded()
        .ok()?
        .by_url(url)
        .map(|entry| entry.sha256.clone())
}

/// Starts downloading every file of one variant as a single download: each
/// part of a split GGUF, the vision projector when the repository has one, or
/// every file of an MLX repository.
///
/// Returns one `download_id`. Progress is emitted on the same
/// `model-download-progress` event as [`start_model_download`], aggregated over
/// the files, its `dest_path` the model path. Each file is checked against the
/// reference its repository publishes, and the model enters the index once.
#[tauri::command]
pub async fn start_variant_download(
    app: tauri::AppHandle,
    manager: State<'_, SharedDownloadManager>,
    request: VariantDownloadRequest,
) -> Result<String, String> {
    let mut request = request;
    request.hf_token = super::hf_token::passed_or_stored(request.hf_token.take());
    let dest_dir = request
        .dest_dir
        .as_deref()
        .map(expand_tilde)
        .unwrap_or_else(models_dir);
    tokio::fs::create_dir_all(&dest_dir)
        .await
        .map_err(|e| format!("failed to create model directory: {e}"))?;

    let client = apollia_llm::HfRegistryClient::new(request.hf_token.clone());
    let plan = client
        .variant_plan(&request.repo_id, &request.selector)
        .await
        .map_err(|e| format!("cannot prepare the download, refusing it: {e}"))?;

    let repo_id = plan.repo_id.clone();
    let origin_file = plan.origin_file.clone();
    let variant = apollia_llm::VariantDownload {
        plan,
        dest_dir,
        hf_token: request.hf_token,
    };
    let on_progress = Arc::new(move |progress: apollia_llm::DownloadProgress| {
        if progress.status == apollia_llm::DownloadStatus::Completed {
            let path = progress.dest_path.clone();
            let repo = Some(repo_id.clone());
            let file = origin_file.clone();
            let digest = progress.sha256.clone();
            let verified = progress.integrity_verified;
            tauri::async_runtime::spawn_blocking(move || {
                super::model_cards::copy_card_for_download(&path, repo.as_deref());
                super::model_index::record_download(
                    &models_dir(),
                    &path,
                    repo,
                    file,
                    super::model_index::DownloadDigest {
                        sha256: digest,
                        verified,
                    },
                );
            });
        }
        let _ = app.emit("model-download-progress", &progress);
    });

    let id = manager.lock().await.start_variant(variant, on_progress);
    Ok(id)
}

/// Path of a file inside its HuggingFace repository: what follows
/// `/resolve/<revision>/` in its download URL, or the requested filename.
fn hf_repo_path(url: &str, filename: Option<&str>) -> String {
    let from_url = url
        .split_once("/resolve/")
        .and_then(|(_, rest)| rest.split_once('/'))
        .map(|(_, path)| path.split(['?', '#']).next().unwrap_or(path))
        .filter(|p| !p.is_empty());
    match (from_url, filename) {
        (Some(p), _) => p.to_string(),
        (None, Some(f)) => f.to_string(),
        (None, None) => url.rsplit('/').next().unwrap_or_default().to_string(),
    }
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

pub(super) fn home_dir() -> PathBuf {
    apollia_core::paths::home_dir_or_temp()
}

/// The models folder in force: `[models] dir` of `apollia.toml`, else
/// `~/.apollia/models`.
pub(super) fn models_dir() -> PathBuf {
    apollia_core::config::models_dir(&data_dir())
}

/// The models folder in force, as the General settings show it.
///
/// It reads the `apollia.toml` of the data directory on every call, never the
/// runtime, so a folder chosen a moment ago is the one answered.
#[tauri::command]
pub async fn get_models_dir() -> Result<String, String> {
    Ok(models_dir().display().to_string())
}

fn expand_tilde(raw: &str) -> PathBuf {
    if let Some(rest) = raw.strip_prefix("~/") {
        home_dir().join(rest)
    } else {
        PathBuf::from(raw)
    }
}

/// The data directory the models folder sits in.
fn data_dir() -> PathBuf {
    apollia_core::paths::data_dir_under(home_dir())
}

/// The installed models, listed in this process.
pub(crate) async fn local_installed_models() -> Result<serde_json::Value, String> {
    let models =
        tokio::task::spawn_blocking(|| apollia_runtime::installed_models::list(&data_dir()))
            .await
            .map_err(|e| format!("spawn_blocking failed: {e}"))?
            .map_err(|e| e.to_string())?;
    serde_json::to_value(models).map_err(|e| format!("serialization error: {e}"))
}

/// Reconciles the model index with `~/.apollia/models/`. Run once at startup,
/// so a model copied in or removed while the application was closed is seen.
pub(crate) fn reconcile_model_index() {
    let dir = models_dir();
    if let Err(e) = super::model_index::reconcile(&dir) {
        tracing::event!(
            tracing::Level::WARN,
            dir = %dir.display(),
            error = %e,
            "model.index.reconcile.failed"
        );
    }
}

/// Lists model files installed under `~/.apollia/models/` with size and usage.
#[tauri::command]
pub async fn list_installed_models(
    link: State<'_, RuntimeLink>,
) -> Result<serde_json::Value, String> {
    hub_link::installed_on(&link).await
}

/// Deletes an installed model file after a path and in-use safety check.
///
/// Refuses any path outside `~/.apollia/models/` and any file still referenced
/// by an LLM backend or the STT configuration. The check and the removal are
/// the runtime's, served in this process or by the daemon.
#[tauri::command]
pub async fn delete_installed_model(
    link: State<'_, RuntimeLink>,
    path: String,
) -> Result<(), String> {
    delete_installed_through(link.inner(), path).await
}

/// The removal of `path` through `link`: the runtime's function in this
/// process, `DELETE /api/v1/llm/installed-models` on the daemon.
pub(crate) async fn delete_installed_through(
    link: &RuntimeLink,
    path: String,
) -> Result<(), String> {
    match link {
        RuntimeLink::Embedded(_) => tokio::task::spawn_blocking(move || {
            delete_installed(&data_dir(), &path).map_err(|e| e.to_string())
        })
        .await
        .map_err(|e| format!("spawn_blocking failed: {e}"))?,
        RuntimeLink::Attached(remote) => {
            let route = format!("/api/v1/llm/installed-models?path={}", segment(&path));
            let _: DeletedInstalledModel = remote.delete(&route).await?;
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_dictation_file_the_onboarding_offers_is_checked_against_a_pinned_digest() {
        // GIVEN the download addresses of the dictation step of the onboarding
        let catalog = include_str!("../../ui/src/components/onboarding/onboardingCatalogs.ts");
        let urls: Vec<&str> = catalog
            .lines()
            .filter_map(|line| line.trim().strip_prefix("url: \""))
            .filter_map(|rest| rest.strip_suffix("\","))
            .collect();
        assert_eq!(urls.len(), 5, "{urls:?}");

        // WHEN the download command looks up the digest of each
        let digests: Vec<Option<String>> = urls.iter().map(|u| pinned_digest(u)).collect();

        // THEN every one is the 64-digit digest the catalogue pins, and an
        // address outside the catalogue has none
        for (url, digest) in urls.iter().zip(&digests) {
            let digest = digest
                .as_deref()
                .unwrap_or_else(|| panic!("{url} has no digest"));
            assert_eq!(digest.len(), 64, "{url}");
        }
        assert_eq!(
            pinned_digest("https://huggingface.co/acme/model/resolve/main/x.bin"),
            None
        );
    }

    #[test]
    fn hf_repo_path_reads_the_path_after_the_revision() {
        // GIVEN a resolve URL whose file sits in a subfolder, with a query
        let url = "https://huggingface.co/org/repo/resolve/main/Q4_K_M/m-00001-of-00002.gguf?download=true";

        // WHEN the repository path is derived
        let path = hf_repo_path(url, Some("ignored.gguf"));

        // THEN it is the tree path, subfolder included, query dropped
        assert_eq!(path, "Q4_K_M/m-00001-of-00002.gguf");
    }

    #[test]
    fn hf_repo_path_falls_back_to_the_filename() {
        // GIVEN a URL that is not a resolve URL
        let url = "https://mirror.example/files/model.gguf";

        // WHEN the repository path is derived
        let path = hf_repo_path(url, Some("model.gguf"));

        // THEN the requested filename is used
        assert_eq!(path, "model.gguf");
    }

    // GIVEN a tilde-relative destination, as a download request could carry,
    //       under a substituted home directory
    // WHEN it is expanded
    // THEN it lands under that home, joined through `PathBuf` rather than
    //      string concatenation, so the platform separator is never lost or
    //      duplicated
    #[test]
    fn test_expand_tilde_joins_under_home() {
        let _guard = crate::commands::home_env_lock();
        let previous = std::env::var_os("HOME");
        // SAFETY: test-only mutation of a process env var, serialised by the
        // guard above and undone below.
        unsafe {
            std::env::set_var("HOME", "/home/tester");
        }

        let expanded = expand_tilde("~/.apollia/models");

        // SAFETY: same guard, restoring the value observed on entry.
        unsafe {
            match previous {
                Some(value) => std::env::set_var("HOME", value),
                None => std::env::remove_var("HOME"),
            }
        }

        assert_eq!(expanded, PathBuf::from("/home/tester/.apollia/models"));
    }

    // GIVEN an absolute destination the operator picked via the folder dialog
    // WHEN it is expanded
    // THEN it passes through unchanged: the folder picker never returns a
    //      tilde-prefixed path, and nothing here should second-guess it
    #[test]
    fn test_expand_tilde_leaves_an_absolute_path_untouched() {
        let absolute = if cfg!(windows) {
            "D:\\Models"
        } else {
            "/mnt/models"
        };
        assert_eq!(expand_tilde(absolute), PathBuf::from(absolute));
    }
}
