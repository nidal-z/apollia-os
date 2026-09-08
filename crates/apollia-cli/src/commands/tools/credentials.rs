//! `tools credentials` and the legacy `tools show`.

use std::io::IsTerminal;
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
            "  {:<14} {:<18} {:<12} LAST USED",
            "TOOL", "KEY", "ADDED ON"
        );
        for e in &entries {
            let created = format_unix_date(e.created_at);
            let last = e
                .last_used_at
                .map(format_unix_date)
                .unwrap_or_else(|| "never".to_string());
            println!(
                "  {:<14} {:<18} {:<12} {}",
                e.tool_name, e.key_name, created, last
            );
        }
    }
    exit_codes::SUCCESS
}

/// Strip the line terminator a piped value carries, CRLF included.
fn credential_value_from_line(line: &str) -> String {
    line.trim_end_matches(['\r', '\n']).to_string()
}

/// The value to store: read from the console without echo when a human is
/// driving, and from standard input when one is not.
///
/// `rpassword` reads the console itself, `/dev/tty` on Unix and `CONIN$` on
/// Windows, and that is deliberate: a secret must not be echoed and must not
/// be captured by a shell redirection. The consequence is that it ignores
/// standard input entirely, so `echo secret | apollia tools credentials set`
/// never reaches it and `< /dev/null` answers nothing. On Windows under a
/// terminal that owns no console, git-bash among them, the prompt is not even
/// visible and the command waits forever on input nobody knows to type.
/// Measured on 2026-09-08: the end-to-end suite stopped there and never
/// finished. Principle 8, human CLI and machine API, says the same thing.
fn read_credential_value(prompt: &str) -> std::io::Result<String> {
    if std::io::stdin().is_terminal() {
        return rpassword::prompt_password(prompt);
    }
    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    Ok(credential_value_from_line(&line))
}

pub(super) fn run_credentials_set(tool: &str, key: &str, json: bool) -> i32 {
    if !is_valid_credential_target(tool) {
        return emit_unknown_tool(tool, json);
    }
    let data_dir = match resolve_data_dir() {
        Ok(d) => d,
        Err(code) => return code,
    };
    let prompt = format!("Value for {tool}/{key}: ");
    let value = match read_credential_value(&prompt) {
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

#[cfg(test)]
mod tests {
    use super::credential_value_from_line;

    #[test]
    fn a_piped_value_keeps_its_content_and_loses_its_terminator() {
        // GIVEN a value piped by a script, terminated the Unix way and the
        // Windows way
        let unix = "s3cret\n";
        let windows = "s3cret\r\n";

        // WHEN the command reads that line
        let from_unix = credential_value_from_line(unix);
        let from_windows = credential_value_from_line(windows);

        // THEN both store the same secret, with no terminator in it
        assert_eq!(from_unix, "s3cret");
        assert_eq!(from_windows, "s3cret");
    }

    #[test]
    fn a_closed_input_reads_as_empty_so_the_command_refuses() {
        // GIVEN standard input closed with nothing on it, which is what
        // `< /dev/null` hands the command
        let nothing = "";

        // WHEN the command reads that line
        let value = credential_value_from_line(nothing);

        // THEN the value is empty, the case the caller answers with a refusal
        // rather than storing a blank credential
        assert!(value.is_empty());
    }
}
