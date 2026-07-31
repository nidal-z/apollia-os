//! Embedded `llama-server` (upstream llama.cpp) as the local LLM engine.
//!
//! Apollia's own `apollia-runner` sidecar links `llama-cpp-2`, whose last
//! release exposing the OpenAI-compatible tool-calling template API is pinned at
//! `0.1.146`; newer llama.cpp architectures (for example the Gated Delta Net
//! hybrids) are therefore out of reach and abort the runner at load. Running the
//! upstream `llama-server` binary instead tracks llama.cpp directly: broader
//! model coverage, continuous batching, and native tool calling through
//! `--jinja`, all behind the same OpenAI-compatible HTTP surface the
//! `apollia-llm` OpenAI backend already speaks.
//!
//! This supervisor owns the `llama-server` child processes: it picks a loopback
//! port, launches the binary, waits for `/health`, and respawns a process if it
//! dies. A process serves exactly one model, so serving several means running
//! several. [`ENV_MAX_LOADED`] caps how many stay resident, defaulting to one,
//! which reproduces the historical behaviour of stopping the running server on
//! every model change. Whisper STT still lives in `apollia-runner`; only the LLM
//! engine moves here.

mod config;

pub use config::{FlashAttn, LlamaServerConfig, ParseFlashAttnError};

use std::net::TcpListener;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use thiserror::Error;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

use config::{build_args, display_opt, env_getter, resolve_env_overrides};

/// Errors from spawning or supervising the embedded `llama-server`.
#[derive(Debug, Error)]
pub enum LlamaServerError {
    /// The `llama-server` binary was not found next to the executable or on PATH.
    #[error("llama-server binary not found: {0}")]
    BinaryNotFound(String),

    /// I/O error while spawning or communicating with the process.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// The server did not answer `/health` within the allotted time.
    #[error("llama-server did not become healthy within {timeout_secs}s")]
    HealthTimeout { timeout_secs: u64 },

    /// The process exited before it became healthy.
    #[error("llama-server exited during startup (status: {0})")]
    ExitedDuringStartup(String),
}

/// One running `llama-server`, serving exactly one model.
///
/// A process serves a single model: `-m` is fixed at launch. Serving a second
/// model therefore means a second process, not a reconfiguration of this one.
struct Instance {
    model_path: String,
    port: u16,
    child: Child,
    /// Value of the supervisor's tick when this instance was last requested.
    /// Orders eviction; a wall clock would not, since two requests inside the
    /// same millisecond are common.
    last_used: u64,
}

impl Instance {
    fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}/v1", self.port)
    }

    /// Whether the child is still alive (has not exited).
    fn is_running(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }
}

/// Environment variable setting how many models stay resident at once.
const ENV_MAX_LOADED: &str = "APOLLIA_LLAMA_MAX_LOADED";

/// Default number of resident models.
///
/// One, deliberately. Every additional resident model holds its weights in
/// memory for as long as it stays loaded, and the right ceiling depends on the
/// machine and on which models an installation actually alternates between.
/// Auto-sizing it from total memory would silently commit gigabytes on the
/// operator's behalf, so raising it is an explicit act.
const DEFAULT_MAX_LOADED: usize = 1;

/// Resolve [`ENV_MAX_LOADED`], floored at one.
///
/// A zero or unparseable value keeps the default rather than disabling local
/// inference outright: a typo in an environment variable must not be the reason
/// no model can load.
fn resolve_max_loaded(get: impl Fn(&str) -> Option<String>) -> usize {
    match get(ENV_MAX_LOADED) {
        None => DEFAULT_MAX_LOADED,
        Some(raw) => match raw.trim().parse::<usize>() {
            Ok(n) if n >= 1 => n,
            _ => {
                tracing::warn!(
                    var = ENV_MAX_LOADED,
                    value = %raw,
                    "llama.server.max_loaded.invalid"
                );
                DEFAULT_MAX_LOADED
            }
        },
    }
}

/// Timeout for the initial (and post-respawn) `/health` handshake. Model load
/// compiles GPU kernels and maps a multi-gigabyte file, so it is generous.
const HEALTH_TIMEOUT_SECS: u64 = 180;

/// Interval between `/health` polls while waiting for startup.
const HEALTH_POLL_INTERVAL: Duration = Duration::from_millis(500);

/// Interval between liveness checks of the running child.
const SUPERVISION_POLL: Duration = Duration::from_secs(2);

/// Supervisor that owns the embedded `llama-server` child processes.
///
/// Holds up to [`ENV_MAX_LOADED`] models resident at once, evicting the least
/// recently used beyond that. With the default of one this is the historical
/// behaviour exactly: every model change stops the running process and starts
/// another. Raising it trades memory for hand-off latency, which is the whole
/// cost of a fleet of agents that do not share a model.
///
/// Cloneable state lives behind `Arc`; the supervisor itself is held behind an
/// `Arc` by the runtime so the supervision task can respawn a dead child.
pub struct LlamaServerSupervisor {
    bin_path: PathBuf,
    /// Context window the server is launched with (`-c`), reported to the router
    /// as the model's usable window so it can size compaction. Immutable: every
    /// (re)spawn uses the same value.
    n_ctx: u32,
    /// How many models may stay resident simultaneously. At least one.
    max_loaded: usize,
    /// Launch configuration shared by every instance. Its `model_path` is a
    /// template slot: the real path is supplied per instance at spawn.
    config: Arc<Mutex<LlamaServerConfig>>,
    /// Running instances, most-recently-used ordering carried by `last_used`.
    instances: Arc<Mutex<Vec<Instance>>>,
    /// Monotonic counter feeding `Instance::last_used`.
    tick: Arc<AtomicU64>,
    /// Serialises (re)spawns so [`ensure_model`](Self::ensure_model) and the
    /// supervision task never launch two servers at once.
    respawn_lock: Arc<Mutex<()>>,
    /// Set during shutdown so the supervision task stops respawning.
    shutting_down: Arc<Mutex<bool>>,
}

impl LlamaServerSupervisor {
    /// Create the supervisor without launching anything.
    ///
    /// Locates the `llama-server` binary (returning [`LlamaServerError::BinaryNotFound`]
    /// when it is absent, so the caller can treat local inference as unavailable)
    /// but does not spawn a process. A server starts lazily on the first
    /// [`ensure_model`](Self::ensure_model), so a fresh install with no model yet
    /// still yields a usable supervisor that the router factory can capture.
    /// Call [`spawn_supervision`](Self::spawn_supervision) to enable auto-respawn.
    pub fn new(config: LlamaServerConfig) -> Result<Arc<Self>, LlamaServerError> {
        let bin_path = locate_llama_server_binary()?;
        // The reported window must be the one the server will actually launch
        // with, otherwise an `APOLLIA_LLAMA_N_CTX` override would leave the
        // router sizing compaction against a value no process ever used. The
        // resolution is pure and applied to the same base at spawn, so both
        // agree.
        let n_ctx = resolve_env_overrides(&config, env_getter).n_ctx;
        let max_loaded = resolve_max_loaded(env_getter);
        Ok(Arc::new(Self {
            bin_path,
            n_ctx,
            max_loaded,
            config: Arc::new(Mutex::new(config)),
            instances: Arc::new(Mutex::new(Vec::new())),
            tick: Arc::new(AtomicU64::new(0)),
            respawn_lock: Arc::new(Mutex::new(())),
            shutting_down: Arc::new(Mutex::new(false)),
        }))
    }

    /// Context window (`-c`) the server is launched with, in tokens.
    pub fn n_ctx(&self) -> u32 {
        self.n_ctx
    }

    /// Number of models kept resident at once.
    pub fn max_loaded(&self) -> usize {
        self.max_loaded
    }

    /// Make `model_path` servable and return the base URL that serves it.
    ///
    /// Returns immediately when that model already has a live process, which is
    /// what makes a second resident model worth having: the hand-off between two
    /// agents on two models costs a lookup rather than a reload. Otherwise a
    /// process is started, evicting the least recently used one first when the
    /// residency ceiling is reached.
    ///
    /// The URL is returned rather than read back through a separate accessor:
    /// with several instances alive, "the current one" is not a well-defined
    /// question, and a caller that asked for a specific model must not receive
    /// another one's port.
    ///
    /// # Errors
    ///
    /// [`LlamaServerError`] when the process cannot be spawned or never becomes
    /// healthy.
    pub async fn ensure_model(&self, model_path: String) -> Result<String, LlamaServerError> {
        let _guard = self.respawn_lock.lock().await;
        if let Some(url) = self.touch_live_instance(&model_path).await {
            return Ok(url);
        }
        self.evict_until_below_ceiling().await;
        self.spawn_instance(model_path).await
    }

    /// Return the base URL of a live instance for `model_path`, marking it as
    /// just used. Drops an instance whose process has died, so the caller
    /// respawns instead of handing out a dead port.
    async fn touch_live_instance(&self, model_path: &str) -> Option<String> {
        let mut instances = self.instances.lock().await;
        let idx = instances.iter().position(|i| i.model_path == model_path)?;
        if !instances[idx].is_running() {
            let dead = instances.remove(idx);
            tracing::warn!(model = %dead.model_path, port = dead.port, "llama.server.instance.dead");
            return None;
        }
        instances[idx].last_used = self.next_tick();
        Some(instances[idx].base_url())
    }

    /// Stop least-recently-used instances until one more fits under the ceiling.
    async fn evict_until_below_ceiling(&self) {
        let mut instances = self.instances.lock().await;
        while instances.len() >= self.max_loaded {
            let Some(idx) = instances
                .iter()
                .enumerate()
                .min_by_key(|(_, i)| i.last_used)
                .map(|(idx, _)| idx)
            else {
                return;
            };
            let mut evicted = instances.remove(idx);
            tracing::info!(
                model = %evicted.model_path,
                port = evicted.port,
                "llama.server.instance.evicted"
            );
            let _ = evicted.child.kill().await;
            let _ = evicted.child.wait().await;
        }
    }

    /// Next value of the monotonic use counter.
    fn next_tick(&self) -> u64 {
        self.tick.fetch_add(1, Ordering::Relaxed)
    }

    /// Watch the child and respawn it if it dies unexpectedly.
    ///
    /// `llama-server` can exit on a fatal load error; without this the endpoint
    /// would stay dead until the app restarts. Mirrors the runner supervisor's
    /// approach: poll liveness, drop the stale handle, respawn with backoff.
    pub fn spawn_supervision(self: Arc<Self>) {
        tokio::spawn(async move {
            const MIN_BACKOFF: Duration = Duration::from_secs(1);
            const MAX_BACKOFF: Duration = Duration::from_secs(30);
            let mut backoff = MIN_BACKOFF;
            // Models owed a respawn: newly dead ones, plus any whose respawn has
            // not succeeded yet. Carried across iterations so a failure is
            // retried under backoff instead of being forgotten, which is what
            // keeps an endpoint alive between two requests.
            let mut owed: Vec<String> = Vec::new();

            loop {
                if *self.shutting_down.lock().await {
                    return;
                }

                // An instance is only listed once it has existed, so a fresh
                // install with no model yet has nothing to supervise and never
                // launches llama-server without a `-m`. An evicted model is
                // absent from this list on purpose: it was stopped deliberately
                // and must not come back on its own.
                for (model_path, reason) in self.take_dead_instances().await {
                    tracing::error!(
                        model = %model_path,
                        reason = %reason,
                        "llama-server exited, respawning"
                    );
                    if !owed.contains(&model_path) {
                        owed.push(model_path);
                    }
                }

                if owed.is_empty() {
                    backoff = MIN_BACKOFF;
                    tokio::time::sleep(SUPERVISION_POLL).await;
                    continue;
                }

                tokio::time::sleep(backoff).await;
                if *self.shutting_down.lock().await {
                    return;
                }

                // Serialise with ensure_model so two spawns never race.
                let _guard = self.respawn_lock.lock().await;
                let mut still_owed = Vec::new();
                for model_path in owed.drain(..) {
                    // A caller may have re-requested this model while we waited
                    // for the lock, in which case it is live again.
                    if self.touch_live_instance(&model_path).await.is_some() {
                        continue;
                    }
                    match self.spawn_instance(model_path.clone()).await {
                        Ok(_) => tracing::info!(model = %model_path, "llama-server respawned"),
                        Err(e) => {
                            tracing::error!(
                                model = %model_path,
                                error = %e,
                                "llama-server respawn failed"
                            );
                            still_owed.push(model_path);
                        }
                    }
                }
                backoff = if still_owed.is_empty() {
                    MIN_BACKOFF
                } else {
                    (backoff * 2).min(MAX_BACKOFF)
                };
                owed = still_owed;
            }
        });
    }

    /// Stop every server without consuming the supervisor (for the exit hook).
    pub async fn shutdown_in_place(&self) {
        *self.shutting_down.lock().await = true;
        let _guard = self.respawn_lock.lock().await;
        for mut instance in std::mem::take(&mut *self.instances.lock().await) {
            let _ = instance.child.kill().await;
            let _ = instance.child.wait().await;
        }
    }

    /// Remove every instance whose process has exited, returning what they were
    /// serving and why they are gone, so the caller can respawn them.
    async fn take_dead_instances(&self) -> Vec<(String, String)> {
        let mut instances = self.instances.lock().await;
        let mut dead = Vec::new();
        instances.retain_mut(|instance| match instance.child.try_wait() {
            Ok(None) => true,
            Ok(Some(status)) => {
                dead.push((instance.model_path.clone(), status.to_string()));
                false
            }
            Err(e) => {
                dead.push((instance.model_path.clone(), format!("try_wait failed: {e}")));
                false
            }
        });
        dead
    }

    /// Spawn `llama-server` for `model_path` and wait until it is healthy.
    ///
    /// Picks a free loopback port, resolves the `APOLLIA_LLAMA_` overrides onto
    /// the stored configuration, launches, drains the logs into tracing, and
    /// polls `/health`. Returns the base URL that serves the model.
    async fn spawn_instance(&self, model_path: String) -> Result<String, LlamaServerError> {
        let mut config = resolve_env_overrides(&*self.config.lock().await, env_getter);
        config.model_path = model_path;
        let port = pick_free_port()?;
        let args = build_args(&config, port);

        tracing::info!(
            binary = %self.bin_path.display(),
            model = %config.model_path,
            port,
            n_ctx = config.n_ctx,
            n_gpu_layers = config.n_gpu_layers,
            n_batch = %display_opt(config.n_batch.as_ref()),
            n_ubatch = %display_opt(config.n_ubatch.as_ref()),
            n_parallel = %display_opt(config.n_parallel.as_ref()),
            cont_batching = %display_opt(config.cont_batching.as_ref()),
            cache_type_k = %display_opt(config.cache_type_k.as_ref()),
            cache_type_v = %display_opt(config.cache_type_v.as_ref()),
            flash_attn = %display_opt(config.flash_attn.as_ref()),
            cache_reuse = %display_opt(config.cache_reuse.as_ref()),
            // The full vector is the provenance record a measurement campaign
            // quotes, so it is logged verbatim alongside the individual fields.
            args = %args.join(" "),
            "llama.server.spawn.config"
        );

        // Publish the resolved configuration so a turn record can quote the
        // command line it was measured under. Two decompositions taken with
        // different launch flags are not comparable, and only the recorded
        // vector reveals that.
        crate::perf_trace::set_launch_config(serde_json::json!({
            "model_path": config.model_path,
            "n_ctx": config.n_ctx,
            "n_gpu_layers": config.n_gpu_layers,
            "n_batch": config.n_batch,
            "n_ubatch": config.n_ubatch,
            "n_parallel": config.n_parallel,
            "cont_batching": config.cont_batching,
            "cache_type_k": config.cache_type_k,
            "cache_type_v": config.cache_type_v,
            "flash_attn": config.flash_attn.map(|m| m.to_string()),
            "cache_reuse": config.cache_reuse,
            "args": args,
        }));

        let mut child = Command::new(&self.bin_path)
            .args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| {
                tracing::error!(error = %e, "failed to spawn llama-server");
                LlamaServerError::Io(e)
            })?;

        // Drain the child's pipes into tracing so a load failure is visible and
        // the OS pipe buffer never fills (which would stall the load).
        if let Some(stdout) = child.stdout.take() {
            tokio::spawn(drain_pipe(BufReader::new(stdout)));
        }
        if let Some(stderr) = child.stderr.take() {
            tokio::spawn(drain_pipe(BufReader::new(stderr)));
        }

        self.wait_until_healthy(&mut child, port).await?;

        let instance = Instance {
            model_path: config.model_path,
            port,
            child,
            last_used: self.next_tick(),
        };
        let url = instance.base_url();
        tracing::info!(port, model = %instance.model_path, "llama-server healthy");
        self.instances.lock().await.push(instance);
        Ok(url)
    }

    /// Poll `GET /health` until the server answers or the deadline elapses,
    /// aborting early if the child exits first.
    async fn wait_until_healthy(
        &self,
        child: &mut Child,
        port: u16,
    ) -> Result<(), LlamaServerError> {
        let url = format!("http://127.0.0.1:{port}/health");
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(2))
            .build()
            .map_err(|e| LlamaServerError::ExitedDuringStartup(format!("http client: {e}")))?;

        let deadline = tokio::time::Instant::now() + Duration::from_secs(HEALTH_TIMEOUT_SECS);
        loop {
            if let Ok(Some(status)) = child.try_wait() {
                let _ = child.kill().await;
                return Err(LlamaServerError::ExitedDuringStartup(status.to_string()));
            }
            if let Ok(resp) = client.get(&url).send().await {
                if resp.status().is_success() {
                    return Ok(());
                }
            }
            if tokio::time::Instant::now() >= deadline {
                let _ = child.kill().await;
                return Err(LlamaServerError::HealthTimeout {
                    timeout_secs: HEALTH_TIMEOUT_SECS,
                });
            }
            tokio::time::sleep(HEALTH_POLL_INTERVAL).await;
        }
    }
}

/// Reserve a free loopback TCP port by binding to `127.0.0.1:0` and releasing it.
///
/// A brief window exists between release and `llama-server` binding the port; on
/// loopback with an ephemeral port the collision risk is negligible, and a
/// failed bind surfaces as a startup exit that the supervisor respawns.
fn pick_free_port() -> Result<u16, LlamaServerError> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    Ok(port)
}

/// Locate the bundled `llama-server` binary, falling back to `PATH` in dev.
///
/// Resolution order: the `APOLLIA_LLAMA_SERVER_BIN` override, then the bundle
/// layouts (next to the executable, a `runners/` sibling, the macOS
/// `Contents/Resources/runners/`, and the Linux `lib/apollia-os/runners/`), then
/// the ambient `PATH`, then the common install dirs. The override lets a
/// developer whose `llama-server` lives in a non-standard directory (a llama.cpp
/// build tree) point the app at it, which a GUI launch cannot reach via `PATH`.
fn locate_llama_server_binary() -> Result<PathBuf, LlamaServerError> {
    let ext = if cfg!(windows) { ".exe" } else { "" };
    let name = format!("llama-server{ext}");

    // Explicit override wins: `APOLLIA_LLAMA_SERVER_BIN=/path/to/llama-server`.
    if let Some(bin) = std::env::var_os("APOLLIA_LLAMA_SERVER_BIN") {
        let path = PathBuf::from(bin);
        if path.is_file() {
            tracing::info!(path = %path.display(), "using llama-server from APOLLIA_LLAMA_SERVER_BIN");
            return Ok(path);
        }
        tracing::warn!(
            path = %path.display(),
            "APOLLIA_LLAMA_SERVER_BIN is set but not a file, falling back to auto-detection"
        );
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidates = [
                dir.join(&name),
                dir.join("runners").join(&name),
                dir.join("../Resources/runners").join(&name),
                dir.join("../lib/apollia-os/runners").join(&name),
            ];
            if let Some(found) = candidates.iter().find(|c| c.exists()) {
                return Ok(found.clone());
            }
        }
    }

    if let Ok(path) = which_on_path(&name) {
        tracing::warn!(
            path = %path.display(),
            "bundled llama-server not found, using the one on PATH (dev fallback)"
        );
        return Ok(path);
    }

    // A GUI launch (Finder / .desktop) inherits a minimal PATH that omits the
    // usual install dirs, so a developer's system llama-server is invisible to
    // `which_on_path`. Probe the common locations explicitly as a last resort.
    let home = apollia_core::paths::home_string().unwrap_or_default();
    let common = [
        format!("/opt/homebrew/bin/{name}"),
        format!("/usr/local/bin/{name}"),
        format!("/usr/bin/{name}"),
        format!("{home}/.local/bin/{name}"),
        format!("{home}/.cargo/bin/{name}"),
    ];
    if let Some(found) = common.iter().map(PathBuf::from).find(|c| c.is_file()) {
        tracing::warn!(
            path = %found.display(),
            "bundled llama-server not found, using a system install (dev fallback)"
        );
        return Ok(found);
    }

    Err(LlamaServerError::BinaryNotFound(format!(
        "{name} not found next to the executable, in a bundled runners/ dir, on PATH, \
         or in the common install directories"
    )))
}

/// Minimal `PATH` lookup, avoiding a dependency on an external `which` crate.
fn which_on_path(name: &str) -> Result<PathBuf, LlamaServerError> {
    let path_var = std::env::var_os("PATH")
        .ok_or_else(|| LlamaServerError::BinaryNotFound("PATH is unset".to_owned()))?;
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    Err(LlamaServerError::BinaryNotFound(format!(
        "{name} not on PATH"
    )))
}

/// Forward a child pipe line-by-line into the daemon's tracing log.
///
/// Uses lossy UTF-8 decoding and never stops on a decode error (llama.cpp can
/// emit non-UTF-8 bytes): a stalled drain would fill the OS pipe buffer and
/// block the server. Stops only on EOF or a read error (process gone).
async fn drain_pipe<R: AsyncBufRead + Unpin>(mut reader: R) {
    let mut buf = Vec::with_capacity(256);
    loop {
        buf.clear();
        match reader.read_until(b'\n', &mut buf).await {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                let line = String::from_utf8_lossy(&buf);
                let line = line.trim_end_matches(['\r', '\n']);
                tracing::info!(target: "llama-server", line = %line);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a supervisor without touching the filesystem or spawning anything.
    ///
    /// `LlamaServerSupervisor::new` locates the binary, which is absent on a CI
    /// runner; these tests exercise the residency bookkeeping, not the process
    /// lifecycle, so the struct is assembled directly.
    fn supervisor_with_ceiling(max_loaded: usize) -> LlamaServerSupervisor {
        LlamaServerSupervisor {
            bin_path: PathBuf::from("/nonexistent/llama-server"),
            n_ctx: 32_768,
            max_loaded,
            config: Arc::new(Mutex::new(LlamaServerConfig::default())),
            instances: Arc::new(Mutex::new(Vec::new())),
            tick: Arc::new(AtomicU64::new(0)),
            respawn_lock: Arc::new(Mutex::new(())),
            shutting_down: Arc::new(Mutex::new(false)),
        }
    }

    /// A instance backed by a long-lived child, so liveness checks see it alive.
    async fn live_instance(sup: &LlamaServerSupervisor, model: &str, port: u16) -> Instance {
        let child = Command::new("sleep")
            .arg("60")
            .kill_on_drop(true)
            .spawn()
            .expect("spawning sleep must succeed");
        Instance {
            model_path: model.to_owned(),
            port,
            child,
            last_used: sup.next_tick(),
        }
    }

    // GIVEN no environment override
    // WHEN the residency ceiling is resolved
    // THEN exactly one model stays resident, which is the historical behaviour:
    //      raising memory use must never be a side effect of an upgrade
    #[test]
    fn test_residency_defaults_to_a_single_model() {
        assert_eq!(resolve_max_loaded(|_| None), 1);
    }

    // GIVEN an explicit ceiling
    // WHEN it is resolved
    // THEN it is honoured verbatim
    #[test]
    fn test_residency_honours_an_explicit_ceiling() {
        assert_eq!(resolve_max_loaded(|_| Some("3".to_owned())), 3);
        assert_eq!(resolve_max_loaded(|_| Some(" 2 ".to_owned())), 2);
    }

    // GIVEN a value that is zero or unparseable
    // WHEN it is resolved
    // THEN the default holds, because a typo must not be the reason no model
    //      can load at all
    #[test]
    fn test_residency_rejects_zero_and_garbage_without_disabling_inference() {
        assert_eq!(resolve_max_loaded(|_| Some("0".to_owned())), 1);
        assert_eq!(resolve_max_loaded(|_| Some("many".to_owned())), 1);
        assert_eq!(resolve_max_loaded(|_| Some(String::new())), 1);
    }

    // GIVEN a live instance for a model
    // WHEN that same model is requested again
    // THEN its URL is returned without a respawn, which is the whole point of
    //      residency: a hand-off costs a lookup, not a reload
    #[tokio::test]
    async fn test_a_resident_model_is_served_without_respawning() {
        let sup = supervisor_with_ceiling(2);
        let instance = live_instance(&sup, "/models/a.gguf", 9001).await;
        sup.instances.lock().await.push(instance);

        let url = sup.touch_live_instance("/models/a.gguf").await;

        assert_eq!(url.as_deref(), Some("http://127.0.0.1:9001/v1"));
        assert_eq!(sup.instances.lock().await.len(), 1);
    }

    // GIVEN a ceiling of two and two resident models, the first used least
    //       recently
    // WHEN room is made for a third
    // THEN the least recently used one is stopped, not an arbitrary one
    #[tokio::test]
    async fn test_eviction_removes_the_least_recently_used_model() {
        let sup = supervisor_with_ceiling(2);
        let a = live_instance(&sup, "/models/a.gguf", 9001).await;
        let b = live_instance(&sup, "/models/b.gguf", 9002).await;
        sup.instances.lock().await.push(a);
        sup.instances.lock().await.push(b);
        // Re-request A, making B the least recently used.
        sup.touch_live_instance("/models/a.gguf").await;

        sup.evict_until_below_ceiling().await;

        let instances = sup.instances.lock().await;
        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].model_path, "/models/a.gguf");
    }

    // GIVEN a ceiling of one and a resident model
    // WHEN room is made for another
    // THEN the pool empties first, reproducing the stop-then-start behaviour the
    //      default has always had
    #[tokio::test]
    async fn test_a_ceiling_of_one_stops_the_running_model_before_the_next() {
        let sup = supervisor_with_ceiling(1);
        let a = live_instance(&sup, "/models/a.gguf", 9001).await;
        sup.instances.lock().await.push(a);

        sup.evict_until_below_ceiling().await;

        assert!(sup.instances.lock().await.is_empty());
    }

    // GIVEN an instance whose process has exited
    // WHEN that model is requested
    // THEN nothing is handed back, so the caller respawns instead of receiving
    //      a port that answers nothing
    #[tokio::test]
    async fn test_a_dead_instance_is_dropped_rather_than_served() {
        let sup = supervisor_with_ceiling(2);
        let mut instance = live_instance(&sup, "/models/a.gguf", 9001).await;
        instance.child.kill().await.expect("kill must succeed");
        instance.child.wait().await.expect("wait must succeed");
        sup.instances.lock().await.push(instance);

        let url = sup.touch_live_instance("/models/a.gguf").await;

        assert!(url.is_none());
        assert!(sup.instances.lock().await.is_empty());
    }

    // GIVEN one dead instance and one live one
    // WHEN the supervision sweep collects the dead
    // THEN only the dead one is reported and removed, and the live one keeps
    //      serving
    #[tokio::test]
    async fn test_supervision_collects_only_the_dead_instances() {
        let sup = supervisor_with_ceiling(2);
        let mut dead = live_instance(&sup, "/models/dead.gguf", 9001).await;
        dead.child.kill().await.expect("kill must succeed");
        dead.child.wait().await.expect("wait must succeed");
        let live = live_instance(&sup, "/models/live.gguf", 9002).await;
        sup.instances.lock().await.push(dead);
        sup.instances.lock().await.push(live);

        let collected = sup.take_dead_instances().await;

        assert_eq!(collected.len(), 1);
        assert_eq!(collected[0].0, "/models/dead.gguf");
        let instances = sup.instances.lock().await;
        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].model_path, "/models/live.gguf");
    }

    // GIVEN two resident models
    // WHEN the supervisor shuts down in place
    // THEN every process is stopped, not only the most recent one
    #[tokio::test]
    async fn test_shutdown_stops_every_resident_model() {
        let sup = supervisor_with_ceiling(2);
        let a = live_instance(&sup, "/models/a.gguf", 9001).await;
        let b = live_instance(&sup, "/models/b.gguf", 9002).await;
        sup.instances.lock().await.push(a);
        sup.instances.lock().await.push(b);

        sup.shutdown_in_place().await;

        assert!(sup.instances.lock().await.is_empty());
        assert!(*sup.shutting_down.lock().await);
    }
}
