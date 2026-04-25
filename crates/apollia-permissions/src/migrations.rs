//! Helpers SQLite partagés pour les migrations idempotentes du schéma de gouvernance.
//!
//! Ces helpers permettent d'ajouter une colonne à une table existante sans
//! échouer si la colonne est déjà présente — utile lors de la migration
//! d'une base `permissions.db` legacy vers le schéma `governance.db` étendu.

use rusqlite::Connection;

use crate::error::PermissionError;

/// Indique si la table `table` contient déjà la colonne `column`.
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

/// Exécute un `ALTER TABLE ADD COLUMN` uniquement si la colonne est absente.
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
