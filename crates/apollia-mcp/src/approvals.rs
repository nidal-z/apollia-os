//! SQLite-backed HITL approval store for MCP tool calls.
//!
//! [`McpApprovalStore`] persists two tables:
//! - `mcp_approvals`: approved `(server_name, tool_name)` pairs with optional TTL.
//! - `mcp_pending_approvals`: approval requests awaiting human decision.
//!
//! The store is opened independently by both the runtime actor (for gate checks)
//! and the CLI commands (for direct management), using WAL mode for concurrent access.
//!
//! `rusqlite::Connection` is `Send` but not `Sync`. The connection is wrapped in a
//! `Mutex` so that `McpApprovalStore` is both `Send + Sync`, allowing the actor that
//! holds it to await other futures without violating Tokio's `Send` requirement.

use std::path::Path;
use std::sync::Mutex;

use rusqlite::{params, Connection};
use thiserror::Error;

// ─── errors ──────────────────────────────────────────────────────────────────

/// Errors from the MCP approval store.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum McpApprovalError {
    /// A SQLite operation failed.
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),

    /// A JSON serialisation failed.
    #[error("serialization error: {0}")]
    Serialization(String),

    /// The database schema could not be brought to the supported version.
    #[error(transparent)]
    Schema(#[from] apollia_core::schema::SchemaError),
}

/// Current schema version of `mcp_approvals.db`.
const APPROVALS_SCHEMA_VERSION: u32 = 1;

/// Numbered migration steps of `mcp_approvals.db`.
///
/// Step `k` migrates the file from version `k` to `k + 1`; the list length
/// always equals [`APPROVALS_SCHEMA_VERSION`].
const APPROVALS_MIGRATIONS: &[apollia_core::schema::Migration] = &[migrate_v1];

/// v1: the pre-versioning schema of the file, replayed idempotently.
fn migrate_v1(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS mcp_approvals (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            server_name TEXT NOT NULL,
            tool_name   TEXT NOT NULL,
            approved_at TEXT NOT NULL,
            expires_at  TEXT,
            UNIQUE(server_name, tool_name)
        );
        CREATE TABLE IF NOT EXISTS mcp_pending_approvals (
            id           TEXT PRIMARY KEY,
            server_name  TEXT NOT NULL,
            tool_name    TEXT NOT NULL,
            arguments    TEXT NOT NULL,
            requested_at TEXT NOT NULL,
            status       TEXT NOT NULL DEFAULT 'pending'
        );",
    )
}

// ─── public types ────────────────────────────────────────────────────────────

/// A pending approval request row as returned by [`McpApprovalStore::list_pending`].
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PendingApprovalEntry {
    /// UUID of the approval request.
    pub id: String,
    /// MCP server name (e.g. `"code-tools"`).
    pub server_name: String,
    /// Tool name within the server (e.g. `"bash_exec"`).
    pub tool_name: String,
    /// ISO 8601 UTC timestamp of when the request was created.
    pub requested_at: String,
    /// Current status: `"pending"`, `"approved"`, `"rejected"`, or `"expired"`.
    pub status: String,
}

// ─── store ───────────────────────────────────────────────────────────────────

/// SQLite-backed HITL approval store for MCP tool calls.
///
/// Each open instance holds one `rusqlite::Connection` behind a `Mutex`, making the
/// store both `Send` and `Sync`. This is required because the runtime actor that
/// holds the store awaits other futures while owning `&self`.
///
/// Opened by the runtime actor at startup and independently by CLI subcommands
/// (direct DB access, no runtime required). WAL mode allows concurrent readers.
///
/// When `approval_ttl_hours == 0`, approvals never expire.
pub struct McpApprovalStore {
    conn: Mutex<Connection>,
    /// Configured TTL for new approvals, in hours. `0` means no expiry.
    approval_ttl_hours: u64,
}

impl McpApprovalStore {
    /// Open the approval database at `path`, creating the schema if needed.
    ///
    /// Enables WAL journal mode for safe concurrent reads by CLI and runtime.
    pub fn open(path: &Path, approval_ttl_hours: u64) -> Result<Self, McpApprovalError> {
        let conn = Connection::open(path)?;
        Self::init_schema(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
            approval_ttl_hours,
        })
    }

    /// Open an in-memory database. Available only in tests.
    #[cfg(test)]
    pub fn in_memory(approval_ttl_hours: u64) -> Result<Self, McpApprovalError> {
        let conn = Connection::open_in_memory()?;
        Self::init_schema(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
            approval_ttl_hours,
        })
    }

    fn init_schema(conn: &Connection) -> Result<(), McpApprovalError> {
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        apollia_core::schema::open_versioned(
            conn,
            apollia_core::paths::DataFile::McpApprovals.file_name(),
            APPROVALS_SCHEMA_VERSION,
            APPROVALS_MIGRATIONS,
        )?;
        Ok(())
    }

    /// Return `true` when `(server_name, tool_name)` has a valid non-expired approval.
    pub fn is_approved(&self, server_name: &str, tool_name: &str) -> bool {
        let now = chrono::Utc::now().to_rfc3339();
        let conn = match self.conn.lock() {
            Ok(c) => c,
            Err(_) => return false,
        };
        let result: rusqlite::Result<bool> = conn.query_row(
            "SELECT COUNT(*) FROM mcp_approvals
             WHERE server_name = ?1
               AND tool_name   = ?2
               AND (expires_at IS NULL OR expires_at > ?3)",
            params![server_name, tool_name, now],
            |row| Ok(row.get::<_, i64>(0)? > 0),
        );
        result.unwrap_or(false)
    }

    /// Register a new pending approval request.
    ///
    /// Returns the UUID assigned to this request. The entry is inserted with
    /// `status = 'pending'`. If the same `(server, tool)` already has a pending
    /// row, a new UUID is still generated so each call is independently tracked.
    pub fn register(
        &self,
        server_name: &str,
        tool_name: &str,
        arguments: &serde_json::Value,
    ) -> Result<String, McpApprovalError> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let args_json = serde_json::to_string(arguments)
            .map_err(|e| McpApprovalError::Serialization(e.to_string()))?;

        self.conn
            .lock()
            .expect("approval store mutex poisoned")
            .execute(
                "INSERT INTO mcp_pending_approvals
                 (id, server_name, tool_name, arguments, requested_at, status)
                 VALUES (?1, ?2, ?3, ?4, ?5, 'pending')",
                params![id, server_name, tool_name, args_json, now],
            )?;

        tracing::info!(
            server = %server_name,
            tool   = %tool_name,
            id     = %id,
            "MCP tool call suspended: pending HITL approval"
        );

        Ok(id)
    }

    /// Approve `(server_name, tool_name)`, persisting with the configured TTL.
    ///
    /// Upserts the `mcp_approvals` row. Also transitions all matching
    /// `mcp_pending_approvals` rows from `'pending'` to `'approved'`.
    pub fn approve(&self, server_name: &str, tool_name: &str) -> Result<(), McpApprovalError> {
        let now = chrono::Utc::now();
        let approved_at = now.to_rfc3339();
        let expires_at: Option<String> = if self.approval_ttl_hours == 0 {
            None
        } else {
            Some((now + chrono::Duration::hours(self.approval_ttl_hours as i64)).to_rfc3339())
        };

        let conn = self.conn.lock().expect("approval store mutex poisoned");

        conn.execute(
            "INSERT INTO mcp_approvals (server_name, tool_name, approved_at, expires_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(server_name, tool_name) DO UPDATE SET
               approved_at = excluded.approved_at,
               expires_at  = excluded.expires_at",
            params![server_name, tool_name, approved_at, expires_at],
        )?;

        conn.execute(
            "UPDATE mcp_pending_approvals
             SET status = 'approved'
             WHERE server_name = ?1 AND tool_name = ?2 AND status = 'pending'",
            params![server_name, tool_name],
        )?;

        tracing::info!(
            server      = %server_name,
            tool        = %tool_name,
            ttl_hours   = self.approval_ttl_hours,
            expires_at  = ?expires_at,
            "MCP tool access approved"
        );

        Ok(())
    }

    /// Revoke the approval for `(server_name, tool_name)`.
    ///
    /// Removes the row from `mcp_approvals`. Subsequent `is_approved` calls for
    /// this pair return `false`.
    pub fn revoke(&self, server_name: &str, tool_name: &str) -> Result<(), McpApprovalError> {
        self.conn
            .lock()
            .expect("approval store mutex poisoned")
            .execute(
                "DELETE FROM mcp_approvals WHERE server_name = ?1 AND tool_name = ?2",
                params![server_name, tool_name],
            )?;

        tracing::info!(
            server = %server_name,
            tool   = %tool_name,
            "MCP tool access revoked"
        );

        Ok(())
    }

    /// Return all requests currently in `status = 'pending'`, ordered by `requested_at`.
    pub fn list_pending(&self) -> Result<Vec<PendingApprovalEntry>, McpApprovalError> {
        let conn = self.conn.lock().expect("approval store mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT id, server_name, tool_name, requested_at, status
             FROM mcp_pending_approvals
             WHERE status = 'pending'
             ORDER BY requested_at ASC",
        )?;

        let entries = stmt
            .query_map([], |row| {
                Ok(PendingApprovalEntry {
                    id: row.get(0)?,
                    server_name: row.get(1)?,
                    tool_name: row.get(2)?,
                    requested_at: row.get(3)?,
                    status: row.get(4)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        Ok(entries)
    }
}

// ─── tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> McpApprovalStore {
        McpApprovalStore::in_memory(24).expect("in-memory store must open")
    }

    // is_approved returns false when no entry

    #[test]
    fn test_is_approved_returns_false_when_no_entry() {
        // GIVEN an empty store
        let s = store();
        // THEN is_approved returns false
        assert!(!s.is_approved("code-tools", "bash_exec"));
    }

    // approve then is_approved returns true

    #[test]
    fn test_approve_then_is_approved_returns_true() {
        // GIVEN
        let s = store();
        // WHEN
        s.approve("code-tools", "bash_exec")
            .expect("approve must succeed");
        // THEN
        assert!(s.is_approved("code-tools", "bash_exec"));
    }

    // expired approval returns false

    #[test]
    fn test_expired_approval_returns_false() {
        // GIVEN a store with TTL = 0 (no expiry by default), insert an already-expired row
        let s = store();
        let past = "2000-01-01T00:00:00+00:00".to_string();
        s.conn
            .lock()
            .expect("mutex poisoned")
            .execute(
                "INSERT INTO mcp_approvals (server_name, tool_name, approved_at, expires_at)
                 VALUES ('srv', 'tool', ?1, ?1)",
                params![past],
            )
            .expect("insert must succeed");
        // THEN is_approved returns false because expires_at is in the past
        assert!(!s.is_approved("srv", "tool"));
    }

    // ── register creates a pending row ───────────────────────────────────────

    #[test]
    fn test_register_creates_pending_entry() {
        // GIVEN
        let s = store();
        let args = serde_json::json!({"cmd": "ls"});
        // WHEN
        let id = s
            .register("srv", "bash", &args)
            .expect("register must succeed");
        // THEN the returned UUID is non-empty and the row exists
        assert!(!id.is_empty());
        let pending = s.list_pending().expect("list_pending must succeed");
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].server_name, "srv");
        assert_eq!(pending[0].tool_name, "bash");
        assert_eq!(pending[0].status, "pending");
    }

    // ── revoke removes an approval ───────────────────────────────────────────

    #[test]
    fn test_revoke_removes_approval() {
        // GIVEN an approved entry
        let s = store();
        s.approve("srv", "tool").expect("approve must succeed");
        assert!(s.is_approved("srv", "tool"));
        // WHEN
        s.revoke("srv", "tool").expect("revoke must succeed");
        // THEN
        assert!(!s.is_approved("srv", "tool"));
    }

    // ── approve updates pending rows to 'approved' ───────────────────────────

    #[test]
    fn test_approve_transitions_pending_rows() {
        // GIVEN a pending request
        let s = store();
        s.register("srv", "tool", &serde_json::json!({}))
            .expect("register must succeed");
        assert_eq!(s.list_pending().expect("list").len(), 1);
        // WHEN approved
        s.approve("srv", "tool").expect("approve must succeed");
        // THEN the pending list is now empty (row moved to approved)
        assert_eq!(s.list_pending().expect("list").len(), 0);
    }

    // ── list_pending returns only pending rows ───────────────────────────────

    #[test]
    fn test_list_pending_returns_pending_only() {
        // GIVEN 2 pending requests
        let s = store();
        s.register("srv", "tool-a", &serde_json::json!({}))
            .expect("register a");
        s.register("srv", "tool-b", &serde_json::json!({}))
            .expect("register b");
        // WHEN
        let pending = s.list_pending().expect("list");
        // THEN both are present
        assert_eq!(pending.len(), 2);
    }

    // ── TTL = 0 produces no expires_at ───────────────────────────────────────

    #[test]
    fn test_zero_ttl_produces_no_expiry() {
        // GIVEN a store with TTL = 0
        let s = McpApprovalStore::in_memory(0).expect("store");
        // WHEN
        s.approve("srv", "tool").expect("approve");
        // THEN the approval has no expires_at and is_approved returns true
        assert!(s.is_approved("srv", "tool"));
    }
}
