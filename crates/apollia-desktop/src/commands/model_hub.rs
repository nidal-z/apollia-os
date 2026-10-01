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
use apollia_llm::{
    AcceleratorProfile, HardwareProfile, IndexedModel, ModelCategory, ModelFormat, ModelOrigin,
};
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
        .map(expand_tilde)
        .unwrap_or_else(|| {
            let home = apollia_core::paths::home_dir_or_temp();
            apollia_core::paths::data_dir_under(home).join("models")
        });

    // Ensure destination directory exists.
    tokio::fs::create_dir_all(&dest_dir)
        .await
        .map_err(|e| format!("failed to create model directory: {e}"))?;

    // A file from a HuggingFace repository is checked against the SHA-256 its
    // tree publishes. When that reference cannot be read, the download is
    // refused rather than run unverified.
    let expected_sha256 = match request.repo_id.as_deref() {
        Some(repo_id) => {
            let path = hf_repo_path(&request.url, request.filename.as_deref());
            let client = apollia_llm::HfRegistryClient::new(request.hf_token.clone());
            let sha = client
                .lfs_sha256(repo_id, &path)
                .await
                .map_err(|e| format!("cannot verify the download, refusing it: {e}"))?;
            Some(sha)
        }
        None => None,
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
            tauri::async_runtime::spawn_blocking(move || {
                super::model_index::record_download(&models_dir(), &path, repo, file);
            });
        }
        let _ = app.emit("model-download-progress", &progress);
    });

    let id = manager.lock().await.start(dl_request, on_progress);
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

/// Classification of an installed model, from the category its header declares.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum InstalledModelKind {
    /// GGUF weights for local LLM inference.
    Llm,
    /// GGML weights for local speech-to-text.
    Stt,
}

/// A model installed under `~/.apollia/models/`, as the model index holds it.
#[derive(Debug, Clone, Serialize)]
pub struct InstalledModelView {
    /// Identifier in the model index.
    pub id: String,
    /// Absolute path on disk: the file of a GGUF, the folder of an MLX model.
    pub path: String,
    /// File or folder name (basename).
    pub name: String,
    /// Size in bytes, every part of a split file included.
    pub size_bytes: u64,
    /// LLM or STT, from the category in force.
    pub kind: InstalledModelKind,
    /// Storage format.
    pub format: ModelFormat,
    /// Category in force (the developer override when set).
    pub category: ModelCategory,
    /// Parameter count label read from the header, for example `"8B"`.
    pub parameters: Option<String>,
    /// Last modification time, RFC 3339.
    pub modified_at: Option<String>,
    /// When the model was pinned to the top of the list, RFC 3339.
    pub pinned_at: Option<String>,
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
    collect_installed_models_in(&models_dir(), &collect_in_use_refs())
}

/// The installed models of `dir`, read from its model index.
fn collect_installed_models_in(
    dir: &Path,
    refs: &[ModelRef],
) -> Result<Vec<InstalledModelView>, String> {
    let index = super::model_index::current(dir)
        .map_err(|e| format!("failed to read the model index: {e}"))?;
    let mut out: Vec<InstalledModelView> = index
        .models
        .iter()
        .map(|model| installed_view(model, refs))
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

fn installed_view(model: &IndexedModel, refs: &[ModelRef]) -> InstalledModelView {
    let category = model.effective_category();
    let kind = match category {
        ModelCategory::Dictation => InstalledModelKind::Stt,
        _ => InstalledModelKind::Llm,
    };
    let owner = used_by(&model.path, refs);
    InstalledModelView {
        id: model.id.clone(),
        path: model.path.to_string_lossy().to_string(),
        name: model
            .path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default(),
        size_bytes: model.size_bytes,
        kind,
        format: model.format,
        category,
        parameters: model.parameters.clone(),
        modified_at: model.modified_at.map(|t| t.to_rfc3339()),
        pinned_at: model.pinned_at.map(|t| t.to_rfc3339()),
        in_use: owner.is_some(),
        used_by: owner,
    }
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

    std::fs::remove_file(&canonical_target)
        .map_err(|e| format!("failed to delete model file: {e}"))?;
    if let Err(e) = super::model_index::forget(&models_dir(), &target)
        .and_then(|()| super::model_index::forget(&models_dir(), &canonical_target))
    {
        tracing::event!(
            tracing::Level::WARN,
            path = %target.display(),
            error = %e,
            "model.index.forget.failed"
        );
    }
    Ok(())
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
    tokio::task::spawn_blocking(move || {
        import_model_file_into(&source_path, &allowed_extensions, &models_dir())
    })
    .await
    .map_err(|e| format!("spawn_blocking failed: {e}"))?
    .map_err(|e| e.to_string())
}

fn import_model_file_into(
    source_path: &str,
    allowed_extensions: &[String],
    dir: &Path,
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
    std::fs::create_dir_all(dir).map_err(ImportModelError::CreateDir)?;
    let dest = dir.join(file_name);

    // `std::fs::copy` streams the file (it does not buffer the whole payload in
    // memory), which matters for multi-gigabyte GGUF weights.
    if dest != source {
        std::fs::copy(&source, &dest).map_err(ImportModelError::Copy)?;
    }

    tracing::info!(dest = %dest.display(), "model.file.imported");
    if let Err(e) = super::model_index::record(dir, &dest, ModelOrigin::Import) {
        tracing::event!(
            tracing::Level::WARN,
            path = %dest.display(),
            error = %e,
            "model.index.record.failed"
        );
    }
    Ok(dest.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

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

    // GIVEN a GGUF file outside the models folder, picked by the operator
    // WHEN it is imported
    // THEN the copy is in the model index, with an import origin
    #[test]
    fn test_an_import_adds_its_entry_to_the_index() {
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("picked.gguf");
        std::fs::write(&source, b"GGUF").unwrap();
        let dir = tmp.path().join("models");

        import_model_file_into(&source.to_string_lossy(), &["gguf".to_string()], &dir).unwrap();

        let index = super::super::model_index::current(&dir).unwrap();
        let entry = index.get("picked").unwrap();
        assert_eq!(entry.path, dir.join("picked.gguf"));
        assert_eq!(entry.origin, ModelOrigin::Import);
    }

    // GIVEN a GGUF kept in a sub-folder of the models folder, the shape the
    //       listing at the top level used to miss
    // WHEN the installed models are listed
    // THEN it is among them, read from the index
    #[test]
    fn test_the_installed_list_reads_the_index_and_sees_subfolders() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        std::fs::create_dir_all(dir.join("vendor")).unwrap();
        std::fs::write(dir.join("vendor/deep.gguf"), b"GGUF").unwrap();

        let list = collect_installed_models_in(dir, &[]).unwrap();

        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "deep");
        assert_eq!(list[0].name, "deep.gguf");
        assert!(super::super::model_index::index_path(dir).is_file());
    }

    // GIVEN a dictation model saved as .gguf, whose header names whisper
    // WHEN the installed models are listed
    // THEN it is filed as STT: the header decides, where the extension alone
    //      used to file every .gguf as an LLM
    #[test]
    fn test_a_gguf_dictation_model_is_listed_as_stt() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let mut header = b"GGUF".to_vec();
        header.extend_from_slice(&3u32.to_le_bytes());
        header.extend_from_slice(&0u64.to_le_bytes());
        header.extend_from_slice(&1u64.to_le_bytes());
        header.extend_from_slice(&20u64.to_le_bytes());
        header.extend_from_slice(b"general.architecture");
        header.extend_from_slice(&8u32.to_le_bytes());
        header.extend_from_slice(&7u64.to_le_bytes());
        header.extend_from_slice(b"whisper");
        std::fs::write(dir.join("whisper-fr.gguf"), header).unwrap();

        let list = collect_installed_models_in(dir, &[]).unwrap();

        let json = serde_json::to_value(&list).unwrap();
        assert_eq!(json[0]["kind"], "stt");
        assert_eq!(json[0]["category"], "dictation");
    }
}
