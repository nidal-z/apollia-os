//! The state the manager publishes after each change.
//!
//! The actor stays the only writer of its sessions. After every command that
//! changes them, and after every detached call whose outcome it recorded, it
//! publishes a [`McpSnapshot`] on a `watch` channel: the status of every
//! server, its health included, a dormant one as well as a running one, and
//! the tool index. The handle reads the
//! latest one without sending the actor anything, so a list of the
//! connections or the tool index of a chat turn never waits behind a slow
//! tool call.

use std::time::Instant;

use crate::manager::views::build_status;
use crate::manager::{McpClientManager, McpServerStatus};
use crate::tool_search::ToolIndexSnapshot;

/// What the handle reads without a round trip to the actor.
#[derive(Debug, Clone, Default)]
pub(crate) struct McpSnapshot {
    servers: Vec<ServerEntry>,
    tool_index: Vec<ToolIndexSnapshot>,
}

/// One server's status as of the last publication, and the instant its
/// session started, from which the uptime is read at each look; a dormant
/// server has no session, and no uptime.
#[derive(Debug, Clone)]
struct ServerEntry {
    status: McpServerStatus,
    started_at: Option<Instant>,
}

impl McpSnapshot {
    /// The status of every server, its uptime counted to now.
    pub(super) fn statuses(&self) -> Vec<McpServerStatus> {
        self.servers
            .iter()
            .map(|entry| {
                let mut status = entry.status.clone();
                status.uptime_secs = entry.started_at.map(|at| at.elapsed().as_secs());
                status
            })
            .collect()
    }

    /// The tool index across every deferred-mode session.
    pub(super) fn tool_index(&self) -> Vec<ToolIndexSnapshot> {
        self.tool_index.clone()
    }
}

impl McpClientManager {
    /// The state of every server, as the handle reads it: each session, then
    /// each dormant server, which the list shows as known and starting at
    /// its next call rather than as gone.
    fn snapshot(&self) -> McpSnapshot {
        let mut servers: Vec<ServerEntry> = self
            .sessions
            .iter()
            .map(|(name, session)| ServerEntry {
                status: build_status(
                    name,
                    session,
                    self.last_call_at.get(name).map(String::as_str),
                ),
                started_at: Some(session.started_at()),
            })
            .collect();
        servers.extend(
            self.dormant
                .keys()
                .filter(|name| !self.sessions.contains_key(*name))
                .filter_map(|name| self.dormant_status(name))
                .map(|status| ServerEntry {
                    status,
                    started_at: None,
                }),
        );
        McpSnapshot {
            servers,
            tool_index: self.collect_tool_index(),
        }
    }

    /// Publish the state of every session to the readers of the handle.
    pub(super) fn publish(&self) {
        self.published.send_replace(self.snapshot());
    }

    /// Publish an empty state: the actor is stopping, and a handle that
    /// outlives it reads no server, as it did when it asked a stopped actor.
    pub(super) fn publish_empty(&self) {
        self.published.send_replace(McpSnapshot::default());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manager::McpClientManagerHandle;
    use apollia_core::McpHealth;
    use futures::FutureExt;
    use tokio::sync::{mpsc, watch};

    fn status(name: &str) -> McpServerStatus {
        McpServerStatus {
            name: name.to_string(),
            server_info: "srv".to_string(),
            tools_count: 1,
            connected: true,
            dormant: false,
            pid: None,
            uptime_secs: None,
            last_call_at: None,
            error: None,
            package: None,
            transport: "stdio".to_string(),
            health: McpHealth::Healthy { verified: true },
        }
    }

    #[tokio::test]
    async fn test_the_handle_reads_the_published_state_without_the_actor() {
        // GIVEN a handle whose command channel nothing reads, and a state
        // published with one server and one indexed tool
        let (tx, _never_read) = mpsc::channel(1);
        let published = McpSnapshot {
            servers: vec![ServerEntry {
                status: status("notion"),
                started_at: Some(Instant::now()),
            }],
            tool_index: vec![ToolIndexSnapshot {
                server_name: "notion".to_string(),
                tool_name: "search".to_string(),
                description: None,
                tags: vec![],
                input_schema: None,
                ..Default::default()
            }],
        };
        let (_sender, snapshot) = watch::channel(published);
        let handle = McpClientManagerHandle { tx, snapshot };

        // WHEN the statuses and the tool index are polled once
        let statuses = handle.status().now_or_never();
        let index = handle.get_tool_index().now_or_never();

        // THEN both complete at the first poll, from the published state: a
        // read through the actor would never complete, since nothing drains
        // its channel; the uptime is counted to now
        let statuses = statuses.expect("the status read waited on the actor");
        let index = index.expect("the tool index read waited on the actor");
        assert_eq!(statuses.len(), 1);
        assert_eq!(statuses[0].name, "notion");
        assert_eq!(statuses[0].uptime_secs, Some(0));
        assert_eq!(index.len(), 1);
        assert_eq!(index[0].tool_name, "search");
    }

    #[tokio::test]
    async fn test_a_new_publication_replaces_the_state_the_handle_reads() {
        // GIVEN a handle reading an empty state
        let (tx, _never_read) = mpsc::channel(1);
        let (sender, snapshot) = watch::channel(McpSnapshot::default());
        let handle = McpClientManagerHandle { tx, snapshot };
        assert!(handle.status().await.is_empty());

        // WHEN a state with one server is published
        sender.send_replace(McpSnapshot {
            servers: vec![ServerEntry {
                status: status("github"),
                started_at: Some(Instant::now()),
            }],
            tool_index: Vec::new(),
        });

        // THEN the next read sees it
        let statuses = handle.status().await;
        assert_eq!(statuses.len(), 1);
        assert_eq!(statuses[0].name, "github");
    }
}
