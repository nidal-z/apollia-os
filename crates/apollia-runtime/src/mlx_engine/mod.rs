//! The MLX engine, `vllm-mlx`, supervised like `llama-server`.
//!
//! An MLX model runs on `vllm-mlx`, an OpenAI-compatible server for Apple
//! Silicon. This supervisor owns its processes the way
//! [`LlamaServerSupervisor`](crate::llama_server::LlamaServerSupervisor) owns
//! the `llama-server` ones: one process per model, on `127.0.0.1` and a free
//! port, launched on the first request and only once `/health` answers,
//! respawned with backoff after a crash, stopped at shutdown. The resident
//! models are listed and unloaded through the same types, with the engine
//! named on each.
//!
//! The process is started by the command [`MlxLaunch`] describes, which the
//! tests replace by a stand-in process answering `/health`.

mod locate;
mod room;

pub use locate::{locate, locate_for_launch, MlxLaunch, ENV_ENGINE_DIR, ENV_PYTHON};

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

use crate::engine_store::Engine;
use crate::llama_server::{LoadedModel, ModelLease, UnloadError};
use crate::local_engine::guardrails::{self, GuardrailPolicy, LoadEstimate};
use crate::local_engine::{LoadConsent, LocalEngine, LocalEngineError};

/// Context window, in tokens, for a backend that asks for none. `vllm-mlx`
/// grows its cache on demand rather than reserving a window, so this is the
/// size the router plans compaction against, capped at what the model was
/// trained for.
const DEFAULT_N_CTX: u32 = 32_768;

/// `layers on the accelerator` reported for an MLX model: all of them, since
/// MLX runs the whole model on the GPU of the unified memory.
const ALL_LAYERS: i32 = 999;

/// Timeouts and intervals of the supervision, replaced in tests.
#[derive(Debug, Clone, Copy)]
struct Timing {
    /// How long a process may take to answer `/health` after launch. Loading
    /// a model maps several gigabytes and compiles Metal kernels.
    health_timeout: Duration,
    /// Interval between `/health` polls during startup.
    health_poll: Duration,
    /// Interval between liveness checks of the running processes.
    supervision_poll: Duration,
    /// First respawn delay, doubled on each failure up to `max_backoff`.
    min_backoff: Duration,
    /// Longest respawn delay.
    max_backoff: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Self {
            health_timeout: Duration::from_secs(300),
            health_poll: Duration::from_millis(500),
            supervision_poll: Duration::from_secs(2),
            min_backoff: Duration::from_secs(1),
            max_backoff: Duration::from_secs(30),
        }
    }
}

/// One running `vllm-mlx`, serving exactly one model folder.
struct Instance {
    model_path: String,
    n_ctx: u32,
    port: u16,
    child: Child,
    last_used: u64,
    loaded_at: DateTime<Utc>,
    last_used_at: DateTime<Utc>,
    in_flight: Arc<AtomicUsize>,
    /// Footprint the load guardrail estimated at launch, in gigabytes; `None`
    /// when it was not estimated.
    footprint_gb: Option<f64>,
}

impl Instance {
    fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}/v1", self.port)
    }

    fn is_running(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    fn lease(&self) -> ModelLease {
        ModelLease::new(self.base_url(), self.n_ctx, Arc::clone(&self.in_flight))
    }

    fn loaded(&self) -> LoadedModel {
        LoadedModel {
            engine: Engine::VllmMlx,
            kind: crate::llama_server::ModelKind::Chat,
            model: display_name(&self.model_path),
            model_path: self.model_path.clone(),
            port: self.port,
            n_ctx: self.n_ctx,
            n_gpu_layers: ALL_LAYERS,
            n_parallel: None,
            loaded_at: self.loaded_at,
            last_used_at: self.last_used_at,
            active_requests: self.in_flight.load(Ordering::Acquire),
        }
    }
}

/// Display name of an MLX model: its folder name.
fn display_name(model_path: &str) -> String {
    Path::new(model_path)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| model_path.to_owned())
}

/// The training length an MLX folder declares in its `config.json`, top level
/// or under `text_config` for a multimodal conversion.
#[must_use]
pub fn trained_context(dir: &Path) -> Option<u64> {
    let text = std::fs::read_to_string(dir.join("config.json")).ok()?;
    let config: serde_json::Value = serde_json::from_str(&text).ok()?;
    config
        .get("max_position_embeddings")
        .or_else(|| config.get("text_config")?.get("max_position_embeddings"))
        .and_then(serde_json::Value::as_u64)
}

/// Supervisor of the `vllm-mlx` processes.
pub struct MlxEngineSupervisor {
    launch: MlxLaunch,
    timing: Timing,
    instances: Mutex<Vec<Instance>>,
    tick: AtomicU64,
    /// Serialises launches, so a request and the supervision task never start
    /// two processes for one model.
    respawn_lock: Mutex<()>,
    shutting_down: AtomicBool,
    /// `apollia.toml`, whose `[llm.guardrails]` is read at every launch.
    /// `None` holds every launch to the default level.
    config_file: Option<PathBuf>,
    /// The leases and pins that keep a model loaded, shared with the other
    /// engine. Unset, every resident model may be unloaded to make room.
    residency: std::sync::OnceLock<Arc<crate::residency::Residency>>,
    /// The guardrail settings the organisation policy fixes, over the file.
    imposed: apollia_core::config::PolicySettings,
    /// The window each model gets from the default of the settings, kept so a
    /// request to a resident model does not read its folder again.
    sized_windows: Arc<crate::local_engine::context_default::SizedWindows>,
}

impl MlxEngineSupervisor {
    /// A supervisor that starts processes with `launch`, without starting any,
    /// and holds them to the load guardrail of `config_file`, `apollia.toml`.
    /// Call [`spawn_supervision`](Self::spawn_supervision) for the respawns.
    #[must_use]
    pub fn new(
        launch: MlxLaunch,
        config_file: Option<PathBuf>,
        imposed: apollia_core::config::PolicySettings,
    ) -> Arc<Self> {
        let mut engine = Self::with_timing(launch, Timing::default());
        engine.config_file = config_file;
        engine.imposed = imposed;
        Arc::new(engine)
    }

    /// The engine of this installation, on a Mac with Apple Silicon only,
    /// holding its launches to the guardrail of `config_file` and to the
    /// guardrail settings the organisation policy fixes in `imposed`.
    ///
    /// `None` elsewhere, and when the build carries no engine: an MLX model is
    /// then refused with its reason by the router factory.
    #[must_use]
    pub fn for_this_machine(
        config_file: Option<PathBuf>,
        imposed: apollia_core::config::PolicySettings,
    ) -> Option<Arc<Self>> {
        if !crate::local_engine::is_apple_silicon() {
            return None;
        }
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(Path::to_path_buf));
        let store_root =
            apollia_core::paths::data_dir().map(|d| crate::engine_store::store_root(&d));
        let choices = crate::engine_store::EngineChoices::read(config_file.as_deref(), &imposed);
        let launch = locate_for_launch(
            exe_dir.as_deref(),
            store_root.as_deref(),
            choices.mlx,
            |var| std::env::var(var).ok(),
        )?;
        Some(Self::new(launch, config_file, imposed))
    }

    fn with_timing(launch: MlxLaunch, timing: Timing) -> Self {
        Self {
            launch,
            timing,
            instances: Mutex::new(Vec::new()),
            tick: AtomicU64::new(0),
            respawn_lock: Mutex::new(()),
            shutting_down: AtomicBool::new(false),
            config_file: None,
            residency: std::sync::OnceLock::new(),
            imposed: apollia_core::config::PolicySettings::none(),
            sized_windows: Arc::default(),
        }
    }

    fn next_tick(&self) -> u64 {
        self.tick.fetch_add(1, Ordering::Relaxed)
    }

    /// The window a model is served with: the request's, else the default,
    /// capped at what the folder declares it was trained for.
    pub(crate) fn window(model_path: &str, requested: Option<u32>) -> u32 {
        let wanted = requested.filter(|n| *n > 0).unwrap_or(DEFAULT_N_CTX);
        match trained_context(Path::new(model_path)) {
            Some(trained) => u32::try_from(trained).map_or(wanted, |t| wanted.min(t.max(1))),
            None => wanted,
        }
    }

    /// The window `model_path` is launched with: the one asked for, else the
    /// default of the settings (`[llm.defaults]`), else the built-in one,
    /// capped at what the folder declares it was trained for.
    pub async fn window_for(&self, model_path: &str, requested: Option<u32>) -> u32 {
        if requested.is_some_and(|n| n > 0) {
            return Self::window(model_path, requested);
        }
        self.sized_windows
            .window(
                self.config_file.clone(),
                self.imposed.clone(),
                DEFAULT_N_CTX,
                model_path,
            )
            .await
    }

    /// A lease on the live process of `model_path`, marked as just used. A
    /// dead process is dropped, so the caller launches instead.
    async fn touch_live_instance(&self, model_path: &str) -> Option<ModelLease> {
        let mut instances = self.instances.lock().await;
        let idx = instances.iter().position(|i| i.model_path == model_path)?;
        if !instances[idx].is_running() {
            let dead = instances.remove(idx);
            tracing::event!(
                tracing::Level::WARN,
                model = %dead.model_path,
                port = dead.port,
                "mlx.engine.instance.dead"
            );
            return None;
        }
        instances[idx].last_used = self.next_tick();
        instances[idx].last_used_at = Utc::now();
        Some(instances[idx].lease())
    }

    /// Make `model_path` servable and return a lease on its process.
    ///
    /// A launch is first held to the load guardrail, the resident models
    /// counted; the ones nothing holds are stopped to make room when the sum
    /// is refused (see [`crate::residency::room`]). `consent` says whether the
    /// user confirmed a launch the guardrail would refuse.
    ///
    /// # Errors
    ///
    /// [`LocalEngineError::GuardrailRefused`] when the guardrail refuses the
    /// model and the user did not confirm; [`LocalEngineError::Launch`] when
    /// the process cannot be started or never answers `/health`.
    pub async fn ensure_model(
        &self,
        model_path: String,
        n_ctx: Option<u32>,
        consent: LoadConsent,
    ) -> Result<ModelLease, LocalEngineError> {
        let _guard = self.respawn_lock.lock().await;
        if let Some(lease) = self.touch_live_instance(&model_path).await {
            return Ok(lease);
        }
        let n_ctx = self.window_for(&model_path, n_ctx).await;
        let footprint_gb = self.admit(&model_path, n_ctx, consent).await?;
        let lease = self.spawn_instance(model_path, n_ctx).await?;
        self.record_footprint(lease.base_url(), footprint_gb).await;
        Ok(lease)
    }

    /// Hold the launch of `model_path` at `n_ctx` to the guardrail in force
    /// now, read from `apollia.toml` at every call, with the resident models
    /// counted. One sequence, as served. Returns the footprint judged, in
    /// gigabytes.
    async fn admit(
        &self,
        model_path: &str,
        n_ctx: u32,
        consent: LoadConsent,
    ) -> Result<Option<f64>, LocalEngineError> {
        let config_file = self.config_file.clone();
        let imposed = self.imposed.clone();
        let folder = model_path.to_owned();
        let sized = tokio::task::spawn_blocking(move || {
            let policy = GuardrailPolicy::read_under(config_file.as_deref(), &imposed);
            if policy.level == guardrails::GuardrailLevel::Disabled {
                return (policy, None);
            }
            let shape = apollia_llm::recommend::RuntimeShape {
                n_ctx,
                n_parallel: 1,
                ..apollia_llm::recommend::RuntimeShape::default()
            };
            let estimate =
                LoadEstimate::mlx(Path::new(&folder), &shape, &apollia_llm::hardware::detect());
            (policy, estimate)
        })
        .await;
        // A panicked estimate is no estimate: the launch goes ahead, as for a
        // folder without its configuration.
        let Ok((policy, estimate)) = sized else {
            return Ok(None);
        };
        self.judge_with_residents(&policy, estimate, consent, model_path)
            .await
            .map_err(LocalEngineError::from)
    }

    /// Launch a process for `model_path` and wait until it is healthy.
    async fn spawn_instance(
        &self,
        model_path: String,
        n_ctx: u32,
    ) -> Result<ModelLease, LocalEngineError> {
        let launch_err = |reason: String| LocalEngineError::Launch {
            engine: Engine::VllmMlx.as_str(),
            reason,
        };
        let port = crate::llama_server::pick_free_port()
            .map_err(|e| launch_err(format!("no free loopback port: {e}")))?;
        let args = self.launch.args(&model_path, port);
        tracing::event!(
            tracing::Level::INFO,
            program = %self.launch.program.display(),
            model = %model_path,
            port,
            args = %args.join(" "),
            "mlx.engine.spawn"
        );

        let mut command = Command::new(&self.launch.program);
        apollia_core::subprocess_window::hide_console_async(&mut command);
        apollia_core::subprocess_env::scrub_bundled_python_async(&mut command);
        let mut child = command
            .args(&args)
            .envs(
                self.launch
                    .env
                    .iter()
                    .map(|(k, v)| (k.as_str(), v.as_str())),
            )
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| launch_err(format!("cannot start the engine: {e}")))?;
        if let Some(stdout) = child.stdout.take() {
            tokio::spawn(drain_pipe(BufReader::new(stdout)));
        }
        if let Some(stderr) = child.stderr.take() {
            tokio::spawn(drain_pipe(BufReader::new(stderr)));
        }

        self.wait_until_healthy(&mut child, port)
            .await
            .map_err(launch_err)?;

        let now = Utc::now();
        let instance = Instance {
            model_path,
            n_ctx,
            port,
            child,
            last_used: self.next_tick(),
            loaded_at: now,
            last_used_at: now,
            in_flight: Arc::new(AtomicUsize::new(0)),
            footprint_gb: None,
        };
        let lease = instance.lease();
        tracing::event!(
            tracing::Level::INFO,
            port,
            model = %instance.model_path,
            "mlx.engine.healthy"
        );
        self.instances.lock().await.push(instance);
        Ok(lease)
    }

    /// Poll `GET /health` until it answers healthy, the child exits, or the
    /// deadline passes. Returns the reason on failure.
    async fn wait_until_healthy(&self, child: &mut Child, port: u16) -> Result<(), String> {
        let url = format!("http://127.0.0.1:{port}/health");
        let client = apollia_core::net::loopback_client_builder()
            .timeout(Duration::from_secs(2))
            .build()
            .map_err(|e| format!("http client: {e}"))?;
        let deadline = tokio::time::Instant::now() + self.timing.health_timeout;
        loop {
            if let Ok(Some(status)) = child.try_wait() {
                return Err(format!("the engine exited during startup ({status})"));
            }
            if let Ok(resp) = client.get(&url).send().await {
                if resp.status().is_success() {
                    let body = resp.text().await.unwrap_or_default();
                    if !body.contains("\"unhealthy\"") {
                        return Ok(());
                    }
                }
            }
            if tokio::time::Instant::now() >= deadline {
                let _ = child.kill().await;
                return Err(format!(
                    "the engine did not become healthy within {}s",
                    self.timing.health_timeout.as_secs()
                ));
            }
            tokio::time::sleep(self.timing.health_poll).await;
        }
    }

    /// Remove every process that has exited, returning what each served.
    async fn take_dead_instances(&self) -> Vec<(String, u32, String)> {
        let mut instances = self.instances.lock().await;
        let mut dead = Vec::new();
        instances.retain_mut(|i| match i.child.try_wait() {
            Ok(None) => true,
            Ok(Some(status)) => {
                dead.push((i.model_path.clone(), i.n_ctx, status.to_string()));
                false
            }
            Err(e) => {
                dead.push((
                    i.model_path.clone(),
                    i.n_ctx,
                    format!("try_wait failed: {e}"),
                ));
                false
            }
        });
        dead
    }

    /// Watch the processes and respawn one that dies, with backoff. An evicted
    /// or unloaded model is not in the list, so it never comes back alone.
    pub fn spawn_supervision(self: Arc<Self>) {
        tokio::spawn(async move {
            let timing = self.timing;
            let mut backoff = timing.min_backoff;
            let mut owed: Vec<(String, u32)> = Vec::new();
            loop {
                if self.shutting_down.load(Ordering::SeqCst) {
                    return;
                }
                for (model_path, n_ctx, reason) in self.take_dead_instances().await {
                    tracing::event!(
                        tracing::Level::ERROR,
                        model = %model_path,
                        reason = %reason,
                        "mlx.engine.exited"
                    );
                    if !owed.iter().any(|(p, _)| p == &model_path) {
                        owed.push((model_path, n_ctx));
                    }
                }
                if owed.is_empty() {
                    backoff = timing.min_backoff;
                    tokio::time::sleep(timing.supervision_poll).await;
                    continue;
                }
                tokio::time::sleep(backoff).await;
                if self.shutting_down.load(Ordering::SeqCst) {
                    return;
                }
                let _guard = self.respawn_lock.lock().await;
                let mut still_owed = Vec::new();
                for (model_path, n_ctx) in owed.drain(..) {
                    if self.touch_live_instance(&model_path).await.is_some() {
                        continue;
                    }
                    match self.spawn_instance(model_path.clone(), n_ctx).await {
                        Ok(_) => tracing::event!(
                            tracing::Level::INFO,
                            model = %model_path,
                            "mlx.engine.respawned"
                        ),
                        Err(e) => {
                            tracing::event!(
                                tracing::Level::ERROR,
                                model = %model_path,
                                error = %e,
                                "mlx.engine.respawn.failed"
                            );
                            still_owed.push((model_path, n_ctx));
                        }
                    }
                }
                backoff = if still_owed.is_empty() {
                    timing.min_backoff
                } else {
                    (backoff * 2).min(timing.max_backoff)
                };
                owed = still_owed;
            }
        });
    }

    /// A lease on the live process already serving `model_path`, marked as
    /// just used; `None` when none does. Never launches anything.
    pub async fn lease_resident(&self, model_path: &str) -> Option<ModelLease> {
        self.touch_live_instance(model_path).await
    }

    /// Every model with a live process, most recently used first.
    pub async fn loaded_models(&self) -> Vec<LoadedModel> {
        let mut instances = self.instances.lock().await;
        let mut live: Vec<(u64, LoadedModel)> = instances
            .iter_mut()
            .filter_map(|i| i.is_running().then(|| (i.last_used, i.loaded())))
            .collect();
        live.sort_by_key(|(tick, _)| std::cmp::Reverse(*tick));
        live.into_iter().map(|(_, m)| m).collect()
    }

    /// Stop the process serving `model_path`, refused while it answers.
    ///
    /// # Errors
    ///
    /// [`UnloadError::NotLoaded`] when nothing serves the path,
    /// [`UnloadError::InUse`] while a request holds a lease on it.
    pub async fn unload_model(&self, model_path: &str) -> Result<usize, UnloadError> {
        let _guard = self.respawn_lock.lock().await;
        let mut instances = self.instances.lock().await;
        if !instances.iter().any(|i| i.model_path == model_path) {
            return Err(UnloadError::NotLoaded {
                model_path: model_path.to_owned(),
            });
        }
        let active_requests: usize = instances
            .iter()
            .filter(|i| i.model_path == model_path)
            .map(|i| i.in_flight.load(Ordering::Acquire))
            .sum();
        if active_requests > 0 {
            return Err(UnloadError::InUse {
                model_path: model_path.to_owned(),
                active_requests,
            });
        }
        let (stopping, kept): (Vec<_>, Vec<_>) = std::mem::take(&mut *instances)
            .into_iter()
            .partition(|i| i.model_path == model_path);
        *instances = kept;
        let stopped = stopping.len();
        for mut instance in stopping {
            let _ = instance.child.kill().await;
            let _ = instance.child.wait().await;
            tracing::event!(
                tracing::Level::INFO,
                model = %instance.model_path,
                port = instance.port,
                "mlx.engine.instance.unloaded"
            );
        }
        Ok(stopped)
    }

    /// Stop every process and the respawns, without consuming the supervisor.
    pub async fn shutdown_in_place(&self) {
        self.shutting_down.store(true, Ordering::SeqCst);
        let _guard = self.respawn_lock.lock().await;
        for mut instance in std::mem::take(&mut *self.instances.lock().await) {
            let _ = instance.child.kill().await;
            let _ = instance.child.wait().await;
        }
    }
}

impl LocalEngine for MlxEngineSupervisor {
    fn engine(&self) -> Engine {
        Engine::VllmMlx
    }

    fn launch_window(&self, _backend: &str, model_path: &str, requested: Option<u32>) -> u32 {
        if requested.is_some_and(|n| n > 0) {
            return Self::window(model_path, requested);
        }
        self.sized_windows.window_blocking(
            self.config_file.as_deref(),
            &self.imposed,
            DEFAULT_N_CTX,
            model_path,
        )
    }

    async fn ensure_model(
        &self,
        _backend: &str,
        model_path: String,
        n_ctx: Option<u32>,
        consent: LoadConsent,
    ) -> Result<ModelLease, LocalEngineError> {
        MlxEngineSupervisor::ensure_model(self, model_path, n_ctx, consent).await
    }

    async fn loaded_models(&self) -> Vec<LoadedModel> {
        MlxEngineSupervisor::loaded_models(self).await
    }

    async fn unload_model(&self, model_path: &str) -> Result<usize, UnloadError> {
        MlxEngineSupervisor::unload_model(self, model_path).await
    }

    fn max_loaded(&self) -> Option<usize> {
        None
    }

    fn reads_llama_cpp_grammar(&self) -> bool {
        false
    }

    fn request_model(&self, _model_id: &str, model_path: &str) -> String {
        model_path.to_owned()
    }
}

/// Forward a child pipe line by line into the daemon's log, never stopping on
/// a decoding error, so the pipe buffer never fills and stalls the engine.
async fn drain_pipe<R: AsyncBufRead + Unpin>(mut reader: R) {
    let mut buf = Vec::with_capacity(256);
    loop {
        buf.clear();
        match reader.read_until(b'\n', &mut buf).await {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                let line = String::from_utf8_lossy(&buf);
                let line = line.trim_end_matches(['\r', '\n']);
                tracing::event!(
                    target: "vllm-mlx",
                    tracing::Level::INFO,
                    line = %line,
                    "mlx.engine.line"
                );
            }
        }
    }
}

#[cfg(test)]
mod tests;
