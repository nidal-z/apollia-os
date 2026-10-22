//! The family the operator gave each tool of a connection, kept in
//! `mcp.db` apart from the server row.
//!
//! [`McpServerRepository::save`] never writes this table, so the families
//! survive a connection added again; [`McpServerRepository::list`] and
//! [`McpServerRepository::find_by_name`] read them into
//! [`McpServerConfig::operator_families`](crate::config::McpServerConfig::operator_families).

use std::collections::{BTreeMap, HashMap};

use apollia_tools::ActionFamily;
use rusqlite::params;

use crate::server_repository::{McpRepoError, McpServerRepository};

/// Families of one server, by tool name.
type Families = BTreeMap<String, ActionFamily>;

fn parse_family(index: usize, raw: String) -> rusqlite::Result<ActionFamily> {
    serde_json::from_value(serde_json::Value::String(raw)).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(index, rusqlite::types::Type::Text, Box::new(e))
    })
}

impl McpServerRepository {
    /// Store `family` as the operator's family for `tool` of `server`,
    /// replacing the one stored before.
    ///
    /// Returns [`McpRepoError::NotFound`] when no server has that name.
    pub fn set_operator_family(
        &self,
        server: &str,
        tool: &str,
        family: ActionFamily,
    ) -> Result<(), McpRepoError> {
        let known = self
            .conn
            .prepare("SELECT 1 FROM mcp_servers WHERE name = ?1")?
            .exists(params![server])?;
        if !known {
            return Err(McpRepoError::NotFound(server.to_string()));
        }
        self.conn.execute(
            "INSERT INTO mcp_operator_families (server, tool, family)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(server, tool) DO UPDATE SET
                family = excluded.family,
                set_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')",
            params![server, tool, family.as_str()],
        )?;
        Ok(())
    }

    /// The operator's families for the tools of `server`.
    pub fn operator_families(&self, server: &str) -> Result<Families, McpRepoError> {
        let mut stmt = self.conn.prepare(
            "SELECT tool, family FROM mcp_operator_families WHERE server = ?1 ORDER BY tool",
        )?;
        let rows = stmt.query_map(params![server], |row| {
            Ok((row.get::<_, String>(0)?, parse_family(1, row.get(1)?)?))
        })?;
        rows.map(|r| r.map_err(McpRepoError::Db)).collect()
    }

    /// The operator's families of every server, by server.
    pub(crate) fn all_operator_families(&self) -> Result<HashMap<String, Families>, McpRepoError> {
        let mut stmt = self
            .conn
            .prepare("SELECT server, tool, family FROM mcp_operator_families")?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                parse_family(2, row.get(2)?)?,
            ))
        })?;
        let mut by_server: HashMap<String, Families> = HashMap::new();
        for row in rows {
            let (server, tool, family) = row?;
            by_server.entry(server).or_default().insert(tool, family);
        }
        Ok(by_server)
    }

    /// Forget the operator's families of `server`.
    pub(crate) fn forget_operator_families(&self, server: &str) -> Result<(), McpRepoError> {
        self.conn.execute(
            "DELETE FROM mcp_operator_families WHERE server = ?1",
            params![server],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;
    use tempfile::TempDir;

    use super::*;
    use crate::config::McpServerConfig;
    use crate::server_repository::{
        migrate_v1, migrate_v2, migrate_v3, migrate_v4, migrate_v5, migrate_v6,
    };

    fn config(name: &str) -> McpServerConfig {
        serde_json::from_value(serde_json::json!({ "name": name, "command": "npx" }))
            .expect("config")
    }

    #[test]
    fn test_the_operator_family_survives_the_connection_saved_again() {
        // GIVEN a saved server whose selection classification is rewritten at
        //       each addition, and a family the operator gave one of its tools
        let dir = TempDir::new().unwrap();
        let repo = McpServerRepository::open(&dir.path().join("mcp.db")).unwrap();
        let mut first = config("files");
        first.tool_families = [("list".to_string(), ActionFamily::Read)].into();
        repo.save(&first).unwrap();
        repo.set_operator_family("files", "write", ActionFamily::SendExternal)
            .unwrap();

        // WHEN the connection is added again, with another classification
        let mut again = config("files");
        again.tool_families = [("other".to_string(), ActionFamily::Read)].into();
        repo.save(&again).unwrap();

        // THEN the selection's column holds the new classification, and the
        //      operator's family is still read with the server
        let found = repo.find_by_name("files").unwrap().unwrap();
        assert_eq!(found.tool_families, again.tool_families);
        assert_eq!(
            found.operator_families,
            [("write".to_string(), ActionFamily::SendExternal)].into()
        );
        let listed = repo.list().unwrap();
        assert_eq!(listed[0].operator_families, found.operator_families);
    }

    #[test]
    fn test_a_second_family_replaces_the_first_and_removal_forgets_it() {
        // GIVEN a server whose tool the operator classified
        let dir = TempDir::new().unwrap();
        let repo = McpServerRepository::open(&dir.path().join("mcp.db")).unwrap();
        repo.save(&config("files")).unwrap();
        repo.set_operator_family("files", "write", ActionFamily::Read)
            .unwrap();

        // WHEN the operator classifies it again
        repo.set_operator_family("files", "write", ActionFamily::WriteOrDelete)
            .unwrap();

        // THEN only the last family is kept
        assert_eq!(
            repo.operator_families("files").unwrap(),
            [("write".to_string(), ActionFamily::WriteOrDelete)].into()
        );
        // AND removing the server forgets it, so a server added under the
        //     same name starts with none
        repo.delete("files").unwrap();
        repo.save(&config("files")).unwrap();
        assert!(repo.operator_families("files").unwrap().is_empty());
    }

    #[test]
    fn test_a_family_for_an_unknown_server_is_refused() {
        // GIVEN an empty repository
        let dir = TempDir::new().unwrap();
        let repo = McpServerRepository::open(&dir.path().join("mcp.db")).unwrap();
        // WHEN a family is set on a server nobody added
        let result = repo.set_operator_family("ghost", "write", ActionFamily::Read);
        // THEN it is refused and nothing is written
        assert!(matches!(result, Err(McpRepoError::NotFound(_))));
        assert!(repo.all_operator_families().unwrap().is_empty());
    }

    #[test]
    fn test_a_v6_db_gains_the_operator_table_without_loss() {
        // GIVEN a db at schema version 6 holding a server
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("mcp.db");
        {
            let conn = Connection::open(&path).unwrap();
            for step in [
                migrate_v1, migrate_v2, migrate_v3, migrate_v4, migrate_v5, migrate_v6,
            ] {
                step(&conn).unwrap();
            }
            conn.execute_batch("PRAGMA user_version = 6;").unwrap();
            conn.execute(
                "INSERT INTO mcp_servers (name, command) VALUES ('files', 'npx')",
                [],
            )
            .unwrap();
        }

        // WHEN this build opens it
        let repo = McpServerRepository::open(&path).unwrap();

        // THEN the server is intact, with no operator family, and one can be set
        let found = repo.find_by_name("files").unwrap().unwrap();
        assert_eq!(found.command, "npx");
        assert!(found.operator_families.is_empty());
        repo.set_operator_family("files", "write", ActionFamily::Read)
            .unwrap();
        assert_eq!(repo.operator_families("files").unwrap().len(), 1);
    }
}
