//! `tools list`, `tools enable` and `tools disable`.

use apollia_core::{ToolsConfig, WebSearchBackend};
use apollia_tools::CredentialEntry;

use crate::exit_codes;
use crate::note;

use super::support::{
    emit_error, emit_unknown_tool, is_known_tool, load_tools_config, open_credential_store,
    open_registry, resolve_data_dir,
};

// ─── List ─────────────────────────────────────────────────────────────

pub(super) fn run_list(json: bool) -> i32 {
    let data_dir = match resolve_data_dir() {
        Ok(d) => d,
        Err(code) => return code,
    };
    let registry = match open_registry(&data_dir, json) {
        Ok(r) => r,
        Err(code) => return code,
    };
    let statuses = match registry.list() {
        Ok(s) => s,
        Err(e) => return emit_error(format!("registry list failed: {e}"), json),
    };

    let credentials = open_credential_store(&data_dir).and_then(|s| s.list(None).ok());

    let tools_cfg = load_tools_config(json);

    if json {
        let arr: Vec<serde_json::Value> = statuses
            .iter()
            .map(|s| {
                let backend = backend_label(&s.name, &tools_cfg);
                let creds = credentials_summary(&s.name, credentials.as_deref());
                serde_json::json!({
                    "name": s.name,
                    "enabled": s.enabled,
                    "backend": backend,
                    "credentials": creds,
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({"tools": arr}))
                .unwrap_or_else(|_| "{}".to_string())
        );
    } else {
        println!(
            "  {:<16} {:<7} {:<22} CREDENTIALS",
            "NOM", "ACTIF", "BACKEND"
        );
        for s in &statuses {
            let active = if s.enabled { "✓" } else { "✗" };
            let backend = backend_label(&s.name, &tools_cfg);
            let creds = credentials_text(&s.name, credentials.as_deref());
            println!("  {:<16} {:<7} {:<22} {}", s.name, active, backend, creds);
        }
    }
    exit_codes::SUCCESS
}

pub(super) fn backend_label(tool: &str, cfg: &ToolsConfig) -> String {
    if tool == "web_search" {
        match cfg.web_search.backend {
            WebSearchBackend::Auto => "DuckDuckGo (auto)".to_string(),
            WebSearchBackend::DuckDuckGo => "DuckDuckGo".to_string(),
            WebSearchBackend::Brave => "Brave".to_string(),
        }
    } else {
        "-".to_string()
    }
}

pub(super) fn credentials_summary(
    tool: &str,
    credentials: Option<&[CredentialEntry]>,
) -> serde_json::Value {
    let needs = required_credentials(tool);
    if needs.is_empty() {
        return serde_json::Value::Null;
    }
    let entries = credentials.unwrap_or(&[]);
    let arr: Vec<serde_json::Value> = needs
        .iter()
        .map(|key| {
            let present = entries
                .iter()
                .any(|e| e.tool_name == tool && e.key_name == *key);
            serde_json::json!({"key": key, "present": present})
        })
        .collect();
    serde_json::Value::Array(arr)
}

pub(super) fn credentials_text(tool: &str, credentials: Option<&[CredentialEntry]>) -> String {
    let needs = required_credentials(tool);
    if needs.is_empty() {
        return "-".to_string();
    }
    let entries = credentials.unwrap_or(&[]);
    needs
        .iter()
        .map(|key| {
            let present = entries
                .iter()
                .any(|e| e.tool_name == tool && e.key_name == *key);
            if present {
                format!("{key}: present")
            } else {
                format!("{key}: absent ⚠")
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

pub(super) fn required_credentials(tool: &str) -> &'static [&'static str] {
    match tool {
        "web_search" => &["brave.api_key"],
        _ => &[],
    }
}

// ─── Enable / Disable ─────────────────────────────────────────────────

pub(super) fn run_set_enabled(name: &str, enabled: bool, json: bool) -> i32 {
    if !is_known_tool(name) {
        return emit_unknown_tool(name, json);
    }
    let data_dir = match resolve_data_dir() {
        Ok(d) => d,
        Err(code) => return code,
    };
    let mut registry = match open_registry(&data_dir, json) {
        Ok(r) => r,
        Err(code) => return code,
    };
    if let Err(e) = registry.set_enabled(name, enabled) {
        return emit_error(format!("registry update failed: {e}"), json);
    }
    let action = if enabled { "enabled" } else { "disabled" };
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "tool": name,
                "enabled": enabled,
            }))
            .unwrap_or_default()
        );
    } else {
        note!("✔ {name} {action}");
    }
    exit_codes::SUCCESS
}
