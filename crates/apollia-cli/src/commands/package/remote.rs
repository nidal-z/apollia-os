//! The `package` leaves that go through the runtime: `preview`, `install` and
//! `trust`.
//!
//! The archive is sent to the runtime by its absolute path: both run on this
//! machine. A file that does not exist is refused here, exit 1, before the
//! runtime is reached.

use std::path::{Path, PathBuf};

use serde_json::Value;

use super::TrustCommand;
use crate::client::{default_socket_path, RuntimeClient};
use crate::exit_codes;
use crate::output::{emit_client_error, emit_error, require_confirmation};

/// The confirmations `package install` passes on.
#[derive(Debug, Clone, Copy)]
pub(super) struct InstallFlags {
    pub(super) confirm_deps: bool,
    pub(super) confirm_changes: bool,
    pub(super) allow_downgrade: bool,
}

fn client(socket: Option<PathBuf>) -> RuntimeClient {
    RuntimeClient::new(socket.unwrap_or_else(default_socket_path))
}

/// The absolute path of `file`, or the message that refuses it.
fn archive_path(file: &Path) -> Result<String, String> {
    if !file.is_file() {
        return Err(format!("{}: no such package file", file.display()));
    }
    std::path::absolute(file)
        .map(|p| p.display().to_string())
        .map_err(|e| format!("{}: {e}", file.display()))
}

/// `apollia-os package preview <file>`.
pub(super) async fn run_preview(file: &Path, socket: Option<PathBuf>, json: bool) -> i32 {
    let path = match archive_path(file) {
        Ok(path) => path,
        Err(message) => return emit_error(json, exit_codes::GENERAL_ERROR, &message),
    };
    match client(socket).preview_package(&path).await {
        Ok(preview) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&preview).unwrap_or_default()
                );
            } else {
                print_preview(&preview);
            }
            exit_codes::SUCCESS
        }
        Err(e) => emit_client_error(json, &e),
    }
}

/// `apollia-os package install <file>`.
pub(super) async fn run_install(
    file: &Path,
    flags: InstallFlags,
    socket: Option<PathBuf>,
    json: bool,
) -> i32 {
    let path = match archive_path(file) {
        Ok(path) => path,
        Err(message) => return emit_error(json, exit_codes::GENERAL_ERROR, &message),
    };
    let result = client(socket)
        .install_package(
            &path,
            flags.confirm_deps,
            flags.confirm_changes,
            flags.allow_downgrade,
        )
        .await;
    match result {
        Ok(report) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&report).unwrap_or_default()
                );
            } else {
                print_report(&report);
            }
            exit_codes::SUCCESS
        }
        Err(e) => emit_client_error(json, &e),
    }
}

/// `apollia-os package trust <verb>`.
pub(super) async fn run_trust(cmd: &TrustCommand, socket: Option<PathBuf>, json: bool) -> i32 {
    let client = client(socket);
    match cmd {
        TrustCommand::Import { key, label } => {
            let key = match read_key(key) {
                Ok(key) => key,
                Err(message) => return emit_error(json, exit_codes::GENERAL_ERROR, &message),
            };
            match client.trust_key(&key, label).await {
                Ok(view) => {
                    if json {
                        println!(
                            "{}",
                            serde_json::to_string_pretty(&view).unwrap_or_default()
                        );
                    } else {
                        println!("{}  {}", text(&view, "fingerprint"), text(&view, "label"));
                    }
                    exit_codes::SUCCESS
                }
                Err(e) => emit_client_error(json, &e),
            }
        }
        TrustCommand::List => match client.list_trusted_keys().await {
            Ok(keys) => {
                if json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&keys).unwrap_or_default()
                    );
                } else {
                    print_keys(&keys);
                }
                exit_codes::SUCCESS
            }
            Err(e) => emit_client_error(json, &e),
        },
        TrustCommand::Remove {
            fingerprint,
            confirm,
        } => {
            let keys = match client.list_trusted_keys().await {
                Ok(keys) => keys,
                Err(e) => return emit_client_error(json, &e),
            };
            let known = keys.as_array().is_some_and(|keys| {
                keys.iter()
                    .any(|k| text(k, "fingerprint").eq_ignore_ascii_case(fingerprint))
            });
            if !known {
                return emit_error(
                    json,
                    exit_codes::GENERAL_ERROR,
                    &format!("no trusted key of fingerprint {fingerprint}"),
                );
            }
            if let Some(code) = require_confirmation(
                *confirm,
                json,
                &format!("stop trusting the publisher key {fingerprint}"),
            ) {
                return code;
            }
            match client.remove_trusted_key(fingerprint).await {
                Ok(body) => {
                    if json {
                        println!(
                            "{}",
                            serde_json::to_string_pretty(&body).unwrap_or_default()
                        );
                    } else {
                        println!("{fingerprint}");
                        crate::note!("  removed from the trusted keys");
                    }
                    exit_codes::SUCCESS
                }
                Err(e) => emit_client_error(json, &e),
            }
        }
    }
}

/// The key `arg` names: the content of the file `arg` when it is one, the
/// argument itself otherwise.
fn read_key(arg: &str) -> Result<String, String> {
    let path = Path::new(arg);
    if path.is_file() {
        return std::fs::read_to_string(path)
            .map(|text| text.trim().to_owned())
            .map_err(|e| format!("{arg}: {e}"));
    }
    Ok(arg.trim().to_owned())
}

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or_default()
}

fn items(value: &Value, key: &str) -> Vec<Value> {
    value
        .get(key)
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn signature_line(signature: &Value) -> String {
    let fingerprint = text(signature, "keyFingerprint");
    match text(signature, "status") {
        "signed" if signature["trusted"] == Value::Bool(true) => format!(
            "{} (trusted, key {fingerprint})",
            text(signature, "publisher")
        ),
        "signed" => format!(
            "{} (declared, not verified; key {fingerprint})",
            text(signature, "declaredPublisher")
        ),
        "invalid" => format!(
            "{} (INVALID signature: {})",
            text(signature, "declaredPublisher"),
            text(signature, "reason")
        ),
        _ => format!("{} (unsigned)", text(signature, "declaredPublisher")),
    }
}

fn print_preview(preview: &Value) {
    println!("{} {}", text(preview, "name"), text(preview, "version"));
    println!("  publisher  {}", signature_line(&preview["signature"]));
    if let Some(installed) = preview.get("installedVersion").and_then(Value::as_str) {
        println!("  installed  {installed}");
    }
    let agents = items(preview, "agents");
    if !agents.is_empty() {
        crate::note!("agents");
        for agent in &agents {
            println!("  {}  {}", text(agent, "id"), text(agent, "name"));
            for access in items(agent, "access") {
                let mark = if access["sensitive"] == Value::Bool(true) {
                    "  [sensitive]"
                } else {
                    ""
                };
                println!(
                    "    {}  {}{mark}",
                    text(&access, "accessId"),
                    text(&access, "permits")
                );
            }
        }
    }
    for (title, key) in [("workflows", "workflows"), ("tools", "tools")] {
        let list = items(preview, key);
        if !list.is_empty() {
            crate::note!("{title}");
            for item in list {
                println!(
                    "  {}  {}  ({})",
                    text(&item, "name"),
                    text(&item, "description"),
                    text(&item, "family")
                );
            }
        }
    }
    let requires = items(preview, "requires");
    if !requires.is_empty() {
        crate::note!("required connections");
        for required in requires {
            let state = if required["present"] == Value::Bool(true) {
                "present"
            } else if required["broughtByPackage"] == Value::Bool(true) {
                "brought by the package"
            } else {
                "missing"
            };
            println!("  {}  {state}", text(&required, "type"));
        }
    }
    let servers = items(preview, "mcpServers");
    if !servers.is_empty() {
        crate::note!("MCP servers brought");
        for server in servers {
            let present = if server["alreadyPresent"] == Value::Bool(true) {
                "  (already present, left as it is)"
            } else {
                ""
            };
            println!(
                "  {}  {}{present}",
                text(&server, "name"),
                text(&server, "transport")
            );
        }
    }
    let pip = items(preview, "pip");
    if !pip.is_empty() {
        crate::note!("Python dependencies");
        for requirement in pip {
            println!("  {}", requirement.as_str().unwrap_or_default());
        }
    }
    if let Some(update) = preview.get("update").filter(|u| !u.is_null()) {
        crate::note!(
            "update from {} to {}",
            text(update, "fromVersion"),
            text(update, "toVersion")
        );
        for access in items(update, "addedAccess") {
            println!(
                "  + access {} for {}",
                text(&access, "accessId"),
                text(&access, "agentId")
            );
        }
        for kind in items(update, "addedRequires") {
            println!("  + requires {}", kind.as_str().unwrap_or_default());
        }
        for name in items(update, "addedMcpServers") {
            println!("  + MCP server {}", name.as_str().unwrap_or_default());
        }
    }
    for notice in items(preview, "notices") {
        crate::note!("note: {}", notice_line(&notice));
    }
    if let Some(refusal) = preview.get("refusal").filter(|r| !r.is_null()) {
        println!("  refused  {}", text(refusal, "code"));
    }
}

fn notice_line(notice: &Value) -> String {
    match text(notice, "code") {
        "unsigned" => "the package is not signed".to_owned(),
        "untrusted_publisher" => format!(
            "the key {} is not in the trusted keys",
            text(notice, "fingerprint")
        ),
        "missing_connection" => format!("the connection {} is missing", text(notice, "type")),
        "server_already_present" => format!(
            "an MCP server named {} already exists",
            text(notice, "name")
        ),
        other => other.to_owned(),
    }
}

fn print_report(report: &Value) {
    println!("{} {}", text(report, "name"), text(report, "version"));
    if let Some(previous) = report.get("previousVersion").and_then(Value::as_str) {
        crate::note!("  updated from {previous}");
    }
    crate::note!("  publisher  {}", signature_line(&report["signature"]));
    for notice in items(report, "notices") {
        crate::note!("note: {}", notice_line(&notice));
    }
}

fn print_keys(keys: &Value) {
    let list = keys.as_array().cloned().unwrap_or_default();
    if list.is_empty() {
        crate::note!("no trusted publisher key");
        return;
    }
    crate::note!("{:<64}  LABEL", "FINGERPRINT");
    for key in list {
        println!("{:<64}  {}", text(&key, "fingerprint"), text(&key, "label"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_archive_path_refuses_a_missing_file() {
        // GIVEN a path that names no file
        let missing = Path::new("/nonexistent/crm.apo");

        // WHEN it is resolved
        let result = archive_path(missing);

        // THEN it is refused, naming the path
        assert!(result.unwrap_err().contains("no such package file"));
    }

    #[test]
    fn test_read_key_takes_a_file_content_or_the_argument() {
        // GIVEN a file holding a key, and a key given inline
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("acme.pub");
        std::fs::write(&file, "ab12\n").unwrap();

        // WHEN each is read
        let from_file = read_key(&file.display().to_string()).unwrap();
        let inline = read_key(" cd34 ").unwrap();

        // THEN the file content and the argument are each trimmed
        assert_eq!(from_file, "ab12");
        assert_eq!(inline, "cd34");
    }

    #[test]
    fn test_signature_line_names_trust_and_verification() {
        // GIVEN a trusted, an untrusted and an invalid signature
        let trusted = serde_json::json!({
            "status": "signed", "trusted": true, "publisher": "Acme Corp",
            "declaredPublisher": "Acme", "keyFingerprint": "ff"
        });
        let untrusted = serde_json::json!({
            "status": "signed", "trusted": false, "publisher": "Acme",
            "declaredPublisher": "Acme", "keyFingerprint": "ee"
        });
        let invalid = serde_json::json!({
            "status": "invalid", "declaredPublisher": "Acme", "reason": "altered"
        });

        // WHEN each is rendered
        let lines = [
            signature_line(&trusted),
            signature_line(&untrusted),
            signature_line(&invalid),
        ];

        // THEN the label of the keyring, the declared name, or the refusal shows
        assert_eq!(lines[0], "Acme Corp (trusted, key ff)");
        assert_eq!(lines[1], "Acme (declared, not verified; key ee)");
        assert_eq!(lines[2], "Acme (INVALID signature: altered)");
    }
}
