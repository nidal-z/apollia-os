//! `mcp secret`: the keychain entries an MCP server's environment needs.

use crate::exit_codes;

use super::McpSecretCommand;

/// Same keychain service name the Desktop uses for MCP server secrets.
/// Mirrors `SecretStore::new()` in `apollia-desktop/src/mcp/secret_store.rs`
/// so CLI writes are read transparently by the daemon.
pub(super) const MCP_SECRET_SERVICE: &str = "apollia-mcp";

pub(super) fn mcp_secret_key(server: &str, env_var: &str) -> String {
    format!("{server}:{env_var}")
}

pub(super) fn run_secret(cmd: &McpSecretCommand, json: bool) -> i32 {
    match cmd {
        McpSecretCommand::Set {
            server,
            env_var,
            value,
        } => run_secret_set(server, env_var, value, json),
        McpSecretCommand::Delete {
            server,
            env_var,
            confirm,
        } => run_secret_delete(server, env_var, *confirm, json),
    }
}

pub(super) fn emit_secret_error(msg: String, json: bool) -> i32 {
    crate::output::emit_error(json, exit_codes::GENERAL_ERROR, &msg.to_string())
}

pub(super) fn run_secret_set(server: &str, env_var: &str, value: &str, json: bool) -> i32 {
    if server.trim().is_empty() {
        return emit_secret_error("server name must not be empty".into(), json);
    }
    if env_var.trim().is_empty() {
        return emit_secret_error("env_var must not be empty".into(), json);
    }
    let trimmed_value = value.trim();
    if trimmed_value.is_empty() {
        return emit_secret_error(
            "value must not be empty (use `mcp secret delete` to remove a secret)".into(),
            json,
        );
    }
    let store = match apollia_auth::select_secret_store() {
        Ok(s) => s,
        Err(e) => return emit_secret_error(format!("keychain unavailable: {e}"), json),
    };
    let key = mcp_secret_key(server, env_var);
    match store.set(MCP_SECRET_SERVICE, &key, trimmed_value) {
        Ok(()) => {
            if json {
                let out = serde_json::json!({
                    "server": server,
                    "env_var": env_var,
                    "stored": true,
                });
                println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
            } else {
                println!("  * secret stored for {server} / {env_var}");
            }
            exit_codes::SUCCESS
        }
        Err(e) => emit_secret_error(format!("keychain write failed: {e}"), json),
    }
}

pub(super) fn run_secret_delete(server: &str, env_var: &str, confirm: bool, json: bool) -> i32 {
    if server.trim().is_empty() {
        return emit_secret_error("server name must not be empty".into(), json);
    }
    if env_var.trim().is_empty() {
        return emit_secret_error("env_var must not be empty".into(), json);
    }
    if let Some(code) = crate::output::require_confirmation(
        confirm,
        json,
        &format!("delete the stored secret '{env_var}' of MCP server '{server}'"),
    ) {
        return code;
    }
    let store = match apollia_auth::select_secret_store() {
        Ok(s) => s,
        Err(e) => return emit_secret_error(format!("keychain unavailable: {e}"), json),
    };
    let key = mcp_secret_key(server, env_var);
    match store.delete(MCP_SECRET_SERVICE, &key) {
        Ok(()) => {
            if json {
                let out = serde_json::json!({
                    "server": server,
                    "env_var": env_var,
                    "deleted": true,
                });
                println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
            } else {
                println!("  * secret deleted for {server} / {env_var}");
            }
            exit_codes::SUCCESS
        }
        Err(e) => emit_secret_error(format!("keychain delete failed: {e}"), json),
    }
}
