//! `apollia-os permissions`: local management of permission rules.
//!
//! This subcommand operates directly on `governance.db` (the
//! `permission_rules` and `permission_audit` tables) without going through the
//! runtime. It exposes three operations to the operator:
//!
//! - `list`:   show the persisted rules (project + global) with their scope.
//! - `revoke`: delete a persisted rule by identifier, or in bulk by scope.
//! - `audit`:  inspect the immutable history of permission decisions.
//!
//! `session` rules live only in memory inside the running chat manager; they
//! are neither listable nor revocable from this subcommand, which runs outside
//! the daemon process. When an identifier prefixed with `s` is passed
//! to `revoke`, a clear message reports this limit and redirects the operator
//! to the desktop app.

use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};

use apollia_permissions::{
    PermissionAuditLog, PermissionScope, PrefixRule, PrefixRuleEngine, RuleAction,
};
use apollia_tools::GovernanceDb;
use clap::Subcommand;

use crate::exit_codes;

/// Subcommands of `apollia-os permissions`.
#[derive(Debug, Subcommand)]
pub enum PermissionsCommand {
    /// List persisted rules (project + global).
    List {
        /// Filter by scope: `global`, `project` or `session`.
        #[arg(long)]
        scope: Option<String>,
        /// Filter by tool name.
        #[arg(long)]
        tool: Option<String>,
    },
    /// Revoke a rule by identifier, or every matching rule with `--all`.
    Revoke {
        /// Numeric identifier of a persisted rule.
        ///
        /// IDs prefixed with `s` denote session-scoped rules; they are not
        /// revocable from the CLI (use the desktop app or restart the daemon).
        id: Option<String>,
        /// Revoke every rule matching `--scope`.
        #[arg(long)]
        all: bool,
        /// Scope targeted by `--all`: `global` (default) or `project`.
        #[arg(long)]
        scope: Option<String>,
        /// Skip the interactive confirmation (useful for scripts).
        #[arg(long)]
        yes: bool,
    },
    /// Show the permission decision history.
    Audit {
        /// Filter by tool name.
        #[arg(long)]
        tool: Option<String>,
        /// Maximum number of entries to display.
        #[arg(long, default_value = "50")]
        limit: u32,
    },
    /// Add a persisted permission rule for a tool + optional argument prefix.
    ///
    /// Persists into `governance.db` directly (no runtime required). Session
    /// scope is not supported here because session rules live in the runtime
    /// memory; use the chat REPL approval flow instead.
    Add {
        /// Tool name (e.g. `file_write`, `bash_executor`).
        #[arg(long, value_name = "NAME")]
        tool: String,
        /// Optional argument prefix the rule applies to (e.g. a path). When
        /// omitted, the rule pre-authorizes any invocation of `--tool`, except
        /// for code executors (`bash_executor`, `python_executor`), which are
        /// never blanket-authorized. With a prefix, the rule is evaluated on
        /// every invocation against the call's argument; for a code executor
        /// it only ever covers a single simple command (no chaining, pipe,
        /// redirection or substitution).
        #[arg(long, value_name = "PREFIX")]
        prefix: Option<String>,
        /// Rule action: `allow` or `deny`.
        #[arg(long, value_parser = ["allow", "deny"], default_value = "allow")]
        action: String,
        /// Rule scope: `project` or `global` (session is not persistable).
        #[arg(long, value_parser = ["project", "global"], default_value = "global")]
        scope: String,
        /// Project canonical path (required when `--scope project`).
        #[arg(long, value_name = "PATH")]
        project_path: Option<std::path::PathBuf>,
    },
}

/// Execute a `permissions` subcommand.
///
/// The `socket` parameter is accepted for consistency with the other
/// subcommands but is not used: everything goes through direct access to
/// `governance.db`.
pub async fn run(cmd: &PermissionsCommand, _socket: Option<PathBuf>, json: bool) -> i32 {
    match cmd {
        PermissionsCommand::List { scope, tool } => {
            run_list(scope.as_deref(), tool.as_deref(), json)
        }
        PermissionsCommand::Revoke {
            id,
            all,
            scope,
            yes,
        } => run_revoke(id.as_deref(), *all, scope.as_deref(), *yes, json),
        PermissionsCommand::Audit { tool, limit } => run_audit(tool.as_deref(), *limit, json),
        PermissionsCommand::Add {
            tool,
            prefix,
            action,
            scope,
            project_path,
        } => run_add(
            AddRuleInput {
                tool,
                prefix: prefix.as_deref(),
                action,
                scope,
                project_path: project_path.as_deref(),
            },
            json,
        ),
    }
}

// ─── List ────────────────────────────────────────────────────────────

fn run_list(scope_filter: Option<&str>, tool_filter: Option<&str>, json: bool) -> i32 {
    let parsed_scope = match scope_filter.map(parse_scope_filter).transpose() {
        Ok(s) => s,
        Err(msg) => return emit_error(msg, json),
    };

    let db_path = match ensure_governance_db_path() {
        Ok(p) => p,
        Err(msg) => return emit_error(msg, json),
    };
    let engine = match PrefixRuleEngine::new(&db_path) {
        Ok(e) => e,
        Err(e) => return emit_error(format!("failed to open governance.db: {e}"), json),
    };

    let rules = match load_rules(&engine, parsed_scope) {
        Ok(r) => r,
        Err(msg) => return emit_error(msg, json),
    };
    let filtered: Vec<PrefixRule> = match tool_filter {
        Some(t) => rules.into_iter().filter(|r| r.tool_name == t).collect(),
        None => rules,
    };

    if json {
        let arr: Vec<serde_json::Value> = filtered.iter().map(rule_to_json).collect();
        let payload = serde_json::json!({
            "rules": arr,
            "session_rules_visible": false,
            "note": "session rules live in the runtime memory and are not visible from the CLI",
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string())
        );
    } else {
        print_rules_table(&filtered);
        println!("  (session-scoped rules live in the runtime memory - not listable from the CLI)");
    }
    exit_codes::SUCCESS
}

fn load_rules(
    engine: &PrefixRuleEngine,
    scope: Option<PermissionScope>,
) -> Result<Vec<PrefixRule>, String> {
    engine
        .list_rules_filtered(scope, None)
        .map_err(|e| format!("failed to read rules: {e}"))
}

fn print_rules_table(rules: &[PrefixRule]) {
    println!(
        "  {:<5} {:<18} {:<8} {:<22} {:<14} CREATED AT",
        "ID", "TOOL", "SCOPE", "ARGUMENT", "EXPIRY"
    );
    if rules.is_empty() {
        println!("  (no persisted rules)");
        return;
    }
    for r in rules {
        let arg = display_arg(r.arg_prefix.as_deref(), r.scope, r.project_path.as_deref());
        let expiration = display_expiration(r.expires_at);
        let created = format_unix_date(r.created_at);
        println!(
            "  {:<5} {:<18} {:<8} {:<22} {:<14} {}",
            r.id,
            r.tool_name,
            r.scope.as_str(),
            arg,
            expiration,
            created
        );
    }
}

fn display_arg(
    arg_prefix: Option<&str>,
    scope: PermissionScope,
    project_path: Option<&Path>,
) -> String {
    let prefix = arg_prefix.unwrap_or("(tous)");
    if matches!(scope, PermissionScope::Project) {
        if let Some(p) = project_path {
            return format!("{prefix} @ {}", p.display());
        }
    }
    prefix.to_string()
}

fn display_expiration(expires_at: Option<i64>) -> String {
    match expires_at {
        Some(ts) => format_unix_date(ts),
        None => "permanente".to_string(),
    }
}

fn rule_to_json(r: &PrefixRule) -> serde_json::Value {
    serde_json::json!({
        "id": r.id,
        "tool_name": r.tool_name,
        "arg_prefix": r.arg_prefix,
        "action": match r.action { RuleAction::Allow => "allow", RuleAction::Deny => "deny" },
        "scope": r.scope.as_str(),
        "project_path": r.project_path.as_ref().map(|p| p.to_string_lossy().to_string()),
        "expires_at": r.expires_at,
        "created_at": r.created_at,
        "created_by_agent": r.created_by_agent,
    })
}

// ─── Revoke ──────────────────────────────────────────────────────────

fn run_revoke(
    id: Option<&str>,
    all: bool,
    scope_filter: Option<&str>,
    yes: bool,
    json: bool,
) -> i32 {
    if all {
        return run_revoke_all(scope_filter, yes, json);
    }

    let raw_id = match id {
        Some(s) => s.trim(),
        None => {
            return emit_error(
                "identifier required (or use --all --scope <scope>)".to_string(),
                json,
            );
        }
    };

    if raw_id.starts_with('s') || raw_id.starts_with('S') {
        return emit_error(
            format!(
                "{raw_id} denotes a session rule living in the runtime memory - \
                 use the desktop app or restart the daemon to remove it"
            ),
            json,
        );
    }

    let parsed: i64 = match raw_id.parse() {
        Ok(n) => n,
        Err(_) => return emit_error(format!("identifiant invalide : '{raw_id}'"), json),
    };

    if !confirm(&format!("Revoke rule #{parsed}? [y/N] "), yes, json) {
        return cancelled(json);
    }

    let db_path = match ensure_governance_db_path() {
        Ok(p) => p,
        Err(msg) => return emit_error(msg, json),
    };
    let mut engine = match PrefixRuleEngine::new(&db_path) {
        Ok(e) => e,
        Err(e) => return emit_error(format!("failed to open governance.db: {e}"), json),
    };

    let removed = match engine.remove_rule_checked(parsed) {
        Ok(b) => b,
        Err(e) => return emit_error(format!("delete failed: {e}"), json),
    };
    if !removed {
        return emit_error(format!("rule #{parsed} not found (already revoked?)"), json);
    }

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "revoked_id": parsed,
                "removed": 1,
            }))
            .unwrap_or_default()
        );
    } else {
        println!("✔ rule #{parsed} revoked");
    }
    exit_codes::SUCCESS
}

fn run_revoke_all(scope_filter: Option<&str>, yes: bool, json: bool) -> i32 {
    let scope = match scope_filter.map(parse_scope_filter).transpose() {
        Ok(Some(s)) => s,
        Ok(None) => {
            return emit_error(
                "--all requires --scope global|project (the CLI does not touch session rules)"
                    .to_string(),
                json,
            );
        }
        Err(msg) => return emit_error(msg, json),
    };
    if matches!(scope, PermissionScope::Session) {
        return emit_error(
            "--scope session not applicable from the CLI (those rules live in the runtime memory)"
                .to_string(),
            json,
        );
    }

    let db_path = match ensure_governance_db_path() {
        Ok(p) => p,
        Err(msg) => return emit_error(msg, json),
    };
    let mut engine = match PrefixRuleEngine::new(&db_path) {
        Ok(e) => e,
        Err(e) => return emit_error(format!("failed to open governance.db: {e}"), json),
    };

    let candidates = match engine.list_rules_filtered(Some(scope), None) {
        Ok(r) => r,
        Err(e) => return emit_error(format!("failed to read rules: {e}"), json),
    };
    let count = candidates.len();
    if count == 0 {
        if json {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "scope": scope.as_str(),
                    "removed": 0,
                }))
                .unwrap_or_default()
            );
        } else {
            println!("  (no {} rule to revoke)", scope.as_str());
        }
        return exit_codes::SUCCESS;
    }

    if !confirm(
        &format!("Revoke {count} {} rule(s)? [y/N] ", scope.as_str()),
        yes,
        json,
    ) {
        return cancelled(json);
    }

    let removed = match engine.remove_rules_by_scope(scope, None) {
        Ok(n) => n,
        Err(e) => return emit_error(format!("bulk revoke failed: {e}"), json),
    };

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "scope": scope.as_str(),
                "removed": removed,
            }))
            .unwrap_or_default()
        );
    } else {
        println!("✔ {removed} {} rule(s) revoked", scope.as_str());
    }
    exit_codes::SUCCESS
}

// ─── Audit ───────────────────────────────────────────────────────────

fn run_audit(tool_filter: Option<&str>, limit: u32, json: bool) -> i32 {
    let db_path = match ensure_governance_db_path() {
        Ok(p) => p,
        Err(msg) => return emit_error(msg, json),
    };
    let log = match PermissionAuditLog::new(&db_path) {
        Ok(l) => l,
        Err(e) => return emit_error(format!("failed to open audit log: {e}"), json),
    };
    let entries = match log.query(tool_filter, limit, 0) {
        Ok(e) => e,
        Err(e) => return emit_error(format!("failed to read audit log: {e}"), json),
    };

    if json {
        let arr: Vec<serde_json::Value> = entries
            .iter()
            .map(|e| {
                serde_json::json!({
                    "id": e.id,
                    "decided_at": e.decided_at,
                    "tool_name": e.tool_name,
                    "first_arg": e.first_arg,
                    "decision": e.decision,
                    "scope": e.scope,
                    "rule_id": e.rule_id,
                    "agent": e.agent,
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({"events": arr}))
                .unwrap_or_else(|_| "{}".to_string())
        );
    } else {
        println!(
            "  {:<19} {:<18} {:<28} AGENT",
            "TIMESTAMP", "TOOL", "DECISION"
        );
        if entries.is_empty() {
            println!("  (no decision recorded)");
        } else {
            for e in &entries {
                let ts = format_unix_datetime(e.decided_at);
                let agent = e.agent.as_deref().unwrap_or("-");
                println!(
                    "  {:<19} {:<18} {:<28} {}",
                    ts, e.tool_name, e.decision, agent
                );
            }
        }
    }
    exit_codes::SUCCESS
}

// ─── Add ─────────────────────────────────────────────────────────────

/// Raw CLI input for `permissions add` (string action/scope, uncanonicalised path).
struct AddRuleInput<'a> {
    tool: &'a str,
    prefix: Option<&'a str>,
    action: &'a str,
    scope: &'a str,
    project_path: Option<&'a Path>,
}

/// Validated rule fields ready to persist via the governance engine.
struct ResolvedRule<'a> {
    rule_action: RuleAction,
    tool: &'a str,
    prefix: Option<&'a str>,
    scope_enum: PermissionScope,
    project_buf: Option<PathBuf>,
}

fn run_add(input: AddRuleInput<'_>, json: bool) -> i32 {
    let AddRuleInput {
        tool,
        prefix,
        action,
        scope,
        project_path,
    } = input;
    if tool.trim().is_empty() {
        return emit_error("--tool must not be empty".into(), json);
    }
    let rule_action = match action {
        "allow" => RuleAction::Allow,
        "deny" => RuleAction::Deny,
        other => return emit_error(format!("unknown action '{other}'"), json),
    };
    let scope_enum = match scope {
        "project" => PermissionScope::Project,
        "global" => PermissionScope::Global,
        other => {
            return emit_error(
                format!("unknown scope '{other}' (use 'project' or 'global')"),
                json,
            )
        }
    };
    let project_buf = if scope_enum == PermissionScope::Project {
        let Some(raw) = project_path else {
            return emit_error(
                "--project-path is required when --scope project".into(),
                json,
            );
        };
        // Canonicalise so the rule matches the runtime's canonical-path lookup.
        let canonical = match raw.canonicalize() {
            Ok(p) => p,
            Err(e) => {
                return emit_error(format!("cannot canonicalize {}: {e}", raw.display()), json);
            }
        };
        Some(canonical)
    } else {
        None
    };

    run_add_with_engine(
        ResolvedRule {
            rule_action,
            tool: tool.trim(),
            prefix,
            scope_enum,
            project_buf,
        },
        None,
        json,
    )
}

/// Inner helper exposed for tests so they can inject an explicit governance
/// directory instead of mutating `$HOME` globally (which races across the
/// parallel test runner).
fn run_add_with_engine(rule: ResolvedRule<'_>, governance_dir: Option<&Path>, json: bool) -> i32 {
    let ResolvedRule {
        rule_action,
        tool,
        prefix,
        scope_enum,
        project_buf,
    } = rule;
    let db_path = match governance_dir {
        Some(dir) => {
            if !dir.exists() {
                if let Err(e) = std::fs::create_dir_all(dir) {
                    return emit_error(
                        format!("failed to create governance dir {}: {e}", dir.display()),
                        json,
                    );
                }
            }
            match GovernanceDb::open(dir) {
                Ok(db) => db.path().to_path_buf(),
                Err(e) => return emit_error(format!("open governance.db: {e}"), json),
            }
        }
        None => match ensure_governance_db_path() {
            Ok(p) => p,
            Err(msg) => return emit_error(msg, json),
        },
    };
    let mut engine = match PrefixRuleEngine::new(&db_path) {
        Ok(e) => e,
        Err(e) => return emit_error(format!("open governance.db: {e}"), json),
    };

    let now = chrono::Utc::now().timestamp();
    let rule = PrefixRule {
        id: 0,
        tool_name: tool.to_string(),
        arg_prefix: prefix.map(|s| s.to_string()),
        action: rule_action,
        created_at: now,
        created_by_agent: None,
        scope: scope_enum,
        project_path: project_buf.clone(),
        agent_id: None,
        expires_at: None,
    };

    let id = match engine.add_rule(&rule) {
        Ok(id) => id,
        Err(e) => return emit_error(format!("add rule failed: {e}"), json),
    };

    let action_str = match rule.action {
        RuleAction::Allow => "allow",
        RuleAction::Deny => "deny",
    };
    if json {
        let body = serde_json::json!({
            "id": id,
            "tool": rule.tool_name,
            "prefix": rule.arg_prefix,
            "action": action_str,
            "scope": scope_enum.as_str(),
            "project_path": project_buf.as_ref().map(|p| p.display().to_string()),
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&body).unwrap_or_default()
        );
    } else {
        let prefix_str = rule.arg_prefix.as_deref().unwrap_or("*");
        let project_str = project_buf
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "-".to_string());
        println!(
            "  * rule #{id} added: tool={} action={} scope={} prefix={} project={}",
            rule.tool_name,
            action_str,
            scope_enum.as_str(),
            prefix_str,
            project_str,
        );
    }
    exit_codes::SUCCESS
}

// ─── Helpers ─────────────────────────────────────────────────────────

fn parse_scope_filter(raw: &str) -> Result<PermissionScope, String> {
    match raw {
        "session" => Ok(PermissionScope::Session),
        "project" => Ok(PermissionScope::Project),
        "global" => Ok(PermissionScope::Global),
        other => Err(format!(
            "scope inconnu '{other}' - attendu : session | project | global"
        )),
    }
}

fn ensure_governance_db_path() -> Result<PathBuf, String> {
    let home = apollia_core::paths::home_string_or_err()?;
    let base = apollia_core::paths::data_dir_under(home);
    if !base.exists() {
        std::fs::create_dir_all(&base)
            .map_err(|e| format!("failed to create {}: {e}", base.display()))?;
    }
    let db = GovernanceDb::open(&base).map_err(|e| format!("failed to open governance.db: {e}"))?;
    Ok(db.path().to_path_buf())
}

fn confirm(prompt: &str, yes: bool, json: bool) -> bool {
    if yes {
        return true;
    }
    if json || !io::stdin().is_terminal() {
        eprintln!("Error: interactive confirmation required - re-run with --yes for scripts");
        return false;
    }
    print!("{prompt}");
    if io::stdout().flush().is_err() {
        return false;
    }
    let mut answer = String::new();
    if io::stdin().read_line(&mut answer).is_err() {
        return false;
    }
    let trimmed = answer.trim();
    trimmed.eq_ignore_ascii_case("o") || trimmed.eq_ignore_ascii_case("y")
}

fn cancelled(json: bool) -> i32 {
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({"cancelled": true}))
                .unwrap_or_default()
        );
    } else {
        println!("Cancelled.");
    }
    exit_codes::SUCCESS
}

fn emit_error(msg: String, json: bool) -> i32 {
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({"error": msg}))
                .unwrap_or_else(|_| "{\"error\":\"unknown\"}".to_string())
        );
    } else {
        eprintln!("Error: {msg}");
    }
    exit_codes::GENERAL_ERROR
}

fn format_unix_date(ts: i64) -> String {
    chrono::DateTime::<chrono::Utc>::from_timestamp(ts, 0)
        .map(|dt| dt.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| ts.to_string())
}

fn format_unix_datetime(ts: i64) -> String {
    chrono::DateTime::<chrono::Utc>::from_timestamp(ts, 0)
        .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|| ts.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_permissions::RuleAction;
    use apollia_tools::governance_db::GOVERNANCE_DB_FILENAME;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn fresh_engine(dir: &TempDir) -> (PrefixRuleEngine, PathBuf) {
        GovernanceDb::open(dir.path()).expect("init governance");
        let db_path = dir.path().join(GOVERNANCE_DB_FILENAME);
        (
            PrefixRuleEngine::new(&db_path).expect("open engine"),
            db_path,
        )
    }

    fn now() -> i64 {
        chrono::Utc::now().timestamp()
    }

    #[test]
    fn test_list_shows_all_scopes() {
        let dir = TempDir::new().expect("tempdir");
        let (mut engine, _) = fresh_engine(&dir);
        engine
            .add_rule(&PrefixRule {
                tool_name: "web_search".into(),
                action: RuleAction::Allow,
                created_at: now(),
                scope: PermissionScope::Global,
                ..PrefixRule::default()
            })
            .expect("add global");
        engine
            .add_rule(&PrefixRule {
                tool_name: "file_write".into(),
                arg_prefix: Some("/tmp/".into()),
                action: RuleAction::Allow,
                created_at: now(),
                scope: PermissionScope::Project,
                project_path: Some(PathBuf::from("/home/user/projet")),
                ..PrefixRule::default()
            })
            .expect("add project");

        let all = engine.list_rules_filtered(None, None).expect("list");
        assert_eq!(all.len(), 2);
        let scopes: Vec<&str> = all.iter().map(|r| r.scope.as_str()).collect();
        assert!(scopes.contains(&"global"));
        assert!(scopes.contains(&"project"));
    }

    #[test]
    fn test_revoke_by_id_removes_rule() {
        let dir = TempDir::new().expect("tempdir");
        let (mut engine, _) = fresh_engine(&dir);
        let id = engine
            .add_rule(&PrefixRule {
                tool_name: "web_search".into(),
                action: RuleAction::Allow,
                created_at: now(),
                scope: PermissionScope::Global,
                ..PrefixRule::default()
            })
            .expect("add");

        let removed = engine.remove_rule_checked(id).expect("remove");
        assert!(removed);
        let after = engine.list_rules().expect("list");
        assert!(after.is_empty());
    }

    #[test]
    fn test_revoke_all_global_keeps_project() {
        let dir = TempDir::new().expect("tempdir");
        let (mut engine, _) = fresh_engine(&dir);
        engine
            .add_rule(&PrefixRule {
                tool_name: "web_search".into(),
                action: RuleAction::Allow,
                created_at: now(),
                scope: PermissionScope::Global,
                ..PrefixRule::default()
            })
            .expect("add global");
        engine
            .add_rule(&PrefixRule {
                tool_name: "file_write".into(),
                action: RuleAction::Allow,
                created_at: now(),
                scope: PermissionScope::Project,
                project_path: Some(PathBuf::from("/projet")),
                ..PrefixRule::default()
            })
            .expect("add project");

        let removed = engine
            .remove_rules_by_scope(PermissionScope::Global, None)
            .expect("remove global");
        assert_eq!(removed, 1);
        let remaining = engine.list_rules().expect("list");
        assert_eq!(remaining.len(), 1);
        assert!(matches!(remaining[0].scope, PermissionScope::Project));
    }

    #[test]
    fn test_audit_filtered_by_tool() {
        // GIVEN two audit rows for two different tools
        let dir = TempDir::new().expect("tempdir");
        let db_path = dir.path().join(GOVERNANCE_DB_FILENAME);
        let governance = GovernanceDb::open(dir.path()).expect("init governance");
        let log = PermissionAuditLog::new(&db_path).expect("audit log");
        for (tool, arg, decision) in [
            ("web_search", "apollia", "AutoAllowedSafeList"),
            ("file_write", "/tmp/x", "NeedsApproval"),
        ] {
            governance
                .connection()
                .execute(
                    "INSERT INTO permission_audit (tool_name, first_arg, decision, decided_at) \
                     VALUES (?, ?, ?, ?)",
                    rusqlite::params![tool, arg, decision, now()],
                )
                .expect("insert audit row");
        }

        // WHEN one tool is queried, then all tools
        let only_web = log.query(Some("web_search"), 10, 0).expect("query");
        let all = log.query(None, 10, 0).expect("query all");

        // THEN the filter selects one row and the unfiltered query both
        assert_eq!(only_web.len(), 1);
        assert_eq!(only_web[0].tool_name, "web_search");
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn parse_scope_filter_accepts_known_values() {
        assert!(matches!(
            parse_scope_filter("global"),
            Ok(PermissionScope::Global)
        ));
        assert!(matches!(
            parse_scope_filter("project"),
            Ok(PermissionScope::Project)
        ));
        assert!(matches!(
            parse_scope_filter("session"),
            Ok(PermissionScope::Session)
        ));
        assert!(parse_scope_filter("xyz").is_err());
    }

    #[test]
    fn display_arg_includes_project_path_for_project_scope() {
        let s = display_arg(
            Some("/tmp/"),
            PermissionScope::Project,
            Some(Path::new("/home/u/projet")),
        );
        assert!(s.contains("/tmp/"));
        assert!(s.contains("/home/u/projet"));

        let g = display_arg(Some("git"), PermissionScope::Global, None);
        assert_eq!(g, "git");
    }

    #[test]
    fn display_expiration_handles_none_and_some() {
        assert_eq!(display_expiration(None), "permanente");
        let s = display_expiration(Some(0));
        assert!(s.starts_with("1970"));
    }

    #[test]
    fn parses_add_global() {
        use clap::Parser;
        #[derive(Parser, Debug)]
        struct TestCli {
            #[command(subcommand)]
            cmd: PermissionsCommand,
        }
        let cli = TestCli::parse_from(["x", "add", "--tool", "web_search", "--scope", "global"]);
        match cli.cmd {
            PermissionsCommand::Add {
                tool,
                prefix,
                action,
                scope,
                project_path,
            } => {
                assert_eq!(tool, "web_search");
                assert!(prefix.is_none());
                assert_eq!(action, "allow");
                assert_eq!(scope, "global");
                assert!(project_path.is_none());
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parses_add_project_with_prefix() {
        use clap::Parser;
        #[derive(Parser, Debug)]
        struct TestCli {
            #[command(subcommand)]
            cmd: PermissionsCommand,
        }
        let cli = TestCli::parse_from([
            "x",
            "add",
            "--tool",
            "file_write",
            "--prefix",
            "/tmp/",
            "--scope",
            "project",
            "--project-path",
            "/home/u/proj",
            "--action",
            "deny",
        ]);
        match cli.cmd {
            PermissionsCommand::Add {
                tool,
                prefix,
                action,
                scope,
                project_path,
            } => {
                assert_eq!(tool, "file_write");
                assert_eq!(prefix.as_deref(), Some("/tmp/"));
                assert_eq!(action, "deny");
                assert_eq!(scope, "project");
                assert_eq!(project_path, Some(PathBuf::from("/home/u/proj")));
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn add_global_persists_rule() {
        let dir = TempDir::new().expect("tempdir");
        // Use the inner helper with an explicit governance dir so the test
        // does not race on `$HOME` with other parallel tests.
        let code = run_add_with_engine(
            ResolvedRule {
                rule_action: RuleAction::Allow,
                tool: "web_search",
                prefix: None,
                scope_enum: PermissionScope::Global,
                project_buf: None,
            },
            Some(dir.path()),
            true,
        );
        assert_eq!(code, exit_codes::SUCCESS);

        let db_path = dir.path().join(GOVERNANCE_DB_FILENAME);
        let engine = PrefixRuleEngine::new(&db_path).expect("open engine");
        let rules = engine.list_rules().expect("list");
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].tool_name, "web_search");
        assert_eq!(rules[0].action, RuleAction::Allow);
        assert_eq!(rules[0].scope, PermissionScope::Global);
    }

    #[test]
    fn add_project_requires_project_path() {
        // No need to set HOME: the surface validation happens in `run_add`
        // before any DB access, so the helper short-circuits.
        let code = run_add(
            AddRuleInput {
                tool: "file_write",
                prefix: None,
                action: "allow",
                scope: "project",
                project_path: None,
            },
            true,
        );
        assert_eq!(code, exit_codes::GENERAL_ERROR);
    }

    #[test]
    fn add_rejects_empty_tool() {
        let code = run_add(
            AddRuleInput {
                tool: "   ",
                prefix: None,
                action: "allow",
                scope: "global",
                project_path: None,
            },
            true,
        );
        assert_eq!(code, exit_codes::GENERAL_ERROR);
    }

    #[test]
    fn add_with_prefix_persists() {
        let dir = TempDir::new().expect("tempdir");
        let code = run_add_with_engine(
            ResolvedRule {
                rule_action: RuleAction::Deny,
                tool: "file_write",
                prefix: Some("/tmp/"),
                scope_enum: PermissionScope::Global,
                project_buf: None,
            },
            Some(dir.path()),
            true,
        );
        assert_eq!(code, exit_codes::SUCCESS);
        let db_path = dir.path().join(GOVERNANCE_DB_FILENAME);
        let engine = PrefixRuleEngine::new(&db_path).expect("open engine");
        let rules = engine.list_rules().expect("list");
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].action, RuleAction::Deny);
        assert_eq!(rules[0].arg_prefix.as_deref(), Some("/tmp/"));
    }
}
