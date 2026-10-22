//! Synthetic `tool_search` tool: substring search over the deferred tool index.
//!
//! In [`LoadingMode::Deferred`] the runtime keeps only a lightweight index of
//! MCP tool names and descriptions. The synthetic `tool_search` tool lets a
//! ReAct agent find tools by intent without paying the context cost of every
//! schema. It is never registered on a real MCP server; it is injected by the
//! runtime alongside native tools when deferred mode is active.
//!
//! Search is a case-insensitive substring match over tool name, description, and
//! server tags. No embeddings, no vector store: stdlib substring matching only.
//!
//! [`LoadingMode::Deferred`]: crate::session::LoadingMode::Deferred

use apollia_tools::executor::{ToolExecutionError, ToolExecutor};
use apollia_tools::{ActionFamily, FamilySource};
use serde_json::Value;

use crate::protocol::McpToolAnnotations;
use std::future::Future;
use std::pin::Pin;

/// Name under which the synthetic search tool is advertised and dispatched.
pub const TOOL_SEARCH_NAME: &str = "tool_search";

/// Upper bound the pure [`search_index`] accepts for `limit`.
const MAX_SEARCH_LIMIT: usize = 500;

/// Default `limit` applied when the caller omits it.
const DEFAULT_SEARCH_LIMIT: usize = 5;

// ─── types ─────────────────────────────────────────────────────────────────

/// A single entry in the aggregated tool index used for search.
///
/// Produced from all connected MCP sessions running in deferred mode. `tags`
/// carries the owning server's configured tags so a query can match by tag as
/// well as by tool name or description.
#[derive(Debug, Clone, Default)]
pub struct ToolIndexSnapshot {
    /// MCP server name (e.g. `"notion"`).
    pub server_name: String,
    /// Tool name within the server (e.g. `"search_pages"`).
    pub tool_name: String,
    /// Human-readable description, when provided by the server.
    pub description: Option<String>,
    /// Tags configured on the owning server, used as extra search terms.
    pub tags: Vec<String>,
    /// The tool's JSON input schema, when the owning session has it cached.
    ///
    /// Carried so a search result can tell the model what arguments the tool
    /// takes. Without it the model learns a callable name and still has to
    /// guess its parameters, which is a call that fails on the server side.
    pub input_schema: Option<Value>,
    /// The behaviour hints the server declared for the tool, if any.
    pub annotations: Option<McpToolAnnotations>,
    /// The tool's family, decided by the manager as the registry would decide
    /// it: the selection entry first, then the server's hints, else
    /// unclassified. A deferred tool has no descriptor, so this is what the
    /// list of connections and governance read.
    pub family: ActionFamily,
    /// Where [`ToolIndexSnapshot::family`] came from.
    pub family_source: FamilySource,
}

impl ToolIndexSnapshot {
    /// The fully qualified name this tool answers to, `mcp:{server}/{tool}`.
    #[must_use]
    pub fn full_name(&self) -> String {
        format!("mcp:{}/{}", self.server_name, self.tool_name)
    }
}

/// The family of the tool `name` as the deferred tool `index` carries it, when
/// the index holds the tool.
#[must_use]
pub fn indexed_family(name: &str, index: &[ToolIndexSnapshot]) -> Option<ActionFamily> {
    let (server, tool) = name.strip_prefix("mcp:")?.split_once('/')?;
    index
        .iter()
        .find(|entry| entry.server_name == server && entry.tool_name == tool)
        .map(|entry| entry.family)
}

/// The family a call of the tool `name` is decided on: the one its
/// descriptor declares; a deferred MCP tool has none, and takes the one its
/// entry in `index` carries; a tool neither describes is unclassified.
///
/// Every path that decides a call reads the family here, so that a deferred
/// tool is classified the same way whoever calls it.
#[must_use]
pub fn tool_family(
    name: &str,
    descriptor: Option<&apollia_tools::ToolDescriptor>,
    index: &[ToolIndexSnapshot],
) -> ActionFamily {
    descriptor
        .map(|d| d.family)
        .or_else(|| indexed_family(name, index))
        .unwrap_or(ActionFamily::Unclassified)
}

/// A single match returned by [`search_index`].
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ToolSearchMatch {
    /// Fully qualified tool name usable by `McpToolExecutor`
    /// (e.g. `"mcp:notion/search_pages"`).
    pub full_name: String,
    /// Tool name within the server.
    pub tool_name: String,
    /// Server name.
    pub server_name: String,
    /// Description, when available.
    pub description: Option<String>,
    /// JSON input schema, when the index carries one. This is what makes the
    /// match actionable: the model reads the arguments here and calls
    /// `full_name` directly.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_schema: Option<Value>,
}

// ─── pure search ───────────────────────────────────────────────────────────

/// Search `entries` for tools whose name, description, or any tag contains
/// `query` (case-insensitive substring match). Returns at most `limit` results,
/// sorted by `(server_name, tool_name)`.
///
/// An empty `query` matches every entry, yielding the first `limit` after
/// sorting. The function performs no I/O and is safe to call from any context.
///
/// # Errors
///
/// Returns an error string when `limit` is `0` or exceeds 500.
pub fn search_index(
    entries: &[ToolIndexSnapshot],
    query: &str,
    limit: usize,
) -> Result<Vec<ToolSearchMatch>, String> {
    if limit == 0 || limit > MAX_SEARCH_LIMIT {
        return Err(format!(
            "limit out of bounds: {limit} (expected 1..={MAX_SEARCH_LIMIT})"
        ));
    }

    let needle = query.to_lowercase();
    let mut matched: Vec<&ToolIndexSnapshot> = entries
        .iter()
        .filter(|entry| {
            needle.is_empty()
                || entry.tool_name.to_lowercase().contains(&needle)
                || entry
                    .description
                    .as_deref()
                    .unwrap_or("")
                    .to_lowercase()
                    .contains(&needle)
                || entry
                    .tags
                    .iter()
                    .any(|tag| tag.to_lowercase().contains(&needle))
        })
        .collect();

    matched.sort_by(|a, b| {
        a.server_name
            .cmp(&b.server_name)
            .then_with(|| a.tool_name.cmp(&b.tool_name))
    });

    // A model asks in sentences ("latest notion page"), and no tool name
    // contains that whole string: the search came back empty, the model asked
    // again, and it never reached a tool. When the whole query matches nothing,
    // fall back to its words, best match first.
    if matched.is_empty() {
        let terms = query_terms(&needle);
        let mut scored: Vec<(usize, &ToolIndexSnapshot)> = entries
            .iter()
            .map(|entry| (matched_terms(entry, &terms), entry))
            .filter(|(score, _)| *score > 0)
            .collect();
        scored.sort_by(|(sa, a), (sb, b)| {
            sb.cmp(sa)
                .then_with(|| a.server_name.cmp(&b.server_name))
                .then_with(|| a.tool_name.cmp(&b.tool_name))
        });
        matched = scored.into_iter().map(|(_, entry)| entry).collect();
    }
    matched.truncate(limit);

    Ok(matched
        .into_iter()
        .map(|entry| ToolSearchMatch {
            full_name: format!("mcp:{}/{}", entry.server_name, entry.tool_name),
            tool_name: entry.tool_name.clone(),
            server_name: entry.server_name.clone(),
            description: entry.description.clone(),
            input_schema: entry.input_schema.clone(),
        })
        .collect())
}

/// Words of a lowercased query: alphanumeric runs of at least two characters.
fn query_terms(needle: &str) -> Vec<&str> {
    needle
        .split(|c: char| !c.is_alphanumeric())
        .filter(|term| term.chars().count() >= 2)
        .collect()
}

/// How many distinct query words occur in the tool's name, description, tags
/// or server name.
fn matched_terms(entry: &ToolIndexSnapshot, terms: &[&str]) -> usize {
    let haystack = format!(
        "{} {} {} {}",
        entry.server_name,
        entry.tool_name,
        entry.description.as_deref().unwrap_or(""),
        entry.tags.join(" ")
    )
    .to_lowercase();
    let mut seen: Vec<&str> = Vec::new();
    for term in terms {
        if !seen.contains(term) && haystack.contains(term) {
            seen.push(term);
        }
    }
    seen.len()
}

/// JSON Schema for the synthetic `tool_search` tool's input.
///
/// Consumed by the runtime when building the tool descriptor that is injected
/// alongside native tools in deferred mode.
pub fn tool_search_input_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "query": {
                "type": "string",
                "description": "Words or a substring to search in tool names, descriptions, server names, and server tags. Short keywords work best (e.g. `notion search`). Empty returns the top results."
            },
            "limit": {
                "type": "integer",
                "description": "Maximum number of tools to return. Default: 5. Max: 500.",
                "default": DEFAULT_SEARCH_LIMIT
            }
        },
        "required": []
    })
}

// ─── executor ──────────────────────────────────────────────────────────────

/// [`ToolExecutor`] implementation for the synthetic `tool_search` tool.
///
/// Queries the in-memory tool index and returns matches as a JSON object of the
/// form `{"matches": [...], "total": N}`. It performs no network I/O; on-demand
/// schema resolution happens later, at real tool-invocation time, via
/// `McpSession::fetch_tool_schema`.
pub struct ToolSearchExecutor {
    /// Snapshot of all deferred tool indexes, captured when the executor is built.
    index: Vec<ToolIndexSnapshot>,
    /// Maximum `limit` accepted from the caller; larger values are rejected.
    max_limit: usize,
}

impl ToolSearchExecutor {
    /// Create a new executor over `index`.
    ///
    /// `max_limit` caps the `limit` argument accepted from the LLM; values above
    /// this cap (or `0`) are rejected with [`ToolExecutionError::InvalidInput`].
    /// A typical value is 20, sourced from the `mcp.tool_search_limit` setting.
    pub fn new(index: Vec<ToolIndexSnapshot>, max_limit: usize) -> Self {
        Self { index, max_limit }
    }
}

impl ToolExecutor for ToolSearchExecutor {
    fn name(&self) -> &str {
        TOOL_SEARCH_NAME
    }

    fn is_read_only(&self) -> bool {
        true
    }

    /// Run the substring search and return `{"matches": [...], "total": N}`.
    ///
    /// # Errors
    ///
    /// Returns [`ToolExecutionError::InvalidInput`] when `limit` is `0` or
    /// exceeds the configured `max_limit`; the search is not run in that case.
    fn execute(
        &self,
        input: Value,
    ) -> Pin<Box<dyn Future<Output = Result<Value, ToolExecutionError>> + Send + '_>> {
        Box::pin(async move {
            let query = input
                .get("query")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let limit = input
                .get("limit")
                .and_then(Value::as_u64)
                .map(|n| n as usize)
                .unwrap_or(DEFAULT_SEARCH_LIMIT);

            if limit == 0 || limit > self.max_limit {
                return Err(ToolExecutionError::InvalidInput {
                    message: format!(
                        "limit must be between 1 and {} (got {limit})",
                        self.max_limit
                    ),
                });
            }

            let matches = search_index(&self.index, &query, limit)
                .map_err(|message| ToolExecutionError::InvalidInput { message })?;
            let total = matches.len();

            Ok(serde_json::json!({ "matches": matches, "total": total }))
        })
    }
}

// ─── tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(server: &str, tool: &str, desc: Option<&str>, tags: &[&str]) -> ToolIndexSnapshot {
        ToolIndexSnapshot {
            server_name: server.to_string(),
            tool_name: tool.to_string(),
            description: desc.map(str::to_string),
            tags: tags.iter().map(|t| t.to_string()).collect(),
            input_schema: None,
            ..Default::default()
        }
    }

    #[test]
    fn test_a_tool_is_decided_on_its_descriptor_then_its_index_entry() {
        // GIVEN an index holding a tool its manager classified read
        let mut search = entry("notion", "search", None, &[]);
        search.family = ActionFamily::Read;
        let index = vec![search, entry("notion", "write", None, &[])];
        let mut described = apollia_tools::tools::native_descriptors()[0].clone();
        described.family = ActionFamily::WriteOrDelete;

        // WHEN the family of a call is looked up
        // THEN a descriptor decides first, then the index entry, else nothing
        assert_eq!(
            tool_family("mcp:notion/search", Some(&described), &index),
            ActionFamily::WriteOrDelete
        );
        assert_eq!(
            tool_family("mcp:notion/search", None, &index),
            ActionFamily::Read
        );
        assert_eq!(
            tool_family("mcp:notion/write", None, &index),
            ActionFamily::Unclassified
        );
        assert_eq!(
            tool_family("mcp:drive/search", None, &index),
            ActionFamily::Unclassified
        );
        assert_eq!(
            tool_family("search", None, &index),
            ActionFamily::Unclassified
        );
        assert_eq!(
            indexed_family("mcp:notion/search", &index),
            Some(ActionFamily::Read)
        );
    }

    fn sample_index() -> Vec<ToolIndexSnapshot> {
        vec![
            entry(
                "google",
                "calendar_create",
                Some("Create a calendar event"),
                &[],
            ),
            entry(
                "google",
                "calendar_list",
                Some("List events"),
                &["calendar"],
            ),
            // Matches "calendar" via its description, not its name.
            entry(
                "notion",
                "search_pages",
                Some("Search Notion pages and calendars"),
                &[],
            ),
            entry("notion", "create_page", Some("Create a page"), &[]),
            entry("slack", "post_message", Some("Send a message"), &[]),
        ]
    }

    #[test]
    fn test_search_substring_matches_name_and_description() {
        // GIVEN an index where three tools mention "calendar" in name or description
        let index = sample_index();
        // WHEN searching for "calendar"
        let matches = search_index(&index, "calendar", 10).unwrap();
        // THEN the three calendar tools come back, sorted by server/name
        assert_eq!(matches.len(), 3);
        assert_eq!(matches[0].full_name, "mcp:google/calendar_create");
        assert_eq!(matches[1].full_name, "mcp:google/calendar_list");
        assert_eq!(matches[2].full_name, "mcp:notion/search_pages");
    }

    #[test]
    fn test_search_matches_server_tag() {
        // GIVEN an index where one tool carries the tag "calendar" but a name that
        // does not contain it
        let index = vec![entry(
            "acme",
            "schedule",
            Some("Plan things"),
            &["calendar"],
        )];
        // WHEN searching for "calendar"
        let matches = search_index(&index, "calendar", 10).unwrap();
        // THEN the tagged tool is matched
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].full_name, "mcp:acme/schedule");
    }

    #[test]
    fn test_search_empty_query_returns_top_n() {
        // GIVEN an index of 50 tools
        let index: Vec<ToolIndexSnapshot> = (0..50)
            .map(|i| entry("srv", &format!("tool_{i:02}"), None, &[]))
            .collect();
        // WHEN searching with an empty query and limit 5
        let matches = search_index(&index, "", 5).unwrap();
        // THEN exactly 5 results come back
        assert_eq!(matches.len(), 5);
    }

    #[test]
    fn test_search_falls_back_to_query_words_ranked_by_matches() {
        // GIVEN an index and a sentence-like query no tool contains as a whole
        let index = sample_index();
        // WHEN searching for "create calendar event"
        let matches = search_index(&index, "create calendar event", 10).unwrap();
        // THEN tools matching more of the words come first: calendar_create
        // matches all three, the other calendar tools and create_page fewer
        assert_eq!(matches[0].full_name, "mcp:google/calendar_create");
        assert!(matches.len() > 1);
    }

    #[test]
    fn test_search_words_reach_a_tool_through_its_server_name() {
        // GIVEN a tool whose name and description never say the server name
        let index = vec![entry("notion", "fetch", Some("Retrieve a page"), &[])];
        // WHEN the query mixes the server name with an unrelated word
        let matches = search_index(&index, "latest notion", 10).unwrap();
        // THEN the tool is found through its server name
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].full_name, "mcp:notion/fetch");
    }

    #[test]
    fn test_search_whole_query_match_keeps_precedence_over_word_fallback() {
        // GIVEN a query that matches one tool as a whole string
        let index = sample_index();
        // WHEN searching for "search_pages"
        let matches = search_index(&index, "search_pages", 10).unwrap();
        // THEN only that tool comes back, the word fallback does not widen it
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn test_search_no_match_returns_empty() {
        // GIVEN an index with no tool mentioning the needle
        let index = sample_index();
        // WHEN searching for an absent term
        let matches = search_index(&index, "zzzyyyxxx", 10).unwrap();
        // THEN the result is empty, not an error
        assert!(matches.is_empty());
    }

    #[test]
    fn test_search_full_name_format() {
        // GIVEN an index containing notion/search_pages
        let index = sample_index();
        // WHEN searching for it
        let matches = search_index(&index, "search_pages", 10).unwrap();
        // THEN the full name is the directly-invocable identifier
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].full_name, "mcp:notion/search_pages");
        assert_eq!(matches[0].server_name, "notion");
        assert_eq!(matches[0].tool_name, "search_pages");
    }

    #[test]
    fn test_search_limit_zero_errors() {
        // GIVEN any index
        let index = sample_index();
        // WHEN searching with limit 0
        let result = search_index(&index, "", 0);
        // THEN a bounds error is returned
        assert!(result.is_err());
    }

    #[test]
    fn test_search_limit_too_large_errors() {
        // GIVEN any index
        let index = sample_index();
        // WHEN searching with a limit above the hard cap
        let result = search_index(&index, "", 501);
        // THEN a bounds error is returned
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_executor_name_and_read_only() {
        // GIVEN a tool_search executor
        let executor = ToolSearchExecutor::new(sample_index(), 20);
        // WHEN its name and its read-only flag are read
        // THEN it identifies as tool_search and is read-only
        assert_eq!(executor.name(), "tool_search");
        assert!(executor.is_read_only());
    }

    #[tokio::test]
    async fn test_executor_returns_matches_and_total() {
        // GIVEN a tool_search executor over the sample index
        let executor = ToolSearchExecutor::new(sample_index(), 20);
        // WHEN invoked with a query and limit
        let output = executor
            .execute(serde_json::json!({"query": "calendar", "limit": 10}))
            .await
            .unwrap();
        // THEN the JSON carries the matches and a total count
        assert_eq!(output["total"], 3);
        assert_eq!(output["matches"].as_array().unwrap().len(), 3);
        assert_eq!(
            output["matches"][0]["full_name"],
            "mcp:google/calendar_create"
        );
    }

    #[tokio::test]
    async fn test_executor_empty_query_defaults_to_five() {
        // GIVEN a tool_search executor over a 50-tool index
        let index: Vec<ToolIndexSnapshot> = (0..50)
            .map(|i| entry("srv", &format!("tool_{i:02}"), None, &[]))
            .collect();
        let executor = ToolSearchExecutor::new(index, 20);
        // WHEN invoked with no arguments
        let output = executor.execute(serde_json::json!({})).await.unwrap();
        // THEN the default limit of 5 applies
        assert_eq!(output["total"], 5);
    }

    #[tokio::test]
    async fn test_executor_no_match_is_not_an_error() {
        // GIVEN a tool_search executor
        let executor = ToolSearchExecutor::new(sample_index(), 20);
        // WHEN invoked with a query that matches nothing
        let output = executor
            .execute(serde_json::json!({"query": "zzzyyyxxx"}))
            .await
            .unwrap();
        // THEN the call succeeds with an empty matches array
        assert_eq!(output["total"], 0);
        assert!(output["matches"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_executor_invalid_limit_errors_before_search() {
        // GIVEN a tool_search executor
        let executor = ToolSearchExecutor::new(sample_index(), 20);
        // WHEN invoked with limit 0
        let result = executor
            .execute(serde_json::json!({"query": "calendar", "limit": 0}))
            .await;
        // THEN a typed InvalidInput error is returned, no search performed
        assert!(matches!(
            result,
            Err(ToolExecutionError::InvalidInput { .. })
        ));
    }

    #[tokio::test]
    async fn test_executor_limit_above_max_errors() {
        // GIVEN a tool_search executor capped at max_limit 20
        let executor = ToolSearchExecutor::new(sample_index(), 20);
        // WHEN invoked with a limit above the cap
        let result = executor.execute(serde_json::json!({"limit": 21})).await;
        // THEN a typed InvalidInput error is returned
        assert!(matches!(
            result,
            Err(ToolExecutionError::InvalidInput { .. })
        ));
    }

    #[test]
    fn test_input_schema_shape() {
        // GIVEN the published input schema
        let schema = tool_search_input_schema();
        // WHEN its shape is inspected
        // THEN it advertises an object with query and limit, both optional
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["query"].is_object());
        assert_eq!(schema["properties"]["limit"]["default"], 5);
        assert_eq!(schema["required"].as_array().unwrap().len(), 0);
    }
}
