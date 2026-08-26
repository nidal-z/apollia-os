//! Paths, registry access, config loading and error mapping.

use std::path::{Path, PathBuf};

use apollia_core::ToolsConfig;
use apollia_tools::{
    governance_db::GOVERNANCE_DB_FILENAME, NativeToolRegistry, ToolCredentialStore,
    ToolGovernanceError, AGENT_CREDENTIALS_NAMESPACE, NATIVE_TOOL_NAMES,
};

use crate::client::ClientError;
use crate::config::parse_apollia_toml;
use crate::exit_codes;

// ─── Helpers ──────────────────────────────────────────────────────────

pub(super) fn resolve_data_dir() -> Result<PathBuf, i32> {
    match apollia_core::paths::home_string() {
        Some(h) => Ok(apollia_core::paths::data_dir_under(h)),
        None => Err(emit_error(
            "variable d'environnement HOME absente".to_string(),
            false,
        )),
    }
}

pub(super) fn db_path(data_dir: &Path) -> PathBuf {
    data_dir.join(GOVERNANCE_DB_FILENAME)
}

pub(super) fn keyfile_path(data_dir: &Path) -> PathBuf {
    data_dir.join(".keyfile")
}

pub(super) fn open_registry(data_dir: &Path, json: bool) -> Result<NativeToolRegistry, i32> {
    if !data_dir.exists() {
        if let Err(e) = std::fs::create_dir_all(data_dir) {
            return Err(emit_error(
                format!("create {} failed: {e}", data_dir.display()),
                json,
            ));
        }
    }
    if !db_path(data_dir).exists() {
        if let Err(e) = apollia_tools::GovernanceDb::open(data_dir) {
            return Err(emit_error(format!("init governance.db failed: {e}"), json));
        }
    }
    NativeToolRegistry::new(&db_path(data_dir))
        .map_err(|e: ToolGovernanceError| emit_error(format!("open registry failed: {e}"), json))
}

pub(super) fn open_credential_store(data_dir: &Path) -> Option<ToolCredentialStore> {
    if !db_path(data_dir).exists() {
        return None;
    }
    ToolCredentialStore::new(&db_path(data_dir), &keyfile_path(data_dir)).ok()
}

pub(super) fn is_known_tool(name: &str) -> bool {
    NATIVE_TOOL_NAMES.contains(&name)
}

/// A credential can be attached to a native tool or to the shared `agent`
/// namespace (secrets an agent declares in its manifest).
pub(super) fn is_valid_credential_target(name: &str) -> bool {
    is_known_tool(name) || name == AGENT_CREDENTIALS_NAMESPACE
}

pub(super) fn emit_unknown_tool(name: &str, json: bool) -> i32 {
    let known = NATIVE_TOOL_NAMES.join(", ");
    emit_error(
        format!(
            "cible inconnue '{name}' - outils natifs disponibles : {known} ; \
             ou '{AGENT_CREDENTIALS_NAMESPACE}' pour un secret déclaré par un agent"
        ),
        json,
    )
}

pub(super) fn emit_error(msg: String, json: bool) -> i32 {
    crate::output::emit_error(json, exit_codes::GENERAL_ERROR, &msg.to_string())
}

pub(super) fn load_tools_config(json: bool) -> ToolsConfig {
    match find_config_path() {
        Some(p) => match parse_apollia_toml(&p) {
            Ok(c) => c.tools.unwrap_or_default(),
            Err(e) => {
                if !json {
                    eprintln!("Warning: apollia.toml unreadable ({e}) - using defaults");
                }
                ToolsConfig::default()
            }
        },
        None => ToolsConfig::default(),
    }
}

/// Looks for `apollia.toml` in the current directory, then `~/.config/apollia/`.
pub(super) fn find_config_path() -> Option<PathBuf> {
    let cwd = std::env::current_dir().ok()?;
    let local = cwd.join("apollia.toml");
    if local.exists() {
        return Some(local);
    }
    let home = apollia_core::paths::home_string()?;
    let user = PathBuf::from(home).join(".config/apollia/apollia.toml");
    if user.exists() {
        Some(user)
    } else {
        None
    }
}

/// Returns a path to write the config to: prefers an existing file, otherwise
/// `~/.config/apollia/apollia.toml` (created on the fly if needed).
pub(super) fn resolve_writable_config_path() -> Result<PathBuf, i32> {
    if let Some(p) = find_config_path() {
        return Ok(p);
    }
    let home = apollia_core::paths::home_string_or_err()
        .map_err(|_| emit_error("variable d'environnement HOME absente".to_string(), false))?;
    let dir = PathBuf::from(home).join(".config/apollia");
    if !dir.exists() {
        std::fs::create_dir_all(&dir)
            .map_err(|e| emit_error(format!("create {} failed: {e}", dir.display()), false))?;
    }
    Ok(dir.join("apollia.toml"))
}

pub(super) fn read_or_empty(path: &Path) -> std::io::Result<String> {
    match std::fs::read_to_string(path) {
        Ok(s) => Ok(s),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(e),
    }
}

pub(super) fn write_config_file(path: &Path, content: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(path, content)
}

pub(super) fn format_unix_date(ts: i64) -> String {
    chrono::DateTime::<chrono::Utc>::from_timestamp(ts, 0)
        .map(|dt| dt.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| ts.to_string())
}

pub(super) fn handle_client_error(err: ClientError, json: bool) -> i32 {
    match err {
        ClientError::ConnectionRefused => {
            // Daemon-off is exit 2, distinct from generic failures (exit 1).
            // `emit_error` returns GENERAL_ERROR, so we emit the message
            // ourselves and override the return code here.
            crate::output::emit_error(
                json,
                exit_codes::RUNTIME_ERROR,
                "runtime not started (connection refused)",
            )
        }
        other => emit_error(other.to_string(), json),
    }
}

pub(super) fn handle_server_error(status: u16, body: &str, json: bool) -> i32 {
    let msg = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(String::from))
        .unwrap_or_else(|| format!("server error ({status})"));
    emit_error(msg, json)
}
