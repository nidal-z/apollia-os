//! The tool Apollia calls, in a conversation, to learn what connection is
//! missing for a request.
//!
//! The curated selection the host hands over (the desktop's, see
//! [`ConnectionSelection`]) is searched on this machine: nothing goes to the
//! network, nothing is added, nothing is granted. Each entry that matches is
//! answered with where it stands: not connected, connected but not granted to
//! the agent, or connected and granted. An entry the organisation policy would
//! refuse to add is never answered. Every match names its entry, which is what
//! the add flow of the interface opens.
//!
//! The tool is offered to Apollia only, in a conversation, and only when a
//! selection was handed over. Nothing about the missing connections is written
//! into the prompt: the model asks when it needs to, and the answer comes back
//! as the result of its own call.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

use apollia_mcp::config::McpServerConfig;
use apollia_mcp::server_repository::McpServerRepository;
use apollia_tools::executor::{ToolExecutionError, ToolExecutor};
use serde::Serialize;
use serde_json::Value;

use crate::governance::{AdmissionDecision, AdmissionRequest, ConnectionGovernance};

/// Name under which the tool is advertised and dispatched.
pub const CONNECTION_LOOKUP_NAME: &str = "connection_lookup";

/// Matches returned when the call names no `limit`.
const DEFAULT_LIMIT: usize = 5;

/// Largest `limit` a call may ask for.
const MAX_LIMIT: usize = 20;

/// Shortest word of a query that is searched; shorter ones are ignored.
const MIN_TERM_CHARS: usize = 2;

/// One entry of the curated selection, as the host describes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionEntry {
    /// Identifier of the entry, the one the add flow opens
    /// (`@notionhq/notion-mcp-server`). A connection made from it records it as
    /// its origin entry.
    pub id: String,
    /// Name of the service (`Notion`).
    pub label: String,
    /// What the service lets an agent do.
    pub description: String,
    /// Further words the entry is found by: its labels and descriptions in
    /// other languages, its category, its registry names.
    pub keywords: Vec<String>,
    /// What the organisation policy is asked about before the entry is added.
    pub admission: AdmissionRequest,
}

/// The curated selection, with the governance that decides which of its
/// entries may be offered.
#[derive(Clone)]
pub struct ConnectionSelection {
    entries: Vec<SelectionEntry>,
    governance: ConnectionGovernance,
}

impl ConnectionSelection {
    /// The selection `entries`, filtered at each search by `governance`.
    #[must_use]
    pub fn new(entries: Vec<SelectionEntry>, governance: ConnectionGovernance) -> Self {
        Self {
            entries,
            governance,
        }
    }

    /// Number of entries handed over.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether no entry was handed over.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// The selection a conversation exchange of `agent_id` searches: Apollia's
/// alone, no other agent's.
#[must_use]
pub fn offered_to(
    selection: Option<Arc<ConnectionSelection>>,
    agent_id: &str,
) -> Option<Arc<ConnectionSelection>> {
    selection.filter(|_| agent_id == crate::chat::APOLLIA_CHAT_AGENT_ID)
}

/// Where an entry of the selection stands for the agent that asks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum EntryStatus {
    /// No connection was made from the entry.
    NotConnected,
    /// A connection was made from the entry, and none of its tools is granted
    /// to the agent.
    ConnectedNotGranted {
        /// Identifier of that connection (`mcp:<server>`).
        connection: String,
    },
    /// A connection was made from the entry, and the agent is granted tools of
    /// it.
    Granted {
        /// Identifier of that connection (`mcp:<server>`).
        connection: String,
    },
}

/// One entry that matches a search.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LookupMatch {
    /// Identifier of the entry, the one the add flow opens.
    pub entry: String,
    /// Name of the service.
    pub label: String,
    /// What the service lets an agent do.
    pub description: String,
    /// Where the entry stands.
    #[serde(flatten)]
    pub status: EntryStatus,
    /// The same, in one line the model can repeat.
    pub summary: String,
}

/// The entries of `selection` that `query` finds, best first, at most `limit`.
///
/// `connected` are the MCP servers configured on this machine, `granted` the
/// access list of the agent that asks. An entry the policy refuses is left out,
/// whatever it matches.
#[must_use]
pub fn lookup(
    selection: &ConnectionSelection,
    query: &str,
    limit: usize,
    connected: &[McpServerConfig],
    granted: &[String],
) -> Vec<LookupMatch> {
    let terms = terms_of(query);
    if terms.is_empty() {
        return Vec::new();
    }
    let mut scored: Vec<(usize, &SelectionEntry)> = selection
        .entries
        .iter()
        .filter_map(|entry| {
            let score = score_of(entry, &terms);
            (score > 0).then_some((score, entry))
        })
        .filter(|(_, entry)| {
            matches!(
                selection.governance.admit(&entry.admission),
                AdmissionDecision::Admitted
            )
        })
        .collect();
    scored.sort_by(|(a, ea), (b, eb)| b.cmp(a).then_with(|| ea.label.cmp(&eb.label)));
    scored
        .into_iter()
        .take(limit)
        .map(|(_, entry)| matched(entry, connected, granted))
        .collect()
}

/// The lower-cased words of `query` long enough to be searched.
fn terms_of(query: &str) -> Vec<String> {
    query
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().count() >= MIN_TERM_CHARS)
        .map(str::to_lowercase)
        .collect()
}

/// How many of `terms` the entry holds, in its identifier, name, description
/// or keywords.
fn score_of(entry: &SelectionEntry, terms: &[String]) -> usize {
    let haystack = std::iter::once(entry.id.as_str())
        .chain(std::iter::once(entry.label.as_str()))
        .chain(std::iter::once(entry.description.as_str()))
        .chain(entry.keywords.iter().map(String::as_str))
        .map(str::to_lowercase)
        .collect::<Vec<_>>()
        .join("\n");
    terms
        .iter()
        .filter(|term| haystack.contains(term.as_str()))
        .count()
}

/// `entry`, with where it stands among `connected` for an agent granted
/// `granted`.
fn matched(
    entry: &SelectionEntry,
    connected: &[McpServerConfig],
    granted: &[String],
) -> LookupMatch {
    let server = connected.iter().find(|config| {
        config
            .origin
            .as_ref()
            .and_then(|origin| origin.entry.as_deref())
            == Some(entry.id.as_str())
    });
    let (status, summary) = match server {
        None => (
            EntryStatus::NotConnected,
            format!("not connected: {}", entry.id),
        ),
        Some(config) => {
            let connection = format!("mcp:{}", config.name);
            let prefix = format!("{connection}/");
            if granted.iter().any(|access| access.starts_with(&prefix)) {
                (
                    EntryStatus::Granted {
                        connection: connection.clone(),
                    },
                    format!("connected and granted to you: {connection}"),
                )
            } else {
                (
                    EntryStatus::ConnectedNotGranted {
                        connection: connection.clone(),
                    },
                    format!("connected but not granted to you: {connection}"),
                )
            }
        }
    };
    LookupMatch {
        entry: entry.id.clone(),
        label: entry.label.clone(),
        description: entry.description.clone(),
        status,
        summary,
    }
}

/// The MCP servers configured in `mcp_db`; none when the file does not exist
/// or cannot be read. The file is never created here.
fn configured_servers(mcp_db: &Path) -> Vec<McpServerConfig> {
    if !mcp_db.is_file() {
        return Vec::new();
    }
    match McpServerRepository::open(mcp_db).and_then(|repo| repo.list()) {
        Ok(configs) => configs,
        Err(e) => {
            tracing::event!(
                tracing::Level::WARN,
                error = %e,
                detail = "every entry reads as not connected",
                "chat.connection_lookup.servers_unreadable"
            );
            Vec::new()
        }
    }
}

/// Description of the tool, as the model reads it.
pub const CONNECTION_LOOKUP_DESCRIPTION: &str = "Search the connections the user can add to \
     Apollia (its curated selection of services), when none of your tools can do what the user \
     asks. Reads this machine only. Each match names its `entry` and says where it stands: \
     `not_connected` (the user can add it from that entry), `connected_not_granted` \
     (the user can grant it to you on your sheet) or `granted` (its tools are yours: find them \
     with tool_search). Tell the user what is missing instead of answering that you cannot. \
     Takes a `query` of a few words naming the service or the task (`calendar`, `notion pages`) \
     and an optional `limit` between 1 and 20.";

/// JSON Schema of the tool's input.
#[must_use]
pub fn connection_lookup_input_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "query": {
                "type": "string",
                "description": "A few words naming the service or the task, e.g. `calendar` or `notion pages`."
            },
            "limit": {
                "type": "integer",
                "description": "Maximum number of entries to return. Default: 5. Max: 20.",
                "default": DEFAULT_LIMIT
            }
        },
        "required": ["query"]
    })
}

/// The executor of [`CONNECTION_LOOKUP_NAME`] for one exchange.
pub struct ConnectionLookupExecutor {
    selection: Arc<ConnectionSelection>,
    mcp_db: PathBuf,
    granted: Vec<String>,
}

impl ConnectionLookupExecutor {
    /// Searches `selection` for an agent granted `granted`, the servers
    /// configured read from `mcp_db` at each call.
    #[must_use]
    pub fn new(selection: Arc<ConnectionSelection>, mcp_db: PathBuf, granted: Vec<String>) -> Self {
        Self {
            selection,
            mcp_db,
            granted,
        }
    }
}

impl ToolExecutor for ConnectionLookupExecutor {
    fn name(&self) -> &str {
        CONNECTION_LOOKUP_NAME
    }

    fn is_read_only(&self) -> bool {
        true
    }

    /// Returns `{"matches": [...], "total": N}`.
    ///
    /// # Errors
    ///
    /// [`ToolExecutionError::InvalidInput`] when `query` holds no word to
    /// search, or `limit` is `0` or above 20.
    fn execute(
        &self,
        input: Value,
    ) -> Pin<Box<dyn Future<Output = Result<Value, ToolExecutionError>> + Send + '_>> {
        Box::pin(async move {
            let query = input
                .get("query")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned();
            if terms_of(&query).is_empty() {
                return Err(ToolExecutionError::InvalidInput {
                    message: "query must name the service or the task in a word or more".into(),
                });
            }
            let limit = input
                .get("limit")
                .and_then(Value::as_u64)
                .map_or(DEFAULT_LIMIT, |n| usize::try_from(n).unwrap_or(usize::MAX));
            if limit == 0 || limit > MAX_LIMIT {
                return Err(ToolExecutionError::InvalidInput {
                    message: format!("limit must be between 1 and {MAX_LIMIT} (got {limit})"),
                });
            }
            let mcp_db = self.mcp_db.clone();
            let connected = tokio::task::spawn_blocking(move || configured_servers(&mcp_db))
                .await
                .unwrap_or_default();
            let matches = lookup(&self.selection, &query, limit, &connected, &self.granted);
            let total = matches.len();
            tracing::event!(
                tracing::Level::DEBUG,
                total = total,
                "chat.connection_lookup.searched"
            );
            Ok(serde_json::json!({ "matches": matches, "total": total }))
        })
    }
}

#[cfg(test)]
mod tests;
