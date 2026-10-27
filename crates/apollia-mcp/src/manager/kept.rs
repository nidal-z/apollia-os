//! The tools the manager keeps for a server between launches.
//!
//! Each `tools/list` a session answers at its start is written to the
//! [`ToolIndexStore`], and at the next launch a server whose session did not
//! start still contributes the tools it last published to the tool index, in
//! deferred loading, classified as they were. A server that starts again
//! replaces them with its new answer; a server that is removed forgets them.
//!
//! [`ToolIndexStore`]: crate::tool_store::ToolIndexStore

use crate::config::McpServerConfig;
use crate::manager::views::{index_snapshot, IndexedTool, ServerHints};
use crate::manager::McpClientManager;
use crate::server_repository::StoredTool;
use crate::session::{LoadingMode, McpSession};
use crate::tool_search::ToolIndexSnapshot;

/// The instant a `tools/list` answer is kept under, RFC 3339.
pub(super) fn fetched_now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

/// The index entries of the tools kept for the server `config` describes.
pub(super) fn kept_snapshots(
    config: &McpServerConfig,
    tools: &[StoredTool],
) -> Vec<ToolIndexSnapshot> {
    tools
        .iter()
        .map(|tool| {
            index_snapshot(
                config,
                ServerHints::Read,
                IndexedTool {
                    name: &tool.name,
                    description: tool.description.clone(),
                    annotations: tool.annotations,
                    input_schema: Some(tool.input_schema.clone()),
                },
            )
        })
        .collect()
}

/// Keep what `session` of `name` published, when there is a store.
pub(super) async fn keep_session_tools(
    store: Option<&crate::tool_store::ToolIndexStore>,
    name: &str,
    session: &McpSession,
) {
    if let Some(store) = store {
        store
            .replace(name.to_string(), session.stored_tools(&fetched_now()))
            .await;
    }
}

impl McpClientManager {
    /// The index entries of `name` as the actor knows them now: its live
    /// session's, or the ones kept for it.
    pub(super) fn index_of(&self, name: &str) -> Vec<ToolIndexSnapshot> {
        self.collect_tool_index()
            .into_iter()
            .filter(|entry| entry.server_name == name)
            .collect()
    }

    /// After a command started, restarted or reloaded `name`: keep what its
    /// new session published, tools and resources, or, when no session is left, go on serving
    /// the entries it had before the command (`before`).
    pub(super) async fn settle_kept(&mut self, name: &str, before: Vec<ToolIndexSnapshot>) {
        if self.request_servers.contains(name) {
            return;
        }
        if let Some(session) = self.sessions.get(name) {
            self.kept_index.remove(name);
            keep_session_tools(self.tool_store.as_ref(), name, session).await;
            self.keep_live_resources(name).await;
        } else if self.loading_mode == LoadingMode::Deferred && !before.is_empty() {
            self.kept_index.insert(name.to_string(), before);
        }
    }

    /// `name` was removed: forget the tools kept for it.
    pub(super) async fn forget_kept(&mut self, name: &str) {
        self.kept_index.remove(name);
        if let Some(store) = &self.tool_store {
            store.forget(name.to_string()).await;
        }
    }
}
