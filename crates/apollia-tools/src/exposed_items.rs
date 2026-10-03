//! The agents and workflows the user exposes to an MCP client, in `agents.db`.
//!
//! The MCP bridge (`apollia-os mcp server`) serves these items and nothing
//! else. An item is exposed when this table holds a row for it; there is no
//! row by default, so nothing is exposed until the user marks it. The flag
//! lives beside the agent rather than in its overrides, so going back to an
//! agent's defaults leaves it as the user set it.

use std::path::Path;
use std::sync::Mutex;

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

/// What an exposed item is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExposedKind {
    /// A stored agent, named by its identifier.
    Agent,
    /// A workflow of an installed package, named `<package>/<workflow>`.
    Workflow,
}

impl ExposedKind {
    /// Stored form.
    pub fn as_str(self) -> &'static str {
        match self {
            ExposedKind::Agent => "agent",
            ExposedKind::Workflow => "workflow",
        }
    }

    /// The kind written as `raw`, `None` for any other text.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "agent" => Some(ExposedKind::Agent),
            "workflow" => Some(ExposedKind::Workflow),
            _ => None,
        }
    }
}

/// One exposed item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExposedItem {
    /// Agent or workflow.
    pub kind: ExposedKind,
    /// The agent name, or `<package>/<workflow>`.
    pub item_id: String,
}

/// Errors of the exposure store.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ExposedItemError {
    /// SQLite failure.
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// The database schema could not be brought to the supported version.
    #[error(transparent)]
    Schema(#[from] apollia_core::schema::SchemaError),
    /// The identifier is empty.
    #[error("an exposed item needs a non-empty identifier")]
    EmptyId,
}

/// SQLite repository of the exposed items.
pub struct ExposedItemRepository {
    conn: Mutex<Connection>,
}

impl ExposedItemRepository {
    /// Opens (or creates) `agents.db` and brings it to the current schema.
    ///
    /// # Errors
    ///
    /// [`ExposedItemError::Sqlite`] or [`ExposedItemError::Schema`].
    pub fn open(path: &Path) -> Result<Self, ExposedItemError> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        crate::agents_db::open_agents_schema(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Every exposed item, agents first, each kind by identifier.
    ///
    /// # Errors
    ///
    /// [`ExposedItemError::Sqlite`].
    pub fn list(&self) -> Result<Vec<ExposedItem>, ExposedItemError> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        let mut stmt = conn.prepare("SELECT kind, item_id FROM exposed_items")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut items: Vec<ExposedItem> = rows
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .filter_map(|(kind, item_id)| {
                ExposedKind::parse(&kind).map(|kind| ExposedItem { kind, item_id })
            })
            .collect();
        items.sort_by(|a, b| (a.kind, &a.item_id).cmp(&(b.kind, &b.item_id)));
        Ok(items)
    }

    /// Marks `item_id` of kind `kind` exposed or not. Setting a flag to the
    /// value it already has changes nothing.
    ///
    /// # Errors
    ///
    /// [`ExposedItemError::EmptyId`] or [`ExposedItemError::Sqlite`].
    pub fn set(
        &self,
        kind: ExposedKind,
        item_id: &str,
        exposed: bool,
    ) -> Result<(), ExposedItemError> {
        let item_id = item_id.trim();
        if item_id.is_empty() {
            return Err(ExposedItemError::EmptyId);
        }
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        if exposed {
            conn.execute(
                "INSERT OR IGNORE INTO exposed_items (kind, item_id) VALUES (?1, ?2)",
                params![kind.as_str(), item_id],
            )?;
        } else {
            conn.execute(
                "DELETE FROM exposed_items WHERE kind = ?1 AND item_id = ?2",
                params![kind.as_str(), item_id],
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn repo() -> (TempDir, ExposedItemRepository) {
        let dir = TempDir::new().unwrap();
        let repo = ExposedItemRepository::open(&dir.path().join("agents.db")).unwrap();
        (dir, repo)
    }

    #[test]
    fn test_nothing_is_exposed_by_default() {
        // GIVEN a fresh agents.db
        let (_dir, repo) = repo();

        // WHEN the exposed items are read
        let items = repo.list().unwrap();

        // THEN there is none
        assert!(items.is_empty());
    }

    #[test]
    fn test_an_item_marked_exposed_is_listed_until_it_is_unmarked() {
        // GIVEN an agent and a workflow marked exposed, the agent twice
        let (_dir, repo) = repo();
        repo.set(ExposedKind::Workflow, "acme/report", true)
            .unwrap();
        repo.set(ExposedKind::Agent, "mailer", true).unwrap();
        repo.set(ExposedKind::Agent, "mailer", true).unwrap();

        // WHEN they are listed, then the agent is unmarked
        let listed = repo.list().unwrap();
        repo.set(ExposedKind::Agent, "mailer", false).unwrap();

        // THEN both were listed once, agents first, and only the workflow stays
        assert_eq!(
            listed,
            vec![
                ExposedItem {
                    kind: ExposedKind::Agent,
                    item_id: "mailer".into()
                },
                ExposedItem {
                    kind: ExposedKind::Workflow,
                    item_id: "acme/report".into()
                },
            ]
        );
        assert_eq!(
            repo.list().unwrap(),
            vec![ExposedItem {
                kind: ExposedKind::Workflow,
                item_id: "acme/report".into()
            }]
        );
    }

    #[test]
    fn test_an_empty_identifier_is_refused() {
        // GIVEN a fresh store
        let (_dir, repo) = repo();

        // WHEN a blank identifier is marked exposed
        let result = repo.set(ExposedKind::Agent, "  ", true);

        // THEN it is refused and nothing is stored
        assert!(matches!(result, Err(ExposedItemError::EmptyId)));
        assert!(repo.list().unwrap().is_empty());
    }

    #[test]
    fn test_a_v4_file_gains_an_empty_exposure_table() {
        // GIVEN an agents.db at v4, with an agent
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("agents.db");
        {
            let conn = Connection::open(&path).unwrap();
            apollia_core::schema::open_versioned(
                &conn,
                "agents.db",
                4,
                &crate::agents_db::AGENTS_DB_MIGRATIONS[..4],
            )
            .unwrap();
        }

        // WHEN the exposure store opens it
        let repo = ExposedItemRepository::open(&path).unwrap();

        // THEN the file is at the current version and nothing is exposed
        let version: i64 = Connection::open(&path)
            .unwrap()
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(
            version,
            i64::from(crate::agents_db::AGENTS_DB_SCHEMA_VERSION)
        );
        assert!(repo.list().unwrap().is_empty());
    }
}
