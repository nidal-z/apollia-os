//! The resources each server answered its last `resources/list` with, kept
//! in `mcp.db` so a server that is not running still offers them.
//!
//! A server's list is replaced as a whole at each `resources/list`. A server
//! that answered one, even empty, has a row in `mcp_resource_lists`; a
//! server with none was never asked, which is how the manager knows to ask.

use std::collections::HashMap;

use rusqlite::params;

use crate::server_repository::{McpRepoError, McpServerRepository};

/// One resource a server listed, as its last `resources/list` described it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredResource {
    /// Stable URI identifying the resource.
    pub uri: String,
    /// Display name.
    pub name: String,
    /// MIME type, when the server declared one.
    pub mime_type: Option<String>,
    /// Description, when the server declared one.
    pub description: Option<String>,
}

impl McpServerRepository {
    /// Replace the resources kept for `server` with `resources`, answered at
    /// `fetched_at` (RFC 3339), in one transaction.
    pub fn replace_resources(
        &self,
        server: &str,
        resources: &[StoredResource],
        fetched_at: &str,
    ) -> Result<(), McpRepoError> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "DELETE FROM mcp_resources WHERE server = ?1",
            params![server],
        )?;
        tx.execute(
            "INSERT OR REPLACE INTO mcp_resource_lists (server, fetched_at) VALUES (?1, ?2)",
            params![server, fetched_at],
        )?;
        {
            let mut insert = tx.prepare(
                "INSERT OR REPLACE INTO mcp_resources
                    (server, uri, name, mime_type, description)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;
            for resource in resources {
                insert.execute(params![
                    server,
                    resource.uri,
                    resource.name,
                    resource.mime_type,
                    resource.description,
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Forget the resources kept for `server`, and that it answered a list.
    /// Forgetting a server with none is not an error.
    pub fn forget_resources(&self, server: &str) -> Result<(), McpRepoError> {
        self.conn.execute(
            "DELETE FROM mcp_resources WHERE server = ?1",
            params![server],
        )?;
        self.conn.execute(
            "DELETE FROM mcp_resource_lists WHERE server = ?1",
            params![server],
        )?;
        Ok(())
    }

    /// Every kept list, by server, each in URI order. A server that answered
    /// an empty list is present with no resource.
    pub fn stored_resources(&self) -> Result<HashMap<String, Vec<StoredResource>>, McpRepoError> {
        let mut by_server: HashMap<String, Vec<StoredResource>> = HashMap::new();
        let mut lists = self.conn.prepare("SELECT server FROM mcp_resource_lists")?;
        for server in lists.query_map([], |row| row.get::<_, String>(0))? {
            by_server.entry(server?).or_default();
        }
        let mut stmt = self.conn.prepare(
            "SELECT r.server, r.uri, r.name, r.mime_type, r.description
             FROM mcp_resources r
             JOIN mcp_resource_lists l ON l.server = r.server
             ORDER BY r.server, r.uri",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                StoredResource {
                    uri: row.get(1)?,
                    name: row.get(2)?,
                    mime_type: row.get(3)?,
                    description: row.get(4)?,
                },
            ))
        })?;
        for row in rows {
            let (server, resource) = row?;
            by_server.entry(server).or_default().push(resource);
        }
        Ok(by_server)
    }
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;
    use tempfile::TempDir;

    use super::*;
    use crate::server_repository::{MCP_MIGRATIONS, MCP_SCHEMA_VERSION};

    fn resource(uri: &str) -> StoredResource {
        StoredResource {
            uri: uri.to_string(),
            name: format!("Name of {uri}"),
            mime_type: Some("text/plain".to_string()),
            description: Some(format!("About {uri}")),
        }
    }

    #[test]
    fn test_kept_resources_round_trip_and_an_empty_list_is_kept() {
        // GIVEN a repository
        let dir = TempDir::new().unwrap();
        let repo = McpServerRepository::open(&dir.path().join("mcp.db")).unwrap();
        let docs = vec![resource("file:///a.md"), resource("file:///b.md")];

        // WHEN one server's list is kept, and another answered an empty one
        repo.replace_resources("docs", &docs, "2026-10-27T08:00:00Z")
            .unwrap();
        repo.replace_resources("calc", &[], "2026-10-27T08:00:00Z")
            .unwrap();
        let stored = repo.stored_resources().unwrap();

        // THEN each reads back what was kept, the empty list included
        assert_eq!(stored.get("docs"), Some(&docs));
        assert_eq!(stored.get("calc"), Some(&Vec::new()));
        assert!(!stored.contains_key("never"));
    }

    #[test]
    fn test_a_new_resources_list_replaces_the_kept_one() {
        // GIVEN a server whose list is kept
        let dir = TempDir::new().unwrap();
        let repo = McpServerRepository::open(&dir.path().join("mcp.db")).unwrap();
        repo.replace_resources(
            "docs",
            &[resource("file:///old.md"), resource("file:///kept.md")],
            "2026-10-27T08:00:00Z",
        )
        .unwrap();

        // WHEN it answers a list without one of them
        repo.replace_resources(
            "docs",
            &[resource("file:///kept.md")],
            "2026-10-27T09:00:00Z",
        )
        .unwrap();

        // THEN only the new answer is kept
        assert_eq!(
            repo.stored_resources().unwrap()["docs"],
            vec![resource("file:///kept.md")]
        );
    }

    #[test]
    fn test_deleting_a_server_forgets_its_resources() {
        // GIVEN a saved server with a kept list, and another server's list
        let dir = TempDir::new().unwrap();
        let repo = McpServerRepository::open(&dir.path().join("mcp.db")).unwrap();
        let config = crate::config::McpServerConfig {
            format_version: 1,
            name: "docs".to_string(),
            command: "python3".to_string(),
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
        repo.replace_resources("docs", &[resource("file:///a.md")], "2026-10-27T08:00:00Z")
            .unwrap();
        repo.replace_resources("calc", &[], "2026-10-27T08:00:00Z")
            .unwrap();

        // WHEN the server is deleted
        repo.delete("docs").unwrap();

        // THEN its list leaves with it, marker included; the other stays
        let stored = repo.stored_resources().unwrap();
        assert!(!stored.contains_key("docs"));
        assert!(stored.contains_key("calc"));
    }

    #[test]
    fn test_a_v7_db_of_the_previous_release_migrates_without_loss() {
        // GIVEN a db at schema version 7 holding a server and its kept tools
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("mcp.db");
        {
            let conn = Connection::open(&path).unwrap();
            for step in &MCP_MIGRATIONS[..7] {
                step(&conn).unwrap();
            }
            conn.execute_batch("PRAGMA user_version = 7;").unwrap();
            conn.execute(
                "INSERT INTO mcp_servers (name, command, args_json) VALUES ('docs', 'npx', '[\"-y\"]')",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO mcp_tools (server, name, fetched_at)
                 VALUES ('docs', 'search', '2026-10-21T08:00:00Z')",
                [],
            )
            .unwrap();
        }

        // WHEN this build opens it
        let repo = McpServerRepository::open(&path).unwrap();

        // THEN it is at version 8, the server and its tools are intact, and
        //      no server reads as having answered a resources list yet
        let version: u32 = Connection::open(&path)
            .unwrap()
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, MCP_SCHEMA_VERSION);
        assert_eq!(version, 8);
        let found = repo.find_by_name("docs").unwrap().unwrap();
        assert_eq!(found.args, vec!["-y"]);
        assert_eq!(repo.stored_tools().unwrap()["docs"].len(), 1);
        assert!(repo.stored_resources().unwrap().is_empty());
        repo.replace_resources("docs", &[resource("file:///a.md")], "2026-10-27T08:00:00Z")
            .unwrap();
        assert_eq!(repo.stored_resources().unwrap()["docs"].len(), 1);
    }
}
