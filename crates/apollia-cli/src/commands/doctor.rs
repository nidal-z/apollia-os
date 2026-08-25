//! `apollia-os doctor`: diagnose the local Apollia environment.
//!
//! Performs read-only checks that do not require the runtime to be running:
//! - Apollia home directory presence and writability
//! - `apollia.toml` parses successfully
//! - Local SQLite databases (agents, governance, plan_cache) are openable
//! - Models directory state
//! - Python availability for the PyO3 bridge
//! - Runtime socket reachability (informational)
//!
//! Exits with code 0 when everything is OK or only produces warnings, and
//! with code 1 when at least one check fails.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::client::{ClientError, RuntimeClient, DEFAULT_SOCKET_PATH};
use crate::exit_codes;

/// Status of a single diagnostic check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckStatus {
    /// Check passed.
    Ok,
    /// Check produced a non-fatal warning.
    Warn,
    /// Check failed: `doctor` exits with code 1.
    Error,
}

/// A single diagnostic result.
#[derive(Debug, serde::Serialize)]
pub struct CheckResult {
    /// Short identifier of the check (e.g. `"apollia_home"`).
    pub id: String,
    /// Human-readable label.
    pub label: String,
    /// Status of the check.
    pub status: CheckStatus,
    /// Short message describing the result.
    pub message: String,
    /// Optional next-step hint shown when status is `Warn` or `Error`.
    pub hint: Option<String>,
}

impl CheckResult {
    fn ok(id: &str, label: &str, message: impl Into<String>) -> Self {
        Self {
            id: id.to_string(),
            label: label.to_string(),
            status: CheckStatus::Ok,
            message: message.into(),
            hint: None,
        }
    }

    fn warn(id: &str, label: &str, message: impl Into<String>, hint: impl Into<String>) -> Self {
        Self {
            id: id.to_string(),
            label: label.to_string(),
            status: CheckStatus::Warn,
            message: message.into(),
            hint: Some(hint.into()),
        }
    }

    fn error(id: &str, label: &str, message: impl Into<String>, hint: impl Into<String>) -> Self {
        Self {
            id: id.to_string(),
            label: label.to_string(),
            status: CheckStatus::Error,
            message: message.into(),
            hint: Some(hint.into()),
        }
    }
}

/// Run all diagnostic checks and produce a report.
///
/// `socket` is the optional Unix socket path override used for the runtime
/// reachability check.
pub async fn run(socket: Option<PathBuf>, json: bool) -> i32 {
    let home = apollia_core::paths::home_dir_or_temp();
    let data_dir = home.join(".apollia");

    let mut checks: Vec<CheckResult> = Vec::new();
    checks.push(check_apollia_home(&data_dir));
    checks.push(check_config_file(&data_dir));
    checks.push(check_governance_db(&data_dir));
    checks.push(check_agents_db(&data_dir));
    checks.push(check_models_dir(&data_dir));
    checks.push(check_python());
    checks.push(check_sandbox_posture());
    checks.push(check_runtime_socket(socket).await);

    let any_error = checks.iter().any(|c| c.status == CheckStatus::Error);

    if json {
        let overall = if any_error { "error" } else { "ok" };
        let report = serde_json::json!({
            "overall": overall,
            "checks": checks,
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&report).unwrap_or_default()
        );
    } else {
        print_text_report(&checks, any_error);
    }

    if any_error {
        exit_codes::GENERAL_ERROR
    } else {
        exit_codes::SUCCESS
    }
}

/// Verify that `~/.apollia/` exists and is writable.
fn check_apollia_home(data_dir: &Path) -> CheckResult {
    if !data_dir.exists() {
        return CheckResult::warn(
            "apollia_home",
            "Apollia home",
            format!(
                "{} does not exist (will be created on first use)",
                data_dir.display()
            ),
            "Run `apollia-os start` once: it creates the directory before writing the API token",
        );
    }
    // Probe writability by attempting to create a tmpfile.
    let probe = data_dir.join(".doctor_probe");
    match std::fs::write(&probe, b"") {
        Ok(()) => {
            let _ = std::fs::remove_file(&probe);
            CheckResult::ok(
                "apollia_home",
                "Apollia home",
                format!("{} (writable)", data_dir.display()),
            )
        }
        Err(e) => CheckResult::error(
            "apollia_home",
            "Apollia home",
            format!("{} not writable: {e}", data_dir.display()),
            "Check filesystem permissions",
        ),
    }
}

/// Verify that `apollia.toml` parses if present.
///
/// `apollia.toml` is **optional**: it only carries runtime tuning sections
/// (`[api]`, `[runtime]`, `[hitl]`, `[tools]`, `[a2a]`, `[oria]`, `[llm]`).
/// All operational data (triggers, notifications, agents, MCP servers, LLM
/// backends, projects, ...) lives in SQLite and is managed via `apollia-os`
/// subcommands or the Desktop app, so absence is expected for most setups.
fn check_config_file(data_dir: &Path) -> CheckResult {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let local = cwd.join("apollia.toml");
    let user_cfg = data_dir
        .parent()
        .map(|p| p.join(".config").join("apollia").join("apollia.toml"))
        .unwrap_or_else(|| PathBuf::from("~/.config/apollia/apollia.toml"));

    let path = if local.exists() {
        local
    } else if user_cfg.exists() {
        user_cfg
    } else {
        return CheckResult::ok(
            "config_file",
            "Config file",
            "no apollia.toml (optional - runtime tuning only; defaults are fine)",
        );
    };

    match crate::config::parse_apollia_toml(&path) {
        Ok(_) => CheckResult::ok(
            "config_file",
            "Config file",
            format!("{} (valid)", path.display()),
        ),
        Err(e) => CheckResult::error(
            "config_file",
            "Config file",
            format!("{} invalid: {e}", path.display()),
            "Fix the TOML syntax or remove the offending section",
        ),
    }
}

/// Verify that `governance.db` opens cleanly if it exists.
fn check_governance_db(data_dir: &Path) -> CheckResult {
    let db_path = data_dir.join(apollia_tools::GOVERNANCE_DB_FILENAME);
    if !db_path.exists() {
        return CheckResult::warn(
            "governance_db",
            "Governance DB",
            "not yet created",
            "Will be initialized on first tool governance command or runtime start",
        );
    }
    let keyfile = data_dir.join(".keyfile");
    match apollia_tools::ToolCredentialStore::new(&db_path, &keyfile) {
        Ok(_) => CheckResult::ok(
            "governance_db",
            "Governance DB",
            format!("{} (openable)", db_path.display()),
        ),
        Err(e) => CheckResult::error(
            "governance_db",
            "Governance DB",
            format!("failed to open: {e}"),
            "Inspect the database file or remove it to reinitialize",
        ),
    }
}

/// Verify that `agents.db` opens cleanly if it exists.
fn check_agents_db(data_dir: &Path) -> CheckResult {
    let db_path = data_dir.join("agents.db");
    if !db_path.exists() {
        return CheckResult::ok(
            "agents_db",
            "Agents DB",
            "not yet created (no agents installed)",
        );
    }
    match apollia_tools::AgentRepository::open(&db_path) {
        Ok(_) => CheckResult::ok(
            "agents_db",
            "Agents DB",
            format!("{} (openable)", db_path.display()),
        ),
        Err(e) => CheckResult::error(
            "agents_db",
            "Agents DB",
            format!("failed to open: {e}"),
            "Restore from backup or remove the file to reinitialize",
        ),
    }
}

/// Report the local model directory state (informational).
fn check_models_dir(data_dir: &Path) -> CheckResult {
    let dir = data_dir.join("models");
    if !dir.exists() {
        return CheckResult::warn(
            "models_dir",
            "Local models",
            "~/.apollia/models/ does not exist",
            "Place a .gguf file in ~/.apollia/models/, or run `apollia-os llm setup --local --model <path.gguf>`",
        );
    }
    let entries = std::fs::read_dir(&dir).ok().map(|d| d.count()).unwrap_or(0);
    if entries == 0 {
        CheckResult::warn(
            "models_dir",
            "Local models",
            "directory is empty",
            "Place a .gguf file in ~/.apollia/models/, or run `apollia-os llm setup --local --model <path.gguf>`",
        )
    } else {
        CheckResult::ok(
            "models_dir",
            "Local models",
            format!("{entries} file(s) in {}", dir.display()),
        )
    }
}

/// Verify that the Python interpreter linked against PyO3 is usable.
fn check_python() -> CheckResult {
    use pyo3::Python;
    Python::with_gil(|py| {
        let version = py.version().to_string();
        CheckResult::ok(
            "python",
            "Python runtime",
            format!("PyO3 bridge functional ({version})"),
        )
    })
}

/// Report the active isolation level for native tools and agent code.
///
/// This is a `Warn`, never an `Error`: the absence of an OS sandbox on macOS is
/// the documented development posture, not a failure, so it must not
/// turn a healthy mac into a non-zero `doctor` exit.
fn check_sandbox_posture() -> CheckResult {
    use apollia_core::{SecurityPosture, ToolSandbox};

    let posture = SecurityPosture::detect();
    let rlimits = if posture.rlimits_active {
        "per-process rlimits active"
    } else {
        "no per-process rlimits"
    };
    match posture.tool_sandbox {
        ToolSandbox::LinuxNamespaces => CheckResult::ok(
            "sandbox_posture",
            "Sandbox posture",
            format!(
                "{}: tool sandbox = Linux namespaces (PID + mount), {rlimits}; \
                 agent code runs in-process (trusted)",
                posture.platform
            ),
        ),
        ToolSandbox::DevNoSandbox => CheckResult::warn(
            "sandbox_posture",
            "Sandbox posture",
            format!(
                "{}: no OS sandbox for native tools (dev mode), {rlimits}; \
                 agent code runs in-process (trusted)",
                posture.platform
            ),
            "Agent Python is trusted code with your full rights: only \
             run agents you have audited. Production tool isolation requires Linux.",
        ),
    }
}

/// Probe the runtime Unix socket without holding the connection open.
async fn check_runtime_socket(socket: Option<PathBuf>) -> CheckResult {
    let socket_path = socket.unwrap_or_else(|| PathBuf::from(DEFAULT_SOCKET_PATH));
    if !socket_path.exists() {
        return CheckResult::warn(
            "runtime_socket",
            "Runtime socket",
            format!(
                "{} not present (runtime not started)",
                socket_path.display()
            ),
            "Start the runtime with `apollia-os start`",
        );
    }
    let client = RuntimeClient::new(socket_path.clone());
    match tokio::time::timeout(Duration::from_millis(500), client.health()).await {
        Ok(Ok(_)) => CheckResult::ok(
            "runtime_socket",
            "Runtime socket",
            format!("{} (reachable, runtime up)", socket_path.display()),
        ),
        Ok(Err(ClientError::ConnectionRefused)) => CheckResult::warn(
            "runtime_socket",
            "Runtime socket",
            "socket exists but connection refused (stale socket file?)",
            "Remove the stale socket and start the runtime again",
        ),
        Ok(Err(e)) => CheckResult::warn(
            "runtime_socket",
            "Runtime socket",
            format!("reachable but health check failed: {e}"),
            "Inspect runtime logs",
        ),
        Err(_) => CheckResult::warn(
            "runtime_socket",
            "Runtime socket",
            "health check timed out (>500ms)",
            "Inspect runtime logs for hung initialization",
        ),
    }
}

/// Render the report in human-readable form.
fn print_text_report(checks: &[CheckResult], any_error: bool) {
    println!();
    println!("  Apollia OS Doctor");
    println!("  -----------------");
    let label_width = checks.iter().map(|c| c.label.len()).max().unwrap_or(20);
    for c in checks {
        let glyph = match c.status {
            CheckStatus::Ok => "*",
            CheckStatus::Warn => "!",
            CheckStatus::Error => "x",
        };
        println!(
            "  {glyph} {:<width$}  {}",
            c.label,
            c.message,
            width = label_width
        );
        if let Some(hint) = &c.hint {
            println!("    -> {hint}");
        }
    }
    println!("  -----------------");
    if any_error {
        println!("  Overall: FAILED (one or more checks errored)");
    } else {
        println!("  Overall: HEALTHY");
    }
    println!();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_status_serialises_lowercase() {
        let json = serde_json::to_string(&CheckStatus::Ok).unwrap();
        assert_eq!(json, "\"ok\"");
        let json = serde_json::to_string(&CheckStatus::Warn).unwrap();
        assert_eq!(json, "\"warn\"");
        let json = serde_json::to_string(&CheckStatus::Error).unwrap();
        assert_eq!(json, "\"error\"");
    }

    #[test]
    fn check_result_ok_has_no_hint() {
        let r = CheckResult::ok("id", "label", "all good");
        assert_eq!(r.status, CheckStatus::Ok);
        assert!(r.hint.is_none());
    }

    #[test]
    fn check_result_warn_carries_hint() {
        let r = CheckResult::warn("id", "label", "missing", "create it");
        assert_eq!(r.status, CheckStatus::Warn);
        assert_eq!(r.hint.as_deref(), Some("create it"));
    }

    /// Serialises the HOME mutation below: the variable is a process global,
    /// and the same binary carries tests that read the resolved home.
    static HOME_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    #[tokio::test]
    async fn doctor_runs_without_runtime() {
        // GIVEN no runtime is running and a fresh temp HOME, held under the
        // lock and restored before the test returns.
        let _guard = HOME_LOCK.lock().await;
        let tmp = tempfile::tempdir().unwrap();
        let previous = std::env::var_os("HOME");
        std::env::set_var("HOME", tmp.path());
        // WHEN doctor runs.
        let code = run(Some(tmp.path().join("nonexistent.sock")), true).await;
        match previous {
            Some(v) => std::env::set_var("HOME", v),
            None => std::env::remove_var("HOME"),
        }
        // THEN we must not crash; exit code is either 0 or 1 depending on warns.
        assert!(code == 0 || code == 1);
    }

    #[test]
    fn sandbox_posture_check_is_never_an_error() {
        // GIVEN the running platform
        // WHEN building the sandbox posture check
        let check = check_sandbox_posture();
        // THEN it has a stable id and never fails doctor (dev-no-sandbox is a warn)
        assert_eq!(check.id, "sandbox_posture");
        assert_ne!(check.status, CheckStatus::Error);
    }
}
