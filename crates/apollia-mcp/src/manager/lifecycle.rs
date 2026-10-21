//! What the manager says when a session starts, restarts or ends.
//!
//! A session that starts announces its health, whether it started or not, so
//! a reader of the bus learns that a server once unreachable answers again. A
//! session whose transport closes on its own (the process exited, the
//! connection dropped) is seen by a watcher of its dispatch task: the actor
//! marks it unavailable and announces it, instead of reading it healthy until
//! the next call fails.

use std::time::Instant;

use apollia_core::McpHealth;

use crate::manager::{McpClientManager, McpCommand};
use crate::session::{McpSession, McpSessionError};

/// Reason a session whose transport closed on its own is unavailable.
const DISPATCH_ENDED: &str = "process_exited";

impl McpClientManager {
    /// Watch the dispatch task of `session`, and tell the actor when it ends.
    ///
    /// The watcher holds the command channel weakly, so it never keeps a
    /// stopped actor alive. A session that is shut down on purpose ends its
    /// dispatch task as well; the actor then no longer holds that session,
    /// and the report is ignored.
    pub(super) fn watch_session_end(&self, name: &str, session: &McpSession) {
        let mut ended = session.dispatch_ended();
        let started_at = session.started_at();
        let commands = self.commands.clone();
        let server_name = name.to_string();
        tokio::spawn(async move {
            // An error means the dispatch task is gone without saying so:
            // the session cannot answer either way.
            let _ = ended.wait_for(|ended| *ended).await;
            if let Some(tx) = commands.upgrade() {
                let _ = tx
                    .send(McpCommand::SessionEnded {
                        server_name,
                        started_at,
                    })
                    .await;
            }
        });
    }

    /// Mark the session `name` that started at `started_at` unavailable, and
    /// announce it, when it is still the one the actor holds.
    pub(super) fn session_ended(&mut self, name: &str, started_at: Instant) {
        let Some(session) = self.sessions.get_mut(name) else {
            return;
        };
        if session.started_at() != started_at {
            return;
        }
        let health = McpHealth::Unavailable {
            reason: DISPATCH_ENDED.to_string(),
        };
        if *session.health() == health {
            return;
        }
        tracing::event!(tracing::Level::WARN, server = %name, "mcp.session.dispatch_ended");
        session.set_health(health.clone());
        self.emit_health_changed(name, health);
        self.publish();
    }

    /// Announce the health a start of `name` left: the new session's, or the
    /// one its failure says.
    pub(super) fn announce_start<T>(&self, name: &str, result: &Result<T, McpSessionError>) {
        let health = match result {
            Ok(_) => self
                .sessions
                .get(name)
                .map_or(McpHealth::Healthy { verified: false }, |s| {
                    s.health().clone()
                }),
            Err(e) => crate::health::from_start_error(e),
        };
        self.emit_health_changed(name, health);
    }
}
