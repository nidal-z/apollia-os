//! Where the manager keeps the tools and the resources each server
//! published, between launches.
//!
//! The store is a path to `mcp.db`, never an open connection: each read or
//! write opens the file on a blocking thread, does its one transaction, and
//! closes it. The manager awaits each write in turn, so two answers of the
//! same server are kept in the order they came. A store that cannot be read
//! or written is logged and never fails the manager: the tools of a live
//! session are still served from memory.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use crate::server_repository::{McpRepoError, McpServerRepository, StoredResource, StoredTool};

/// How long a write waits for another connection to the same file.
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

/// The `mcp.db` the manager keeps the tools of its servers in.
#[derive(Debug, Clone)]
pub struct ToolIndexStore {
    path: PathBuf,
}

impl ToolIndexStore {
    /// A store over the `mcp.db` at `path`. Nothing is opened until the
    /// first read or write.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Every kept tool, by server; empty when the file cannot be read.
    pub(crate) async fn load(&self) -> HashMap<String, Vec<StoredTool>> {
        self.with_repository(|repo| repo.stored_tools())
            .await
            .unwrap_or_else(|e| {
                tracing::event!(tracing::Level::WARN, error = %e, "mcp.tools.store.read_failed");
                HashMap::new()
            })
    }

    /// Keep `tools` as everything `server` publishes.
    pub(crate) async fn replace(&self, server: String, tools: Vec<StoredTool>) {
        let count = tools.len();
        let name = server.clone();
        match self
            .with_repository(move |repo| repo.replace_tools(&server, &tools))
            .await
        {
            Ok(()) => tracing::event!(
                tracing::Level::DEBUG,
                server = %name,
                tools_count = count,
                "mcp.tools.store.kept"
            ),
            Err(e) => tracing::event!(
                tracing::Level::WARN,
                server = %name,
                error = %e,
                "mcp.tools.store.write_failed"
            ),
        }
    }

    /// Every kept resource list, by server; a server never asked is absent.
    /// Empty when the file cannot be read.
    pub(crate) async fn load_resources(&self) -> HashMap<String, Vec<StoredResource>> {
        self.with_repository(|repo| repo.stored_resources())
            .await
            .unwrap_or_else(|e| {
                tracing::event!(tracing::Level::WARN, error = %e, "mcp.resources.store.read_failed");
                HashMap::new()
            })
    }

    /// The servers in pause: configured, and not enabled. Empty when the
    /// file cannot be read.
    pub(crate) async fn paused(&self) -> std::collections::HashSet<String> {
        self.with_repository(|repo| repo.disabled_names())
            .await
            .unwrap_or_else(|e| {
                tracing::event!(tracing::Level::WARN, error = %e, "mcp.servers.store.read_failed");
                std::collections::HashSet::new()
            })
    }

    /// Keep `resources`, answered at `fetched_at`, as everything `server`
    /// lists.
    pub(crate) async fn replace_resources(
        &self,
        server: String,
        resources: Vec<StoredResource>,
        fetched_at: String,
    ) {
        let count = resources.len();
        let name = server.clone();
        match self
            .with_repository(move |repo| repo.replace_resources(&server, &resources, &fetched_at))
            .await
        {
            Ok(()) => tracing::event!(
                tracing::Level::DEBUG,
                server = %name,
                resources_count = count,
                "mcp.resources.store.kept"
            ),
            Err(e) => tracing::event!(
                tracing::Level::WARN,
                server = %name,
                error = %e,
                "mcp.resources.store.write_failed"
            ),
        }
    }

    /// Forget the tools and the resources kept for `server`.
    pub(crate) async fn forget(&self, server: String) {
        let name = server.clone();
        if let Err(e) = self
            .with_repository(move |repo| {
                repo.forget_tools(&server)?;
                repo.forget_resources(&server)
            })
            .await
        {
            tracing::event!(
                tracing::Level::WARN,
                server = %name,
                error = %e,
                "mcp.tools.store.write_failed"
            );
        }
    }

    /// Open the file on a blocking thread and run `op` on it.
    async fn with_repository<T, F>(&self, op: F) -> Result<T, McpRepoError>
    where
        T: Send + 'static,
        F: FnOnce(&McpServerRepository) -> Result<T, McpRepoError> + Send + 'static,
    {
        let path = self.path.clone();
        tokio::task::spawn_blocking(move || {
            let repo = McpServerRepository::open(&path)?;
            repo.wait_when_busy(BUSY_TIMEOUT)?;
            op(&repo)
        })
        .await
        .map_err(|e| McpRepoError::Serialization(format!("tool store task failed: {e}")))?
    }
}
