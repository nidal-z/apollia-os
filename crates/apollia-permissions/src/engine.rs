//! Three-layer permission engine, main entry point.
//!
//! `PermissionEngine::decide()` evaluates each tool invocation in this order:
//!
//! 1. **Layer 3, InjectionDetector** (absolute priority, blocking)
//!    Scans every string argument for dangerous shell patterns.
//!
//! 2. **Layer 1, SafeList** (operator config, empty by default)
//!    Auto-approves invocations explicitly configured by the operator.
//!
//! 3. **Layer 2, PrefixRuleEngine** (persisted SQLite rules)
//!    Auto-approves or auto-denies based on rules saved by the operator
//!    or by the desktop HITL "Always allow" button.
//!
//! 4. **Fallback**, `NeedsApproval`
//!    Any invocation without a match requires human approval.
//!
//! Every decision is persisted in `PermissionAuditLog` (immutable).

use std::path::Path;

use apollia_core::config::PermissionsConfig;
use apollia_core::manifest::AgentManifest;
use serde_json::Value;

use crate::audit_log::PermissionAuditLog;
use crate::error::PermissionError;
use crate::injection_detector::InjectionDetector;
use crate::prefix_rule_engine::{
    PermissionScope, PrefixRule, PrefixRuleEngine, RuleAction, ScopeContext,
};
use crate::safe_list::SafeList;

/// `created_by` marker stamped on rules ingested from `PermissionsConfig.safe_commands`
/// at engine startup (single source of truth: `governance.db`).
pub const CONFIG_IMPORT_CREATOR: &str = "config-import";

// ─────────────────────────────────────────────
// PermissionDecision
// ─────────────────────────────────────────────

/// Decision emitted by `PermissionEngine::decide()`.
///
/// The runtime uses this decision to emit a `RuntimeEvent::PermissionRequired`
/// event (NeedsApproval) or to return `ToolError::PermissionDenied`
/// (AutoDenied*).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionDecision {
    /// Invocation auto-approved by the operator SafeList (layer 1).
    AutoAllowedSafeList,
    /// Invocation auto-approved by a SQLite prefix rule (layer 2).
    AutoAllowedPrefixRule {
        /// Identifier of the rule that triggered the approval.
        rule_id: i64,
    },
    /// Invocation auto-denied by a SQLite prefix rule (layer 2).
    AutoDeniedPrefixRule {
        /// Identifier of the rule that triggered the denial.
        rule_id: i64,
    },
    /// Invocation blocked by shell injection detection (layer 3).
    AutoDeniedInjection {
        /// Name of the detected injection pattern (e.g. `";"`, `"$("`, ...).
        pattern: String,
    },
    /// No layer made a call: human approval is required.
    NeedsApproval,
}

// ─────────────────────────────────────────────
// PermissionEngine
// ─────────────────────────────────────────────

/// Three-layer permission engine.
///
/// Instantiate once per runtime and share via `Arc<Mutex<PermissionEngine>>`
/// when several actors need it concurrently.
pub struct PermissionEngine {
    safe_list: SafeList,
    prefix_rules: PrefixRuleEngine,
    injection_detector: InjectionDetector,
    audit_log: PermissionAuditLog,
    injection_detection_enabled: bool,
    session_rules: Vec<PrefixRule>,
    scope_context: Option<ScopeContext>,
}

impl PermissionEngine {
    /// Builds a `PermissionEngine` from the configuration and the SQLite path.
    ///
    /// The same SQLite file backs both the `PrefixRuleEngine` and the `PermissionAuditLog`.
    ///
    /// # Errors
    ///
    /// - [`PermissionError::Database`] if SQLite initialization fails.
    pub fn new(config: &PermissionsConfig, db_path: &Path) -> Result<Self, PermissionError> {
        let mut prefix_rules = PrefixRuleEngine::new(db_path)?;
        let safe_list = SafeList::from_config(config);

        // Idempotent migration of the TOML SafeList into governance.db.
        // On the first boot with a non-empty SafeList, each pattern is ingested
        // as an Allow rule with scope=Global and created_by="config-import".
        // Later boots detect the rules already present and rewrite none of them.
        migrate_safe_list_to_governance(&mut prefix_rules, &safe_list)?;

        Ok(Self {
            safe_list,
            prefix_rules,
            injection_detector: InjectionDetector::new(),
            audit_log: PermissionAuditLog::new(db_path)?,
            injection_detection_enabled: config.injection_detection,
            session_rules: Vec::new(),
            scope_context: None,
        })
    }

    /// Adds a session rule (in memory only, never persisted).
    ///
    /// Used by the HITL dialog's "Always allow for this session" button.
    /// The rule disappears when the process stops.
    ///
    /// The rule is forced to `scope = Session` regardless of the incoming
    /// `PrefixRule` scope, so a session rule cannot accidentally enter the DB path.
    pub fn add_session_rule(&mut self, mut rule: PrefixRule) {
        rule.scope = PermissionScope::Session;
        rule.project_path = None;
        self.session_rules.push(rule);
    }

    /// Clears the session rules (call at process end if needed).
    pub fn clear_session_rules(&mut self) {
        self.session_rules.clear();
    }

    /// Sets the current scope context used by `decide()` when a `Project` rule
    /// must be filtered by path.
    pub fn set_scope_context(&mut self, ctx: ScopeContext) {
        self.scope_context = Some(ctx);
    }

    /// Returns an immutable view of the current scope context.
    pub fn scope_context(&self) -> Option<&ScopeContext> {
        self.scope_context.as_ref()
    }

    /// Returns an immutable view of the in-memory session rules.
    pub fn session_rules(&self) -> &[PrefixRule] {
        &self.session_rules
    }

    /// Evaluates the 3 permission layers for a tool invocation.
    ///
    /// Evaluation order:
    /// 1. InjectionDetector (layer 3, absolute priority)
    /// 2. SafeList (layer 1, operator config)
    /// 3. PrefixRuleEngine (layer 2, SQLite rules)
    /// 4. NeedsApproval (fallback)
    ///
    /// The decision is always recorded in the audit log.
    ///
    /// # Errors
    ///
    /// - [`PermissionError::Database`] if the audit log cannot be written.
    pub fn decide(
        &mut self,
        tool_name: &str,
        input: &Value,
        _agent_manifest: &AgentManifest,
    ) -> Result<PermissionDecision, PermissionError> {
        let first_arg = extract_first_arg(input);

        // ── Layer 3: InjectionDetector (absolute priority) ──────────────────
        if self.injection_detection_enabled {
            let suspicious_value = find_suspicious_string(input, &self.injection_detector);
            if let Some(pattern) = suspicious_value {
                let decision = PermissionDecision::AutoDeniedInjection {
                    pattern: pattern.clone(),
                };
                tracing::warn!(
                    tool = %tool_name,
                    injection_pattern = %pattern,
                    "injection detected - invocation blocked"
                );
                self.audit_log
                    .record(tool_name, first_arg.as_deref(), &decision)?;
                return Ok(decision);
            }
        }

        // ── Layer 1: SafeList ────────────────────────────────────────────────
        if self.safe_list.matches(tool_name, first_arg.as_deref()) {
            let decision = PermissionDecision::AutoAllowedSafeList;
            tracing::debug!(tool = %tool_name, "auto-allowed by safe list");
            self.audit_log
                .record(tool_name, first_arg.as_deref(), &decision)?;
            return Ok(decision);
        }

        // ── Layer 2: PrefixRuleEngine ────────────────────────────────────────
        let prefix_hit = match &self.scope_context {
            Some(ctx) => self.prefix_rules.check_with_scope(
                tool_name,
                first_arg.as_deref(),
                ctx,
                &self.session_rules,
            )?,
            None => self
                .prefix_rules
                .check_with_id(tool_name, first_arg.as_deref())?,
        };
        if let Some((rule_id, action)) = prefix_hit {
            use crate::prefix_rule_engine::RuleAction;
            let decision = match action {
                RuleAction::Allow => PermissionDecision::AutoAllowedPrefixRule { rule_id },
                RuleAction::Deny => PermissionDecision::AutoDeniedPrefixRule { rule_id },
            };
            tracing::debug!(tool = %tool_name, rule_id, "prefix rule matched");
            self.audit_log
                .record(tool_name, first_arg.as_deref(), &decision)?;
            return Ok(decision);
        }

        // ── Fallback: NeedsApproval ──────────────────────────────────────────
        let decision = PermissionDecision::NeedsApproval;
        tracing::debug!(tool = %tool_name, "needs human approval");
        self.audit_log
            .record(tool_name, first_arg.as_deref(), &decision)?;
        Ok(decision)
    }

    /// Exposes the `PrefixRuleEngine` so rules can be added from outside
    /// (for example, via the desktop HITL "Always allow" button).
    pub fn prefix_rules_mut(&mut self) -> &mut PrefixRuleEngine {
        &mut self.prefix_rules
    }

    /// Exposes the `PermissionAuditLog` read-only for audit queries.
    pub fn audit_log(&self) -> &PermissionAuditLog {
        &self.audit_log
    }
}

// ─────────────────────────────────────────────
// Private helpers
// ─────────────────────────────────────────────

/// Extracts the first string argument from the JSON input.
///
/// Strategy: try the common native-tool keys first
/// (`cmd`, `command`, `path`, `query`, `input`, `text`, `content`, `prompt`),
/// then take the first string value found in the object.
/// If the input is itself a string, return it as is.
fn extract_first_arg(input: &Value) -> Option<String> {
    match input {
        Value::String(s) => Some(s.clone()),
        Value::Object(map) => {
            // Try the common native-tool keys first.
            const PRIORITY_KEYS: &[&str] = &[
                "cmd", "command", "path", "query", "input", "text", "content", "prompt",
            ];

            for key in PRIORITY_KEYS {
                if let Some(Value::String(s)) = map.get(*key) {
                    return Some(s.clone());
                }
            }
            // Fallback: first string value found in the object.
            map.values().find_map(|v| {
                if let Value::String(s) = v {
                    Some(s.clone())
                } else {
                    None
                }
            })
        }
        _ => None,
    }
}

/// Looks for an injection pattern across every string value in the JSON input.
///
/// Recursively inspects strings inside objects and arrays.
/// Returns the name of the first detected pattern, or `None`.
fn find_suspicious_string(input: &Value, detector: &InjectionDetector) -> Option<String> {
    match input {
        Value::String(s) => detector.detected_pattern(s),
        Value::Object(map) => map
            .values()
            .find_map(|v| find_suspicious_string(v, detector)),
        Value::Array(arr) => arr.iter().find_map(|v| find_suspicious_string(v, detector)),
        _ => None,
    }
}

// ─────────────────────────────────────────────
// SafeList migration into governance.db
// ─────────────────────────────────────────────

/// Ingests parsed `SafeList` patterns as `permission_rules` with
/// `created_by="config-import"` when no rule from that author exists yet.
///
/// Idempotent: a second call after a successful migration is a no-op (the
/// presence of at least one `created_by="config-import"` rule short-circuits
/// the import).
fn migrate_safe_list_to_governance(
    prefix_rules: &mut PrefixRuleEngine,
    safe_list: &SafeList,
) -> Result<(), PermissionError> {
    let patterns = safe_list.parsed_patterns();
    if patterns.is_empty() {
        return Ok(());
    }

    let already_imported = prefix_rules
        .list_rules_by_creator(CONFIG_IMPORT_CREATOR)?
        .len();
    if already_imported > 0 {
        tracing::debug!(
            already_imported,
            "safe_list migration skipped (governance.db already contains config-import rules)"
        );
        return Ok(());
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let mut imported = 0u32;
    for (tool_name, arg_prefix) in patterns {
        let rule = PrefixRule {
            tool_name,
            arg_prefix,
            action: RuleAction::Allow,
            created_at: now,
            created_by_agent: Some(CONFIG_IMPORT_CREATOR.to_string()),
            scope: PermissionScope::Global,
            ..PrefixRule::default()
        };
        prefix_rules.add_rule(&rule)?;
        imported += 1;
    }

    tracing::info!(
        count = imported,
        "safe_list migrated to governance.db"
    );
    Ok(())
}

// ─────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_core::config::PermissionsConfig;
    use apollia_core::manifest::AgentManifest;
    use serde_json::json;
    use std::path::PathBuf;
    use tempfile::NamedTempFile;

    fn dummy_manifest() -> AgentManifest {
        AgentManifest {
            name: "test-agent".into(),
            version: "1.0.0".into(),
            description: "test".into(),
            tools_required: vec![],
            tools_optional: vec![],
            supports_streaming: false,
            supports_a2a: false,
            memory_namespace: None,
            shared_memory_namespaces: vec![],
            max_concurrent_tasks: 1,
            step_budget: None,
            network_allowlist: None,
            dangerous_tools_allowed: false,
            tags: vec![],
            skills: vec![],
            execution_mode: "auto".into(),
            supports_mailbox: false,
            mailbox_allowlist: None,
            system_prompt: None,
            tools_requiring_approval: vec![],
            llm_backend: None,
            packages: vec![],
            memory_config: None,
            agent_type: None,
            examples: vec![],
            limitations: vec![],
            setup_notes: None,
            agent_class: None,
            user_memory_write: false,
            datasources: vec![],
            templates: vec![],
            secrets: vec![],
            check_commands: vec![],
        }
    }

    fn engine_with_config(config: PermissionsConfig) -> (PermissionEngine, NamedTempFile) {
        let db_file = NamedTempFile::new().expect("tempfile");
        let engine = PermissionEngine::new(&config, db_file.path()).expect("engine init");
        (engine, db_file)
    }

    fn empty_config() -> PermissionsConfig {
        PermissionsConfig {
            safe_commands: vec![],
            injection_detection: true,
            prefix_rule_ttl_hours: 168,
            db_path: PathBuf::from("/tmp/test.db"),
        }
    }

    fn config_with_safe_cmd(cmd: &str) -> PermissionsConfig {
        PermissionsConfig {
            safe_commands: vec![cmd.to_string()],
            injection_detection: true,
            prefix_rule_ttl_hours: 168,
            db_path: PathBuf::from("/tmp/test.db"),
        }
    }

    #[test]
    fn engine_empty_safe_list_needs_approval() {
        // GIVEN a PermissionEngine with an empty SafeList (default)
        let (mut engine, _tmp) = engine_with_config(empty_config());
        let manifest = dummy_manifest();
        // WHEN
        let decision = engine
            .decide("bash_executor", &json!({"cmd": "git status"}), &manifest)
            .expect("decide");
        // THEN
        assert_eq!(decision, PermissionDecision::NeedsApproval);
    }

    #[test]
    fn engine_layer1_configured_command_auto_allowed() {
        // GIVEN a PermissionEngine with SafeList configured for "git status"
        let (mut engine, _tmp) =
            engine_with_config(config_with_safe_cmd("bash_executor(git status)"));
        let manifest = dummy_manifest();
        // WHEN
        let decision = engine
            .decide("bash_executor", &json!({"cmd": "git status"}), &manifest)
            .expect("decide");
        // THEN
        assert_eq!(decision, PermissionDecision::AutoAllowedSafeList);
    }

    #[test]
    fn engine_injection_overrides_safe_list() {
        // GIVEN a PermissionEngine with SafeList for the injected command
        // AND a command with command substitution
        let (mut engine, _tmp) = engine_with_config(config_with_safe_cmd(
            "bash_executor(git status $(rm -rf /))",
        ));
        let manifest = dummy_manifest();
        // WHEN
        let decision = engine
            .decide(
                "bash_executor",
                &json!({"cmd": "git status $(rm -rf /)"}),
                &manifest,
            )
            .expect("decide");
        // THEN layer 3 takes absolute priority
        assert!(matches!(
            decision,
            PermissionDecision::AutoDeniedInjection { .. }
        ));
    }

    #[test]
    fn engine_unknown_tool_needs_approval() {
        // GIVEN a PermissionEngine with no applicable rule
        let (mut engine, _tmp) = engine_with_config(empty_config());
        let manifest = dummy_manifest();
        // WHEN
        let decision = engine
            .decide("custom_tool", &json!({"input": "foo"}), &manifest)
            .expect("decide");
        // THEN
        assert_eq!(decision, PermissionDecision::NeedsApproval);
    }

    #[test]
    fn engine_layer2_prefix_rule_allows_git_push() {
        // GIVEN a PermissionEngine with an Allow rule "bash_executor(git:*)"
        let (mut engine, _tmp) = engine_with_config(empty_config());
        use crate::prefix_rule_engine::{PrefixRule, RuleAction};
        let rule = PrefixRule {
            tool_name: "bash_executor".into(),
            arg_prefix: Some("git".into()),
            action: RuleAction::Allow,
            ..PrefixRule::default()
        };
        engine.prefix_rules_mut().add_rule(&rule).expect("add rule");
        let manifest = dummy_manifest();
        // WHEN
        let decision = engine
            .decide("bash_executor", &json!({"cmd": "git push"}), &manifest)
            .expect("decide");
        // THEN
        assert!(matches!(
            decision,
            PermissionDecision::AutoAllowedPrefixRule { .. }
        ));
    }

    #[test]
    fn engine_injection_detected_on_non_safe_command() {
        // GIVEN a fully configured PermissionEngine
        let (mut engine, _tmp) = engine_with_config(empty_config());
        let manifest = dummy_manifest();
        // WHEN a command with command substitution
        let decision = engine
            .decide(
                "bash_executor",
                &json!({"cmd": "git status $(rm -rf /)"}),
                &manifest,
            )
            .expect("decide");
        // THEN AutoDeniedInjection
        assert!(matches!(
            decision,
            PermissionDecision::AutoDeniedInjection { .. }
        ));
    }

    #[test]
    fn engine_injection_detection_disabled_skips_check() {
        // GIVEN a PermissionEngine with injection_detection = false
        let config = PermissionsConfig {
            safe_commands: vec![],
            injection_detection: false,
            prefix_rule_ttl_hours: 168,
            db_path: PathBuf::from("/tmp/test.db"),
        };
        let (mut engine, _tmp) = engine_with_config(config);
        let manifest = dummy_manifest();
        // WHEN a command with injection but detection disabled
        let decision = engine
            .decide(
                "bash_executor",
                &json!({"cmd": "git status; rm -rf /"}),
                &manifest,
            )
            .expect("decide");
        // THEN NeedsApproval (not AutoDeniedInjection)
        assert_eq!(decision, PermissionDecision::NeedsApproval);
    }

    #[test]
    fn extract_first_arg_uses_cmd_key() {
        let input = json!({"cmd": "git status", "other": "value"});
        assert_eq!(extract_first_arg(&input), Some("git status".to_string()));
    }

    #[test]
    fn extract_first_arg_string_input() {
        let input = json!("direct string");
        assert_eq!(extract_first_arg(&input), Some("direct string".to_string()));
    }

    #[test]
    fn extract_first_arg_empty_object_returns_none() {
        let input = json!({});
        assert!(extract_first_arg(&input).is_none());
    }

    // ─────────────────────────────────────────────
    // SafeList migration into governance.db
    // ─────────────────────────────────────────────

    fn config_with_safe_cmds(cmds: Vec<&str>) -> PermissionsConfig {
        PermissionsConfig {
            safe_commands: cmds.into_iter().map(String::from).collect(),
            injection_detection: true,
            prefix_rule_ttl_hours: 168,
            db_path: PathBuf::from("/tmp/test.db"),
        }
    }

    #[test]
    fn migrate_safe_list_imports_patterns_on_first_boot() {
        // GIVEN a config with two safe_commands entries and a fresh DB
        let db_file = NamedTempFile::new().expect("tempfile");
        let config = config_with_safe_cmds(vec!["bash_executor(git status)", "file_read"]);

        // WHEN building the engine
        let engine = PermissionEngine::new(&config, db_file.path()).expect("engine init");

        // THEN two config-import rules exist in the DB
        let imported = engine
            .prefix_rules
            .list_rules_by_creator(CONFIG_IMPORT_CREATOR)
            .expect("list");
        assert_eq!(imported.len(), 2);
        assert!(imported.iter().all(|r| r.action == RuleAction::Allow));
        assert!(imported.iter().all(|r| r.scope == PermissionScope::Global));

        let bash_rule = imported
            .iter()
            .find(|r| r.tool_name == "bash_executor")
            .expect("bash rule");
        assert_eq!(bash_rule.arg_prefix.as_deref(), Some("git status"));

        let read_rule = imported
            .iter()
            .find(|r| r.tool_name == "file_read")
            .expect("file_read rule");
        assert!(read_rule.arg_prefix.is_none());
    }

    #[test]
    fn migrate_safe_list_is_idempotent() {
        // GIVEN a shared DB and a config with one pattern
        let db_file = NamedTempFile::new().expect("tempfile");
        let config = config_with_safe_cmds(vec!["bash_executor(pwd)"]);

        // WHEN building the engine twice on the same DB
        {
            let _engine = PermissionEngine::new(&config, db_file.path()).expect("init 1");
        }
        let engine2 = PermissionEngine::new(&config, db_file.path()).expect("init 2");

        // THEN a single config-import rule exists (no duplicates)
        let imported = engine2
            .prefix_rules
            .list_rules_by_creator(CONFIG_IMPORT_CREATOR)
            .expect("list");
        assert_eq!(imported.len(), 1);
    }

    #[test]
    fn migrate_safe_list_empty_config_creates_no_rule() {
        // GIVEN a config with no safe_commands
        let db_file = NamedTempFile::new().expect("tempfile");
        let config = empty_config();

        // WHEN building the engine
        let engine = PermissionEngine::new(&config, db_file.path()).expect("init");

        // THEN no config-import rule
        let imported = engine
            .prefix_rules
            .list_rules_by_creator(CONFIG_IMPORT_CREATOR)
            .expect("list");
        assert!(imported.is_empty());
    }
}
