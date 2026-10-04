//! Removal of rules that leaves, for each rule removed, its event in the
//! journal outbox, in the same transaction as the removal.

use rusqlite::{params_from_iter, Connection};

use super::matching::row_to_rule;
use super::RULE_COLUMNS;
use crate::error::PermissionError;
use crate::journal_outbox::{enqueue, RuleRecord, StoreEvent};

/// Removes the rules of `conn` that `filter` (an SQL condition on
/// `permission_rules`) selects with `values`, and returns how many went.
///
/// A row this binary cannot decode (a scope retired before the access
/// upgrade converted it) is removed all the same and reported in the log.
pub(super) fn remove_where(
    conn: &mut Connection,
    filter: &str,
    values: &[rusqlite::types::Value],
) -> Result<u32, PermissionError> {
    let tx = conn.transaction()?;
    let rows = {
        let mut stmt = tx.prepare(&format!(
            "SELECT {RULE_COLUMNS} FROM permission_rules WHERE {filter} ORDER BY id ASC"
        ))?;
        let rows = stmt
            .query_map(params_from_iter(values.iter()), |row| {
                Ok((row.get::<_, i64>(0)?, row_to_rule(row)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows
    };
    for (id, rule) in &rows {
        tx.execute("DELETE FROM permission_rules WHERE id = ?", [id])?;
        match rule {
            Ok(rule) => enqueue(&tx, &StoreEvent::RuleRemoved(RuleRecord::of(*id, rule)))?,
            Err(e) => tracing::event!(
                tracing::Level::WARN,
                rule_id = id,
                error = %e,
                "permissions.rule.removed_unjournaled"
            ),
        }
    }
    tx.commit()?;
    Ok(rows.len() as u32)
}
