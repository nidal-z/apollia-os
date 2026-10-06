//! Inference engines: what is installed, the followed channel, updates, and
//! the verified download or import of an engine.
//!
//! - `GET  /api/v1/engines`          , engines embedded and installed with the
//!   version each states and the ones in service, the dictation engine,
//!   channel, updates
//! - `GET  /api/v1/engines/settings` , the followed channel and the mirror address
//! - `PUT  /api/v1/engines/settings` , change them; a new channel is checked at once
//! - `POST /api/v1/engines/check`    , fetch and verify the channel manifest
//!
//! A check that finds an update, and a change of channel, start the automatic
//! update of the chosen engines when `[engines] auto_update` is on.
//! - `POST /api/v1/engines/install`  , download, verify and install one variant
//! - `POST /api/v1/engines/import`   , install from a directory on a disk
//! - `GET  /api/v1/engines/nodejs`   , where Node.js stands: absent, downloading, ready, failed
//! - `POST /api/v1/engines/nodejs`   , start installing Node.js unless it is there or on its way

use std::path::{Path, PathBuf};

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::api::routes_llm::servers::{api_err, ApiError};
use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;
use crate::engine_store::{
    self, cached_manifest, installed_packages, node::NodeState, version, Channel, Engine,
    EngineChoices, EngineStore, EngineStoreError, EngineStoreSettings, Source, Variant,
    VariantUpdate,
};

/// One engine on this machine.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct EngineEntry {
    /// Engine.
    pub engine: Engine,
    /// Upstream tag; unknown for an embedded engine.
    pub tag: Option<String>,
    /// Variant; unknown for an embedded engine whose build is not stated.
    pub variant: Option<Variant>,
    /// Where it lives.
    pub path: String,
    /// Embedded, downloaded or imported.
    pub source: Source,
    /// The version the engine states at its source: the `--version` banner of
    /// a binary, the package metadata of a Python environment. Unknown when it
    /// does not answer.
    pub version: Option<String>,
    /// True for the build this runtime starts for the model format it serves:
    /// the `llama-server` binary it resolves, the MLX environment it imports.
    pub in_service: bool,
}

/// The dictation engine, whisper.cpp inside the runner the daemon starts.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct DictationEngine {
    /// The runner binary.
    pub path: String,
    /// The whisper.cpp version the runner states with `--version`; unknown
    /// when it does not answer.
    pub version: Option<String>,
}

/// Body of `GET /api/v1/engines`.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct EnginesResponse {
    /// Engines embedded in the bundle, then installed ones.
    pub engines: Vec<EngineEntry>,
    /// The followed channel.
    pub channel: Channel,
    /// Sequence of the last verified manifest of that channel, if any.
    pub manifest_sequence: Option<u64>,
    /// Per variant offered by that manifest for this platform, whether a newer
    /// build than the installed one exists. Empty until a manifest verified.
    pub updates: Vec<VariantUpdate>,
    /// True when an NVIDIA card is detected and no CUDA engine is installed.
    pub cuda_proposable: bool,
    /// The dictation engine, when a runner is in reach.
    pub dictation: Option<DictationEngine>,
}

/// Body of the settings routes.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct EngineSettingsBody {
    /// The followed channel.
    pub channel: Channel,
    /// Base address manifests and archives are fetched from.
    pub base_url: String,
}

/// Body of `POST /api/v1/engines/check`.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct EngineCheckResponse {
    /// The channel checked.
    pub channel: Channel,
    /// Sequence of the manifest just verified.
    pub manifest_sequence: u64,
    /// Per variant, whether an update exists.
    pub updates: Vec<VariantUpdate>,
}

/// Body of `POST /api/v1/engines/install`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct EngineInstallRequest {
    /// Engine to install.
    pub engine: Engine,
    /// Variant to install; its newest build on the channel is taken.
    pub variant: Variant,
}

/// Body of `POST /api/v1/engines/import`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct EngineImportRequest {
    /// Directory holding `engines.json`, `engines.json.minisig` and archives.
    pub path: String,
}

/// Engines installed by a request.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct EngineInstalledResponse {
    /// What was installed.
    pub installed: Vec<EngineEntry>,
}

fn status_of(error: &EngineStoreError) -> StatusCode {
    match error {
        EngineStoreError::NoTrustedKey => StatusCode::SERVICE_UNAVAILABLE,
        EngineStoreError::PackageNotFound { .. } => StatusCode::NOT_FOUND,
        EngineStoreError::HttpStatus { .. } | EngineStoreError::Transport { .. } => {
            StatusCode::BAD_GATEWAY
        }
        EngineStoreError::Io { .. } | EngineStoreError::StoreFile { .. } => {
            StatusCode::INTERNAL_SERVER_ERROR
        }
        _ => StatusCode::UNPROCESSABLE_ENTITY,
    }
}

fn store_err(error: EngineStoreError) -> ApiError {
    tracing::event!(tracing::Level::WARN, error = %error, "engine_store.request.refused");
    api_err(status_of(&error), error)
}

fn open_store(data_dir: &Path) -> Result<EngineStore, ApiError> {
    EngineStore::open(data_dir).map_err(store_err)
}

fn entry(p: engine_store::InstalledPackage) -> EngineEntry {
    EngineEntry {
        engine: p.engine,
        tag: Some(p.tag),
        variant: Some(p.variant),
        path: p.path.display().to_string(),
        source: p.source,
        version: None,
        in_service: false,
    }
}

/// The version `entry` states at its source, read from what is on disk.
async fn stated_version(entry: &EngineEntry) -> Option<String> {
    let path = Path::new(&entry.path);
    let binary = |name: &str| {
        let file = format!("{name}{}", std::env::consts::EXE_SUFFIX);
        if path.is_file() {
            Some(path.to_path_buf())
        } else {
            engine_store::find_binary(path, &file)
        }
    };
    match entry.engine {
        Engine::LlamaServer => version::probe_banner(&binary("llama-server")?).await,
        Engine::NodeJs => version::probe_banner(&binary("node")?).await,
        Engine::VllmMlx => version::dist_info_version(&path.join("site-packages"), "vllm_mlx"),
    }
}

/// `entries`, each with the version it states.
async fn with_versions(mut entries: Vec<EngineEntry>) -> Vec<EngineEntry> {
    for entry in &mut entries {
        entry.version = stated_version(entry).await;
    }
    entries
}

/// What this runtime starts under the engine `choices` of the settings: the
/// `llama-server` binary and the package directory of the MLX environment,
/// each `None` when none is in reach.
fn engines_in_service(choices: EngineChoices) -> (Option<PathBuf>, Option<PathBuf>) {
    let llama = crate::llama_server::locate::in_service_binary(choices.gguf);
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf));
    let store_root = apollia_core::paths::data_dir().map(|d| engine_store::store_root(&d));
    let mlx = crate::mlx_engine::locate(
        exe_dir.as_deref(),
        store_root.as_deref(),
        choices.mlx,
        |var| std::env::var(var).ok(),
    )
    .and_then(|launch| launch.site_packages().map(Path::to_path_buf));
    (llama, mlx)
}

/// Mark, among `entries`, the builds the runtime starts: the `llama-server`
/// entry that holds `llama_binary`, the MLX entry whose packages are
/// `mlx_site_packages`. Node.js serves no model and is never marked.
fn mark_in_service(
    entries: &mut [EngineEntry],
    llama_binary: Option<&Path>,
    mlx_site_packages: Option<&Path>,
) {
    let llama_name = format!("llama-server{}", std::env::consts::EXE_SUFFIX);
    for entry in entries {
        let path = Path::new(&entry.path);
        entry.in_service = match entry.engine {
            Engine::LlamaServer => llama_binary.is_some_and(|wanted| {
                if path.is_file() {
                    path == wanted
                } else {
                    engine_store::find_binary(path, &llama_name).as_deref() == Some(wanted)
                }
            }),
            Engine::VllmMlx => {
                mlx_site_packages.is_some_and(|wanted| path.join("site-packages") == wanted)
            }
            Engine::NodeJs => false,
        };
    }
}

/// The dictation engine of this machine and the version its runner states.
async fn dictation_engine() -> Option<DictationEngine> {
    let binary = tokio::task::spawn_blocking(crate::runner_supervisor::dictation_runner_binary)
        .await
        .ok()
        .flatten()?;
    Some(DictationEngine {
        path: binary.display().to_string(),
        version: version::probe_banner(&binary).await,
    })
}

/// The engines shipped inside the bundle next to the running executable.
fn embedded_engines() -> Vec<EngineEntry> {
    let Some(exe_dir) = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(Path::to_path_buf))
    else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let name = if cfg!(windows) {
        "llama-server.exe"
    } else {
        "llama-server"
    };
    let mut llama: Vec<PathBuf> = vec![exe_dir.join(name)];
    llama.extend(
        apollia_core::paths::bundled_resource_dirs(&exe_dir, "runners")
            .into_iter()
            .map(|d| d.join(name)),
    );
    if let Some(found) = llama.into_iter().find(|p| p.is_file()) {
        out.push(EngineEntry {
            engine: Engine::LlamaServer,
            tag: None,
            variant: cfg!(target_os = "macos").then_some(Variant::Metal),
            path: found.display().to_string(),
            source: Source::Embedded,
            version: None,
            in_service: false,
        });
    }
    let mlx = apollia_core::paths::bundled_resource_dirs(&exe_dir, "python")
        .into_iter()
        .map(|d| d.join("engines").join("mlx"))
        .find(|d| d.join("site-packages").is_dir());
    if let Some(found) = mlx {
        out.push(EngineEntry {
            engine: Engine::VllmMlx,
            tag: None,
            variant: Some(Variant::Metal),
            path: found.display().to_string(),
            source: Source::Embedded,
            version: None,
            in_service: false,
        });
    }
    out
}

/// What the list route answers, without the hardware probe.
fn engines_view(
    data_dir: &Path,
    embedded: Vec<EngineEntry>,
    nvidia_detected: bool,
) -> Result<EnginesResponse, EngineStoreError> {
    let root = engine_store::store_root(data_dir);
    let settings = EngineStoreSettings::load(&root)?;
    let installed = installed_packages(&root);
    let has_cuda = installed
        .iter()
        .any(|p| p.engine == Engine::LlamaServer && p.variant == Variant::Cuda);
    let manifest = apollia_signing::distribution::distribution_public_key()
        .and_then(|key| cached_manifest(&root, settings.channel, key));
    let platform = engine_store::current_platform();
    let updates = manifest
        .as_ref()
        .map(|m| engine_store::updates(&root, m, &platform))
        .unwrap_or_default();
    let mut engines = embedded;
    engines.extend(installed.into_iter().map(entry));
    Ok(EnginesResponse {
        engines,
        channel: settings.channel,
        manifest_sequence: manifest.map(|m| m.sequence),
        updates,
        cuda_proposable: nvidia_detected && !has_cuda,
        dictation: None,
    })
}

#[utoipa::path(
    get,
    path = "/api/v1/engines",
    tag = "engines",
    responses(
        (status = 200, description = "Engines, channel and updates", body = EnginesResponse),
        (status = 500, description = "Store files unreadable", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn list_engines<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Result<Json<EnginesResponse>, ApiError> {
    let data_dir = state.data_dir.clone();
    let nvidia = tokio::task::spawn_blocking(|| {
        matches!(
            apollia_llm::hardware::detect().accelerator,
            apollia_llm::AcceleratorProfile::Cuda { .. }
        )
    })
    .await
    .unwrap_or(false);
    let embedded = embedded_engines();
    let mut view = engines_view(&data_dir, embedded, nvidia).map_err(store_err)?;
    view.engines = with_versions(view.engines).await;
    let config_file = state.config_path.clone();
    let imposed = state.inference_governance.imposed_settings();
    let (llama, mlx) = tokio::task::spawn_blocking(move || {
        engines_in_service(EngineChoices::read(config_file.as_deref(), &imposed))
    })
    .await
    .unwrap_or((None, None));
    mark_in_service(&mut view.engines, llama.as_deref(), mlx.as_deref());
    view.dictation = dictation_engine().await;
    Ok(Json(view))
}

/// Check the followed channel in the background and, when the settings ask
/// for it, install the update of each chosen engine it announces.
fn spawn_auto_update<B: ExecutionBackend + Clone>(state: &AppState<B>) {
    engine_store::auto_update::spawn(
        &state.data_dir,
        state.config_path.clone(),
        state.inference_governance.imposed_settings(),
    );
}

#[utoipa::path(
    get,
    path = "/api/v1/engines/settings",
    tag = "engines",
    responses(
        (status = 200, description = "Engine store settings", body = EngineSettingsBody),
        (status = 500, description = "Settings file unreadable", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn get_engine_settings<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Result<Json<EngineSettingsBody>, ApiError> {
    let s =
        EngineStoreSettings::load(&engine_store::store_root(&state.data_dir)).map_err(store_err)?;
    Ok(Json(EngineSettingsBody {
        channel: s.channel,
        base_url: s.base_url,
    }))
}

#[utoipa::path(
    put,
    path = "/api/v1/engines/settings",
    tag = "engines",
    request_body = EngineSettingsBody,
    responses(
        (status = 200, description = "Settings saved", body = EngineSettingsBody),
        (status = 422, description = "Base address is not http or https", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn set_engine_settings<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Json(body): Json<EngineSettingsBody>,
) -> Result<Json<EngineSettingsBody>, ApiError> {
    let root = engine_store::store_root(&state.data_dir);
    let previous = EngineStoreSettings::load(&root).map_err(store_err)?;
    let settings = EngineStoreSettings {
        channel: body.channel,
        base_url: body.base_url.trim().to_owned(),
    };
    settings.save(&root).map_err(store_err)?;
    if settings.channel != previous.channel {
        spawn_auto_update(&state);
    }
    Ok(Json(EngineSettingsBody {
        channel: settings.channel,
        base_url: settings.base_url,
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/engines/check",
    tag = "engines",
    responses(
        (status = 200, description = "Manifest verified, updates per variant", body = EngineCheckResponse),
        (status = 422, description = "Signature, channel, rollback or format refusal", body = crate::api::openapi::ApiErrorBody),
        (status = 502, description = "The mirror could not be reached", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "This build carries no distribution key", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn check_engines<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Result<Json<EngineCheckResponse>, ApiError> {
    let store = open_store(&state.data_dir)?;
    let settings = EngineStoreSettings::load(&store.root).map_err(store_err)?;
    let (manifest, _) = engine_store::refresh_manifest(&store, &settings)
        .await
        .map_err(store_err)?;
    let updates = engine_store::updates(&store.root, &manifest, &store.platform);
    if updates.iter().any(|u| u.update_available) {
        spawn_auto_update(&state);
    }
    Ok(Json(EngineCheckResponse {
        channel: settings.channel,
        manifest_sequence: manifest.sequence,
        updates,
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/engines/install",
    tag = "engines",
    request_body = EngineInstallRequest,
    responses(
        (status = 200, description = "Engine verified and installed", body = EngineInstalledResponse),
        (status = 404, description = "No such package for this platform", body = crate::api::openapi::ApiErrorBody),
        (status = 422, description = "Signature, digest, rollback or archive refusal", body = crate::api::openapi::ApiErrorBody),
        (status = 502, description = "The mirror could not be reached", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "This build carries no distribution key", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn install_engine<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Json(body): Json<EngineInstallRequest>,
) -> Result<Json<EngineInstalledResponse>, ApiError> {
    let store = open_store(&state.data_dir)?;
    let settings = EngineStoreSettings::load(&store.root).map_err(store_err)?;
    let installed = engine_store::install_package(&store, &settings, body.engine, body.variant)
        .await
        .map_err(store_err)?;
    Ok(Json(EngineInstalledResponse {
        installed: with_versions(vec![entry(installed)]).await,
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/engines/import",
    tag = "engines",
    request_body = EngineImportRequest,
    responses(
        (status = 200, description = "Engines verified and installed", body = EngineInstalledResponse),
        (status = 404, description = "No archive of this platform in the directory", body = crate::api::openapi::ApiErrorBody),
        (status = 422, description = "Missing or invalid signature, digest or archive refusal", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "This build carries no distribution key", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn import_engines<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Json(body): Json<EngineImportRequest>,
) -> Result<Json<EngineInstalledResponse>, ApiError> {
    let store = open_store(&state.data_dir)?;
    let dir = PathBuf::from(body.path);
    let installed =
        tokio::task::spawn_blocking(move || engine_store::import_from_dir(&store, &dir))
            .await
            .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e))?
            .map_err(store_err)?;
    Ok(Json(EngineInstalledResponse {
        installed: with_versions(installed.into_iter().map(entry).collect()).await,
    }))
}

#[utoipa::path(
    get,
    path = "/api/v1/engines/nodejs",
    tag = "engines",
    responses(
        (status = 200, description = "Where Node.js stands", body = NodeState),
    )
)]
pub async fn node_runtime_state<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Json<NodeState> {
    Json(engine_store::node::state(&engine_store::store_root(
        &state.data_dir,
    )))
}

#[utoipa::path(
    post,
    path = "/api/v1/engines/nodejs",
    tag = "engines",
    responses(
        (status = 200, description = "Node.js ready, or its download started or running; a failure is read from the state", body = NodeState),
        (status = 500, description = "Settings file unreadable", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "This build carries no distribution key", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn ensure_node_runtime<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Result<Json<NodeState>, ApiError> {
    let store = open_store(&state.data_dir)?;
    let settings = EngineStoreSettings::load(&store.root).map_err(store_err)?;
    Ok(Json(engine_store::node::ensure(&store, &settings)))
}

/// The engine routes.
pub fn engines_router<B: ExecutionBackend + Clone + 'static>() -> Router<AppState<B>> {
    Router::new()
        .route("/api/v1/engines", get(list_engines::<B>))
        .route(
            "/api/v1/engines/settings",
            get(get_engine_settings::<B>).put(set_engine_settings::<B>),
        )
        .route("/api/v1/engines/check", post(check_engines::<B>))
        .route("/api/v1/engines/install", post(install_engine::<B>))
        .route("/api/v1/engines/import", post(import_engines::<B>))
        .route(
            "/api/v1/engines/nodejs",
            get(node_runtime_state::<B>).post(ensure_node_runtime::<B>),
        )
}

#[cfg(test)]
mod tests;
