//! The local models the compatible server relays to, named by their
//! identifier in the model registry.
//!
//! A model resident in a local engine is served as it is. With `[server]
//! jit_loading` on, the default, a request naming a language model of the
//! registry that is not resident loads it first, through the engine's
//! `ensure_model`, and the process carries the server origin (see
//! [`crate::residency::origin`]); with it off, such a request answers
//! `model_not_loaded`. A model of a remote backend has no process on this
//! machine, so it is never listed and never relayed: the compatible server is
//! not a relay to a cloud.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use apollia_core::config::{
    resolve_setting, PolicySettings, ServerConfig, ALLOW_REQUEST_MCP_KEY, JIT_LOADING_KEY,
    SEPARATE_REASONING_KEY,
};
use apollia_core::llm_backend::LlmBackendRepository;
use apollia_llm::model_index::{
    IndexedModel, ModelCategory, ModelFormat, ModelIndex, INDEX_FILE_NAME,
};
use chrono::Utc;

use super::logs::ServerLogsHandle;
use crate::engine_store::Engine;
use crate::llama_server::{LlamaServerSupervisor, LoadedModel, ModelKind, ModelLease};
use crate::local_engine::{engine_for_format, local_row_for, LoadConsent, LocalEngine};
use crate::mlx_engine::MlxEngineSupervisor;
use crate::residency::Engines;

/// The local engines the compatible server reads, where the registry of
/// models lives, and the files its on-demand loading reads.
#[derive(Clone, Default)]
pub struct LocalModels {
    llama: Option<Arc<LlamaServerSupervisor>>,
    mlx: Option<Arc<MlxEngineSupervisor>>,
    models_dir: Option<PathBuf>,
    /// `apollia.toml`, whose `[server] jit_loading` and
    /// `separate_reasoning_content` are read at every request.
    /// `None` holds the default, on.
    settings: Option<PathBuf>,
    /// `system.db`, whose backend row of a model gives its load preset.
    system_db: Option<PathBuf>,
    /// The settings the organisation policy fixes: a setting it fixes is read
    /// from it, whatever the file says.
    policy: PolicySettings,
}

/// Why a request's model could not be held.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Unheld {
    /// The model is not resident, and is not loaded on demand.
    NotLoaded,
    /// Loading it on demand failed, for this reason.
    LoadFailed(String),
}

impl std::fmt::Debug for LocalModels {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LocalModels")
            .field("llama", &self.llama.is_some())
            .field("mlx", &self.mlx.is_some())
            .field("models_dir", &self.models_dir)
            .field("settings", &self.settings)
            .finish()
    }
}

/// One model the compatible server serves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ServedModel {
    /// The identifier a client names it by: its identifier in the registry,
    /// or its file name when the registry does not know it.
    pub(crate) id: String,
    /// What it serves.
    pub(crate) kind: ModelKind,
    /// The engine that serves it.
    pub(crate) engine: Engine,
    /// The path the engine loaded.
    pub(crate) model_path: String,
    /// When it finished loading, in seconds since the epoch.
    pub(crate) created: i64,
}

/// A model held for the length of one relayed request.
#[derive(Debug)]
pub(crate) struct HeldModel {
    pub(crate) served: ServedModel,
    /// The `model` field the engine expects in the relayed body.
    pub(crate) request_model: String,
    /// Keeps the process from being unloaded while the request runs.
    pub(crate) lease: ModelLease,
}

impl LocalModels {
    /// The engines of this runtime, either of which may be absent, and the
    /// folder holding the registry of models.
    #[must_use]
    pub fn new(
        llama: Option<Arc<LlamaServerSupervisor>>,
        mlx: Option<Arc<MlxEngineSupervisor>>,
        models_dir: Option<PathBuf>,
    ) -> Self {
        Self {
            llama,
            mlx,
            models_dir,
            settings: None,
            system_db: None,
            policy: PolicySettings::none(),
        }
    }

    /// Reads the settings `policy` fixes from it rather than from the file.
    #[must_use]
    pub fn under_policy(mut self, policy: PolicySettings) -> Self {
        self.policy = policy;
        self
    }

    /// Reads `[server] jit_loading` in `settings` and the load presets in
    /// `system_db`.
    #[must_use]
    pub fn with_files(mut self, settings: PathBuf, system_db: PathBuf) -> Self {
        self.settings = Some(settings);
        self.system_db = Some(system_db);
        self
    }

    /// Sends the lines the local `llama-server` processes print to `logs`.
    pub(crate) fn send_engine_lines_to(&self, logs: &ServerLogsHandle) {
        if let Some(llama) = &self.llama {
            llama.send_lines_to(logs.clone());
        }
    }

    /// Every resident model, under the identifier a client names it by.
    pub(crate) async fn served(&self) -> Vec<ServedModel> {
        let mut loaded = Vec::new();
        if let Some(engine) = &self.llama {
            loaded.extend(LocalEngine::loaded_models(engine.as_ref()).await);
        }
        if let Some(engine) = &self.mlx {
            loaded.extend(LocalEngine::loaded_models(engine.as_ref()).await);
        }
        let index = self.index().await;
        loaded
            .into_iter()
            .map(|model| served(model, index.as_ref()))
            .collect()
    }

    /// What `GET /v1/models` lists: every resident model and, while
    /// on-demand loading is on, every language model of the registry an
    /// engine of this machine can load.
    pub(crate) async fn listed(&self) -> Vec<ServedModel> {
        let mut listed = self.served().await;
        if !self.jit_loading() {
            return listed;
        }
        let Some(index) = self.index().await else {
            return listed;
        };
        for entry in &index.models {
            let path = entry.path.display().to_string();
            let loadable = self.engine_runs(entry.format)
                && entry.effective_category() == ModelCategory::Language;
            if loadable
                && !listed
                    .iter()
                    .any(|m| m.id == entry.id || m.model_path == path)
            {
                listed.push(ServedModel {
                    id: entry.id.clone(),
                    kind: ModelKind::Chat,
                    engine: engine_for_format(entry.format),
                    model_path: path,
                    created: entry.modified_at.map_or(0, |at| at.timestamp()),
                });
            }
        }
        listed
    }

    /// The model a client names `id`, held for one request: the resident one,
    /// else, while on-demand loading is on, the one this request loads.
    pub(crate) async fn hold(&self, id: &str) -> Result<HeldModel, Unheld> {
        if let Some(held) = self.hold_resident(id).await {
            return Ok(held);
        }
        if !self.jit_loading() {
            return Err(Unheld::NotLoaded);
        }
        self.load_on_demand(id).await
    }

    /// The resident model a client names `id`, held for one request.
    async fn hold_resident(&self, id: &str) -> Option<HeldModel> {
        let served = self.served().await.into_iter().find(|m| m.id == id)?;
        let (lease, request_model) = match served.engine {
            Engine::LlamaServer => {
                let engine = self.llama.as_ref()?;
                (
                    engine.lease_resident(&served.model_path).await?,
                    LocalEngine::request_model(engine.as_ref(), id, &served.model_path),
                )
            }
            Engine::VllmMlx => {
                let engine = self.mlx.as_ref()?;
                (
                    engine.lease_resident(&served.model_path).await?,
                    LocalEngine::request_model(engine.as_ref(), id, &served.model_path),
                )
            }
            Engine::NodeJs => return None,
        };
        Some(HeldModel {
            served,
            request_model,
            lease,
        })
    }

    /// Whether `[server] jit_loading` is on now; the default, on, when the
    /// file cannot be read.
    fn jit_loading(&self) -> bool {
        self.switch(JIT_LOADING_KEY, ServerConfig::default().jit_loading)
    }

    /// Whether `[server] separate_reasoning_content` is on now, read at every
    /// request; the default, on, when the file cannot be read.
    pub(crate) fn separates_reasoning(&self) -> bool {
        self.switch(
            SEPARATE_REASONING_KEY,
            ServerConfig::default().separate_reasoning_content,
        )
    }

    /// Whether `[server] allow_request_mcp` is on now, read at every request;
    /// the default, off, when the file cannot be read. The organisation
    /// policy that fixes it off wins over the file.
    pub(crate) fn allows_request_mcp(&self) -> bool {
        self.switch(
            ALLOW_REQUEST_MCP_KEY,
            ServerConfig::default().allow_request_mcp,
        )
    }

    /// Whether the organisation policy fixes `[server] allow_request_mcp`, so
    /// that no setting of the user turns it on.
    pub(crate) fn request_mcp_fixed_by_policy(&self) -> bool {
        self.policy.locks(ALLOW_REQUEST_MCP_KEY)
    }

    /// The boolean setting `key` of the file now, `default` when the file
    /// cannot be read or the runtime was given none.
    fn switch(&self, key: &str, default: bool) -> bool {
        if let Some(imposed) = self.policy.imposed(key) {
            return imposed.as_bool().unwrap_or(default);
        }
        let Some(path) = &self.settings else {
            return default;
        };
        match resolve_setting(path, key, &self.policy) {
            Ok(resolved) => resolved.value.as_bool().unwrap_or(default),
            Err(error) => {
                tracing::event!(
                    tracing::Level::WARN,
                    key = key,
                    error = %error,
                    "compat_server.models.setting_unreadable"
                );
                default
            }
        }
    }

    /// Whether an engine of this runtime runs a model of `format`.
    fn engine_runs(&self, format: ModelFormat) -> bool {
        match format {
            ModelFormat::Gguf => self.llama.is_some(),
            ModelFormat::Mlx => self.mlx.is_some(),
        }
    }

    /// Load the language model of the registry named `id` for this request,
    /// mark the process as the server's, and, when `[server]
    /// jit_keep_last_only` asks for it, unload the other models the server
    /// loaded.
    async fn load_on_demand(&self, id: &str) -> Result<HeldModel, Unheld> {
        let index = self.index().await.ok_or(Unheld::NotLoaded)?;
        let entry = index
            .get(id)
            .filter(|m| m.effective_category() == ModelCategory::Language)
            .ok_or(Unheld::NotLoaded)?;
        let model_path = entry.path.display().to_string();
        let backend = self.backend_row(id).await;
        let started = Utc::now();
        let (lease, request_model, loaded) = match entry.format {
            ModelFormat::Gguf => {
                let engine = self.llama.as_deref().ok_or_else(|| no_engine(id))?;
                load_with(engine, &backend, id, &model_path).await?
            }
            ModelFormat::Mlx => {
                let engine = self.mlx.as_deref().ok_or_else(|| no_engine(id))?;
                load_with(engine, &backend, id, &model_path).await?
            }
        };
        let engines = Engines {
            llama: self.llama.as_deref(),
            mlx: self.mlx.as_deref(),
        };
        if let (Some(residency), Some(model)) = (engines.residency(), &loaded) {
            if model.loaded_at >= started {
                residency.mark_loaded_by_server(&model_path, model.loaded_at);
                if residency.keep_last_server_model_only() {
                    engines
                        .unload_other_server_models(&residency, &model_path)
                        .await;
                }
            }
        }
        let served = match loaded {
            Some(model) => served(model, Some(&index)),
            None => indexed(entry, started.timestamp()),
        };
        Ok(HeldModel {
            served,
            request_model,
            lease,
        })
    }

    /// The backend row of the model `id`, written when it has none yet: the
    /// key its load preset is kept under. The identifier itself when there is
    /// no `system.db` or the row cannot be written.
    pub(crate) async fn backend_row(&self, id: &str) -> String {
        let (Some(system_db), Some(models_dir)) = (self.system_db.clone(), self.models_dir.clone())
        else {
            return id.to_owned();
        };
        let model_id = id.to_owned();
        let row = tokio::task::spawn_blocking(move || {
            let repo = LlmBackendRepository::open(&system_db).ok()?;
            local_row_for(&repo, &models_dir, &model_id).ok().flatten()
        })
        .await
        .ok()
        .flatten();
        row.unwrap_or_else(|| id.to_owned())
    }

    /// The registry of models, read at each call so a model indexed after the
    /// server opened is named by its identifier; `None` when there is none.
    async fn index(&self) -> Option<ModelIndex> {
        let path = self.models_dir.as_ref()?.join(INDEX_FILE_NAME);
        match tokio::task::spawn_blocking(move || ModelIndex::load(&path)).await {
            Ok(Ok(index)) => Some(index),
            Ok(Err(error)) => {
                tracing::event!(
                    tracing::Level::WARN,
                    error = %error,
                    "compat_server.models.index_unreadable"
                );
                None
            }
            Err(error) => {
                tracing::event!(
                    tracing::Level::WARN,
                    error = %error,
                    "compat_server.models.index_join_failed"
                );
                None
            }
        }
    }
}

/// How `model` is served: under the identifier of the registry entry that
/// owns its path, or its display name when no entry does.
fn served(model: LoadedModel, index: Option<&ModelIndex>) -> ServedModel {
    let path = Path::new(&model.model_path);
    let id = index
        .and_then(|index| {
            index.models.iter().find(|entry| {
                entry.owns(path) || entry.source_file_override.as_deref() == Some(path)
            })
        })
        .map_or(model.model, |entry| entry.id.clone());
    ServedModel {
        id,
        kind: model.kind,
        engine: model.engine,
        model_path: model.model_path,
        created: model.loaded_at.timestamp(),
    }
}

/// A model of the registry listed before its process has been seen.
fn indexed(entry: &IndexedModel, created: i64) -> ServedModel {
    ServedModel {
        id: entry.id.clone(),
        kind: ModelKind::Chat,
        engine: engine_for_format(entry.format),
        model_path: entry.path.display().to_string(),
        created,
    }
}

fn no_engine(id: &str) -> Unheld {
    Unheld::LoadFailed(format!("No local engine on this machine runs '{id}'."))
}

/// Load `model_path` on `engine` and return the lease of this request, the
/// `model` field the engine expects, and the process as the engine lists it.
async fn load_with<E: LocalEngine>(
    engine: &E,
    backend: &str,
    id: &str,
    model_path: &str,
) -> Result<(ModelLease, String, Option<LoadedModel>), Unheld> {
    let lease = engine
        .ensure_model(
            backend,
            model_path.to_owned(),
            None,
            LoadConsent::Unconfirmed,
        )
        .await
        .map_err(|error| {
            tracing::event!(
                tracing::Level::WARN,
                model = %id,
                error = %error,
                "compat_server.models.load_failed"
            );
            Unheld::LoadFailed(format!("The model '{id}' could not be loaded: {error}"))
        })?;
    let loaded = engine
        .loaded_models()
        .await
        .into_iter()
        .find(|m| m.kind == ModelKind::Chat && m.model_path == model_path);
    Ok((lease, engine.request_model(id, model_path), loaded))
}
