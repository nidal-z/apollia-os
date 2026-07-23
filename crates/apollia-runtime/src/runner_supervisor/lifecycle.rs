//! `RunnerSupervisor`: spawn + health monitoring + automatic restart of the runner.

use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncBufRead, AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, RwLock};
use tokio::time::timeout;

use super::client::RunnerClient;
use super::error::RunnerError;
use super::gpu_detection::{GpuInfo, RunnerBackend};
use super::lifecycle_inner::RunnerInnerHandle;

/// Timeout for the initial `READY <port>` handshake.
const HANDSHAKE_TIMEOUT_SECS: u64 = 10;

/// Health poll interval.
#[allow(
    dead_code,
    reason = "reserved for the periodic health-check task scheduled in a follow-up to runner_supervisor"
)]
const HEALTH_POLL_INTERVAL_SECS: u64 = 30;

/// Supervisor of the runner child process.
///
/// Spawns the `apollia-runner-{backend}` binary at boot, parses `READY <port>`,
/// exposes a `RunnerProxy` for inference calls, and restarts the runner
/// automatically on crash.
pub struct RunnerSupervisor {
    backend: RunnerBackend,
    gpu_info: GpuInfo,
    /// Lightweight handle (HTTP client + port) shared with the `RunnerProxy`.
    inner: Arc<RwLock<Option<RunnerInnerHandle>>>,
    /// Child process managed exclusively by the supervisor (Child is not Clone
    /// and only the supervisor should be able to wait/kill it).
    child: Arc<Mutex<Option<Child>>>,
    /// Flag to prevent new spawns while shutting down.
    shutting_down: Arc<Mutex<bool>>,
}

impl RunnerSupervisor {
    /// Spawn a new runner matching the chosen `backend`.
    ///
    /// The `apollia-runner-{backend}` binary must be present in the same
    /// directory as the current executable.
    pub async fn start(gpu_info: GpuInfo, backend: RunnerBackend) -> Result<Self, RunnerError> {
        let supervisor = Self {
            backend,
            gpu_info,
            inner: Arc::new(RwLock::new(None)),
            child: Arc::new(Mutex::new(None)),
            shutting_down: Arc::new(Mutex::new(false)),
        };
        supervisor.spawn_runner().await?;
        Ok(supervisor)
    }

    /// Spawn the runner binary and wait for the handshake.
    async fn spawn_runner(&self) -> Result<(), RunnerError> {
        let bin_path = locate_runner_binary(self.backend)?;
        tracing::info!(
            backend = ?self.backend,
            binary = %bin_path.display(),
            "spawning apollia-runner"
        );

        let mut child = Command::new(&bin_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| {
                tracing::error!(error = %e, "failed to spawn runner");
                RunnerError::Io(e)
            })?;

        // Parse READY <port>\n on stdout, with a timeout.
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| RunnerError::Io(std::io::Error::other("no stdout pipe")))?;
        let mut reader = BufReader::new(stdout);

        let port = match timeout(
            Duration::from_secs(HANDSHAKE_TIMEOUT_SECS),
            read_ready_line(&mut reader),
        )
        .await
        {
            Ok(Ok(port)) => port,
            Ok(Err(e)) => {
                let _ = child.kill().await;
                return Err(e);
            }
            Err(_) => {
                let _ = child.kill().await;
                return Err(RunnerError::HandshakeTimeout {
                    timeout_secs: HANDSHAKE_TIMEOUT_SECS,
                });
            }
        };

        // Drain the runner's stderr into the daemon's tracing log. This MUST keep
        // draining for the runner's whole life: if it stalls, the OS pipe buffer
        // fills and the runner blocks on its next stderr write, deadlocking the
        // in-progress model load (llama.cpp is very verbose during load).
        if let Some(stderr) = child.stderr.take() {
            let backend = self.backend;
            tokio::spawn(drain_pipe(
                BufReader::new(stderr),
                backend,
                PipeKind::Stderr,
            ));
        }

        // Drain the remaining stdout for the same reason (the READY line has been
        // consumed above; the rest is diagnostics).
        let backend = self.backend;
        tokio::spawn(drain_pipe(reader, backend, PipeKind::Stdout));

        // Check the HTTP handshake.
        let client = RunnerClient::new(port)?;
        let handshake: serde_json::Value = client.get("/handshake").await?;
        let proto = handshake
            .get("data")
            .and_then(|d| d.get("protocol_version"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if !proto.starts_with("1.") {
            let _ = child.kill().await;
            return Err(RunnerError::Http(format!(
                "incompatible protocol version: {proto:?}"
            )));
        }
        tracing::info!(
            backend = ?self.backend,
            port,
            protocol_version = %proto,
            "runner handshake successful"
        );

        *self.inner.write().await = Some(RunnerInnerHandle { client, port });
        *self.child.lock().await = Some(child);
        Ok(())
    }

    /// Monitor the runner child and respawn it if it dies unexpectedly.
    ///
    /// The runner can hard-abort (a `GGML_ASSERT` in llama.cpp calls `abort()`,
    /// which cannot be caught in-process) when asked to load a model whose
    /// architecture the pinned llama.cpp does not support, or on any other fatal
    /// condition. Without supervision the cached handle would keep pointing at
    /// the dead port and every later call would fail with a connection-refused
    /// error until the app restarts. This task detects the exit, logs its
    /// status, drops the stale handle, and respawns a fresh runner (with
    /// backoff) so the system recovers, for example once the user switches back
    /// to a supported model.
    pub fn spawn_supervision(self: Arc<Self>) {
        tokio::spawn(async move {
            const POLL: Duration = Duration::from_secs(2);
            const MIN_BACKOFF: Duration = Duration::from_secs(1);
            const MAX_BACKOFF: Duration = Duration::from_secs(30);
            let mut backoff = MIN_BACKOFF;

            loop {
                if *self.shutting_down.lock().await {
                    return;
                }

                // Probe the child: `Some(_)` means it needs a (re)spawn (exited,
                // errored, or absent); `None` means it is still running.
                let needs_respawn = {
                    let mut guard = self.child.lock().await;
                    match guard.as_mut() {
                        Some(child) => match child.try_wait() {
                            Ok(Some(status)) => Some(Some(status)),
                            Ok(None) => None,
                            Err(e) => {
                                tracing::warn!(error = %e, "runner try_wait failed");
                                Some(None)
                            }
                        },
                        None => Some(None),
                    }
                };

                let Some(exit_status) = needs_respawn else {
                    backoff = MIN_BACKOFF;
                    tokio::time::sleep(POLL).await;
                    continue;
                };

                if *self.shutting_down.lock().await {
                    return;
                }

                match exit_status {
                    Some(status) => tracing::error!(
                        backend = ?self.backend,
                        ?status,
                        "runner exited unexpectedly, respawning"
                    ),
                    None => tracing::warn!(
                        backend = ?self.backend,
                        "runner handle missing, respawning"
                    ),
                }

                // Drop the stale handle so in-flight calls fail fast rather than
                // hanging on a dead port until the respawn lands.
                *self.inner.write().await = None;
                *self.child.lock().await = None;

                tokio::time::sleep(backoff).await;
                if *self.shutting_down.lock().await {
                    return;
                }

                match self.spawn_runner().await {
                    Ok(()) => {
                        tracing::info!(backend = ?self.backend, "runner respawned");
                        backoff = MIN_BACKOFF;
                    }
                    Err(e) => {
                        tracing::error!(backend = ?self.backend, error = %e, "runner respawn failed");
                        backoff = (backoff * 2).min(MAX_BACKOFF);
                    }
                }
            }
        });
    }

    /// Return a `RunnerProxy` that makes HTTP calls to the runner.
    pub fn proxy(&self) -> super::proxy::RunnerProxy {
        super::proxy::RunnerProxy::new(self.inner.clone())
    }

    /// GPU info detected at boot.
    pub fn gpu_info(&self) -> &GpuInfo {
        &self.gpu_info
    }

    /// Active backend (= type of spawned runner).
    pub fn backend(&self) -> RunnerBackend {
        self.backend
    }

    /// Current HTTP port of the runner. For debug / tests.
    pub async fn port(&self) -> Option<u16> {
        self.inner.read().await.as_ref().map(|i| i.port)
    }

    /// Kill the runner child without consuming the supervisor.
    ///
    /// The owning [`shutdown`](Self::shutdown) takes `self` by value, which an
    /// `Arc<RunnerSupervisor>` cannot satisfy. This variant works through a
    /// shared reference, so it can be called from the desktop exit hook where
    /// the supervisor lives behind an `Arc`. Best-effort and time-bounded:
    /// intended for process teardown, not graceful drain.
    pub async fn shutdown_in_place(&self) {
        *self.shutting_down.lock().await = true;
        let child = self.child.lock().await.take();
        if let Some(mut child) = child {
            // SIGKILL and reap, bounded so a wedged runner cannot stall exit.
            let _ = timeout(Duration::from_secs(2), child.kill()).await;
        }
    }

    /// Cleanly stop the runner (`POST /shutdown` + wait for exit).
    pub async fn shutdown(self) -> Result<(), RunnerError> {
        *self.shutting_down.lock().await = true;

        // 1. Try a clean shutdown via HTTP.
        let shutdown_ok = {
            let inner_guard = self.inner.read().await;
            if let Some(handle) = inner_guard.as_ref() {
                matches!(
                    tokio::time::timeout(
                        Duration::from_secs(1),
                        handle
                            .client
                            .post::<_, serde_json::Value>("/shutdown", &serde_json::json!({})),
                    )
                    .await,
                    Ok(Ok(_))
                )
            } else {
                false
            }
        };

        if shutdown_ok {
            tracing::info!("runner shutdown HTTP sent, waiting for exit");
        } else {
            tracing::warn!("runner shutdown HTTP failed, will force kill");
        }

        // 2. Take the Child and wait for exit with a timeout, otherwise kill.
        let mut child_guard = self.child.lock().await;
        if let Some(mut child) = child_guard.take() {
            match tokio::time::timeout(Duration::from_secs(3), child.wait()).await {
                Ok(Ok(status)) => {
                    tracing::info!(?status, "runner exited gracefully");
                }
                _ => {
                    let _ = child.kill().await;
                    let _ = child.wait().await;
                    tracing::info!("runner killed forcefully after timeout");
                }
            }
        }
        // The RunnerInnerHandle is dropped automatically when inner_guard goes out of scope.
        *self.inner.write().await = None;
        Ok(())
    }
}

/// Locate the `apollia-runner-{backend}` binary next to the current executable
/// (`apollia-os`).
pub(super) fn locate_runner_binary(
    backend: RunnerBackend,
) -> Result<std::path::PathBuf, RunnerError> {
    let exe = std::env::current_exe().map_err(|e| {
        RunnerError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("current_exe: {e}"),
        ))
    })?;
    let dir = exe
        .parent()
        .ok_or_else(|| RunnerError::BinaryNotFound("no parent dir of current_exe".into()))?;

    let bin_name = backend.binary_name();
    let ext = if cfg!(windows) { ".exe" } else { "" };
    let candidate = dir.join(format!("{bin_name}{ext}"));
    if candidate.exists() {
        return Ok(candidate);
    }

    // Packaged-bundle locations: the runners are staged under a `runners/`
    // resource directory, not next to the executable. The desktop Tauri app
    // ships them in `Contents/Resources/runners/` (macOS) while the CLI
    // self-contained bundle and the Linux packages place them in a sibling or
    // `lib/apollia-os/runners/` directory. Check each layout.
    let bundled = [
        // macOS .app: Contents/MacOS/apollia-desktop -> Contents/Resources/runners/
        dir.join("../Resources/runners")
            .join(format!("{bin_name}{ext}")),
        // Same-dir `runners/` subdir (CLI bundle / Linux staging).
        dir.join("runners").join(format!("{bin_name}{ext}")),
        // Linux .deb/AppImage: usr/bin/apollia-desktop -> usr/lib/apollia-os/runners/
        dir.join("../lib/apollia-os/runners")
            .join(format!("{bin_name}{ext}")),
    ];
    if let Some(found) = bundled.iter().find(|c| c.exists()) {
        return Ok(found.clone());
    }

    // Dev fallback (`cargo run -p apollia-cli`): the `apollia-runner` binary
    // is placed in target/{debug,release}/ without a backend suffix. We accept
    // it to allow local testing without renaming manually.
    let dev_fallback = dir.join(format!("apollia-runner{ext}"));
    if dev_fallback.exists() {
        tracing::warn!(
            requested = %bin_name,
            fallback = %dev_fallback.display(),
            "runner binary not found with backend suffix, using unsuffixed dev binary"
        );
        return Ok(dev_fallback);
    }

    Err(RunnerError::BinaryNotFound(format!(
        "{} not found near {} (also checked apollia-runner{})",
        bin_name,
        dir.display(),
        ext
    )))
}

async fn read_ready_line(
    reader: &mut BufReader<tokio::process::ChildStdout>,
) -> Result<u16, RunnerError> {
    let mut line = String::new();
    reader.read_line(&mut line).await?;
    let trimmed = line.trim();
    let port = trimmed
        .strip_prefix("READY ")
        .ok_or_else(|| RunnerError::MalformedReady(trimmed.to_string()))?
        .parse::<u16>()
        .map_err(|_| RunnerError::MalformedReady(trimmed.to_string()))?;
    Ok(port)
}

/// Which runner child stream is being drained (controls the log level).
#[derive(Clone, Copy)]
enum PipeKind {
    Stdout,
    Stderr,
}

/// Continuously drains a runner child pipe into the daemon's tracing log.
///
/// Reads with `read_until` + lossy UTF-8 decoding rather than `lines()`: the
/// runner (llama.cpp) writes non-UTF-8 bytes on stderr (raw tokenizer byte
/// tokens), and `AsyncBufReadExt::lines` yields `Err` on the first invalid-UTF-8
/// line, which would end the drain. A stalled drain lets the OS pipe buffer fill,
/// after which the runner blocks on its next write and the in-progress model load
/// deadlocks. So never stop on a decode error, only on EOF or a real read error
/// (the runner process has exited).
async fn drain_pipe<R: AsyncBufRead + Unpin>(
    mut reader: R,
    backend: RunnerBackend,
    kind: PipeKind,
) {
    let mut buf = Vec::with_capacity(256);
    loop {
        buf.clear();
        match reader.read_until(b'\n', &mut buf).await {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                let line = String::from_utf8_lossy(&buf);
                let line = line.trim_end_matches(['\r', '\n']);
                match kind {
                    // The runner emits JSON Lines on stderr; forward as-is.
                    PipeKind::Stderr => {
                        tracing::info!(target: "runner", backend = ?backend, line = %line);
                    }
                    PipeKind::Stdout => {
                        tracing::trace!(target: "runner", backend = ?backend, line = %line);
                    }
                }
            }
        }
    }
}
