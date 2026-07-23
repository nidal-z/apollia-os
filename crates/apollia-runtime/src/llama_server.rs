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
//! This supervisor owns the `llama-server` child process: it picks a loopback
//! port, launches the binary, waits for `/health`, and respawns the process if
//! it dies. Switching the active model restarts the process (one model per
//! server). Whisper STT still lives in `apollia-runner`; only the LLM engine
//! moves here.

use std::net::TcpListener;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use thiserror::Error;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, RwLock};

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

/// Launch configuration for the embedded `llama-server`.
#[derive(Clone, Debug)]
pub struct LlamaServerConfig {
    /// Absolute path to the `.gguf` model file to load.
    pub model_path: String,
    /// Total context window in tokens (`-c`). Split across slots; the desktop
    /// runs a single conversation, so the whole window serves one request.
    pub n_ctx: u32,
    /// Number of layers to offload to the GPU (`-ngl`). `999` offloads all;
    /// a CPU-only build silently ignores the request.
    pub n_gpu_layers: i32,
}

impl Default for LlamaServerConfig {
    fn default() -> Self {
        Self {
            model_path: String::new(),
            n_ctx: 32_768,
            n_gpu_layers: 999,
        }
    }
}

/// The running server's loopback port and the model it was launched with.
#[derive(Clone, Debug)]
struct RunningState {
    port: u16,
    model_path: String,
}

/// Timeout for the initial (and post-respawn) `/health` handshake. Model load
/// compiles GPU kernels and maps a multi-gigabyte file, so it is generous.
const HEALTH_TIMEOUT_SECS: u64 = 180;

/// Interval between `/health` polls while waiting for startup.
const HEALTH_POLL_INTERVAL: Duration = Duration::from_millis(500);

/// Interval between liveness checks of the running child.
const SUPERVISION_POLL: Duration = Duration::from_secs(2);

/// Supervisor that owns the embedded `llama-server` child process.
///
/// Cloneable state lives behind `Arc`; the supervisor itself is held behind an
/// `Arc` by the runtime so the supervision task can respawn the child.
pub struct LlamaServerSupervisor {
    bin_path: PathBuf,
    /// Context window the server is launched with (`-c`), reported to the router
    /// as the model's usable window so it can size compaction. Immutable: every
    /// (re)spawn uses the same value.
    n_ctx: u32,
    /// Desired launch configuration; updated by [`switch_model`](Self::switch_model)
    /// and read by every (re)spawn.
    config: Arc<Mutex<LlamaServerConfig>>,
    /// Port + model of the currently running server, or `None` when down.
    inner: Arc<RwLock<Option<RunningState>>>,
    /// The child process, owned exclusively by the supervisor (`kill_on_drop`).
    child: Arc<Mutex<Option<Child>>>,
    /// Serialises (re)spawns so [`switch_model`](Self::switch_model) and the
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
    /// but does not spawn a process. The server starts lazily on the first
    /// [`switch_model`](Self::switch_model), so a fresh install with no model yet
    /// still yields a usable supervisor that the router factory can capture.
    /// Call [`spawn_supervision`](Self::spawn_supervision) to enable auto-respawn.
    pub fn new(config: LlamaServerConfig) -> Result<Arc<Self>, LlamaServerError> {
        let bin_path = locate_llama_server_binary()?;
        Ok(Arc::new(Self {
            bin_path,
            n_ctx: config.n_ctx,
            config: Arc::new(Mutex::new(config)),
            inner: Arc::new(RwLock::new(None)),
            child: Arc::new(Mutex::new(None)),
            respawn_lock: Arc::new(Mutex::new(())),
            shutting_down: Arc::new(Mutex::new(false)),
        }))
    }

    /// OpenAI-compatible base URL of the running server, or `None` when down.
    ///
    /// The `apollia-llm` OpenAI backend targets this as its `endpoint`.
    pub async fn base_url(&self) -> Option<String> {
        self.inner
            .read()
            .await
            .as_ref()
            .map(|s| format!("http://127.0.0.1:{}/v1", s.port))
    }

    /// Context window (`-c`) the server is launched with, in tokens.
    pub fn n_ctx(&self) -> u32 {
        self.n_ctx
    }

    /// Model path the server is currently serving, or `None` when down.
    pub async fn current_model(&self) -> Option<String> {
        self.inner
            .read()
            .await
            .as_ref()
            .map(|s| s.model_path.clone())
    }

    /// Switch the active model, restarting the server (one model per process).
    ///
    /// A no-op when the requested model is already running. Serialised against
    /// the supervision task via `respawn_lock`.
    pub async fn switch_model(&self, model_path: String) -> Result<(), LlamaServerError> {
        let _guard = self.respawn_lock.lock().await;
        if self.inner.read().await.as_ref().map(|s| &s.model_path) == Some(&model_path) {
            return Ok(());
        }
        self.config.lock().await.model_path = model_path;
        self.kill_child().await;
        self.spawn_process().await
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

            loop {
                if *self.shutting_down.lock().await {
                    return;
                }

                // Nothing to supervise until a model has been requested: a fresh
                // install may never start a local server. Respawning here would
                // launch llama-server with no `-m` and crash-loop.
                if self.config.lock().await.model_path.is_empty() {
                    tokio::time::sleep(SUPERVISION_POLL).await;
                    continue;
                }

                let dead = {
                    let mut guard = self.child.lock().await;
                    match guard.as_mut() {
                        Some(child) => match child.try_wait() {
                            Ok(Some(status)) => Some(status.to_string()),
                            Ok(None) => None,
                            Err(e) => Some(format!("try_wait failed: {e}")),
                        },
                        None => Some("no child".to_owned()),
                    }
                };

                let Some(reason) = dead else {
                    backoff = MIN_BACKOFF;
                    tokio::time::sleep(SUPERVISION_POLL).await;
                    continue;
                };

                if *self.shutting_down.lock().await {
                    return;
                }

                // Serialise with switch_model, then re-check: it may have already
                // respawned the server while we waited for the lock.
                let _guard = self.respawn_lock.lock().await;
                if self.child_is_running().await {
                    continue;
                }

                tracing::error!(reason = %reason, "llama-server exited, respawning");
                *self.inner.write().await = None;

                tokio::time::sleep(backoff).await;
                if *self.shutting_down.lock().await {
                    return;
                }
                match self.spawn_process().await {
                    Ok(()) => {
                        tracing::info!("llama-server respawned");
                        backoff = MIN_BACKOFF;
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "llama-server respawn failed");
                        backoff = (backoff * 2).min(MAX_BACKOFF);
                    }
                }
            }
        });
    }

    /// Stop the server without consuming the supervisor (for the exit hook).
    pub async fn shutdown_in_place(&self) {
        *self.shutting_down.lock().await = true;
        let _guard = self.respawn_lock.lock().await;
        self.kill_child().await;
        *self.inner.write().await = None;
    }

    /// Returns whether the child is currently alive (has not exited).
    async fn child_is_running(&self) -> bool {
        let mut guard = self.child.lock().await;
        match guard.as_mut() {
            Some(child) => matches!(child.try_wait(), Ok(None)),
            None => false,
        }
    }

    /// Kill and reap the current child, if any.
    async fn kill_child(&self) {
        if let Some(mut child) = self.child.lock().await.take() {
            let _ = child.kill().await;
            let _ = child.wait().await;
        }
    }

    /// Spawn `llama-server` for the current config and wait until it is healthy.
    ///
    /// Picks a free loopback port, launches with the tool-calling flags
    /// (`--jinja`), drains its logs into tracing, and polls `/health`.
    async fn spawn_process(&self) -> Result<(), LlamaServerError> {
        let config = self.config.lock().await.clone();
        let port = pick_free_port()?;

        tracing::info!(
            binary = %self.bin_path.display(),
            model = %config.model_path,
            port,
            n_ctx = config.n_ctx,
            "spawning llama-server"
        );

        let mut child = Command::new(&self.bin_path)
            .arg("-m")
            .arg(&config.model_path)
            .arg("-ngl")
            .arg(config.n_gpu_layers.to_string())
            .arg("-c")
            .arg(config.n_ctx.to_string())
            .arg("-np")
            .arg("1")
            .arg("-cb")
            .arg("--flash-attn")
            .arg("on")
            .arg("--jinja")
            .arg("--host")
            .arg("127.0.0.1")
            .arg("--port")
            .arg(port.to_string())
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

        *self.inner.write().await = Some(RunningState {
            port,
            model_path: config.model_path,
        });
        *self.child.lock().await = Some(child);
        tracing::info!(port, "llama-server healthy");
        Ok(())
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
    let home = std::env::var("HOME").unwrap_or_default();
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
