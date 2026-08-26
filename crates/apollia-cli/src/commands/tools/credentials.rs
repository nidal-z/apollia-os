//! `tools credentials` and the legacy `tools show`.

use std::path::PathBuf;
use std::time::Instant;

use apollia_tools::ToolCredentialStore;

use crate::client::{default_socket_path, RuntimeClient};
use crate::exit_codes;
use crate::note;

use super::support::{
    db_path, emit_error, emit_unknown_tool, format_unix_date, handle_client_error,
    handle_server_error, is_valid_credential_target, keyfile_path, load_tools_config,
    open_credential_store, resolve_data_dir,
};
use super::ToolsCredentialsCmd;

// ─── Credentials ──────────────────────────────────────────────────────

pub(super) async fn run_credentials(cmd: &ToolsCredentialsCmd, json: bool) -> i32 {
    match cmd {
        ToolsCredentialsCmd::List { tool } => run_credentials_list(tool.as_deref(), json),
        ToolsCredentialsCmd::Set { tool, key } => run_credentials_set(tool, key, json),
        ToolsCredentialsCmd::Delete { tool, key, confirm } => {
            run_credentials_delete(tool, key, *confirm, json)
        }
        ToolsCredentialsCmd::Test { tool } => run_credentials_test(tool, json).await,
    }
}

pub(super) fn run_credentials_list(filter: Option<&str>, json: bool) -> i32 {
    let data_dir = match resolve_data_dir() {
        Ok(d) => d,
        Err(code) => return code,
    };
    let store = match open_credential_store(&data_dir) {
        Some(s) => s,
        None => {
            return emit_error(
                "unable to open the credential store - check ~/.apollia".to_string(),
                json,
            );
        }
    };
    let entries = match store.list(filter) {
        Ok(e) => e,
        Err(e) => return emit_error(format!("list credentials failed: {e}"), json),
    };
    if json {
        let arr: Vec<serde_json::Value> = entries
            .iter()
            .map(|e| {
                serde_json::json!({
                    "tool": e.tool_name,
                    "key": e.key_name,
                    "created_at": e.created_at,
                    "last_used_at": e.last_used_at,
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({"credentials": arr}))
                .unwrap_or_default()
        );
    } else if entries.is_empty() {
        println!("  (no credential stored)");
    } else {
        println!(
            "  {:<14} {:<18} {:<12} DERNIÈRE UTILISATION",
            "OUTIL", "CLÉ", "AJOUTÉ LE"
        );
        for e in &entries {
            let created = format_unix_date(e.created_at);
            let last = e
                .last_used_at
                .map(format_unix_date)
                .unwrap_or_else(|| "jamais".to_string());
            println!(
                "  {:<14} {:<18} {:<12} {}",
                e.tool_name, e.key_name, created, last
            );
        }
    }
    exit_codes::SUCCESS
}

pub(super) fn run_credentials_set(tool: &str, key: &str, json: bool) -> i32 {
    if !is_valid_credential_target(tool) {
        return emit_unknown_tool(tool, json);
    }
    let data_dir = match resolve_data_dir() {
        Ok(d) => d,
        Err(code) => return code,
    };
    let prompt = format!("Valeur pour {tool}/{key} : ");
    let value = match rpassword::prompt_password(&prompt) {
        Ok(v) => v,
        Err(e) => return emit_error(format!("failed to read prompt: {e}"), json),
    };
    if value.is_empty() {
        return emit_error("empty value - credential not stored".to_string(), json);
    }
    let mut store = match ToolCredentialStore::new(&db_path(&data_dir), &keyfile_path(&data_dir)) {
        Ok(s) => s,
        Err(e) => return emit_error(format!("credential store unavailable: {e}"), json),
    };
    if let Err(e) = store.set(tool, key, &value) {
        return emit_error(format!("set credential failed: {e}"), json);
    }
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "tool": tool,
                "key": key,
                "stored": true,
            }))
            .unwrap_or_default()
        );
    } else {
        note!("✔ credential {tool}/{key} stored (encrypted)");
    }
    exit_codes::SUCCESS
}

pub(super) fn run_credentials_delete(tool: &str, key: &str, confirm: bool, json: bool) -> i32 {
    if let Some(code) = crate::output::require_confirmation(
        confirm,
        json,
        &format!("delete the credential '{tool}/{key}'"),
    ) {
        return code;
    }
    let data_dir = match resolve_data_dir() {
        Ok(d) => d,
        Err(code) => return code,
    };
    let mut store = match ToolCredentialStore::new(&db_path(&data_dir), &keyfile_path(&data_dir)) {
        Ok(s) => s,
        Err(e) => return emit_error(format!("credential store unavailable: {e}"), json),
    };
    let removed = match store.delete(tool, key) {
        Ok(b) => b,
        Err(e) => return emit_error(format!("delete failed: {e}"), json),
    };
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "tool": tool,
                "key": key,
                "removed": removed,
            }))
            .unwrap_or_default()
        );
    } else if removed {
        note!("✔ credential {tool}/{key} deleted");
    } else {
        note!("ℹ no credential {tool}/{key} stored");
    }
    exit_codes::SUCCESS
}

pub(super) async fn run_credentials_test(tool: &str, json: bool) -> i32 {
    if tool != "web_search" {
        return emit_error(
            format!("credential test not implemented for '{tool}' (only web_search is supported)"),
            json,
        );
    }
    let data_dir = match resolve_data_dir() {
        Ok(d) => d,
        Err(code) => return code,
    };
    let store = match ToolCredentialStore::new(&db_path(&data_dir), &keyfile_path(&data_dir)) {
        Ok(s) => s,
        Err(e) => return emit_error(format!("credential store unavailable: {e}"), json),
    };
    let api_key = match store.get("web_search", "brave.api_key") {
        Ok(Some(k)) => k,
        Ok(None) => {
            return emit_error(
                "no brave.api_key stored - use `apollia-os tools credentials set web_search brave.api_key`"
                    .to_string(),
                json,
            );
        }
        Err(e) => return emit_error(format!("failed to read credential: {e}"), json),
    };

    let cfg = load_tools_config(json);
    let timeout = std::time::Duration::from_secs(cfg.web_search.brave.timeout_secs);
    let client = match apollia_core::net::safe_client_builder()
        .timeout(timeout)
        .build()
    {
        Ok(c) => c,
        Err(e) => return emit_error(format!("HTTP client init failed: {e}"), json),
    };
    let url = "https://api.search.brave.com/res/v1/web/search?q=apollia&count=1";
    let started = Instant::now();
    let response = client
        .get(url)
        .header("X-Subscription-Token", &api_key)
        .header("Accept", "application/json")
        .send()
        .await;
    let elapsed_ms = started.elapsed().as_millis();

    match response {
        Ok(resp) => {
            let status = resp.status();
            let mut store_mut = store;
            let _ = store_mut.touch_last_used("web_search", "brave.api_key");
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "tool": "web_search",
                        "key": "brave.api_key",
                        "http_status": status.as_u16(),
                        "latency_ms": elapsed_ms as u64,
                        "ok": status.is_success(),
                    }))
                    .unwrap_or_default()
                );
            } else if status.is_success() {
                note!("✔ brave.api_key valide ({elapsed_ms}ms, HTTP {status})");
            } else {
                note!("✗ brave.api_key rejected (HTTP {status}, {elapsed_ms}ms)");
            }
            if status.is_success() {
                exit_codes::SUCCESS
            } else {
                exit_codes::GENERAL_ERROR
            }
        }
        Err(e) => emit_error(format!("Brave call failed: {e}"), json),
    }
}

// ─── Describe (legacy) ────────────────────────────────────────────────

pub(super) async fn run_describe(socket: Option<PathBuf>, tool_name: &str, json: bool) -> i32 {
    let socket_path = socket.unwrap_or_else(default_socket_path);
    let client = RuntimeClient::new(socket_path);
    let resp = match client.get(&format!("/api/v1/tools/{tool_name}")).await {
        Ok(r) => r,
        Err(e) => return handle_client_error(e, json),
    };
    if resp.status >= 400 {
        return handle_server_error(resp.status, &resp.body, json);
    }
    let parsed: serde_json::Value = match serde_json::from_str(&resp.body) {
        Ok(v) => v,
        Err(e) => return emit_error(format!("invalid JSON response: {e}"), json),
    };
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&parsed).unwrap_or_default()
        );
    } else {
        let name = parsed.get("name").and_then(|v| v.as_str()).unwrap_or("?");
        let kind = parsed
            .get("kind")
            .and_then(|v| v.get("type"))
            .and_then(|t| t.as_str())
            .unwrap_or("?");
        let desc = parsed
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        println!("  Name      : {name}");
        println!("  Kind      : {kind}");
        if !desc.is_empty() {
            println!("  Desc      : {desc}");
        }
    }
    exit_codes::SUCCESS
}
