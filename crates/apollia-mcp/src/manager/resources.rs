//! The resources of the servers, running or not.
//!
//! Each `resources/list` a session answers is kept, as its tools are: in the
//! actor, and in `mcp.db` when the manager has a store. A dormant server
//! offers the list it last answered without starting. A dormant server never
//! asked has nothing to offer yet: a listing starts it in a task, answers
//! with the others at once, and its list, kept when it arrives, joins the
//! next listing. Reading a resource of a dormant server starts it, as a tool
//! call does; a paused server never starts and answers that it is paused.

use tokio::sync::oneshot;

use crate::manager::kept::fetched_now;
use crate::manager::{McpClientManager, McpResourcePayload, McpResourceSummary};
use crate::sanitize::sanitize_resources;
use crate::server_repository::StoredResource;
use crate::session::{McpSession, McpSessionError};

/// The answer of a resource read.
pub(super) type ReadReply = oneshot::Sender<Result<McpResourcePayload, McpSessionError>>;

/// A resource read waiting for its server's handshake.
pub(super) struct WaitingRead {
    pub(super) uri: String,
    pub(super) reply: ReadReply,
}

/// What `session` lists, validated and bounded by `sanitize` before anything
/// keeps or serves it; an empty list for a server that declares no
/// resources, `None` when the list failed.
pub(super) async fn list_of(session: &McpSession) -> Option<Vec<StoredResource>> {
    if session.capabilities().resources.is_none() {
        return Some(Vec::new());
    }
    match session.list_resources().await {
        Ok(result) => Some(sanitize_resources(result.resources, &session.config().name)),
        Err(e) => {
            tracing::event!(
                tracing::Level::WARN,
                server = %session.config().name,
                error = %e,
                "mcp.resources_list_failed"
            );
            None
        }
    }
}

fn summaries(server: &str, resources: &[StoredResource]) -> Vec<McpResourceSummary> {
    resources
        .iter()
        .map(|r| McpResourceSummary {
            server: server.to_string(),
            uri: r.uri.clone(),
            name: r.name.clone(),
            mime_type: r.mime_type.clone(),
            description: r.description.clone(),
        })
        .collect()
}

impl McpClientManager {
    /// Keep `resources` as what `name` lists, when it changed. A server a
    /// request brought is never kept.
    pub(super) async fn keep_resources(&mut self, name: &str, resources: Vec<StoredResource>) {
        if self.request_servers.contains(name) || self.kept_resources.get(name) == Some(&resources)
        {
            return;
        }
        if let Some(store) = &self.tool_store {
            store
                .replace_resources(name.to_string(), resources.clone(), fetched_now())
                .await;
        }
        self.kept_resources.insert(name.to_string(), resources);
    }

    /// List the resources of the live session of `name` and keep them.
    pub(super) async fn keep_live_resources(&mut self, name: &str) {
        let Some(session) = self.sessions.get(name) else {
            return;
        };
        if let Some(resources) = list_of(session).await {
            self.keep_resources(name, resources).await;
        }
    }

    /// `name` was put in pause: nothing starts it, its kept list stays.
    pub(super) fn note_paused(&mut self, name: &str) {
        self.paused.insert(name.to_string());
    }

    /// `name` was removed: its kept list goes.
    pub(super) fn forget_resources(&mut self, name: &str) {
        self.paused.remove(name);
        self.kept_resources.remove(name);
    }

    /// Every resource of every server the manager runs or keeps dormant.
    ///
    /// A live session is asked, and its answer kept; a dormant server
    /// answers its kept list, without starting. A dormant server never asked
    /// is started in a task: the listing does not wait for it; one whose last
    /// start failed is left alone, so a server out of reach is not started
    /// again at each opening of the @ picker. A paused server offers nothing. A per-server failure is logged and skipped.
    pub(super) async fn handle_list_resources(&mut self) -> Vec<McpResourceSummary> {
        let mut out: Vec<McpResourceSummary> = Vec::new();
        let live: Vec<String> = self.sessions.keys().cloned().collect();
        for name in live {
            let Some(session) = self.sessions.get(&name) else {
                continue;
            };
            let Some(resources) = list_of(session).await else {
                continue;
            };
            out.extend(summaries(&name, &resources));
            self.keep_resources(&name, resources).await;
        }
        let mut never_asked = Vec::new();
        for (name, dormant) in &self.dormant {
            match self.kept_resources.get(name) {
                Some(kept) => out.extend(summaries(name, kept)),
                None if dormant.start_failed => {}
                None => never_asked.push(name.clone()),
            }
        }
        for name in never_asked {
            tracing::event!(
                tracing::Level::INFO,
                server = %name,
                "mcp.resources.starting_unlisted"
            );
            self.begin_start(&name);
        }
        out
    }

    /// The server that offers `uri`: a live session that lists it, else a
    /// server whose kept list holds it.
    async fn resolve_resource_server(&self, uri: &str) -> Option<String> {
        for (server_name, session) in &self.sessions {
            if let Some(listed) = list_of(session).await {
                if listed.iter().any(|r| r.uri == uri) {
                    return Some(server_name.clone());
                }
            }
        }
        let mut kept: Vec<(&String, &Vec<StoredResource>)> = self.kept_resources.iter().collect();
        kept.sort_by(|a, b| a.0.cmp(b.0));
        kept.into_iter()
            .find(|(name, list)| {
                !self.sessions.contains_key(*name) && list.iter().any(|r| r.uri == uri)
            })
            .map(|(name, _)| name.clone())
    }

    /// Read `uri` from `server_name`, or from the server that offers it.
    ///
    /// A dormant server starts, and the read answers once its handshake is
    /// done; a paused server answers [`McpSessionError::ServerPaused`] and
    /// does not start.
    pub(super) async fn handle_read_resource(
        &mut self,
        server_name: Option<String>,
        uri: String,
        reply: ReadReply,
    ) {
        let resolved = match server_name {
            Some(name) => name,
            None => match self.resolve_resource_server(&uri).await {
                Some(name) => name,
                None => {
                    let _ = reply.send(Err(McpSessionError::ServerExited {
                        server: format!("(none exposes resource {uri})"),
                    }));
                    return;
                }
            },
        };
        if self.paused.contains(&resolved) {
            let _ = reply.send(Err(McpSessionError::ServerPaused { server: resolved }));
            return;
        }
        if self.sessions.contains_key(&resolved) {
            let _ = reply.send(self.read_live(&resolved, &uri).await);
            return;
        }
        let Some(dormant) = self.dormant.get_mut(&resolved) else {
            let _ = reply.send(Err(McpSessionError::ServerExited { server: resolved }));
            return;
        };
        dormant.waiting_reads.push(WaitingRead { uri, reply });
        self.begin_start(&resolved);
    }

    /// Read `uri` on the live session of `name`.
    pub(super) async fn read_live(
        &mut self,
        name: &str,
        uri: &str,
    ) -> Result<McpResourcePayload, McpSessionError> {
        let session = self
            .sessions
            .get(name)
            .ok_or_else(|| McpSessionError::ServerExited {
                server: name.to_string(),
            })?;
        let result = session.read_resource(uri).await?;
        self.mark_used(name);
        let mime_type = result.contents.iter().find_map(|c| c.mime_type.clone());
        let text = result
            .contents
            .iter()
            .filter_map(|c| c.text.as_deref())
            .collect::<Vec<_>>()
            .join("\n");
        Ok(McpResourcePayload {
            server: name.to_string(),
            uri: uri.to_string(),
            mime_type,
            text,
        })
    }
}
