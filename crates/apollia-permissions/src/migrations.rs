//! Shared SQLite helpers for idempotent governance schema migrations.
//!
//! These helpers add a column to an existing table without failing if the
//! column is already present, useful when migrating a legacy `permissions.db`
//! to the extended `governance.db` schema.

use rusqlite::Connection;

use crate::error::PermissionError;

/// Returns whether `table` already contains `column`.
pub(crate) fn column_exists(
    conn: &Connection,
    table: &str,
    column: &str,
) -> Result<bool, PermissionError> {
    let sql = format!("PRAGMA table_info(\"{table}\")");
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let name: String = row.get(1)?;
        if name == column {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Runs an `ALTER TABLE ADD COLUMN` only when the column is absent.
pub(crate) fn add_column_if_missing(
    conn: &Connection,
    table: &str,
    column: &str,
    type_def: &str,
) -> Result<(), PermissionError> {
    if column_exists(conn, table, column)? {
        return Ok(());
    }
    let sql = format!("ALTER TABLE \"{table}\" ADD COLUMN \"{column}\" {type_def}");
    conn.execute_batch(&sql)?;
    Ok(())
}
