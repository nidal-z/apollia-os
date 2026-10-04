//! The unloads the residency rules call for, on whichever engine serves the
//! model.

use std::path::PathBuf;
use std::sync::{Arc, Weak};
use std::time::Duration;

use apollia_core::{EventBusSender, ModelUnloadReason};
use chrono::{DateTime, Utc};

use crate::llama_server::{LlamaServerSupervisor, ModelKind};
use crate::local_engine::LocalEngine;
use crate::mlx_engine::MlxEngineSupervisor;

use super::{IdleDelays, LoadOrigin, Residency};

/// Interval between two idle sweeps. The delay is counted in minutes, so a
/// model goes at most this much past it.
pub const IDLE_SWEEP_EVERY: Duration = Duration::from_secs(30);

/// Unload `model_path` from `engine` for `reason` when nothing holds it any
/// more: no pin, no open conversation, no request in progress. Returns whether
/// it was unloaded.
///
/// A model the engine does not have, or that is answering a request, is left
/// alone; in the second case the idle sweep unloads it later.
pub async fn unload_if_released<E: LocalEngine>(
    residency: &Residency,
    engine: &E,
    model_path: &str,
    reason: ModelUnloadReason,
) -> bool {
    if residency.is_held(model_path) {
        return false;
    }
    let idle =
        engine.loaded_models().await.iter().any(|m| {
            m.kind == ModelKind::Chat && m.model_path == model_path && m.active_requests == 0
        });
    if !idle {
        return false;
    }
    match engine.unload_model(model_path).await {
        Ok(_) => {
            residency.announce_unloaded(engine.engine().as_str(), model_path, reason);
            true
        }
        Err(_) => false,
    }
}

/// Unload from `engine` every chat model nothing holds and no request has used
/// for `idle` at `now`. Returns the paths unloaded.
///
/// `now` is a parameter so the delay is tested with a clock the test sets.
pub async fn unload_idle<E: LocalEngine>(
    residency: &Residency,
    engine: &E,
    now: DateTime<Utc>,
    idle: Duration,
) -> Vec<String> {
    unload_idle_by_origin(residency, engine, now, IdleDelays::uniform(Some(idle))).await
}

/// Unload from `engine` every chat model nothing holds and no request has used
/// for the delay of its origin at `now`: the compatible server's own delay for
/// a model it loaded, the general one for the others. Returns the paths
/// unloaded.
pub async fn unload_idle_by_origin<E: LocalEngine>(
    residency: &Residency,
    engine: &E,
    now: DateTime<Utc>,
    delays: IdleDelays,
) -> Vec<String> {
    let mut unloaded = Vec::new();
    for model in engine.loaded_models().await {
        let Some(idle) = delays.of(residency.origin_of(&model)) else {
            continue;
        };
        let idle = chrono::Duration::from_std(idle).unwrap_or(chrono::Duration::MAX);
        let expired = now.signed_duration_since(model.last_used_at) >= idle;
        if model.kind != ModelKind::Chat || !expired || model.active_requests > 0 {
            continue;
        }
        if unload_if_released(
            residency,
            engine,
            &model.model_path,
            ModelUnloadReason::Idle,
        )
        .await
        {
            unloaded.push(model.model_path);
        }
    }
    unloaded
}

/// Unload from `engine` every chat model the compatible server loaded, other
/// than `keep`, that nothing holds and no request is using: what keeping only
/// the last model the server loaded means. A model the application loaded is
/// never unloaded here. Returns the paths unloaded.
pub async fn unload_other_server_models<E: LocalEngine>(
    residency: &Residency,
    engine: &E,
    keep: &str,
) -> Vec<String> {
    let mut unloaded = Vec::new();
    for model in engine.loaded_models().await {
        if model.model_path == keep || residency.origin_of(&model) != LoadOrigin::Server {
            continue;
        }
        if unload_if_released(
            residency,
            engine,
            &model.model_path,
            ModelUnloadReason::ServerReplaced,
        )
        .await
        {
            unloaded.push(model.model_path);
        }
    }
    unloaded
}

/// The local engines of the runtime, either of which may be absent.
#[derive(Clone, Copy)]
pub struct Engines<'a> {
    /// `llama-server`, for GGUF models.
    pub llama: Option<&'a LlamaServerSupervisor>,
    /// `vllm-mlx`, for MLX models.
    pub mlx: Option<&'a MlxEngineSupervisor>,
}

impl Engines<'_> {
    /// The residency both engines share, `None` without an engine.
    #[must_use]
    pub fn residency(&self) -> Option<Arc<Residency>> {
        self.llama
            .and_then(LlamaServerSupervisor::residency)
            .or_else(|| self.mlx.and_then(MlxEngineSupervisor::residency))
    }

    /// Whether either engine has `model_path` resident for chat.
    pub async fn serves(&self, model_path: &str) -> bool {
        let resident = |models: Vec<crate::llama_server::LoadedModel>| {
            models
                .iter()
                .any(|m| m.kind == ModelKind::Chat && m.model_path == model_path)
        };
        if let Some(engine) = self.llama {
            if resident(LocalEngine::loaded_models(engine).await) {
                return true;
            }
        }
        match self.mlx {
            Some(engine) => resident(LocalEngine::loaded_models(engine).await),
            None => false,
        }
    }

    /// Unload from both engines the models the compatible server loaded,
    /// other than `keep`, that nothing holds and no request is using.
    pub async fn unload_other_server_models(&self, residency: &Residency, keep: &str) {
        if let Some(engine) = self.llama {
            unload_other_server_models(residency, engine, keep).await;
        }
        if let Some(engine) = self.mlx {
            unload_other_server_models(residency, engine, keep).await;
        }
    }

    /// Unload each of `models` that nothing holds any more, on whichever
    /// engine serves it.
    pub async fn unload_released(
        &self,
        residency: &Residency,
        models: &[String],
        reason: ModelUnloadReason,
    ) {
        for model in models {
            if let Some(engine) = self.llama {
                if unload_if_released(residency, engine, model, reason).await {
                    continue;
                }
            }
            if let Some(engine) = self.mlx {
                unload_if_released(residency, engine, model, reason).await;
            }
        }
    }
}

/// Sweep both engines every [`IDLE_SWEEP_EVERY`] for models left idle past the
/// delay of their origin, read now from `[llm.residency]` and `[server]`. The task ends once neither engine is
/// alive.
pub fn spawn_idle_sweep(
    residency: Arc<Residency>,
    llama: Option<&Arc<LlamaServerSupervisor>>,
    mlx: Option<&Arc<MlxEngineSupervisor>>,
) {
    let llama: Option<Weak<LlamaServerSupervisor>> = llama.map(Arc::downgrade);
    let mlx: Option<Weak<MlxEngineSupervisor>> = mlx.map(Arc::downgrade);
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(IDLE_SWEEP_EVERY);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tick.tick().await;
            let llama = llama.as_ref().and_then(Weak::upgrade);
            let mlx = mlx.as_ref().and_then(Weak::upgrade);
            if llama.is_none() && mlx.is_none() {
                return;
            }
            let delays = residency.idle_delays();
            if delays.never() {
                continue;
            }
            let now = Utc::now();
            if let Some(engine) = &llama {
                unload_idle_by_origin(&residency, engine.as_ref(), now, delays).await;
            }
            if let Some(engine) = &mlx {
                unload_idle_by_origin(&residency, engine.as_ref(), now, delays).await;
            }
        }
    });
}

/// Give both engines the residency of `system_db` and `config_file`, and start
/// the idle sweep over them. Without an engine, nothing is started.
pub fn start_residency(
    system_db: PathBuf,
    config_file: PathBuf,
    events: EventBusSender,
    llama: Option<&Arc<LlamaServerSupervisor>>,
    mlx: Option<&Arc<MlxEngineSupervisor>>,
) {
    if llama.is_none() && mlx.is_none() {
        return;
    }
    let residency = Residency::new(Some(system_db), Some(config_file), Some(events));
    if let Some(engine) = llama {
        engine.attach_residency(Arc::clone(&residency));
    }
    if let Some(engine) = mlx {
        engine.attach_residency(Arc::clone(&residency));
    }
    spawn_idle_sweep(residency, llama, mlx);
}

/// Releases every lease of one client when it is dropped: held by the event
/// stream the client keeps open, so a client that goes away, whatever the
/// reason, keeps no model loaded.
pub struct ClientLeases {
    client: String,
    residency: Arc<Residency>,
    llama: Option<Arc<LlamaServerSupervisor>>,
    mlx: Option<Arc<MlxEngineSupervisor>>,
}

impl ClientLeases {
    /// The guard of `client`'s leases, `None` without an engine.
    #[must_use]
    pub fn new(
        client: String,
        llama: Option<Arc<LlamaServerSupervisor>>,
        mlx: Option<Arc<MlxEngineSupervisor>>,
    ) -> Option<Self> {
        let residency = Engines {
            llama: llama.as_deref(),
            mlx: mlx.as_deref(),
        }
        .residency()?;
        Some(Self {
            client,
            residency,
            llama,
            mlx,
        })
    }
}

impl Drop for ClientLeases {
    fn drop(&mut self) {
        let released = self.residency.release_client(&self.client);
        tracing::event!(
            tracing::Level::INFO,
            client = %self.client,
            released = released.len(),
            "llm.residency.client_gone"
        );
        if released.is_empty() {
            return;
        }
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let residency = Arc::clone(&self.residency);
        let llama = self.llama.take();
        let mlx = self.mlx.take();
        runtime.spawn(async move {
            Engines {
                llama: llama.as_deref(),
                mlx: mlx.as_deref(),
            }
            .unload_released(&residency, &released, ModelUnloadReason::LastConversation)
            .await;
        });
    }
}
