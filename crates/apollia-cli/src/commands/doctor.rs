//! `apollia-os doctor`: diagnose the local Apollia environment.
//!
//! Performs read-only checks that do not require the runtime to be running:
//! - Apollia home directory presence and writability
//! - `apollia.toml` parses successfully
//! - Local SQLite databases (agents, governance, plan_cache) are openable
//! - Models directory state
//! - Optical character recognition: engine compiled in, language data verified
//! - Python availability for the PyO3 bridge
//! - Runtime socket reachability (informational)
//!
//! Exits with code 0 when everything is OK or only produces warnings, and
//! with code 1 when at least one check fails.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::client::{default_socket_path, ClientError, RuntimeClient};
use crate::exit_codes;
use crate::note;

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
    /// Machine-readable facts behind the message, for the checks that have
    /// some; absent from the JSON of the others.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<serde_json::Value>,
}

impl CheckResult {
    fn ok(id: &str, label: &str, message: impl Into<String>) -> Self {
        Self {
            id: id.to_string(),
            label: label.to_string(),
            status: CheckStatus::Ok,
            message: message.into(),
            hint: None,
            detail: None,
        }
    }

    fn warn(id: &str, label: &str, message: impl Into<String>, hint: impl Into<String>) -> Self {
        Self {
            id: id.to_string(),
            label: label.to_string(),
            status: CheckStatus::Warn,
            message: message.into(),
            hint: Some(hint.into()),
            detail: None,
        }
    }

    fn error(id: &str, label: &str, message: impl Into<String>, hint: impl Into<String>) -> Self {
        Self {
            id: id.to_string(),
            label: label.to_string(),
            status: CheckStatus::Error,
            message: message.into(),
            hint: Some(hint.into()),
            detail: None,
        }
    }
}

/// Run all diagnostic checks and produce a report.
///
/// `socket` is the optional Unix socket path override used for the runtime
/// reachability check.
pub async fn run(socket: Option<PathBuf>, json: bool) -> i32 {
    let home = apollia_core::paths::home_dir_or_temp();
    let data_dir = apollia_core::paths::data_dir_under(&home);

    let mut checks: Vec<CheckResult> = Vec::new();
    checks.push(check_apollia_home(&data_dir));
    checks.push(check_config_file(&home));
    let cwd = std::env::current_dir().ok();
    let xdg = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from);
    checks.extend(check_legacy_config(&home, cwd.as_deref(), xdg.as_deref()));
    checks.push(check_governance_db(&data_dir));
    checks.push(check_agents_db(&data_dir));
    checks.push(check_models_dir(&data_dir));
    checks.push(check_recognition(
        apollia_sources::Ocr::compiled(),
        &apollia_llm::ocr_data::tessdata_dir(&apollia_core::config::models_dir(&data_dir)),
        apollia_llm::recommend::OcrCatalogue::embedded(),
    ));
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
/// backends, sources, ...) lives in SQLite and is managed via `apollia-os`
/// subcommands or the Desktop app, so absence is expected for most setups.
///
/// The file is `~/.apollia/apollia.toml`, the one every surface reads.
fn check_config_file(home: &Path) -> CheckResult {
    let path = apollia_core::paths::config_file_under(home);
    if !path.is_file() {
        return CheckResult::ok(
            "config_file",
            "Config file",
            "no apollia.toml (optional - runtime tuning only; defaults are fine)",
        );
    }

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

/// Name every `apollia.toml` left at a location no surface reads any more
/// (the working directory, `$XDG_CONFIG_HOME/apollia/`, `~/.config/apollia/`),
/// with the command that moves it. `None` when there is none.
fn check_legacy_config(
    home: &Path,
    cwd: Option<&Path>,
    xdg_config_home: Option<&Path>,
) -> Option<CheckResult> {
    let legacy = crate::config_location::legacy_files(home, cwd, xdg_config_home);
    if legacy.is_empty() {
        return None;
    }
    let canonical = apollia_core::paths::config_file_under(home);
    let names: Vec<String> = legacy.iter().map(|p| p.display().to_string()).collect();
    let hint = if canonical.is_file() {
        format!(
            "Copy the keys you still want into {}, then delete the ignored file",
            canonical.display()
        )
    } else {
        let commands: Vec<String> = legacy
            .iter()
            .map(|p| crate::config_location::move_command(p, &canonical))
            .collect();
        format!("Move it: {}", commands.join(" ; "))
    };
    Some(CheckResult::warn(
        "config_legacy_location",
        "Former config location",
        format!("ignored, never read: {}", names.join(", ")),
        hint,
    ))
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
    match apollia_tools::ToolCredentialStore::open(&db_path) {
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
    let db_path = data_dir.join(apollia_core::paths::DataFile::Agents.file_name());
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
///
/// Counts `.gguf` models through the same scan `apollia-os model list` uses,
/// rather than directory entries: a directory holding only a `.DS_Store` or a
/// half-downloaded `.part` file carries no model, and reporting it as populated
/// sends the operator looking for a different fault. A directory that cannot be
/// enumerated is reported as such, never as empty, because the two states call
/// for opposite next steps.
fn check_models_dir(data_dir: &Path) -> CheckResult {
    const MODELS_HINT: &str = "Place a .gguf file in ~/.apollia/models/, or run `apollia-os llm setup --local --model <path.gguf>`";

    let dir = apollia_core::config::models_dir(data_dir);
    if !dir.exists() {
        return CheckResult::warn(
            "models_dir",
            "Local models",
            "~/.apollia/models/ does not exist",
            MODELS_HINT,
        );
    }
    let models = match crate::commands::model::list_gguf_files(&dir) {
        Ok(m) => m,
        Err(e) => {
            return CheckResult::warn(
                "models_dir",
                "Local models",
                format!("{} cannot be read: {e}", dir.display()),
                "Check the ownership and permissions of the directory, and that it is a directory",
            );
        }
    };
    let count = models.len();
    if count == 0 {
        CheckResult::warn(
            "models_dir",
            "Local models",
            format!("no .gguf model in {}", dir.display()),
            MODELS_HINT,
        )
    } else {
        CheckResult::ok(
            "models_dir",
            "Local models",
            format!("{count} model(s) in {}", dir.display()),
        )
    }
}

/// Report whether scanned pages and pictures in sources will be read: the
/// engine compiled into this build, and how many of the languages the registry
/// pins are in `tessdata` with their pinned digest. A warning, never an error,
/// in both missing cases: a source still indexes its text without them.
fn check_recognition<E: std::fmt::Display>(
    compiled: bool,
    tessdata: &Path,
    catalogue: Result<apollia_llm::recommend::OcrCatalogue, E>,
) -> CheckResult {
    const ID: &str = "recognition";
    const LABEL: &str = "Recognition";
    let catalogue = match catalogue {
        Ok(c) => c,
        Err(e) => {
            return CheckResult::error(
                ID,
                LABEL,
                format!("the language data registry of this build does not load: {e}"),
                "Reinstall Apollia OS: the registry is embedded in the binary",
            );
        }
    };
    let verified = apollia_llm::ocr_data::verified_languages(tessdata, &catalogue).len();
    let pinned = catalogue.languages().len();
    let counts = format!("{verified}/{pinned} languages verified");
    let mut check = if !compiled {
        CheckResult::warn(
            ID,
            LABEL,
            format!("not compiled, {counts}: this build does not read scanned pages"),
            "Install a build that carries recognition, or build with `--features ocr`",
        )
    } else if verified == 0 {
        CheckResult::warn(
            ID,
            LABEL,
            format!("compiled, {counts} in {}", tessdata.display()),
            "The language data downloads with the first scanned page, or import it",
        )
    } else {
        CheckResult::ok(ID, LABEL, format!("compiled, {counts}"))
    };
    check.detail = Some(serde_json::json!({
        "compiled": compiled,
        "languages_verified": verified,
        "languages_pinned": pinned,
    }));
    check
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
    let socket_path = socket.unwrap_or_else(default_socket_path);
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
    note!();
    println!("  Apollia OS Doctor");
    note!("  -----------------");
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
    note!("  -----------------");
    if any_error {
        println!("  Overall: FAILED (one or more checks errored)");
    } else {
        println!("  Overall: HEALTHY");
    }
    note!();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_status_serialises_lowercase() {
        // GIVEN each status a check can report
        // WHEN it is serialised for the machine-readable output
        // THEN it is the lowercase name a script matches on
        let json = serde_json::to_string(&CheckStatus::Ok).unwrap();
        assert_eq!(json, "\"ok\"");
        let json = serde_json::to_string(&CheckStatus::Warn).unwrap();
        assert_eq!(json, "\"warn\"");
        let json = serde_json::to_string(&CheckStatus::Error).unwrap();
        assert_eq!(json, "\"error\"");
    }

    #[test]
    fn check_result_ok_has_no_hint() {
        // GIVEN a passing check
        // WHEN its result is built
        let r = CheckResult::ok("id", "label", "all good");
        // THEN it carries no hint, since there is nothing to fix
        assert_eq!(r.status, CheckStatus::Ok);
        assert!(r.hint.is_none());
    }

    #[test]
    fn check_result_warn_carries_hint() {
        // GIVEN a check that warns
        // WHEN its result is built
        let r = CheckResult::warn("id", "label", "missing", "create it");
        // THEN it carries the hint that tells the operator what to do
        assert_eq!(r.status, CheckStatus::Warn);
        assert_eq!(r.hint.as_deref(), Some("create it"));
    }

    #[tokio::test]
    async fn doctor_runs_without_runtime() {
        // GIVEN no runtime is running and a fresh temp HOME, held under the
        // shared lock and restored before the test returns.
        let _guard = crate::commands::HOME_ENV_LOCK.lock().await;
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
    fn a_config_at_a_former_location_is_named_with_the_move_command() {
        // GIVEN a configuration in the working directory and none at
        // ~/.apollia/apollia.toml
        let home = tempfile::tempdir().unwrap();
        let cwd = tempfile::tempdir().unwrap();
        std::fs::write(cwd.path().join("apollia.toml"), "[api]\nport = 7001\n").unwrap();
        // WHEN doctor checks the configuration locations
        let check = check_legacy_config(home.path(), Some(cwd.path()), None)
            .expect("a former location is reported");
        // THEN it warns, names the file, and gives the command that moves it
        // to the shared location
        assert_eq!(check.status, CheckStatus::Warn);
        assert!(check.message.contains(&cwd.path().display().to_string()));
        let canonical = apollia_core::paths::config_file_under(home.path());
        let hint = check.hint.unwrap();
        assert!(hint.contains("mv ") || hint.contains("move "), "{hint}");
        assert!(hint.contains(&canonical.display().to_string()), "{hint}");
    }

    #[test]
    fn no_former_location_reports_nothing() {
        // GIVEN an empty home and an empty working directory
        let home = tempfile::tempdir().unwrap();
        let cwd = tempfile::tempdir().unwrap();
        // WHEN doctor checks the configuration locations
        // THEN there is no line to show
        assert!(check_legacy_config(home.path(), Some(cwd.path()), None).is_none());
    }

    #[test]
    fn the_config_check_reads_the_shared_location() {
        // GIVEN an invalid file at ~/.apollia/apollia.toml
        let home = tempfile::tempdir().unwrap();
        let path = apollia_core::paths::config_file_under(home.path());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "this = is = invalid\n").unwrap();
        // WHEN doctor checks the configuration file
        let check = check_config_file(home.path());
        // THEN that file is the one parsed, and reported invalid
        assert_eq!(check.status, CheckStatus::Error, "{}", check.message);
        assert!(check.message.contains(&path.display().to_string()));
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

    #[test]
    fn models_dir_holding_no_gguf_is_not_reported_as_populated() {
        // GIVEN a models directory that holds files, none of them a .gguf model
        let tmp = tempfile::tempdir().unwrap();
        let models = tmp.path().join("models");
        std::fs::create_dir_all(&models).unwrap();
        std::fs::write(models.join(".DS_Store"), b"x").unwrap();
        std::fs::write(models.join("qwen3.gguf.part"), b"half a download").unwrap();
        // WHEN the local-model check runs
        let check = check_models_dir(tmp.path());
        // THEN it reports the absence of a model, not a populated directory
        assert_eq!(
            check.status,
            CheckStatus::Warn,
            "two non-model files must not read as a local model: {}",
            check.message
        );
    }

    #[test]
    fn models_dir_holding_a_gguf_is_reported_ok() {
        // GIVEN a models directory holding one .gguf model
        let tmp = tempfile::tempdir().unwrap();
        let models = tmp.path().join("models");
        std::fs::create_dir_all(&models).unwrap();
        std::fs::write(models.join("qwen3-0.6b-q8_0.gguf"), b"GGUF").unwrap();
        // WHEN the local-model check runs
        let check = check_models_dir(tmp.path());
        // THEN it passes and counts one model, so a constant Warn would fail here
        assert_eq!(check.status, CheckStatus::Ok, "{}", check.message);
        assert!(
            check.message.contains("1 model"),
            "the message must name what was counted: {}",
            check.message
        );
    }

    #[test]
    fn unreadable_models_dir_is_not_reported_as_empty() {
        // GIVEN a `models` entry that exists but cannot be enumerated
        // (a regular file where the directory is expected)
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("models"), b"not a directory").unwrap();
        // WHEN the local-model check runs
        let check = check_models_dir(tmp.path());
        // THEN it says the directory could not be read rather than that it is empty
        assert!(
            check.message.contains("cannot be read"),
            "an unreadable directory must not be reported as empty: {}",
            check.message
        );
    }

    /// A registry pinning two languages, `fra` and `eng`, to the digests of
    /// `fra` and `eng`.
    fn two_languages(fra: &[u8], eng: &[u8]) -> apollia_llm::recommend::OcrCatalogue {
        use sha2::{Digest, Sha256};
        let entry = |language: &str, bytes: &[u8]| {
            format!(
                "[[ocr]]\nlanguage = \"{language}\"\nlabel = \"{language}\"\npublisher = \"test\"\n\
                 released = \"2017-09-14\"\nlicense = \"apache-2.0\"\nrepo = \"owner/tessdata\"\n\
                 commit = \"0123456789abcdef0123456789abcdef01234567\"\n\
                 file = \"{language}.traineddata\"\nsize_bytes = {}\nsha256 = \"{:x}\"\n\
                 source = \"https://example.invalid\"\n",
                bytes.len(),
                Sha256::digest(bytes)
            )
        };
        apollia_llm::recommend::OcrCatalogue::from_toml(&format!(
            "{}\n{}",
            entry("fra", fra),
            entry("eng", eng)
        ))
        .expect("the test registry loads")
    }

    type NoError = std::convert::Infallible;

    #[test]
    fn the_recognition_check_is_named_recognition_and_carries_its_counts() {
        // GIVEN a build with the engine and an empty language folder
        let tessdata = tempfile::tempdir().unwrap();
        let catalogue = Ok::<_, NoError>(two_languages(b"fra data", b"eng data"));
        // WHEN the recognition check runs and is serialised for `--json`
        let check = check_recognition(true, tessdata.path(), catalogue);
        let json = serde_json::to_value(&check).unwrap();
        // THEN it has the stable id and the facts behind its message
        assert_eq!(json["id"], "recognition");
        assert_eq!(json["detail"]["compiled"], true);
        assert_eq!(json["detail"]["languages_verified"], 0);
        assert_eq!(json["detail"]["languages_pinned"], 2);
    }

    #[test]
    fn the_other_checks_keep_their_json_shape() {
        // GIVEN a check that has no detail
        let check = CheckResult::ok("agents_db", "Agents DB", "openable");
        // WHEN it is serialised for `--json`
        let json = serde_json::to_value(&check).unwrap();
        // THEN it holds the same five keys as before, and no detail
        let keys: Vec<&str> = json
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(keys.len(), 5, "{json}");
        assert!(json.get("detail").is_none(), "{json}");
    }

    #[test]
    fn recognition_without_language_data_warns_with_the_way_to_get_it() {
        // GIVEN a build with the engine and no language file
        let tessdata = tempfile::tempdir().unwrap();
        let catalogue = Ok::<_, NoError>(two_languages(b"fra data", b"eng data"));
        // WHEN the recognition check runs
        let check = check_recognition(true, tessdata.path(), catalogue);
        // THEN it warns, counts none of two, and says how the data arrives
        assert_eq!(check.status, CheckStatus::Warn, "{}", check.message);
        assert!(
            check
                .message
                .starts_with("compiled, 0/2 languages verified"),
            "{}",
            check.message
        );
        let hint = check.hint.unwrap();
        assert!(
            hint.contains("downloads with the first scanned page"),
            "{hint}"
        );
        assert!(hint.contains("import it"), "{hint}");
    }

    #[test]
    fn recognition_with_one_language_at_its_digest_is_ok() {
        // GIVEN a build with the engine and `fra.traineddata` holding the
        // bytes the registry pins
        let tessdata = tempfile::tempdir().unwrap();
        std::fs::write(tessdata.path().join("fra.traineddata"), b"fra data").unwrap();
        let catalogue = Ok::<_, NoError>(two_languages(b"fra data", b"eng data"));
        // WHEN the recognition check runs
        let check = check_recognition(true, tessdata.path(), catalogue);
        // THEN it passes and counts one of two
        assert_eq!(check.status, CheckStatus::Ok, "{}", check.message);
        assert_eq!(check.message, "compiled, 1/2 languages verified");
        assert_eq!(check.detail.unwrap()["languages_verified"], 1);
    }

    #[test]
    fn recognition_with_an_altered_language_file_does_not_count_it() {
        // GIVEN a build with the engine and a `fra.traineddata` whose bytes
        // are not the pinned ones
        let tessdata = tempfile::tempdir().unwrap();
        std::fs::write(
            tessdata.path().join("fra.traineddata"),
            b"fra data, altered",
        )
        .unwrap();
        let catalogue = Ok::<_, NoError>(two_languages(b"fra data", b"eng data"));
        // WHEN the recognition check runs
        let check = check_recognition(true, tessdata.path(), catalogue);
        // THEN the file is not counted and the check warns
        assert_eq!(check.status, CheckStatus::Warn, "{}", check.message);
        assert_eq!(check.detail.unwrap()["languages_verified"], 0);
    }

    #[test]
    fn recognition_not_compiled_warns_even_with_language_data() {
        // GIVEN a build without the engine and a verified language file
        let tessdata = tempfile::tempdir().unwrap();
        std::fs::write(tessdata.path().join("eng.traineddata"), b"eng data").unwrap();
        let catalogue = Ok::<_, NoError>(two_languages(b"fra data", b"eng data"));
        // WHEN the recognition check runs
        let check = check_recognition(false, tessdata.path(), catalogue);
        // THEN it warns that scanned pages are not read, and still counts the file
        assert_eq!(check.status, CheckStatus::Warn, "{}", check.message);
        assert!(
            check
                .message
                .contains("this build does not read scanned pages"),
            "{}",
            check.message
        );
        let detail = check.detail.unwrap();
        assert_eq!(detail["compiled"], false);
        assert_eq!(detail["languages_verified"], 1);
    }

    #[test]
    fn recognition_reports_the_engine_of_this_build_over_the_shipped_registry() {
        // GIVEN this build, the registry it embeds, and an empty language folder
        let tessdata = tempfile::tempdir().unwrap();
        let compiled = apollia_sources::Ocr::compiled();
        // WHEN the recognition check runs as `doctor` runs it
        let check = check_recognition(
            compiled,
            tessdata.path(),
            apollia_llm::recommend::OcrCatalogue::embedded(),
        );
        // THEN it says whether the engine is in this build (the `ocr` feature),
        // over the two languages the registry pins, and warns for the folder
        let detail = check.detail.unwrap();
        assert_eq!(detail["compiled"], cfg!(feature = "ocr"));
        assert_eq!(detail["languages_pinned"], 2);
        assert_eq!(check.status, CheckStatus::Warn);
    }

    #[test]
    fn recognition_with_a_registry_that_does_not_load_is_an_error() {
        // GIVEN a registry that fails to load
        let tessdata = tempfile::tempdir().unwrap();
        // WHEN the recognition check runs
        let check = check_recognition(true, tessdata.path(), Err("broken table"));
        // THEN it fails with the reason, since the binary itself is at fault
        assert_eq!(check.status, CheckStatus::Error);
        assert!(check.message.contains("broken table"), "{}", check.message);
    }
}
