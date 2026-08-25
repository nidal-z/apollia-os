//! `apollia-os config` subcommands: manage the global `apollia.toml`.
//!
//! These commands operate directly on the on-disk configuration without
//! touching the runtime. They are orthogonal to the section-specific helpers
//! `tools config` and `stt config`, which target their own respective slices.

use std::io::IsTerminal as _;
use std::path::{Path, PathBuf};

use clap::Subcommand;
use toml_edit::{value, DocumentMut, Item};

use crate::exit_codes;

/// Subcommands of `apollia-os config`.
#[derive(Debug, Subcommand)]
pub enum ConfigCommand {
    /// Print the on-disk apollia.toml (or a single value at `KEY_PATH`).
    ///
    /// `KEY_PATH` follows dotted notation, e.g. `llm.default` or
    /// `runtime.bind_addr`.
    Get {
        /// Optional dotted key path.
        key: Option<String>,
        /// Optional config file path override.
        #[arg(long, value_name = "PATH")]
        file: Option<PathBuf>,
    },

    /// Write `VALUE` at `KEY_PATH`, preserving formatting and comments.
    ///
    /// `VALUE` is parsed as a TOML scalar: bare booleans, integers, and floats
    /// are recognized; everything else is treated as a string. Use a quoted
    /// argument when the value contains spaces or special characters.
    Set {
        /// Dotted key path.
        key: String,
        /// Value to write.
        value: String,
        /// Optional config file path override.
        #[arg(long, value_name = "PATH")]
        file: Option<PathBuf>,
    },

    /// Parse the config file and report any error.
    ///
    /// Exits 0 when the file is absent or valid, 1 otherwise.
    Validate {
        /// Optional config file path override.
        #[arg(long, value_name = "PATH")]
        file: Option<PathBuf>,
    },

    /// Open the config file in `$EDITOR`.
    ///
    /// Refuses to run when stdout is not a TTY or `--json` is set.
    Edit {
        /// Optional config file path override.
        #[arg(long, value_name = "PATH")]
        file: Option<PathBuf>,
    },

    /// Print the resolved configuration (parsed struct as JSON).
    ///
    /// Cascades through file > defaults. Environment overlay is performed at
    /// runtime startup and is not reproduced here.
    Show {
        /// Optional config file path override.
        #[arg(long, value_name = "PATH")]
        file: Option<PathBuf>,
    },

    /// Wipe `~/.apollia/`, an irreversible factory reset.
    ///
    /// Deletes every SQLite database, log, journal, memory file, OAuth client
    /// override, and apollia.toml stored under `~/.apollia/`. Keychain entries
    /// (OS-managed) are NOT touched: use `connector revoke` or
    /// `mcp oauth logout` to clear those.
    Reset {
        /// Skip the interactive confirmation prompt (required for scripts).
        #[arg(long)]
        confirm: bool,
        /// Print the resolved Apollia home and the entries that would be
        /// removed, but do not delete anything.
        #[arg(long)]
        dry_run: bool,
        /// Override the Apollia home path (default: `~/.apollia`).
        #[arg(long, value_name = "PATH")]
        home: Option<PathBuf>,
    },
}

/// Entry point for `apollia-os config <verb>`.
pub fn run(cmd: &ConfigCommand, json: bool) -> i32 {
    match cmd {
        ConfigCommand::Get { key, file } => run_get(key.as_deref(), file.as_deref(), json),
        ConfigCommand::Set { key, value, file } => run_set(key, value, file.as_deref(), json),
        ConfigCommand::Validate { file } => run_validate(file.as_deref(), json),
        ConfigCommand::Edit { file } => run_edit(file.as_deref(), json),
        ConfigCommand::Show { file } => run_show(file.as_deref(), json),
        ConfigCommand::Reset {
            confirm,
            dry_run,
            home,
        } => run_reset(*confirm, *dry_run, home.as_deref(), json),
    }
}

/// Resolve the apollia.toml location: explicit override wins, then `./apollia.toml`,
/// then `$XDG_CONFIG_HOME/apollia/apollia.toml` (or `~/.config/apollia/apollia.toml`).
fn resolve_path(override_path: Option<&Path>) -> PathBuf {
    if let Some(p) = override_path {
        return p.to_path_buf();
    }
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let local = cwd.join("apollia.toml");
    if local.exists() {
        return local;
    }
    let xdg = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from);
    let base = xdg
        .or_else(|| dirs::home_dir().map(|h| h.join(".config")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("apollia").join("apollia.toml")
}

fn emit_error(msg: impl Into<String>, json: bool) {
    let _ = crate::output::emit_error(json, exit_codes::GENERAL_ERROR, &msg.into());
}

// ─── get ─────────────────────────────────────────────────────────────────────

fn run_get(key: Option<&str>, file: Option<&Path>, json: bool) -> i32 {
    let path = resolve_path(file);
    let content = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            if json {
                let out = serde_json::json!({
                    "path": path.display().to_string(),
                    "exists": false,
                });
                println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
            } else {
                eprintln!(
                    "No config file at {} - runtime will start with defaults.",
                    path.display()
                );
            }
            return exit_codes::SUCCESS;
        }
        Err(e) => {
            emit_error(format!("read {} failed: {e}", path.display()), json);
            return exit_codes::GENERAL_ERROR;
        }
    };

    let Some(key) = key else {
        if json {
            // Reparse as a TOML value to render JSON cleanly.
            match toml::from_str::<toml::Value>(&content) {
                Ok(v) => {
                    let json_v = toml_value_to_json(&v);
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&json_v).unwrap_or_default()
                    );
                }
                Err(e) => {
                    emit_error(format!("invalid TOML: {e}"), json);
                    return exit_codes::GENERAL_ERROR;
                }
            }
        } else {
            print!("{content}");
        }
        return exit_codes::SUCCESS;
    };

    let value = match toml::from_str::<toml::Value>(&content) {
        Ok(v) => v,
        Err(e) => {
            emit_error(format!("invalid TOML: {e}"), json);
            return exit_codes::GENERAL_ERROR;
        }
    };

    match navigate(&value, key) {
        Some(v) => {
            if json {
                let json_v = toml_value_to_json(v);
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json_v).unwrap_or_default()
                );
            } else {
                println!("{}", render_scalar(v));
            }
            exit_codes::SUCCESS
        }
        None => {
            emit_error(format!("key '{key}' not found"), json);
            exit_codes::GENERAL_ERROR
        }
    }
}

/// Navigate a `toml::Value` along a dotted key path.
fn navigate<'a>(root: &'a toml::Value, path: &str) -> Option<&'a toml::Value> {
    let mut cur = root;
    for segment in path.split('.') {
        match cur {
            toml::Value::Table(t) => {
                cur = t.get(segment)?;
            }
            _ => return None,
        }
    }
    Some(cur)
}

/// Render a single TOML scalar for human output (tables print as TOML).
fn render_scalar(v: &toml::Value) -> String {
    match v {
        toml::Value::String(s) => s.clone(),
        toml::Value::Integer(i) => i.to_string(),
        toml::Value::Float(f) => f.to_string(),
        toml::Value::Boolean(b) => b.to_string(),
        toml::Value::Datetime(d) => d.to_string(),
        toml::Value::Array(_) | toml::Value::Table(_) => toml::to_string(v).unwrap_or_default(),
    }
}

/// Convert a `toml::Value` tree into the equivalent `serde_json::Value`.
fn toml_value_to_json(v: &toml::Value) -> serde_json::Value {
    match v {
        toml::Value::String(s) => serde_json::Value::String(s.clone()),
        toml::Value::Integer(i) => serde_json::Value::Number(serde_json::Number::from(*i)),
        toml::Value::Float(f) => serde_json::Number::from_f64(*f)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        toml::Value::Boolean(b) => serde_json::Value::Bool(*b),
        toml::Value::Datetime(d) => serde_json::Value::String(d.to_string()),
        toml::Value::Array(a) => {
            serde_json::Value::Array(a.iter().map(toml_value_to_json).collect())
        }
        toml::Value::Table(t) => {
            let mut m = serde_json::Map::new();
            for (k, v) in t {
                m.insert(k.clone(), toml_value_to_json(v));
            }
            serde_json::Value::Object(m)
        }
    }
}

// ─── set ─────────────────────────────────────────────────────────────────────

fn run_set(key: &str, value_str: &str, file: Option<&Path>, json: bool) -> i32 {
    let path = resolve_path(file);
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                emit_error(format!("create {} failed: {e}", parent.display()), json);
                return exit_codes::GENERAL_ERROR;
            }
        }
    }
    let content = std::fs::read_to_string(&path).unwrap_or_default();
    let mut doc: DocumentMut = match content.parse() {
        Ok(d) => d,
        Err(e) => {
            emit_error(format!("invalid TOML in {}: {e}", path.display()), json);
            return exit_codes::GENERAL_ERROR;
        }
    };

    let new_item = parse_scalar(value_str);
    if let Err(e) = set_path(&mut doc, key, new_item) {
        emit_error(e, json);
        return exit_codes::GENERAL_ERROR;
    }

    // Validate the candidate document BEFORE persisting, so a typo'd key path or
    // an invalid value is rejected (nonzero, no write) instead of silently
    // bricking the next boot. A single `config set` legitimately leaves a section
    // incomplete (e.g. `llm.default` before `llm.backends`); enforcing section
    // completeness is `config validate`'s job, so "missing field" errors are
    // tolerated here while invalid values / enum variants are rejected.
    let top_level = key.split('.').next().unwrap_or(key);
    if !crate::config::is_known_top_level_section(top_level) {
        emit_error(format!("unknown config key: '{key}'"), json);
        return exit_codes::GENERAL_ERROR;
    }
    if let Err(e) = crate::config::validate_apollia_toml_str(&doc.to_string()) {
        let msg = e.to_string();
        if !msg.contains("missing field") {
            emit_error(format!("invalid config value for '{key}': {e}"), json);
            return exit_codes::GENERAL_ERROR;
        }
    }

    if let Err(e) = std::fs::write(&path, doc.to_string()) {
        emit_error(format!("write {} failed: {e}", path.display()), json);
        return exit_codes::GENERAL_ERROR;
    }

    if json {
        let out = serde_json::json!({
            "path": path.display().to_string(),
            "key": key,
            "value": value_str,
            "written": true,
        });
        println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
    } else {
        println!(
            "  * config.{} = {} (written to {})",
            key,
            value_str,
            path.display()
        );
    }
    exit_codes::SUCCESS
}

/// Parse a CLI scalar string into the right `toml_edit::Item`.
fn parse_scalar(raw: &str) -> Item {
    if raw.eq_ignore_ascii_case("true") {
        return value(true);
    }
    if raw.eq_ignore_ascii_case("false") {
        return value(false);
    }
    if let Ok(n) = raw.parse::<i64>() {
        return value(n);
    }
    if let Ok(f) = raw.parse::<f64>() {
        return value(f);
    }
    value(raw)
}

/// Navigate `doc` along the dotted `key`, creating intermediate tables as needed,
/// and replace the leaf value with `item`.
fn set_path(doc: &mut DocumentMut, key: &str, item: Item) -> Result<(), String> {
    let segments: Vec<&str> = key.split('.').collect();
    if segments.is_empty() || segments.iter().any(|s| s.is_empty()) {
        return Err("invalid key path".to_string());
    }
    let mut node = doc.as_item_mut();
    for (i, segment) in segments.iter().enumerate() {
        let is_last = i + 1 == segments.len();
        if is_last {
            // Insert / replace the leaf.
            let table = node
                .as_table_like_mut()
                .ok_or_else(|| format!("cannot descend into non-table at '{segment}'"))?;
            table.insert(segment, item);
            return Ok(());
        }
        // Descend, creating empty inline tables on the way if absent.
        let table = node
            .as_table_like_mut()
            .ok_or_else(|| format!("cannot descend into non-table at '{segment}'"))?;
        if table.get(segment).is_none() {
            let mut new_table = toml_edit::Table::new();
            new_table.set_implicit(true);
            table.insert(segment, Item::Table(new_table));
        }
        node = table
            .get_mut(segment)
            .ok_or_else(|| format!("intermediate '{segment}' missing after insert"))?;
    }
    Ok(())
}

// ─── validate ────────────────────────────────────────────────────────────────

fn run_validate(file: Option<&Path>, json: bool) -> i32 {
    let path = resolve_path(file);
    if !path.exists() {
        if json {
            let out = serde_json::json!({
                "path": path.display().to_string(),
                "exists": false,
                "valid": true,
            });
            println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
        } else {
            println!("  * No config file at {} (using defaults)", path.display());
        }
        return exit_codes::SUCCESS;
    }
    match crate::config::parse_apollia_toml(&path) {
        Ok(_) => {
            if json {
                let out = serde_json::json!({
                    "path": path.display().to_string(),
                    "exists": true,
                    "valid": true,
                });
                println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
            } else {
                println!("  * {} valid", path.display());
            }
            exit_codes::SUCCESS
        }
        Err(e) => {
            if json {
                let out = serde_json::json!({
                    "path": path.display().to_string(),
                    "exists": true,
                    "valid": false,
                    "error": e.to_string(),
                });
                println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
            } else {
                eprintln!("x {} invalid: {e}", path.display());
            }
            exit_codes::GENERAL_ERROR
        }
    }
}

// ─── edit ────────────────────────────────────────────────────────────────────

fn run_edit(file: Option<&Path>, json: bool) -> i32 {
    if json {
        emit_error("edit cannot run in --json mode", true);
        return exit_codes::GENERAL_ERROR;
    }
    if !std::io::stdout().is_terminal() {
        emit_error("edit requires a TTY", false);
        return exit_codes::GENERAL_ERROR;
    }
    let path = resolve_path(file);
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                emit_error(format!("create {} failed: {e}", parent.display()), false);
                return exit_codes::GENERAL_ERROR;
            }
        }
    }
    if !path.exists() {
        // Create an empty file so the editor opens cleanly.
        if let Err(e) = std::fs::write(&path, "") {
            emit_error(format!("create {} failed: {e}", path.display()), false);
            return exit_codes::GENERAL_ERROR;
        }
    }
    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
    let status = std::process::Command::new(&editor).arg(&path).status();
    match status {
        Ok(s) if s.success() => exit_codes::SUCCESS,
        Ok(s) => {
            eprintln!("editor exited with status {s}");
            exit_codes::GENERAL_ERROR
        }
        Err(e) => {
            emit_error(format!("failed to spawn editor '{editor}': {e}"), false);
            exit_codes::GENERAL_ERROR
        }
    }
}

// ─── show (resolved) ─────────────────────────────────────────────────────────

fn run_show(file: Option<&Path>, json: bool) -> i32 {
    let path = resolve_path(file);
    if !path.exists() {
        if json {
            let out = serde_json::json!({
                "path": path.display().to_string(),
                "exists": false,
                "resolved": {},
            });
            println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
        } else {
            println!(
                "No config file at {} - runtime will start with defaults.",
                path.display()
            );
        }
        return exit_codes::SUCCESS;
    }
    // We re-render the parsed file as JSON so missing sections naturally show
    // up as absent rather than as `null` defaults: the runtime computes its
    // effective view itself at boot, and reproducing that here would require
    // duplicating the cascade logic.
    let content = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            emit_error(format!("read {} failed: {e}", path.display()), json);
            return exit_codes::GENERAL_ERROR;
        }
    };
    let value: toml::Value = match toml::from_str(&content) {
        Ok(v) => v,
        Err(e) => {
            emit_error(format!("invalid TOML: {e}"), json);
            return exit_codes::GENERAL_ERROR;
        }
    };
    let json_value = toml_value_to_json(&value);
    if json {
        let envelope = serde_json::json!({
            "path": path.display().to_string(),
            "exists": true,
            "resolved": json_value,
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&envelope).unwrap_or_default()
        );
    } else {
        println!(
            "{}",
            serde_json::to_string_pretty(&json_value).unwrap_or_default()
        );
    }
    exit_codes::SUCCESS
}

// ─── reset ───────────────────────────────────────────────────────────────────

fn resolve_apollia_home(override_path: Option<&Path>) -> Option<PathBuf> {
    if let Some(p) = override_path {
        return Some(p.to_path_buf());
    }
    dirs::home_dir().map(apollia_core::paths::data_dir_under)
}

/// Wipe the Apollia home directory (factory reset).
///
/// The implementation iterates the immediate children of `~/.apollia/` rather
/// than calling `remove_dir_all` on the home itself: that keeps the parent
/// directory intact (mounted volumes, ACLs) and matches the Desktop
/// `factory_reset` semantics that the user expects.
fn run_reset(confirm: bool, dry_run: bool, home: Option<&Path>, json: bool) -> i32 {
    let Some(home) = resolve_apollia_home(home) else {
        emit_error("cannot determine $HOME directory", json);
        return exit_codes::GENERAL_ERROR;
    };

    if !home.exists() {
        emit_reset_absent(&home, json);
        return exit_codes::SUCCESS;
    }

    let entries: Vec<PathBuf> = match std::fs::read_dir(&home) {
        Ok(it) => it.filter_map(|r| r.ok().map(|e| e.path())).collect(),
        Err(e) => {
            emit_error(format!("cannot read {}: {e}", home.display()), json);
            return exit_codes::GENERAL_ERROR;
        }
    };

    if dry_run {
        emit_reset_dry_run(&home, &entries, json);
        return exit_codes::SUCCESS;
    }

    if !confirm {
        emit_error(
            format!(
                "factory reset is irreversible. Re-run with --confirm to wipe {} ({} entries).",
                home.display(),
                entries.len()
            ),
            json,
        );
        return exit_codes::GENERAL_ERROR;
    }

    let (removed, failures) = wipe_entries(&entries);
    emit_reset_outcome(&home, &removed, &failures, json);
    if failures.is_empty() {
        exit_codes::SUCCESS
    } else {
        exit_codes::GENERAL_ERROR
    }
}

fn emit_reset_absent(home: &Path, json: bool) {
    if json {
        let body = serde_json::json!({
            "home": home.display().to_string(),
            "reset": false,
            "reason": "home directory absent - nothing to reset",
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&body).unwrap_or_default()
        );
    } else {
        println!(
            "  Apollia home {} does not exist - nothing to reset.",
            home.display()
        );
    }
}

fn emit_reset_dry_run(home: &Path, entries: &[PathBuf], json: bool) {
    if json {
        let body = serde_json::json!({
            "home": home.display().to_string(),
            "dry_run": true,
            "entries": entries.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&body).unwrap_or_default()
        );
    } else {
        println!(
            "  Dry run - {} entries under {} would be removed:",
            entries.len(),
            home.display()
        );
        for p in entries {
            let kind = if p.is_dir() { "dir " } else { "file" };
            println!("    [{kind}] {}", p.display());
        }
        println!(
            "  Run without --dry-run (with --confirm) to actually wipe. Keychain entries are NOT touched."
        );
    }
}

fn wipe_entries(entries: &[PathBuf]) -> (Vec<PathBuf>, Vec<(PathBuf, String)>) {
    let mut removed: Vec<PathBuf> = Vec::new();
    let mut failures: Vec<(PathBuf, String)> = Vec::new();
    for entry in entries {
        let res = if entry.is_dir() {
            std::fs::remove_dir_all(entry)
        } else {
            std::fs::remove_file(entry)
        };
        match res {
            Ok(()) => removed.push(entry.clone()),
            Err(e) => failures.push((entry.clone(), e.to_string())),
        }
    }
    (removed, failures)
}

fn emit_reset_outcome(
    home: &Path,
    removed: &[PathBuf],
    failures: &[(PathBuf, String)],
    json: bool,
) {
    if json {
        let body = serde_json::json!({
            "home": home.display().to_string(),
            "removed": removed.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
            "failures": failures.iter().map(|(p, e)| serde_json::json!({
                "path": p.display().to_string(),
                "error": e,
            })).collect::<Vec<_>>(),
            "reset": failures.is_empty(),
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&body).unwrap_or_default()
        );
    } else {
        println!(
            "  * {} entries removed under {}",
            removed.len(),
            home.display()
        );
        for (p, e) in failures {
            eprintln!("  ! failed to remove {}: {e}", p.display());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser, Debug)]
    struct TestCli {
        #[command(subcommand)]
        cmd: ConfigCommand,
    }

    #[test]
    fn parses_get_without_key() {
        let cli = TestCli::parse_from(["x", "get"]);
        match cli.cmd {
            ConfigCommand::Get { key, file } => {
                assert!(key.is_none());
                assert!(file.is_none());
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parses_get_with_key() {
        let cli = TestCli::parse_from(["x", "get", "llm.default"]);
        match cli.cmd {
            ConfigCommand::Get { key, .. } => assert_eq!(key.as_deref(), Some("llm.default")),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parses_set() {
        let cli = TestCli::parse_from(["x", "set", "llm.default", "anthropic"]);
        match cli.cmd {
            ConfigCommand::Set { key, value, .. } => {
                assert_eq!(key, "llm.default");
                assert_eq!(value, "anthropic");
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parse_scalar_recognises_true_false() {
        assert_eq!(parse_scalar("true").as_bool(), Some(true));
        assert_eq!(parse_scalar("FALSE").as_bool(), Some(false));
    }

    #[test]
    fn parse_scalar_recognises_integers() {
        assert_eq!(parse_scalar("42").as_integer(), Some(42));
    }

    #[test]
    fn parse_scalar_recognises_floats() {
        assert_eq!(parse_scalar("2.5").as_float(), Some(2.5));
    }

    #[test]
    fn parse_scalar_falls_back_to_string() {
        assert_eq!(parse_scalar("hello").as_str(), Some("hello"));
    }

    #[test]
    fn navigate_traverses_nested_tables() {
        let v: toml::Value = toml::from_str(
            r#"
            [llm]
            default = "anthropic"

            [tools.web_search]
            backend = "brave"
            "#,
        )
        .unwrap();
        let got = navigate(&v, "llm.default").unwrap();
        assert_eq!(got.as_str(), Some("anthropic"));
        let got = navigate(&v, "tools.web_search.backend").unwrap();
        assert_eq!(got.as_str(), Some("brave"));
        assert!(navigate(&v, "missing.key").is_none());
    }

    #[test]
    fn set_path_creates_intermediate_tables() {
        let mut doc: DocumentMut = "".parse().unwrap();
        set_path(&mut doc, "tools.web_search.timeout_secs", value(10)).unwrap();
        let rendered = doc.to_string();
        assert!(rendered.contains("[tools.web_search]"));
        assert!(rendered.contains("timeout_secs = 10"));
    }

    #[test]
    fn set_path_replaces_existing_value() {
        let mut doc: DocumentMut = "[llm]\ndefault = \"openai\"\n".parse().unwrap();
        set_path(&mut doc, "llm.default", value("anthropic")).unwrap();
        let rendered = doc.to_string();
        assert!(rendered.contains("default = \"anthropic\""));
        assert!(!rendered.contains("openai"));
    }

    #[test]
    fn run_set_then_run_get_round_trip() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let path = tmp.path().to_path_buf();
        std::fs::write(&path, "").unwrap();
        let code = run_set("llm.default", "anthropic", Some(&path), true);
        assert_eq!(code, exit_codes::SUCCESS);
        let code = run_get(Some("llm.default"), Some(&path), true);
        assert_eq!(code, exit_codes::SUCCESS);
    }

    #[test]
    fn run_set_rejects_unknown_key_path() {
        // GIVEN an empty config file
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let path = tmp.path().to_path_buf();
        std::fs::write(&path, "").unwrap();
        // WHEN setting an unknown top-level key path
        let code = run_set("not.a.real.key", "somevalue", Some(&path), true);
        // THEN it fails and leaves the file unchanged
        assert_eq!(code, exit_codes::GENERAL_ERROR);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "");
    }

    #[test]
    fn run_set_rejects_invalid_enum_value() {
        // GIVEN an empty config file
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let path = tmp.path().to_path_buf();
        std::fs::write(&path, "").unwrap();
        // WHEN setting a known key to an invalid enum value
        let code = run_set("mcp.tool_loading", "BOGUS_VALUE", Some(&path), true);
        // THEN it fails and leaves the file unchanged
        assert_eq!(code, exit_codes::GENERAL_ERROR);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "");
    }

    #[test]
    fn run_set_accepts_valid_enum_value() {
        // GIVEN an empty config file
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let path = tmp.path().to_path_buf();
        std::fs::write(&path, "").unwrap();
        // WHEN setting a known key to a valid enum value
        let code = run_set("mcp.tool_loading", "eager", Some(&path), true);
        // THEN it succeeds and persists
        assert_eq!(code, exit_codes::SUCCESS);
        assert!(std::fs::read_to_string(&path).unwrap().contains("eager"));
    }

    #[test]
    fn run_validate_accepts_missing_file_as_ok() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("does_not_exist.toml");
        assert_eq!(run_validate(Some(&p), true), exit_codes::SUCCESS);
    }

    #[test]
    fn run_validate_rejects_invalid_toml() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), "this = is = invalid\n").unwrap();
        assert_eq!(
            run_validate(Some(tmp.path()), true),
            exit_codes::GENERAL_ERROR
        );
    }

    #[test]
    fn parses_reset_requires_confirm_flag() {
        let cli = TestCli::parse_from(["x", "reset"]);
        match cli.cmd {
            ConfigCommand::Reset {
                confirm,
                dry_run,
                home,
            } => {
                assert!(!confirm);
                assert!(!dry_run);
                assert!(home.is_none());
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parses_reset_with_dry_run() {
        let cli = TestCli::parse_from(["x", "reset", "--dry-run", "--home", "/tmp/.apollia"]);
        match cli.cmd {
            ConfigCommand::Reset {
                confirm,
                dry_run,
                home,
            } => {
                assert!(!confirm);
                assert!(dry_run);
                assert_eq!(home, Some(PathBuf::from("/tmp/.apollia")));
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn reset_without_confirm_errors_when_home_exists() {
        let tmp = tempfile::tempdir().unwrap();
        // Drop a file in so the home is non-empty.
        std::fs::write(tmp.path().join("dummy"), b"x").unwrap();
        let code = run_reset(false, false, Some(tmp.path()), true);
        assert_eq!(code, exit_codes::GENERAL_ERROR);
        // Confirm the file is still there.
        assert!(tmp.path().join("dummy").exists());
    }

    #[test]
    fn reset_dry_run_does_not_delete() {
        let tmp = tempfile::tempdir().unwrap();
        let f = tmp.path().join("dummy");
        std::fs::write(&f, b"x").unwrap();
        let code = run_reset(false, true, Some(tmp.path()), true);
        assert_eq!(code, exit_codes::SUCCESS);
        assert!(f.exists());
    }

    #[test]
    fn reset_with_confirm_wipes_entries() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("a"), b"x").unwrap();
        std::fs::create_dir(tmp.path().join("nested")).unwrap();
        std::fs::write(tmp.path().join("nested").join("b"), b"y").unwrap();
        let code = run_reset(true, false, Some(tmp.path()), true);
        assert_eq!(code, exit_codes::SUCCESS);
        // The home directory itself stays; only its contents are removed.
        assert!(tmp.path().exists());
        assert!(!tmp.path().join("a").exists());
        assert!(!tmp.path().join("nested").exists());
    }

    #[test]
    fn reset_on_missing_home_returns_success() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("does_not_exist");
        let code = run_reset(true, false, Some(&p), true);
        assert_eq!(code, exit_codes::SUCCESS);
    }
}
