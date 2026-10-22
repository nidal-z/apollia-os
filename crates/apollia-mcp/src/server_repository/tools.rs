//! The tools each server answered its last `tools/list` with, kept in
//! `mcp.db` so a server that is not running still says what it can do and
//! how its tools are classified.
//!
//! A server's rows are replaced as a whole at each `tools/list`: a tool the
//! server no longer publishes leaves with the next answer.

use std::collections::HashMap;

use rusqlite::params;

use crate::protocol::McpToolAnnotations;
use crate::server_repository::{McpRepoError, McpServerRepository};

/// One tool a server published, as its last `tools/list` described it.
///
/// Every text field was bounded at ingestion, before it was kept.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredTool {
    /// Tool name within the server.
    pub name: String,
    /// Display name, when the server declared one.
    pub title: Option<String>,
    /// Description, when the server declared one.
    pub description: Option<String>,
    /// JSON input schema.
    pub input_schema: serde_json::Value,
    /// Behaviour hints the server declared, when it declared any.
    pub annotations: Option<McpToolAnnotations>,
    /// When the `tools/list` that described the tool was answered (RFC 3339).
    pub fetched_at: String,
}

impl McpServerRepository {
    /// Wait up to `timeout` for another connection to the same file instead
    /// of failing at once.
    pub(crate) fn wait_when_busy(&self, timeout: std::time::Duration) -> Result<(), McpRepoError> {
        self.conn.busy_timeout(timeout)?;
        Ok(())
    }

    /// Replace the tools kept for `server` with `tools`, in one transaction.
    pub fn replace_tools(&self, server: &str, tools: &[StoredTool]) -> Result<(), McpRepoError> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM mcp_tools WHERE server = ?1", params![server])?;
        {
            let mut insert = tx.prepare(
                "INSERT OR REPLACE INTO mcp_tools
                    (server, name, title, description, input_schema_json,
                     annotations_json, fetched_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            )?;
            for tool in tools {
                let schema = serde_json::to_string(&tool.input_schema)
                    .map_err(|e| McpRepoError::Serialization(e.to_string()))?;
                let annotations = tool.annotations.map(|a| a.to_value().to_string());
                insert.execute(params![
                    server,
                    tool.name,
                    tool.title,
                    tool.description,
                    schema,
                    annotations,
                    tool.fetched_at,
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Forget the tools kept for `server`. Forgetting a server with none is
    /// not an error.
    pub fn forget_tools(&self, server: &str) -> Result<(), McpRepoError> {
        self.conn
            .execute("DELETE FROM mcp_tools WHERE server = ?1", params![server])?;
        Ok(())
    }

    /// Every kept tool, by server, each server's tools in name order.
    pub fn stored_tools(&self) -> Result<HashMap<String, Vec<StoredTool>>, McpRepoError> {
        let mut stmt = self.conn.prepare(
            "SELECT server, name, title, description, input_schema_json,
                    annotations_json, fetched_at
             FROM mcp_tools ORDER BY server, name",
        )?;
        let rows = stmt.query_map([], |row| {
            let schema: String = row.get(4)?;
            let annotations: Option<String> = row.get(5)?;
            Ok((
                row.get::<_, String>(0)?,
                StoredTool {
                    name: row.get(1)?,
                    title: row.get(2)?,
                    description: row.get(3)?,
                    input_schema: super::json_column(4, &schema)?,
                    annotations: annotations
                        .as_deref()
                        .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
                        .and_then(|value| McpToolAnnotations::from_value(&value)),
                    fetched_at: row.get(6)?,
                },
            ))
        })?;
        let mut by_server: HashMap<String, Vec<StoredTool>> = HashMap::new();
        for row in rows {
            let (server, tool) = row?;
            by_server.entry(server).or_default().push(tool);
        }
        Ok(by_server)
    }
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;
    use tempfile::TempDir;

    use super::*;
    use crate::server_repository::{
        migrate_v1, migrate_v2, migrate_v3, migrate_v4, migrate_v5, MCP_SCHEMA_VERSION,
    };

    fn tool(name: &str, read_only: Option<bool>) -> StoredTool {
        StoredTool {
            name: name.to_string(),
            title: Some(format!("Title of {name}")),
            description: Some(format!("Does {name}")),
            input_schema: serde_json::json!({"type": "object", "properties": {"q": {"type": "string"}}}),
            annotations: read_only.map(|r| McpToolAnnotations {
                read_only_hint: Some(r),
                ..McpToolAnnotations::default()
            }),
            fetched_at: "2026-10-21T08:00:00Z".to_string(),
        }
    }

    #[test]
    fn test_kept_tools_round_trip_every_field() {
        // GIVEN a repository and the tools of two servers
        let dir = TempDir::new().unwrap();
        let repo = McpServerRepository::open(&dir.path().join("mcp.db")).unwrap();
        let notion = vec![tool("search", Some(true)), tool("write", None)];

        // WHEN they are kept and read back
        repo.replace_tools("notion", &notion).unwrap();
        repo.replace_tools("drive", &[tool("list", Some(false))])
            .unwrap();
        let stored = repo.stored_tools().unwrap();

        // THEN each server reads back exactly what was kept, hints included
        assert_eq!(stored.get("notion"), Some(&notion));
        assert_eq!(stored.get("drive").map(Vec::len), Some(1));
    }

    #[test]
    fn test_a_new_tools_list_replaces_the_kept_one() {
        // GIVEN a server whose tools are kept
        let dir = TempDir::new().unwrap();
        let repo = McpServerRepository::open(&dir.path().join("mcp.db")).unwrap();
        repo.replace_tools("notion", &[tool("old", None), tool("kept", None)])
            .unwrap();

        // WHEN the server answers a list without one of them
        repo.replace_tools("notion", &[tool("kept", Some(true))])
            .unwrap();

        // THEN only the new answer is kept
        let stored = repo.stored_tools().unwrap();
        assert_eq!(stored["notion"], vec![tool("kept", Some(true))]);
    }

    #[test]
    fn test_deleting_a_server_forgets_its_tools() {
        // GIVEN a saved server with kept tools, and another server's tools
        let dir = TempDir::new().unwrap();
        let repo = McpServerRepository::open(&dir.path().join("mcp.db")).unwrap();
        let config = crate::config::McpServerConfig {
            format_version: 1,
            name: "notion".to_string(),
            command: "npx".to_string(),
            args: vec![],
            env: HashMap::new(),
            transport: "stdio".to_string(),
            url: None,
            init_timeout_secs: 30,
            call_timeout_secs: 60,
            max_response_bytes: 8 * 1024 * 1024,
            max_tools: 256,
            tags: vec![],
            tool_families: Default::default(),
            operator_families: Default::default(),
            origin: None,
        };
        repo.save(&config).unwrap();
        repo.replace_tools("notion", &[tool("search", None)])
            .unwrap();
        repo.replace_tools("drive", &[tool("list", None)]).unwrap();

        // WHEN the server is deleted
        repo.delete("notion").unwrap();

        // THEN its tools leave with it, the other server's stay
        let stored = repo.stored_tools().unwrap();
        assert!(!stored.contains_key("notion"));
        assert!(stored.contains_key("drive"));
    }

    #[test]
    fn test_a_v5_db_of_the_previous_release_migrates_without_loss() {
        // GIVEN a db at schema version 5 holding a configured server
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("mcp.db");
        {
            let conn = Connection::open(&path).unwrap();
            for step in [migrate_v1, migrate_v2, migrate_v3, migrate_v4, migrate_v5] {
                step(&conn).unwrap();
            }
            conn.execute_batch("PRAGMA user_version = 5;").unwrap();
            conn.execute(
                "INSERT INTO mcp_servers
                    (name, command, args_json, env_json, tags_json,
                     tool_families_json, provenance, official, selection_entry)
                 VALUES ('notion', 'npx', '[\"-y\",\"@notionhq/notion-mcp-server\"]',
                         '{\"TOKEN\":\"${APOLLIA_SECRET:notion}\"}', '[\"docs\"]',
                         '{\"search\":\"read_personal\"}', 'selection', 1, 'notion')",
                [],
            )
            .unwrap();
        }

        // WHEN this build opens it
        let repo = McpServerRepository::open(&path).unwrap();

        // THEN the file is at the current version, the server is intact, and
        //      the tool table is there, empty until the next tools/list
        let conn = Connection::open(&path).unwrap();
        let version: u32 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, MCP_SCHEMA_VERSION);
        assert_eq!(version, 7);
        let found = repo.find_by_name("notion").unwrap().unwrap();
        assert_eq!(found.args, vec!["-y", "@notionhq/notion-mcp-server"]);
        assert_eq!(
            found.env.get("TOKEN").map(String::as_str),
            Some("${APOLLIA_SECRET:notion}")
        );
        assert_eq!(found.tags, vec!["docs"]);
        assert_eq!(found.tool_families.len(), 1);
        assert!(found.origin.is_some());
        assert!(repo.stored_tools().unwrap().is_empty());
        repo.replace_tools("notion", &[tool("search", Some(true))])
            .unwrap();
        assert_eq!(repo.stored_tools().unwrap()["notion"].len(), 1);
    }
}
