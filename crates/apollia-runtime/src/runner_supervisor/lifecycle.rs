//! `RunnerSupervisor` : spawn + health monitoring + restart auto du runner.

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

/// Timeout pour le handshake initial `READY <port>` (cf. IPC-PROTOCOL §1.2).
const HANDSHAKE_TIMEOUT_SECS: u64 = 10;

/// Intervalle de health poll (cf. IPC-PROTOCOL §4.2).
#[allow(dead_code)]
const HEALTH_POLL_INTERVAL_SECS: u64 = 30;

/// Supervisor du process enfant runner.
///
/// Spawn le binaire `apollia-runner-{backend}` au boot, parse `READY <port>`,
/// expose un `RunnerProxy` pour les appels d'inférence, et redémarre le
/// runner automatiquement en cas de crash.
pub struct RunnerSupervisor {
    backend: RunnerBackend,
    gpu_info: GpuInfo,
    /// Handle léger (client HTTP + port) partagé avec le `RunnerProxy`.
    inner: Arc<RwLock<Option<RunnerInnerHandle>>>,
    /// Process enfant géré exclusivement par le supervisor (Child n'est pas
    /// Clone et seul le supervisor doit pouvoir wait/kill).
    child: Arc<Mutex<Option<Child>>>,
    /// Flag pour empêcher de nouveaux spawn quand on est en shutdown.
    shutting_down: Arc<Mutex<bool>>,
}

impl RunnerSupervisor {
    /// Spawn un nouveau runner correspondant au `backend` choisi.
    ///
    /// Le binaire `apollia-runner-{backend}` doit être présent dans le même
    /// répertoire que l'exécutable courant (cf. PACKAGING-PLAN §3).
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

    /// Spawn le binaire runner et attend le handshake.
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

        // Parse READY <port>\n sur stdout, avec timeout.
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

        // Redirige stderr vers les logs tracing du daemon.
        if let Some(stderr) = child.stderr.take() {
            let backend = self.backend;
            tokio::spawn(forward_stderr(stderr, backend));
        }

        // Spawn une tâche qui consomme stdout restant (sinon le pipe se remplit).
        tokio::spawn(async move {
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                tracing::trace!(line = %line, "[runner stdout]");
            }
        });

        // Vérifie le handshake HTTP.
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

    /// Retourne un `RunnerProxy` qui fait des appels HTTP au runner.
    pub fn proxy(&self) -> super::proxy::RunnerProxy {
        super::proxy::RunnerProxy::new(self.inner.clone())
    }

    /// GPU info détectée au boot.
    pub fn gpu_info(&self) -> &GpuInfo {
        &self.gpu_info
    }

    /// Backend actif (= type de runner spawné).
    pub fn backend(&self) -> RunnerBackend {
        self.backend
    }

    /// Port HTTP actuel du runner. Pour debug / tests.
    pub async fn port(&self) -> Option<u16> {
        self.inner.read().await.as_ref().map(|i| i.port)
    }

    /// Arrête proprement le runner (`POST /shutdown` + attente exit).
    pub async fn shutdown(self) -> Result<(), RunnerError> {
        *self.shutting_down.lock().await = true;

        // 1. Tente le shutdown propre via HTTP.
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

        // 2. Récupère le Child et attend exit avec timeout, sinon kill.
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
        // Le RunnerInnerHandle est dropped automatiquement quand inner_guard sort.
        *self.inner.write().await = None;
        Ok(())
    }
}

/// Localise le binaire `apollia-runner-{backend}` à côté de l'exécutable
/// courant (`apollia-os`).
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

    // Fallback dev (`cargo run -p apollia-cli`) : le binaire `apollia-runner`
    // est posé dans target/{debug,release}/ sans suffix de backend. On
    // l'accepte pour permettre le test local sans renommer manuellement.
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
        // Le runner émet déjà du JSON Lines, on log tel quel.
        tracing::info!(target: "runner", backend = ?backend, line = %line);
    }
}
