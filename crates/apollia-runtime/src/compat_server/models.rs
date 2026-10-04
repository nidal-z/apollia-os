//! The local models the compatible server relays to: those the user loaded,
//! named by their identifier in the model registry.
//!
//! Only a model resident in a local engine is served. A model of a remote
//! backend has no process on this machine, so it is never listed and never
//! relayed: the compatible server is not a relay to a cloud. Nothing is
//! loaded on demand here either; a model that is not resident answers
//! `model_not_loaded`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use apollia_llm::model_index::{ModelIndex, INDEX_FILE_NAME};

use super::logs::ServerLogsHandle;
use crate::engine_store::Engine;
use crate::llama_server::{LlamaServerSupervisor, LoadedModel, ModelKind, ModelLease};
use crate::local_engine::LocalEngine;
use crate::mlx_engine::MlxEngineSupervisor;

/// The local engines the compatible server reads, and where the registry of
/// models lives.
#[derive(Clone, Default)]
pub struct LocalModels {
    llama: Option<Arc<LlamaServerSupervisor>>,
    mlx: Option<Arc<MlxEngineSupervisor>>,
    models_dir: Option<PathBuf>,
}

impl std::fmt::Debug for LocalModels {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LocalModels")
            .field("llama", &self.llama.is_some())
            .field("mlx", &self.mlx.is_some())
            .field("models_dir", &self.models_dir)
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
        }
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

    /// The resident model a client names `id`, held for one request.
    pub(crate) async fn hold(&self, id: &str) -> Option<HeldModel> {
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
