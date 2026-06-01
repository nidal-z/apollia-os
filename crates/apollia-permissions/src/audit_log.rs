//! Immutable audit log of permission decisions (SQLite).
//!
//! Each call to `PermissionEngine::decide()` is recorded with the invoked
//! tool, the extracted first argument, the decision taken, and the timestamp.
//!
//! SQLite schema:
//! ```sql
//! CREATE TABLE permission_audit (
//!     id          INTEGER PRIMARY KEY AUTOINCREMENT,
//!     tool_name   TEXT NOT NULL,
//!     first_arg   TEXT,
//!     decision    TEXT NOT NULL,
//!     decided_at  INTEGER NOT NULL
//! );
//! ```

use std::path::Path;

use rusqlite::{params, Connection, OpenFlags};
use serde::{Deserialize, Serialize};

use crate::engine::PermissionDecision;
use crate::error::PermissionError;
use crate::migrations::add_column_if_missing;

// ─────────────────────────────────────────────
// Public types
// ─────────────────────────────────────────────

/// Persisted audit entry for a permission decision.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionAuditEntry {
    /// Auto-incremented unique identifier.
    pub id: i64,
    /// Name of the invoked tool.
    pub tool_name: String,
    /// First argument extracted from the input (None if absent or non-string).
    pub first_arg: Option<String>,
    /// Decision serialized as a string (e.g. "AutoAllowedSafeList").
    pub decision: String,
    /// Decision timestamp (Unix epoch, seconds).
    pub decided_at: i64,
    /// Scope of the rule that triggered the decision, when available.
    pub scope: Option<String>,
    /// Identifier of the persisted rule that decided, if any.
    pub rule_id: Option<i64>,
    /// Name of the agent behind the invocation, if provided by the caller.
    pub agent: Option<String>,
}

// ─────────────────────────────────────────────
// PermissionAuditLog
// ─────────────────────────────────────────────

/// Immutable audit log of permission decisions.
///
/// All decisions are recorded in SQLite. The log is append-only: no entry is
/// ever modified or deleted by the runtime.
pub struct PermissionAuditLog {
    db: Connection,
}

impl PermissionAuditLog {
    /// Opens (or creates) the SQLite database at the given path and migrates the schema.
    ///
    /// # Errors
    ///
    /// Returns [`PermissionError::Database`] if opening or migration fails.
    pub fn new(db_path: &Path) -> Result<Self, PermissionError> {
        let db = Connection::open_with_flags(
            db_path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
        )?;
        db.execute_batch("PRAGMA journal_mode=WAL;")?;
        let log = Self { db };
        log.migrate()?;
        Ok(log)
    }

    /// Records a permission decision in the audit log.
    ///
    /// # Errors
    ///
    /// Returns [`PermissionError::Database`] on a SQLite error.
    pub fn record(
        &mut self,
        tool_name: &str,
        first_arg: Option<&str>,
        decision: &PermissionDecision,
    ) -> Result<(), PermissionError> {
        let decided_at = chrono::Utc::now().timestamp();
        let decision_str = decision_to_str(decision);

        self.db.execute(
            "INSERT INTO permission_audit (tool_name, first_arg, decision, decided_at) \
             VALUES (?, ?, ?, ?)",
            params![tool_name, first_arg, decision_str, decided_at],
        )?;

        tracing::debug!(
            tool = %tool_name,
            decision = %decision_str,
            "permission decision recorded"
        );

        Ok(())
    }

    /// Returns the audit entries for a tool (or all tools if `None`).
    ///
    /// Entries are sorted by `decided_at` descending (most recent first).
    ///
    /// # Errors
    ///
    /// Returns [`PermissionError::Database`] on a SQLite error.
    pub fn query(
        &self,
        tool_name: Option<&str>,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<PermissionAuditEntry>, PermissionError> {
        let entries = if let Some(name) = tool_name {
            let mut stmt = self.db.prepare_cached(
                "SELECT id, tool_name, first_arg, decision, decided_at, scope, rule_id, agent \
                 FROM permission_audit \
                 WHERE tool_name = ? \
                 ORDER BY decided_at DESC \
                 LIMIT ? OFFSET ?",
            )?;
            let rows = stmt.query_map(params![name, limit, offset], row_to_entry)?;
            rows.collect::<Result<Vec<_>, _>>()?
        } else {
            let mut stmt = self.db.prepare_cached(
                "SELECT id, tool_name, first_arg, decision, decided_at, scope, rule_id, agent \
                 FROM permission_audit \
                 ORDER BY decided_at DESC \
                 LIMIT ? OFFSET ?",
            )?;
            let rows = stmt.query_map(params![limit, offset], row_to_entry)?;
            rows.collect::<Result<Vec<_>, _>>()?
        };

        Ok(entries)
    }

    // ─────────────────────────────────────────────
    // Private
    // ─────────────────────────────────────────────

    fn migrate(&self) -> Result<(), PermissionError> {
        self.db.execute_batch(
            "CREATE TABLE IF NOT EXISTS permission_audit (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                tool_name   TEXT NOT NULL,
                first_arg   TEXT,
                decision    TEXT NOT NULL,
                decided_at  INTEGER NOT NULL,
                scope       TEXT,
                rule_id     INTEGER,
                agent       TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_audit_tool ON permission_audit(tool_name, decided_at);",
        )?;

        add_column_if_missing(&self.db, "permission_audit", "scope", "TEXT")?;
        add_column_if_missing(&self.db, "permission_audit", "rule_id", "INTEGER")?;
        add_column_if_missing(&self.db, "permission_audit", "agent", "TEXT")?;

        self.db.execute_batch(
            "CREATE TRIGGER IF NOT EXISTS no_update_audit
             BEFORE UPDATE ON permission_audit BEGIN
                 SELECT RAISE(ABORT, 'permission_audit is append-only');
             END;
             CREATE TRIGGER IF NOT EXISTS no_delete_audit
             BEFORE DELETE ON permission_audit BEGIN
                 SELECT RAISE(ABORT, 'permission_audit is append-only');
             END;",
        )?;

        Ok(())
    }
}

fn row_to_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<PermissionAuditEntry> {
    Ok(PermissionAuditEntry {
        id: row.get(0)?,
        tool_name: row.get(1)?,
        first_arg: row.get(2)?,
        decision: row.get(3)?,
        decided_at: row.get(4)?,
        scope: row.get(5)?,
        rule_id: row.get(6)?,
        agent: row.get(7)?,
    })
}

/// Serializes a `PermissionDecision` to a string for the audit log.
pub(crate) fn decision_to_str(decision: &PermissionDecision) -> String {
    match decision {
        PermissionDecision::AutoAllowedSafeList => "AutoAllowedSafeList".to_string(),
        PermissionDecision::AutoAllowedPrefixRule { rule_id } => {
            format!("AutoAllowedPrefixRule({})", rule_id)
        }
        PermissionDecision::AutoDeniedPrefixRule { rule_id } => {
            format!("AutoDeniedPrefixRule({})", rule_id)
        }
        PermissionDecision::AutoDeniedInjection { pattern } => {
            format!("AutoDeniedInjection({})", pattern)
        }
        PermissionDecision::NeedsApproval => "NeedsApproval".to_string(),
    }
}

// ─────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    fn tmp_audit() -> (PermissionAuditLog, NamedTempFile) {
        let file = NamedTempFile::new().expect("tempfile");
        let log = PermissionAuditLog::new(file.path()).expect("audit log init");
        (log, file)
    }

    #[test]
    fn audit_log_persists_and_queries() {
        // GIVEN a PermissionAuditLog
        let (mut log, _tmp) = tmp_audit();
        // WHEN record + query
        log.record(
            "bash_executor",
            Some("git status"),
            &PermissionDecision::AutoAllowedSafeList,
        )
        .expect("record must succeed");
        let entries = log
            .query(Some("bash_executor"), 10, 0)
            .expect("query must succeed");
        // THEN entries are persisted and queryable
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].tool_name, "bash_executor");
        assert_eq!(entries[0].first_arg.as_deref(), Some("git status"));
        assert_eq!(entries[0].decision, "AutoAllowedSafeList");
    }

    #[test]
    fn audit_log_query_all_tools() {
        let (mut log, _tmp) = tmp_audit();
        log.record("tool_a", None, &PermissionDecision::NeedsApproval)
            .expect("record");
        log.record("tool_b", None, &PermissionDecision::NeedsApproval)
            .expect("record");
        let all = log.query(None, 10, 0).expect("query");
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn audit_log_pagination_works() {
        let (mut log, _tmp) = tmp_audit();
        for i in 0..5 {
            log.record(
                "bash_executor",
                Some(&format!("cmd_{}", i)),
                &PermissionDecision::NeedsApproval,
            )
            .expect("record");
        }
        let page = log
            .query(Some("bash_executor"), 2, 0)
            .expect("query page 0");
        assert_eq!(page.len(), 2);
        let page2 = log
            .query(Some("bash_executor"), 2, 2)
            .expect("query page 1");
        assert_eq!(page2.len(), 2);
    }

    #[test]
    fn decision_to_str_injection() {
        let d = PermissionDecision::AutoDeniedInjection {
            pattern: ";".to_string(),
        };
        assert_eq!(decision_to_str(&d), "AutoDeniedInjection(;)");
    }

    #[test]
    fn decision_to_str_prefix_rule() {
        let d = PermissionDecision::AutoAllowedPrefixRule { rule_id: 42 };
        assert_eq!(decision_to_str(&d), "AutoAllowedPrefixRule(42)");
    }
}
