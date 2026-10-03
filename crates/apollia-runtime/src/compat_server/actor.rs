//! The actor that owns the compatible server's listener.

use std::net::Ipv4Addr;
use std::path::PathBuf;

use apollia_core::config::{
    resolve_setting, write_setting, PolicySettings, DEFAULT_COMPAT_SERVER_PORT,
};
use apollia_core::RuntimeEvent;
use serde::Serialize;
use serde_json::Value;
use tokio::net::TcpListener;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use super::logs::ServerLogsHandle;
use super::models::LocalModels;
use super::relay::RelayState;
use super::router::{compat_router, serve};
use super::tokens::ServerTokensHandle;
use super::CompatServerError;
use crate::eventbus::EventBusSender;

/// Capacity of the actor's command channel. Commands come from three HTTP
/// routes driven by a person, so a handful in flight is already a burst.
const COMMAND_CAPACITY: usize = 64;

const ENABLED_KEY: &str = "server.enabled";
const PORT_KEY: &str = "server.port";
const REQUIRE_AUTH_KEY: &str = "server.require_auth";

/// What the compatible server is doing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CompatServerStatus {
    /// Whether the listener is open.
    pub running: bool,
    /// The port it listens on, or the configured port when it is stopped.
    pub port: u16,
    /// Why the last start failed, cleared by the next successful start.
    pub last_error: Option<String>,
    /// How many tokens of the server are in use (not revoked).
    pub active_tokens: u64,
}

type Reply<T> = oneshot::Sender<Result<T, CompatServerError>>;

enum Command {
    Status(oneshot::Sender<CompatServerStatus>),
    Start(Reply<CompatServerStatus>),
    Stop(Reply<CompatServerStatus>),
}

/// Handle to the compatible server's actor.
#[derive(Debug, Clone)]
pub struct CompatServerHandle {
    tx: mpsc::Sender<Command>,
    tokens: ServerTokensHandle,
    logs: ServerLogsHandle,
}

impl CompatServerHandle {
    /// Spawns the actor and returns its handle.
    ///
    /// `settings_path` is the `apollia.toml` holding the `[server]` section,
    /// `system_db` the `system.db` holding the server's tokens, `models` the
    /// local models the server relays to.
    /// When it turns the server on, the actor opens the listener before it
    /// answers its first command; a failure to open it is logged and kept as
    /// the last error, and the runtime goes on without the server. The actor
    /// stops, closing the listener and leaving the setting as it is, when the
    /// bus carries [`RuntimeEvent::ShutdownRequested`] or every handle is
    /// dropped.
    ///
    /// The logs of the server start with it and outlive a stop of the
    /// listener; the lines of the local `llama-server` processes go there too.
    pub fn spawn(
        settings_path: PathBuf,
        system_db: PathBuf,
        models: LocalModels,
        bus: &EventBusSender,
    ) -> Self {
        let (tx, rx) = mpsc::channel(COMMAND_CAPACITY);
        let events = apollia_core::events::subscribe_resilient(bus, "compat_server");
        let tokens = ServerTokensHandle::spawn(system_db);
        let logs = ServerLogsHandle::spawn(bus);
        models.send_engine_lines_to(&logs);
        let actor = CompatServerActor {
            settings_path,
            models,
            tokens: tokens.clone(),
            logs: logs.clone(),
            running: None,
            last_error: None,
        };
        tokio::spawn(actor.run(rx, events));
        Self { tx, tokens, logs }
    }

    /// The tokens of the server: create, list, revoke.
    pub fn tokens(&self) -> &ServerTokensHandle {
        &self.tokens
    }

    /// The logs of the server: follow, clear.
    pub fn logs(&self) -> &ServerLogsHandle {
        &self.logs
    }

    /// Fills the count of tokens in use, which the token actor holds.
    async fn with_tokens(
        &self,
        mut status: CompatServerStatus,
    ) -> Result<CompatServerStatus, CompatServerError> {
        status.active_tokens = self.tokens.active_count().await?;
        Ok(status)
    }

    /// Whether the server listens, on which port, why its last start failed,
    /// and how many of its tokens are in use.
    ///
    /// # Errors
    ///
    /// [`CompatServerError::ActorGone`] once the runtime has stopped,
    /// [`CompatServerError::Tokens`] when the tokens cannot be counted.
    pub async fn status(&self) -> Result<CompatServerStatus, CompatServerError> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(Command::Status(reply))
            .await
            .map_err(|_| CompatServerError::ActorGone)?;
        let status = rx.await.map_err(|_| CompatServerError::ActorGone)?;
        self.with_tokens(status).await
    }

    /// Opens the listener on the configured port and writes
    /// `[server] enabled = true`. Starting a running server changes nothing.
    ///
    /// # Errors
    ///
    /// [`CompatServerError::PortInUse`] when another program holds the port,
    /// [`CompatServerError::Bind`] for any other refusal to listen,
    /// [`CompatServerError::Settings`] when the file cannot be read or
    /// written, [`CompatServerError::ActorGone`] once the runtime has stopped.
    pub async fn start(&self) -> Result<CompatServerStatus, CompatServerError> {
        self.request(Command::Start).await
    }

    /// Closes the listener and writes `[server] enabled = false`. Stopping a
    /// stopped server only writes the setting.
    ///
    /// # Errors
    ///
    /// [`CompatServerError::Settings`] when the file cannot be written,
    /// [`CompatServerError::ActorGone`] once the runtime has stopped.
    pub async fn stop(&self) -> Result<CompatServerStatus, CompatServerError> {
        self.request(Command::Stop).await
    }

    async fn request(
        &self,
        command: fn(Reply<CompatServerStatus>) -> Command,
    ) -> Result<CompatServerStatus, CompatServerError> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(command(reply))
            .await
            .map_err(|_| CompatServerError::ActorGone)?;
        let status = rx.await.map_err(|_| CompatServerError::ActorGone)??;
        self.with_tokens(status).await
    }
}

/// The open listener: its port, the token that stops it, and its task.
struct Running {
    port: u16,
    cancel: CancellationToken,
    task: JoinHandle<()>,
}

struct CompatServerActor {
    settings_path: PathBuf,
    models: LocalModels,
    tokens: ServerTokensHandle,
    logs: ServerLogsHandle,
    running: Option<Running>,
    last_error: Option<String>,
}

impl CompatServerActor {
    async fn run(
        mut self,
        mut rx: mpsc::Receiver<Command>,
        mut events: apollia_core::events::ResilientReceiver,
    ) {
        self.open_if_enabled().await;
        loop {
            tokio::select! {
                biased;
                event = events.recv() => match event {
                    Some(RuntimeEvent::ShutdownRequested) | None => break,
                    Some(_) => {}
                },
                command = rx.recv() => match command {
                    Some(Command::Status(reply)) => {
                        let _ = reply.send(self.status());
                    }
                    Some(Command::Start(reply)) => {
                        let _ = reply.send(self.start().await);
                    }
                    Some(Command::Stop(reply)) => {
                        let _ = reply.send(self.stop().await);
                    }
                    None => break,
                },
            }
        }
        self.close().await;
    }

    /// At start: opens the listener when the file turns the server on.
    async fn open_if_enabled(&mut self) {
        let policy = PolicySettings::none();
        let enabled = match resolve_setting(&self.settings_path, ENABLED_KEY, &policy) {
            Ok(resolved) => resolved.value == Value::Bool(true),
            Err(error) => {
                self.fail(&CompatServerError::from(error));
                return;
            }
        };
        if !enabled {
            return;
        }
        if let Err(error) = self.open().await {
            self.fail(&error);
        }
    }

    fn configured_port(&self) -> Result<u16, CompatServerError> {
        let resolved = resolve_setting(&self.settings_path, PORT_KEY, &PolicySettings::none())?;
        Ok(resolved
            .value
            .as_u64()
            .and_then(|port| u16::try_from(port).ok())
            .unwrap_or(DEFAULT_COMPAT_SERVER_PORT))
    }

    fn status(&self) -> CompatServerStatus {
        let port = match &self.running {
            Some(running) => running.port,
            None => self.configured_port().unwrap_or(DEFAULT_COMPAT_SERVER_PORT),
        };
        CompatServerStatus {
            running: self.running.is_some(),
            port,
            last_error: self.last_error.clone(),
            active_tokens: 0,
        }
    }

    async fn start(&mut self) -> Result<CompatServerStatus, CompatServerError> {
        if self.running.is_none() {
            if let Err(error) = self.open().await {
                self.fail(&error);
                return Err(error);
            }
        }
        if let Err(error) = self.write_enabled(true) {
            self.close().await;
            self.fail(&error);
            return Err(error);
        }
        Ok(self.status())
    }

    async fn stop(&mut self) -> Result<CompatServerStatus, CompatServerError> {
        self.close().await;
        self.write_enabled(false)?;
        Ok(self.status())
    }

    /// Binds the configured port on the loopback interface and spawns the
    /// accept loop, behind the token check when `[server] require_auth` asks
    /// for it.
    async fn open(&mut self) -> Result<(), CompatServerError> {
        let port = self.configured_port()?;
        let require_auth = resolve_setting(
            &self.settings_path,
            REQUIRE_AUTH_KEY,
            &PolicySettings::none(),
        )?
        .value
            == Value::Bool(true);
        let tokens = require_auth.then(|| self.tokens.clone());
        let relay = RelayState::new(self.models.clone()).map_err(CompatServerError::Client)?;
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, port))
            .await
            .map_err(|source| match source.kind() {
                std::io::ErrorKind::AddrInUse => CompatServerError::PortInUse { port },
                _ => CompatServerError::Bind { port, source },
            })?;
        let cancel = CancellationToken::new();
        let task = tokio::spawn(serve(
            listener,
            compat_router(tokens, relay, self.logs.clone()),
            cancel.clone(),
        ));
        self.running = Some(Running { port, cancel, task });
        self.last_error = None;
        tracing::event!(
            tracing::Level::INFO,
            port = port,
            require_auth = require_auth,
            "compat_server.listening"
        );
        Ok(())
    }

    /// Closes the listener and waits until its port is released.
    async fn close(&mut self) {
        let Some(running) = self.running.take() else {
            return;
        };
        running.cancel.cancel();
        if let Err(error) = running.task.await {
            tracing::event!(
                tracing::Level::WARN,
                error = %error,
                "compat_server.listener.join_failed"
            );
        }
        tracing::event!(
            tracing::Level::INFO,
            port = running.port,
            "compat_server.stopped"
        );
    }

    fn write_enabled(&self, enabled: bool) -> Result<(), CompatServerError> {
        write_setting(
            &self.settings_path,
            ENABLED_KEY,
            Value::Bool(enabled),
            &PolicySettings::none(),
        )?;
        Ok(())
    }

    fn fail(&mut self, error: &CompatServerError) {
        tracing::event!(
            tracing::Level::WARN,
            error = %error,
            "compat_server.start.failed"
        );
        self.last_error = Some(error.to_string());
    }
}
