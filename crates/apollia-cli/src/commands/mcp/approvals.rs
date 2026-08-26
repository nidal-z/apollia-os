//! Tool approvals: set, list pending, revoke, and the path resolution.

use std::io::IsTerminal as _;
use std::path::PathBuf;

use apollia_mcp::approvals::McpApprovalStore;

use super::McpCommandError;

// ─── set-approval ────────────────────────────────────────────────────────────

/// Implements `apollia-os mcp set-approval <server> <tool>`.
pub(super) fn run_set_approval(
    server: &str,
    tool: &str,
    db_path: Option<&std::path::Path>,
    ttl_hours: u64,
    json: bool,
) -> Result<String, McpCommandError> {
    let path = resolve_approvals_db_path(db_path);
    let store = McpApprovalStore::open(&path, ttl_hours)?;
    store.approve(server, tool)?;

    if json {
        Ok(serde_json::to_string_pretty(&serde_json::json!({
            "server": server,
            "tool": tool,
            "ttl_hours": ttl_hours,
            "approved": true,
        }))
        .unwrap_or_else(|_| "{}".to_string()))
    } else {
        let expiry = if ttl_hours == 0 {
            "never".to_string()
        } else {
            format!("in {ttl_hours}h")
        };
        Ok(format!("Approved: {server}/{tool}  (expires {expiry})"))
    }
}

// ─── list-pending ─────────────────────────────────────────────────────────────

/// Implements `apollia-os mcp list-pending`.
pub(super) fn run_list_pending(
    db_path: Option<&std::path::Path>,
    json: bool,
) -> Result<String, McpCommandError> {
    let path = resolve_approvals_db_path(db_path);
    let store = McpApprovalStore::open(&path, 24)?;
    let entries = store.list_pending()?;

    if json {
        Ok(serde_json::to_string_pretty(&entries).unwrap_or_else(|_| "[]".to_string()))
    } else {
        format_pending_human(&entries)
    }
}

/// Human-readable formatter for pending approval entries.
pub(super) fn format_pending_human(
    entries: &[apollia_mcp::PendingApprovalEntry],
) -> Result<String, McpCommandError> {
    if entries.is_empty() {
        return Ok("No pending approval requests.\n".to_string());
    }

    let tty = std::io::stdout().is_terminal();
    let mut out = format!("{} pending approval request(s):\n", entries.len());

    for e in entries {
        if tty {
            out.push_str(&format!(
                "  \x1b[33m[{}]\x1b[0m  {}/{}  requested_at={}\n",
                e.id, e.server_name, e.tool_name, e.requested_at,
            ));
        } else {
            out.push_str(&format!(
                "  [{}]  {}/{}  requested_at={}\n",
                e.id, e.server_name, e.tool_name, e.requested_at,
            ));
        }
    }

    out.push_str("\nRun `apollia-os mcp set-approval <server> <tool>` to approve.\n");
    Ok(out)
}

// ─── revoke-approval ─────────────────────────────────────────────────────────

/// Implements `apollia-os mcp revoke-approval <server> <tool>`.
pub(super) fn run_revoke_approval(
    server: &str,
    tool: &str,
    db_path: Option<&std::path::Path>,
    json: bool,
) -> Result<String, McpCommandError> {
    let path = resolve_approvals_db_path(db_path);
    let store = McpApprovalStore::open(&path, 24)?;
    store.revoke(server, tool)?;

    if json {
        Ok(serde_json::to_string_pretty(&serde_json::json!({
            "server": server,
            "tool": tool,
            "revoked": true,
        }))
        .unwrap_or_else(|_| "{}".to_string()))
    } else {
        Ok(format!("Revoked: {server}/{tool}"))
    }
}

// ─── path resolution ─────────────────────────────────────────────────────────

/// Returns the effective path to `mcp.toml`.
///
/// Uses the caller-supplied path when present; otherwise falls back to
/// `~/.apollia/mcp.toml` (or `.apollia/mcp.toml` relative to the current
/// directory when the home directory cannot be determined).
pub(super) fn resolve_config_path(override_path: Option<&std::path::Path>) -> PathBuf {
    if let Some(p) = override_path {
        return p.to_path_buf();
    }
    dirs::home_dir()
        .map(apollia_core::paths::data_dir_under)
        .unwrap_or_else(|| PathBuf::from(apollia_core::paths::DATA_DIR_NAME))
        .join("mcp.toml")
}

/// Returns the effective path to the approvals SQLite database.
///
/// Uses the caller-supplied path when present; otherwise falls back to
/// `~/.apollia/mcp_approvals.db`.
pub(super) fn resolve_approvals_db_path(override_path: Option<&std::path::Path>) -> PathBuf {
    if let Some(p) = override_path {
        return p.to_path_buf();
    }
    dirs::home_dir()
        .map(apollia_core::paths::data_dir_under)
        .unwrap_or_else(|| PathBuf::from(apollia_core::paths::DATA_DIR_NAME))
        .join(apollia_core::paths::DataFile::McpApprovals.file_name())
}
