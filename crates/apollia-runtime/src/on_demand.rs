//! Agents loaded on their first call and unloaded once nobody calls them.
//!
//! An installed agent is registered at boot from the manifest its store
//! holds, without importing anything. Its backend is an [`OnDemandBackend`]:
//! the first execution builds the real backend (for a Python agent, the
//! import of its module and the bridge around it), the executions after it
//! reuse that backend, and once no call has started for the idle delay the
//! backend is dropped, with every reference it held. The next call loads it
//! again.
//!
//! One small actor per agent owns the loaded backend; executions borrow a
//! clone of it for their duration. A backend still borrowed by an execution
//! is never unloaded by the idle check.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};

use apollia_core::{AIPResult, AIPTask};
use tokio::sync::{mpsc, oneshot};

use crate::coordinator::{CancellationToken, DynBackend, ExecutionBackend};

/// How often the actor checks whether its agent has gone idle.
const IDLE_CHECK_PERIOD: Duration = Duration::from_secs(30);

/// Mailbox capacity of one agent's actor.
const CAPACITY: usize = 64;

/// Builds the real backend of one agent. Called on a blocking thread, since
/// it may import Python.
pub type BackendLoader = Arc<dyn Fn() -> DynBackend + Send + Sync>;

/// Errors of an on-demand execution.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum OnDemandError {
    /// The actor that holds the agent has stopped.
    #[error("agent '{0}' cannot be loaded: its loader has stopped")]
    ActorDead(String),
    /// Building the backend failed before it returned.
    #[error("agent '{agent}' failed to load: {reason}")]
    LoadFailed {
        /// The agent's name.
        agent: String,
        /// What went wrong.
        reason: String,
    },
}

enum Command {
    Acquire {
        reply: oneshot::Sender<Result<DynBackend, String>>,
    },
    Released {
        at: Instant,
    },
    IdleCheck {
        now: Instant,
        reply: Option<oneshot::Sender<bool>>,
    },
    Unload,
}

/// An execution backend loaded on its first call and unloaded when idle.
///
/// Cheap to clone: every clone talks to the same actor. The actor stops, and
/// drops what it loaded, once the last clone is gone.
#[derive(Clone)]
pub struct OnDemandBackend {
    agent: Arc<str>,
    tx: mpsc::Sender<Command>,
}

impl OnDemandBackend {
    /// Spawns the actor of `agent`, which builds its backend with `loader` on
    /// the first call and drops it after `idle` without a call.
    ///
    /// Must be called within a Tokio runtime.
    pub fn spawn(agent: &str, loader: BackendLoader, idle: Duration) -> Self {
        let (tx, rx) = mpsc::channel(CAPACITY);
        let slot = Slot {
            agent: agent.to_string(),
            loader,
            loaded: None,
            last_call: Instant::now(),
            idle,
        };
        tokio::spawn(slot.run(rx));
        Self {
            agent: Arc::from(agent),
            tx,
        }
    }

    /// Runs the idle check as if the clock read `now`, and answers whether
    /// the agent is still loaded afterwards.
    ///
    /// The actor runs the same check on its own every thirty seconds with the
    /// real clock; this entry lets a caller drive it at a chosen instant.
    ///
    /// TEST-ONLY: production relies on the actor's own thirty-second check;
    /// this entry is how a test reads the clock it controls.
    ///
    /// # Errors
    ///
    /// The actor has stopped.
    pub async fn check_idle_at(&self, now: Instant) -> Result<bool, OnDemandError> {
        let (reply, answer) = oneshot::channel();
        self.tx
            .send(Command::IdleCheck {
                now,
                reply: Some(reply),
            })
            .await
            .map_err(|_| self.dead())?;
        answer.await.map_err(|_| self.dead())
    }

    fn dead(&self) -> OnDemandError {
        OnDemandError::ActorDead(self.agent.to_string())
    }

    async fn acquire(&self) -> Result<DynBackend, OnDemandError> {
        let (reply, answer) = oneshot::channel();
        self.tx
            .send(Command::Acquire { reply })
            .await
            .map_err(|_| self.dead())?;
        answer
            .await
            .map_err(|_| self.dead())?
            .map_err(|reason| OnDemandError::LoadFailed {
                agent: self.agent.to_string(),
                reason,
            })
    }
}

/// Tells the actor that one execution has ended, even when the execution is
/// dropped before it returns.
struct Lease {
    tx: mpsc::Sender<Command>,
}

impl Drop for Lease {
    fn drop(&mut self) {
        let _ = self.tx.try_send(Command::Released { at: Instant::now() });
    }
}

impl ExecutionBackend for OnDemandBackend {
    fn execute(
        &self,
        task: AIPTask,
    ) -> Pin<Box<dyn Future<Output = Result<AIPResult, String>> + Send>> {
        self.execute_cancellable(task, CancellationToken::new())
    }

    fn execute_cancellable(
        &self,
        task: AIPTask,
        cancel: CancellationToken,
    ) -> Pin<Box<dyn Future<Output = Result<AIPResult, String>> + Send>> {
        let this = self.clone();
        Box::pin(async move {
            let backend = this.acquire().await.map_err(|e| e.to_string())?;
            let lease = Lease {
                tx: this.tx.clone(),
            };
            let result = backend.execute_cancellable(task, cancel).await;
            drop(backend);
            drop(lease);
            result
        })
    }

    fn unload(&self) {
        let _ = self.tx.try_send(Command::Unload);
    }
}

/// The state the actor owns: the backend when it is loaded, and when the
/// agent was last called.
struct Slot {
    agent: String,
    loader: BackendLoader,
    loaded: Option<DynBackend>,
    last_call: Instant,
    idle: Duration,
}

impl Slot {
    async fn run(mut self, mut rx: mpsc::Receiver<Command>) {
        let mut ticker = tokio::time::interval(IDLE_CHECK_PERIOD);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                command = rx.recv() => {
                    let Some(command) = command else { break };
                    self.handle(command).await;
                }
                _ = ticker.tick() => self.check_idle(Instant::now()),
            }
        }
    }

    async fn handle(&mut self, command: Command) {
        match command {
            Command::Acquire { reply } => {
                self.last_call = Instant::now();
                let _ = reply.send(self.load().await);
            }
            Command::Released { at } => {
                if at > self.last_call {
                    self.last_call = at;
                }
            }
            Command::IdleCheck { now, reply } => {
                self.check_idle(now);
                if let Some(reply) = reply {
                    let _ = reply.send(self.loaded.is_some());
                }
            }
            Command::Unload => self.drop_backend("parked"),
        }
    }

    async fn load(&mut self) -> Result<DynBackend, String> {
        if let Some(backend) = &self.loaded {
            return Ok(backend.clone());
        }
        let loader = Arc::clone(&self.loader);
        let started = Instant::now();
        let backend = tokio::task::spawn_blocking(move || loader())
            .await
            .map_err(|e| e.to_string())?;
        tracing::event!(
            tracing::Level::INFO,
            agent = %self.agent,
            elapsed_ms = started.elapsed().as_millis() as u64,
            "agent.loaded"
        );
        self.loaded = Some(backend.clone());
        Ok(backend)
    }

    /// Drops the backend when no call started within the idle delay and no
    /// execution still holds it.
    fn check_idle(&mut self, now: Instant) {
        let Some(backend) = &self.loaded else {
            return;
        };
        let in_use = Arc::strong_count(&backend.0) > 1;
        let idle_for = now.saturating_duration_since(self.last_call);
        if !in_use && idle_for >= self.idle {
            self.drop_backend("idle");
        }
    }

    fn drop_backend(&mut self, reason: &'static str) {
        if self.loaded.take().is_some() {
            tracing::event!(
                tracing::Level::INFO,
                agent = %self.agent,
                reason,
                "agent.unloaded"
            );
        }
    }
}

#[cfg(test)]
mod tests;
