//! The configuration of the built-in chat, read once to be carried over to
//! the Apollia agent, then dropped.
//!
//! Before the built-in chat was an agent, its settings were one row of the
//! `chat_libre_config` table of `governance.db`: a system prompt, the tools
//! allowed without confirmation, and a preferred backend. They now belong to
//! the Apollia agent: the prompt and the tools become overrides of its
//! definition in `agents.db`, the backend its model in `system.db`. This
//! module reads the legacy row and drops the table once the runtime has
//! written them there; the runtime holds the other two files and the journal.
//!
//! The access upgrade ([`crate::access_upgrade`]) reads the same row to turn
//! its tools into standing consents, so the row is handed over only once that
//! upgrade has run: dropping the table earlier would lose them.

use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};

use crate::error::PermissionError;

/// Name of the legacy table.
const LEGACY_TABLE: &str = "chat_libre_config";

/// The settings of the built-in chat as the legacy row holds them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LegacyChatConfig {
    /// System prompt; empty when the user set none.
    pub system_prompt: String,
    /// Tools the user allowed without confirmation.
    pub allowed_tools: Vec<String>,
    /// Preferred backend; `None` for the runtime default.
    pub llm_backend: Option<String>,
}

/// Where the legacy configuration stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LegacyChatConfigState {
    /// The table is gone: the configuration was carried over already.
    Gone,
    /// The access upgrade has not run yet; the row stays until it has.
    Waiting,
    /// The row to carry over. An absent row reads as the default.
    Pending(LegacyChatConfig),
}

/// Reads the legacy configuration of the `governance.db` at `db_path`.
///
/// # Errors
///
/// [`PermissionError::Database`] or [`PermissionError::Schema`] when the file
/// cannot be opened, migrated or read.
pub fn read_legacy_chat_config(db_path: &Path) -> Result<LegacyChatConfigState, PermissionError> {
    let conn = Connection::open(db_path)?;
    crate::governance_schema::open_governance_schema(&conn)?;
    read_state(&conn)
}

fn read_state(conn: &Connection) -> Result<LegacyChatConfigState, PermissionError> {
    let exists: bool = conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
        params![LEGACY_TABLE],
        |row| row.get(0),
    )?;
    if !exists {
        return Ok(LegacyChatConfigState::Gone);
    }
    let upgraded: Option<Option<i64>> = conn
        .query_row(
            "SELECT done_at FROM governance_upgrades WHERE name = 'access_upgrade'",
            [],
            |row| row.get(0),
        )
        .optional()?;
    if matches!(upgraded, Some(None)) {
        return Ok(LegacyChatConfigState::Waiting);
    }
    let row: Option<(String, String, Option<String>)> = conn
        .query_row(
            "SELECT system_prompt, allowed_tools, llm_backend FROM chat_libre_config WHERE id = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    let Some((system_prompt, tools_json, llm_backend)) = row else {
        return Ok(LegacyChatConfigState::Pending(LegacyChatConfig::default()));
    };
    let allowed_tools: Vec<String> = serde_json::from_str(&tools_json).unwrap_or_default();
    Ok(LegacyChatConfigState::Pending(LegacyChatConfig {
        system_prompt,
        allowed_tools,
        llm_backend: llm_backend.filter(|b| !b.trim().is_empty()),
    }))
}

/// Drops the legacy table of the `governance.db` at `db_path`, once its row
/// has been carried over. Dropping a table already gone does nothing.
///
/// # Errors
///
/// [`PermissionError::Database`] or [`PermissionError::Schema`].
pub fn drop_legacy_chat_config(db_path: &Path) -> Result<(), PermissionError> {
    let conn = Connection::open(db_path)?;
    crate::governance_schema::open_governance_schema(&conn)?;
    conn.execute_batch("DROP TABLE IF EXISTS chat_libre_config;")?;
    tracing::event!(tracing::Level::INFO, "permission.chat_config.dropped");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn governance_db(dir: &Path) -> std::path::PathBuf {
        let path = dir.join("governance.db");
        let conn = Connection::open(&path).unwrap();
        crate::governance_schema::open_governance_schema(&conn).unwrap();
        path
    }

    fn finish_access_upgrade(path: &Path) {
        Connection::open(path)
            .unwrap()
            .execute(
                "UPDATE governance_upgrades SET done_at = 1 WHERE name = 'access_upgrade'",
                [],
            )
            .unwrap();
    }

    // GIVEN a governance.db whose access upgrade has not run
    // WHEN the legacy configuration is read
    // THEN it waits, so the tools are not lost before they become consents
    #[test]
    fn the_row_waits_for_the_access_upgrade() {
        let dir = tempfile::tempdir().unwrap();
        let path = governance_db(dir.path());

        let state = read_legacy_chat_config(&path).unwrap();

        assert_eq!(state, LegacyChatConfigState::Waiting);
    }

    // GIVEN a filled row and the access upgrade done
    // WHEN the legacy configuration is read
    // THEN the prompt, the tools and the backend come out as stored
    #[test]
    fn a_filled_row_is_read_whole() {
        let dir = tempfile::tempdir().unwrap();
        let path = governance_db(dir.path());
        Connection::open(&path)
            .unwrap()
            .execute(
                "UPDATE chat_libre_config SET system_prompt = 'Be brief.', \
                 allowed_tools = '[\"web_search\",\"file_read\"]', llm_backend = 'local' \
                 WHERE id = 1",
                [],
            )
            .unwrap();
        finish_access_upgrade(&path);

        let state = read_legacy_chat_config(&path).unwrap();

        assert_eq!(
            state,
            LegacyChatConfigState::Pending(LegacyChatConfig {
                system_prompt: "Be brief.".into(),
                allowed_tools: vec!["web_search".into(), "file_read".into()],
                llm_backend: Some("local".into()),
            })
        );
    }

    // GIVEN the row a fresh install writes and the access upgrade done
    // WHEN the legacy configuration is read
    // THEN it is the default: no prompt, no tool, no backend
    #[test]
    fn the_default_row_reads_as_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = governance_db(dir.path());
        finish_access_upgrade(&path);

        let state = read_legacy_chat_config(&path).unwrap();

        assert_eq!(
            state,
            LegacyChatConfigState::Pending(LegacyChatConfig::default())
        );
    }

    // GIVEN a carried-over configuration
    // WHEN the table is dropped, then dropped again
    // THEN it reads as gone and the second drop does nothing
    #[test]
    fn a_dropped_table_reads_as_gone() {
        let dir = tempfile::tempdir().unwrap();
        let path = governance_db(dir.path());
        finish_access_upgrade(&path);

        drop_legacy_chat_config(&path).unwrap();
        drop_legacy_chat_config(&path).unwrap();

        assert_eq!(
            read_legacy_chat_config(&path).unwrap(),
            LegacyChatConfigState::Gone
        );
    }
}
