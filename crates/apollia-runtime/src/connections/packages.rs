//! The MCP servers an installed package brought.
//!
//! The table that says what a package installed, `package_items` in
//! `agents.db`, is read when it exists: one row per installed item, its
//! `kind` and its `item_id`, a connection being the kind `connection` and the
//! identifier the server name. Without the file or the table, no server comes
//! from a package.

use std::collections::HashSet;
use std::path::Path;

use rusqlite::{Connection, OpenFlags};
use tracing::{event, Level};

/// Names of the MCP servers `package_items` attaches to a package.
///
/// Read-only, and never an error: a file or a table that is absent answers
/// an empty set, and one that cannot be read is logged and answers the same,
/// so the connection list still loads.
#[must_use]
pub fn package_connection_names(agents_db: &Path) -> HashSet<String> {
    if !agents_db.exists() {
        return HashSet::new();
    }
    match read_names(agents_db) {
        Ok(names) => names,
        Err(e) => {
            event!(
                Level::WARN,
                error = %e,
                detail = "no connection is reported as brought by a package",
                "connections.package_items.read.failed"
            );
            HashSet::new()
        }
    }
}

fn read_names(agents_db: &Path) -> Result<HashSet<String>, rusqlite::Error> {
    let conn = Connection::open_with_flags(agents_db, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let has_table: bool = conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM sqlite_master
                        WHERE type = 'table' AND name = 'package_items')",
        [],
        |row| row.get(0),
    )?;
    if !has_table {
        return Ok(HashSet::new());
    }
    let mut stmt = conn.prepare("SELECT item_id FROM package_items WHERE kind = 'connection'")?;
    let names = stmt.query_map([], |row| row.get::<_, String>(0))?;
    names.collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_no_file_means_no_package_server() {
        // GIVEN a data directory without agents.db
        let dir = TempDir::new().unwrap();

        // WHEN the package servers are read
        let names = package_connection_names(&dir.path().join("agents.db"));

        // THEN none is found, and the file is not created
        assert!(names.is_empty());
        assert!(!dir.path().join("agents.db").exists());
    }

    #[test]
    fn test_no_table_means_no_package_server() {
        // GIVEN an agents.db written before package_items existed
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("agents.db");
        Connection::open(&path)
            .unwrap()
            .execute_batch("CREATE TABLE package_agents (package_name TEXT, agent_name TEXT);")
            .unwrap();

        // WHEN the package servers are read
        let names = package_connection_names(&path);

        // THEN none is found
        assert!(names.is_empty());
    }

    #[test]
    fn test_only_connection_items_are_package_servers() {
        // GIVEN a package that installed an agent and a connection
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("agents.db");
        Connection::open(&path)
            .unwrap()
            .execute_batch(
                "CREATE TABLE package_items (package_name TEXT, kind TEXT, item_id TEXT);
                 INSERT INTO package_items VALUES ('crm-pack', 'agent', 'crm-agent');
                 INSERT INTO package_items VALUES ('crm-pack', 'connection', 'crm');",
            )
            .unwrap();

        // WHEN the package servers are read
        let names = package_connection_names(&path);

        // THEN only the connection is reported
        assert_eq!(names, HashSet::from(["crm".to_owned()]));
    }
}
