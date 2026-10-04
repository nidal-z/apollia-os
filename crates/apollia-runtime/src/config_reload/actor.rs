//! The configuration reloader: the one actor that holds the configuration in
//! force and sends its live sections.

use std::path::PathBuf;
use std::time::Duration;

use apollia_core::config::writer::{check_text, fingerprint, key_line, read_snapshot};
use apollia_core::config::{validate, ApolliaConfig, ConfigDiagnostic, PolicySettings};
use apollia_core::RuntimeEvent;
use serde::Serialize;
use tokio::sync::{mpsc, oneshot};
use tracing::{event, Level};

use super::diff::{changed_keys, class_of, split};
use super::{ConfigChannels, ConfigFeed};
use crate::eventbus::EventBusSender;
use apollia_core::config::keys::ReloadClass;

/// Commands waiting for the reloader. A save waits for its application, a
/// file tick is one read of the file: a handful is enough.
const COMMAND_CAPACITY: usize = 16;

/// How long the file stays quiet before a change on disk is read.
const WATCH_DEBOUNCE: Duration = Duration::from_millis(300);

/// A key changed since the start that waits for a restart, with its line in
/// the file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PendingRestartKey {
    /// Dotted key.
    pub key: String,
    /// 1-based line of the key, or of its table, in the file; `None` when the
    /// file no longer writes it (removed, back to its default).
    pub line: Option<usize>,
}

/// What the reloader holds, for the read route and the tests.
#[derive(Debug, Clone, Default)]
pub struct ReloadStatus {
    /// The keys changed since the start that wait for a restart.
    pub pending_restart: Vec<PendingRestartKey>,
    /// The faults of the last text read from disk, when it was refused; empty
    /// once a valid text is applied.
    pub diagnostics: Vec<ConfigDiagnostic>,
    /// How many texts were applied since the start.
    pub applied: u64,
}

enum Command {
    /// The save route wrote this text.
    Saved {
        text: String,
        reply: oneshot::Sender<ReloadStatus>,
    },
    /// The file changed on disk.
    FileChanged { done: Option<oneshot::Sender<()>> },
    /// What the reloader holds.
    Status(oneshot::Sender<ReloadStatus>),
}

/// Clonable handle of the configuration reloader.
#[derive(Clone)]
pub struct ConfigReloadHandle {
    tx: mpsc::Sender<Command>,
    feed: ConfigFeed,
}

impl std::fmt::Debug for ConfigReloadHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConfigReloadHandle").finish_non_exhaustive()
    }
}

impl ConfigReloadHandle {
    /// Starts the reloader over the file at `path`, as it reads now: that text
    /// is the configuration the runtime started on, and the base of the keys
    /// waiting for a restart. A file that does not validate counts as the
    /// defaults, which is what the runtime then runs on.
    ///
    /// `channels` are the section channels the consumers already hold;
    /// `policy` is the organisation policy a text is held to. The actor stops
    /// on `ShutdownRequested` or when every handle is dropped.
    pub fn spawn(
        path: PathBuf,
        channels: ConfigChannels,
        policy: PolicySettings,
        events: &EventBusSender,
    ) -> Self {
        let text = read_snapshot(&path).map(|s| s.text).unwrap_or_default();
        let boot = validate(&text).unwrap_or_default();
        let feed = channels.feed();
        let (tx, rx) = mpsc::channel(COMMAND_CAPACITY);
        let actor = Reloader {
            path,
            policy,
            boot: boot.clone(),
            current: boot,
            text: text.clone(),
            seen_sha: fingerprint(&text),
            diagnostics: Vec::new(),
            channels,
            events: events.clone(),
            applied: 0,
        };
        let bus = apollia_core::events::subscribe_resilient(events, "config.reload");
        tokio::spawn(actor.run(rx, bus));
        Self { tx, feed }
    }

    /// Watches the file and reads it again after each change.
    pub fn watch_file(&self, path: PathBuf) {
        let Some(mut ticks) =
            apollia_triggers::handlers::apollia_config_watch::ApolliaConfigWatcher::spawn(
                path,
                WATCH_DEBOUNCE,
            )
        else {
            return;
        };
        let tx = self.tx.clone();
        tokio::spawn(async move {
            while ticks.recv().await.is_some() {
                if tx.send(Command::FileChanged { done: None }).await.is_err() {
                    break;
                }
            }
        });
    }

    /// The receivers of the live sections.
    pub fn feed(&self) -> ConfigFeed {
        self.feed.clone()
    }

    /// Applies a text the save route has just written, and answers once it is
    /// applied.
    pub async fn saved(&self, text: String) -> ReloadStatus {
        let (reply, rx) = oneshot::channel();
        if self.tx.send(Command::Saved { text, reply }).await.is_err() {
            return ReloadStatus::default();
        }
        rx.await.unwrap_or_default()
    }

    /// Reads the file again, as the watcher does after a change, and answers
    /// once it is read.
    #[cfg(test)]
    pub(crate) async fn file_changed(&self) {
        let (done, rx) = oneshot::channel();
        if self
            .tx
            .send(Command::FileChanged { done: Some(done) })
            .await
            .is_ok()
        {
            let _ = rx.await;
        }
    }

    /// What the reloader holds.
    pub async fn status(&self) -> ReloadStatus {
        let (reply, rx) = oneshot::channel();
        if self.tx.send(Command::Status(reply)).await.is_err() {
            return ReloadStatus::default();
        }
        rx.await.unwrap_or_default()
    }
}

struct Reloader {
    path: PathBuf,
    policy: PolicySettings,
    /// The configuration the runtime started on.
    boot: ApolliaConfig,
    /// The configuration in force.
    current: ApolliaConfig,
    /// The text of `current`, for the lines of the pending keys.
    text: String,
    /// Fingerprint of the last text read or applied: the same text again is
    /// not read twice.
    seen_sha: String,
    diagnostics: Vec<ConfigDiagnostic>,
    channels: ConfigChannels,
    events: EventBusSender,
    applied: u64,
}

impl Reloader {
    async fn run(
        mut self,
        mut rx: mpsc::Receiver<Command>,
        mut bus: apollia_core::events::ResilientReceiver,
    ) {
        loop {
            tokio::select! {
                biased;
                event = bus.recv() => match event {
                    Some(RuntimeEvent::ShutdownRequested) | None => break,
                    Some(_) => {}
                },
                command = rx.recv() => match command {
                    Some(Command::Saved { text, reply }) => {
                        self.offer(text);
                        let _ = reply.send(self.status());
                    }
                    Some(Command::FileChanged { done }) => {
                        self.read_file().await;
                        if let Some(done) = done {
                            let _ = done.send(());
                        }
                    }
                    Some(Command::Status(reply)) => {
                        let _ = reply.send(self.status());
                    }
                    None => break,
                },
            }
        }
        event!(Level::DEBUG, "config.reload.stopped");
    }

    async fn read_file(&mut self) {
        let path = self.path.clone();
        let read = tokio::task::spawn_blocking(move || read_snapshot(&path)).await;
        match read {
            Ok(Ok(snapshot)) => self.offer(snapshot.text),
            Ok(Err(e)) => {
                event!(Level::WARN, error = %e, "config.reload.read.failed");
            }
            Err(e) => {
                event!(Level::WARN, error = %e, "config.reload.read.failed");
            }
        }
    }

    /// Applies `text` unless it was already read; keeps the configuration in
    /// force when it does not validate.
    fn offer(&mut self, text: String) {
        let sha = fingerprint(&text);
        if sha == self.seen_sha {
            event!(Level::DEBUG, "config.reload.unchanged");
            return;
        }
        self.seen_sha = sha;
        let faults = check_text(&text, &self.policy);
        let parsed = if faults.is_empty() {
            validate(&text)
        } else {
            Err(faults)
        };
        match parsed {
            Ok(config) => self.apply(config, text),
            Err(diagnostics) => self.refuse(diagnostics),
        }
    }

    fn apply(&mut self, config: ApolliaConfig, text: String) {
        let changes = split(&self.current, &config);
        self.channels.publish(&config, &changes.sections);
        self.current = config;
        self.text = text;
        self.diagnostics.clear();
        self.applied += 1;
        let pending: Vec<String> = self.pending().into_iter().map(|k| k.key).collect();
        event!(
            Level::INFO,
            sections = %changes.sections.join(","),
            pending_restart = pending.len(),
            "config.reloaded"
        );
        let _ = self.events.send(RuntimeEvent::ConfigReloaded {
            sections: changes.sections,
            pending_restart: pending,
        });
    }

    fn refuse(&mut self, diagnostics: Vec<ConfigDiagnostic>) {
        for d in &diagnostics {
            event!(
                Level::WARN,
                line = d.line,
                key = %d.path,
                detail = %d.message,
                "config.reload.refused"
            );
        }
        self.diagnostics = diagnostics.clone();
        let _ = self.events.send(RuntimeEvent::ConfigInvalid {
            path: self.path.display().to_string(),
            diagnostics,
        });
    }

    /// The keys that differ from the start and wait for a restart.
    fn pending(&self) -> Vec<PendingRestartKey> {
        changed_keys(&self.boot, &self.current)
            .into_iter()
            .filter(|key| class_of(key) == ReloadClass::Restart)
            .map(|key| PendingRestartKey {
                line: key_line(&self.text, &key),
                key,
            })
            .collect()
    }

    fn status(&self) -> ReloadStatus {
        ReloadStatus {
            pending_restart: self.pending(),
            diagnostics: self.diagnostics.clone(),
            applied: self.applied,
        }
    }
}
