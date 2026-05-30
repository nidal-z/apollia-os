//! Registry of shell sessions: one session per agent (identified by [`SessionId`]).

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::Mutex;

use super::session::{PersistentBashError, SessionId, ShellSession};

/// Registry that keeps one [`ShellSession`] alive per [`SessionId`].
///
/// Each session is wrapped in an `Arc<Mutex<ShellSession>>` so that:
/// - The registry lock is held only for the brief HashMap lookup/insert.
/// - The session lock is independent and held only during command execution.
///
/// This follows the principle of one actor, one responsibility: the registry
/// owns the session lifecycle while [`ShellSession`] owns command execution.
pub struct ShellSessionRegistry {
    sessions: Mutex<HashMap<SessionId, Arc<Mutex<ShellSession>>>>,
}

impl ShellSessionRegistry {
    /// Creates an empty registry.
    pub fn new() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
        }
    }

    /// Returns the existing session for `session_id`, or spawns a new one at `cwd`.
    ///
    /// The registry lock is released before the session is returned, so callers can
    /// hold the session lock for an arbitrarily long execution without blocking other
    /// sessions.
    ///
    /// # Errors
    ///
    /// Returns [`PersistentBashError::SpawnFailed`] if a new session cannot be created.
    pub async fn get_or_create(
        &self,
        session_id: SessionId,
        cwd: &str,
    ) -> Result<Arc<Mutex<ShellSession>>, PersistentBashError> {
        // Hold the registry lock only for the HashMap access.
        let mut guard = self.sessions.lock().await;

        if let Some(existing) = guard.get(&session_id) {
            return Ok(existing.clone());
        }

        let session = ShellSession::new(session_id, cwd).await?;
        let arc = Arc::new(Mutex::new(session));
        guard.insert(session_id, arc.clone());

        tracing::debug!(
            session_id = %session_id,
            cwd = %cwd,
            "new persistent_bash session created"
        );

        Ok(arc)
    }

    /// Closes the session identified by `session_id` and kills its shell process.
    ///
    /// A no-op if no session with that ID is registered.
    pub async fn close(&self, session_id: SessionId) {
        let removed = {
            let mut guard = self.sessions.lock().await;
            guard.remove(&session_id)
        };

        if let Some(arc) = removed {
            let mut session = arc.lock().await;
            let _ = session.child_kill().await;
            tracing::debug!(session_id = %session_id, "persistent_bash session closed");
        }
    }

    /// Returns the number of active sessions.
    pub async fn session_count(&self) -> usize {
        self.sessions.lock().await.len()
    }
}

impl Default for ShellSessionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio_util::sync::CancellationToken;
    use uuid::Uuid;

    #[tokio::test]
    async fn test_get_or_create_returns_same_session_for_same_id() {
        // GIVEN
        let registry = ShellSessionRegistry::new();
        let id = Uuid::new_v4();

        // WHEN: two calls with the same session_id
        let arc1 = registry
            .get_or_create(id, "/tmp")
            .await
            .expect("first create failed");
        let arc2 = registry
            .get_or_create(id, "/tmp")
            .await
            .expect("second create failed");

        // THEN: same Arc
        assert!(Arc::ptr_eq(&arc1, &arc2));
        assert_eq!(registry.session_count().await, 1);
    }

    #[tokio::test]
    async fn test_get_or_create_different_ids_produce_different_sessions() {
        // GIVEN
        let registry = ShellSessionRegistry::new();
        let id_a = Uuid::new_v4();
        let id_b = Uuid::new_v4();

        // WHEN
        let arc_a = registry
            .get_or_create(id_a, "/tmp")
            .await
            .expect("create A failed");
        let arc_b = registry
            .get_or_create(id_b, "/tmp")
            .await
            .expect("create B failed");

        // THEN: distinct Arcs
        assert!(!Arc::ptr_eq(&arc_a, &arc_b));
        assert_eq!(registry.session_count().await, 2);
    }

    #[tokio::test]
    async fn test_close_removes_session_from_registry() {
        // GIVEN
        let registry = ShellSessionRegistry::new();
        let id = Uuid::new_v4();
        registry
            .get_or_create(id, "/tmp")
            .await
            .expect("create failed");
        assert_eq!(registry.session_count().await, 1);

        // WHEN
        registry.close(id).await;

        // THEN
        assert_eq!(registry.session_count().await, 0);
    }

    #[tokio::test]
    async fn test_close_noop_for_unknown_session() {
        // GIVEN: empty registry
        let registry = ShellSessionRegistry::new();

        // WHEN: close unknown id, must not panic
        registry.close(Uuid::new_v4()).await;

        // THEN
        assert_eq!(registry.session_count().await, 0);
    }

    #[tokio::test]
    async fn test_session_via_registry_executes_command() {
        // GIVEN
        let registry = ShellSessionRegistry::new();
        let id = Uuid::new_v4();
        let arc = registry
            .get_or_create(id, "/tmp")
            .await
            .expect("create failed");

        // WHEN
        let output = {
            let mut session = arc.lock().await;
            session
                .exec(
                    "echo hello_from_registry",
                    Duration::from_secs(5),
                    CancellationToken::new(),
                )
                .await
                .expect("exec failed")
        };

        // THEN
        assert!(
            output.stdout.contains("hello_from_registry"),
            "stdout: '{}'",
            output.stdout
        );
    }
}
