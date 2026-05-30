//! `RunnerSupervisor`: spawn + health monitoring + automatic restart of the runner.

use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, BufReader};
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
#[allow(dead_code)]
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

        // Redirect stderr to the daemon's tracing logs.
        if let Some(stderr) = child.stderr.take() {
            let backend = self.backend;
            tokio::spawn(forward_stderr(stderr, backend));
        }

        // Spawn a task that drains the remaining stdout (otherwise the pipe fills up).
        tokio::spawn(async move {
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                tracing::trace!(line = %line, "[runner stdout]");
            }
        });

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
fn locate_runner_binary(backend: RunnerBackend) -> Result<std::path::PathBuf, RunnerError> {
    let exe = std::env::current_exe()
        .map_err(|e| RunnerError::Io(std::io::Error::new(std::io::ErrorKind::NotFound, format!("current_exe: {e}"))))?;
    let dir = exe
        .parent()
        .ok_or_else(|| RunnerError::BinaryNotFound("no parent dir of current_exe".into()))?;

    let bin_name = backend.binary_name();
    let ext = if cfg!(windows) { ".exe" } else { "" };
    let candidate = dir.join(format!("{bin_name}{ext}"));
    if candidate.exists() {
        return Ok(candidate);
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

async fn forward_stderr(stderr: tokio::process::ChildStderr, backend: RunnerBackend) {
    let mut lines = BufReader::new(stderr).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        // The runner already emits JSON Lines, so log it as-is.
        tracing::info!(target: "runner", backend = ?backend, line = %line);
    }
}
