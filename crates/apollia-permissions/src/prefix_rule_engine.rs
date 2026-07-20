//! Layer 2 of the permission engine: the SQLite PrefixRuleEngine.
//!
//! Persists Allow/Deny rules keyed by argument prefix in SQLite.
//! Lets the operator (or the desktop HITL "Always allow" button) add rules
//! that survive runtime restarts.
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
//!     scope        TEXT NOT NULL DEFAULT 'global', -- 'project' or 'global'
//!     project_path TEXT,
//!     expires_at   INTEGER
//! );
//! ```

use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OpenFlags};
use serde::{Deserialize, Serialize};

use crate::error::PermissionError;
use crate::migrations::add_column_if_missing;

// ─────────────────────────────────────────────
// Public types
// ─────────────────────────────────────────────

/// Action of a prefix rule: Allow or Deny.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RuleAction {
    /// Auto-approve invocations matching this rule.
    Allow,
    /// Auto-deny invocations matching this rule.
    Deny,
}

impl RuleAction {
    fn as_str(&self) -> &'static str {
        match self {
            RuleAction::Allow => "allow",
            RuleAction::Deny => "deny",
        }
    }

    fn from_str(s: &str) -> Result<Self, PermissionError> {
        match s {
            "allow" => Ok(RuleAction::Allow),
            "deny" => Ok(RuleAction::Deny),
            other => Err(PermissionError::InvalidRule(format!(
                "unknown action '{other}', expected 'allow' or 'deny'"
            ))),
        }
    }
}

/// Scope of a permission rule.
///
/// Determines where the rule is stored and how it is filtered during evaluation.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PermissionScope {
    /// Rule living only in memory inside the `PermissionEngine`.
    ///
    /// Disappears when the process stops. Never persisted to SQLite.
    Session,
    /// Rule persisted and filtered by the canonical path of the current project.
    ///
    /// Applies only to invocations issued from the matching project.
    Project,
    /// Rule persisted and filtered by the identity of the current agent.
    ///
    /// Applies to any invocation issued by the agent whose `agent_id` matches.
    /// Independent of the project: an agent can run outside a project.
    Agent,
    /// Rule persisted and applying to any project.
    #[default]
    Global,
}

impl PermissionScope {
    /// Textual representation stored in the database.
    pub fn as_str(&self) -> &'static str {
        match self {
            PermissionScope::Session => "session",
            PermissionScope::Project => "project",
            PermissionScope::Agent => "agent",
            PermissionScope::Global => "global",
        }
    }

    /// Parse the value stored in the database, defaulting to `Global` for null columns.
    fn from_db_str(s: &str) -> Result<Self, PermissionError> {
        match s {
            "session" => Ok(PermissionScope::Session),
            "project" => Ok(PermissionScope::Project),
            "agent" => Ok(PermissionScope::Agent),
            "global" => Ok(PermissionScope::Global),
            other => Err(PermissionError::InvalidRule(format!(
                "unknown scope '{other}', expected 'session' | 'project' | 'agent' | 'global'"
            ))),
        }
    }
}

/// Evaluation context for scope-aware rule matching.
///
/// Passed to the `PrefixRuleEngine` to filter `Project`/`Agent` rules.
#[derive(Debug, Clone, Default)]
pub struct ScopeContext {
    /// Scope of the current invocation (informational, not used for filtering).
    pub scope: PermissionScope,
    /// Canonical path of the current project (`None` when outside a project).
    pub project_path: Option<PathBuf>,
    /// Identifier of the current agent (`None` when outside an agent context).
    pub agent_id: Option<String>,
}

/// A prefix rule persisted in SQLite.
///
/// A rule binds a tool name and an optional argument prefix to an action.
/// `arg_prefix = None` means the rule applies to any argument.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrefixRule {
    /// Unique identifier (SQLite AUTOINCREMENT). 0 for a non-persisted rule.
    pub id: i64,
    /// Name of the targeted tool.
    pub tool_name: String,
    /// Argument prefix to match (None = any argument).
    pub arg_prefix: Option<String>,
    /// Action to apply when the rule matches.
    pub action: RuleAction,
    /// Creation timestamp (Unix epoch, seconds).
    pub created_at: i64,
    /// Name of the agent that created the rule (None = human operator).
    pub created_by_agent: Option<String>,
    /// Scope of the rule.
    pub scope: PermissionScope,
    /// Canonical project path (set when `scope == Project`).
    pub project_path: Option<PathBuf>,
    /// Agent identifier (set when `scope == Agent`).
    pub agent_id: Option<String>,
    /// Unix expiration timestamp (None = permanent rule).
    pub expires_at: Option<i64>,
}

impl Default for PrefixRule {
    fn default() -> Self {
        Self {
            id: 0,
            tool_name: String::new(),
            arg_prefix: None,
            action: RuleAction::Allow,
            created_at: 0,
            created_by_agent: None,
            scope: PermissionScope::Global,
            project_path: None,
            agent_id: None,
            expires_at: None,
        }
    }
}

// ─────────────────────────────────────────────
// PrefixRuleEngine
// ─────────────────────────────────────────────

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

    /// Checks whether the invocation (`tool_name`, `first_arg`) matches a persisted rule.
    ///
    /// Backward-compatible variant: does not filter by scope (all project + global
    /// rules are considered) but ignores expired rules.
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
        let now = current_unix_secs();
        let mut stmt = self.db.prepare_cached(
            "SELECT id, arg_prefix, action, expires_at FROM permission_rules \
             WHERE tool_name = ? AND scope NOT IN ('session', 'agent') \
             ORDER BY CASE WHEN arg_prefix IS NULL THEN 0 ELSE LENGTH(arg_prefix) END DESC",
        )?;

        let mut rows = stmt.query(params![tool_name])?;
        while let Some(row) = rows.next()? {
            let id: i64 = row.get(0)?;
            let arg_prefix: Option<String> = row.get(1)?;
            let action_str: String = row.get(2)?;
            let expires_at: Option<i64> = row.get(3)?;

            if is_expired(expires_at, now) {
                tracing::warn!(
                    rule_id = id,
                    tool = %tool_name,
                    "expired prefix rule encountered - ignored"
                );
                continue;
            }

            let action = RuleAction::from_str(&action_str)?;
            if prefix_matches(tool_name, arg_prefix.as_deref(), first_arg) {
                return Ok(Some((id, action)));
            }
        }

        Ok(None)
    }

    /// Evaluates rules with scope-aware filtering.
    ///
    /// Evaluation order (most specific to broadest):
    ///
    /// 1. DB rules with `scope = 'project'` filtered by `scope_ctx.project_path`.
    /// 2. DB rules with `scope = 'agent'` filtered by `scope_ctx.agent_id`.
    /// 3. `session_rules` (in memory, never persisted).
    /// 4. DB rules with `scope = 'global'`.
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
        session_rules: &[PrefixRule],
    ) -> Result<Option<(i64, RuleAction)>, PermissionError> {
        let now = current_unix_secs();

        if let Some(project_path) = scope_ctx.project_path.as_deref() {
            if let Some(hit) = self.match_in_db_project(tool_name, first_arg, project_path, now)? {
                return Ok(Some(hit));
            }
        }

        if let Some(agent_id) = scope_ctx.agent_id.as_deref() {
            if let Some(hit) = self.match_in_db_agent(tool_name, first_arg, agent_id, now)? {
                return Ok(Some(hit));
            }
        }

        if let Some(hit) = match_in_session(tool_name, first_arg, session_rules, now) {
            return Ok(Some(hit));
        }

        self.match_in_db_global(tool_name, first_arg, now)
    }

    /// Persists a new rule and returns its auto-incremented identifier.
    ///
    /// # Errors
    ///
    /// - [`PermissionError::InvalidRule`] if `tool_name` is empty, if `scope == Session`
    ///   (session rules live in memory), or if `scope == Project` without a
    ///   `project_path`.
    /// - [`PermissionError::Database`] on SQLite error.
    pub fn add_rule(&mut self, rule: &PrefixRule) -> Result<i64, PermissionError> {
        if rule.tool_name.trim().is_empty() {
            return Err(PermissionError::InvalidRule(
                "tool_name must not be empty".to_string(),
            ));
        }
        if rule.scope == PermissionScope::Session {
            return Err(PermissionError::InvalidRule(
                "session rules must not be persisted; use PermissionEngine::add_session_rule"
                    .to_string(),
            ));
        }
        if rule.scope == PermissionScope::Project && rule.project_path.is_none() {
            return Err(PermissionError::InvalidRule(
                "project scope requires a project_path".to_string(),
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

        let project_path_str = rule
            .project_path
            .as_ref()
            .map(|p| p.to_string_lossy().to_string());

        self.db.execute(
            "INSERT INTO permission_rules \
             (tool_name, arg_prefix, action, created_at, created_by, scope, project_path, agent_id, expires_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                rule.tool_name,
                rule.arg_prefix,
                rule.action.as_str(),
                rule.created_at,
                rule.created_by_agent,
                rule.scope.as_str(),
                project_path_str,
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

    /// Removes all persisted rules matching *scope* (and optionally *project_path*
    /// when *scope* is `Project`).
    ///
    /// Returns the number of rows deleted.
    ///
    /// # Errors
    ///
    /// - [`PermissionError::InvalidRule`] if *scope* is `Session` (session rules
    ///   are not persisted).
    /// - [`PermissionError::Database`] on SQLite error.
    pub fn remove_rules_by_scope(
        &mut self,
        scope: PermissionScope,
        project_path: Option<&Path>,
    ) -> Result<u32, PermissionError> {
        if scope == PermissionScope::Session {
            return Err(PermissionError::InvalidRule(
                "session rules are not persisted; clear them via PermissionEngine::clear_session_rules"
                    .to_string(),
            ));
        }

        let affected = match (scope, project_path) {
            (PermissionScope::Project, Some(p)) => {
                let path_str = p.to_string_lossy().to_string();
                self.db.execute(
                    "DELETE FROM permission_rules WHERE scope = 'project' AND project_path = ?",
                    params![path_str],
                )?
            }
            (PermissionScope::Project, None) => self
                .db
                .execute("DELETE FROM permission_rules WHERE scope = 'project'", [])?,
            (PermissionScope::Agent, _) => self
                .db
                .execute("DELETE FROM permission_rules WHERE scope = 'agent'", [])?,
            (PermissionScope::Global, _) => self
                .db
                .execute("DELETE FROM permission_rules WHERE scope = 'global'", [])?,
            (PermissionScope::Session, _) => 0,
        };
        Ok(affected as u32)
    }

    /// Removes all `scope = 'agent'` rules matching `agent_id`.
    ///
    /// Returns the number of rows deleted.
    ///
    /// # Errors
    ///
    /// Returns [`PermissionError::Database`] on SQLite error.
    pub fn remove_rules_by_agent(&mut self, agent_id: &str) -> Result<u32, PermissionError> {
        let affected = self.db.execute(
            "DELETE FROM permission_rules WHERE scope = 'agent' AND agent_id = ?",
            params![agent_id],
        )?;
        Ok(affected as u32)
    }

    /// Removes all rules whose `created_by` field matches `created_by`.
    ///
    /// Used for audit or targeted reset operations (for example, clearing all
    /// rules written by a particular agent before it proposes new ones). `session`
    /// rules (in RAM) are unaffected since they are not persisted.
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

    /// Lists persisted rules whose `created_by` field matches.
    ///
    /// Returns rules sorted by ascending identifier. Does not include session
    /// rules (RAM only, never persisted).
    ///
    /// # Errors
    ///
    /// Returns [`PermissionError::Database`] on SQLite error.
    pub fn list_rules_by_creator(
        &self,
        created_by: &str,
    ) -> Result<Vec<PrefixRule>, PermissionError> {
        let mut stmt = self.db.prepare(
            "SELECT id, tool_name, arg_prefix, action, created_at, created_by, \
                    scope, project_path, agent_id, expires_at \
             FROM permission_rules \
             WHERE created_by = ? \
             ORDER BY id ASC",
        )?;
        let rules = stmt
            .query_map(params![created_by], row_to_rule)?
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .collect::<Result<Vec<_>, PermissionError>>()?;
        Ok(rules)
    }

    /// Lists `scope = 'agent'` rules filtered by `agent_id`.
    ///
    /// # Errors
    ///
    /// Returns [`PermissionError::Database`] on SQLite error.
    pub fn list_rules_for_agent(&self, agent_id: &str) -> Result<Vec<PrefixRule>, PermissionError> {
        let mut stmt = self.db.prepare(
            "SELECT id, tool_name, arg_prefix, action, created_at, created_by, \
                    scope, project_path, agent_id, expires_at \
             FROM permission_rules \
             WHERE scope = 'agent' AND agent_id = ? \
             ORDER BY id ASC",
        )?;
        let rules = stmt
            .query_map(params![agent_id], row_to_rule)?
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .collect::<Result<Vec<_>, PermissionError>>()?;
        Ok(rules)
    }

    /// Returns all persisted rules, sorted by ascending identifier.
    ///
    /// # Errors
    ///
    /// Returns [`PermissionError::Database`] on SQLite error.
    pub fn list_rules(&self) -> Result<Vec<PrefixRule>, PermissionError> {
        self.list_rules_filtered(None, None)
    }

    /// Returns persisted rules, optionally filtered by scope and project path.
    ///
    /// - `scope = Some(Project)` + `project_path = Some(p)`: rules for project `p`.
    /// - `scope = Some(Project)` + `project_path = None`: all `project` rules.
    /// - `scope = Some(Global)`: `global` rules.
    /// - `scope = None`: all persisted rules (project + global).
    ///
    /// `session` rules are never persisted and therefore do not appear here.
    ///
    /// # Errors
    ///
    /// Returns [`PermissionError::Database`] on SQLite error.
    pub fn list_rules_filtered(
        &self,
        scope: Option<PermissionScope>,
        project_path: Option<&Path>,
    ) -> Result<Vec<PrefixRule>, PermissionError> {
        let project_path_str = project_path.map(|p| p.to_string_lossy().to_string());

        let (sql, params): (String, Vec<rusqlite::types::Value>) = match (scope, &project_path_str)
        {
            (Some(PermissionScope::Session), _) => {
                // No persisted rule has scope=session.
                return Ok(Vec::new());
            }
            (Some(PermissionScope::Project), Some(p)) => (
                "SELECT id, tool_name, arg_prefix, action, created_at, created_by, \
                        scope, project_path, agent_id, expires_at \
                 FROM permission_rules \
                 WHERE scope = 'project' AND project_path = ? \
                 ORDER BY id ASC"
                    .to_string(),
                vec![rusqlite::types::Value::Text(p.clone())],
            ),
            (Some(PermissionScope::Project), None) => (
                "SELECT id, tool_name, arg_prefix, action, created_at, created_by, \
                        scope, project_path, agent_id, expires_at \
                 FROM permission_rules \
                 WHERE scope = 'project' \
                 ORDER BY id ASC"
                    .to_string(),
                Vec::new(),
            ),
            (Some(PermissionScope::Agent), _) => (
                "SELECT id, tool_name, arg_prefix, action, created_at, created_by, \
                        scope, project_path, agent_id, expires_at \
                 FROM permission_rules \
                 WHERE scope = 'agent' \
                 ORDER BY id ASC"
                    .to_string(),
                Vec::new(),
            ),
            (Some(PermissionScope::Global), _) => (
                "SELECT id, tool_name, arg_prefix, action, created_at, created_by, \
                        scope, project_path, agent_id, expires_at \
                 FROM permission_rules \
                 WHERE scope = 'global' \
                 ORDER BY id ASC"
                    .to_string(),
                Vec::new(),
            ),
            (None, _) => (
                "SELECT id, tool_name, arg_prefix, action, created_at, created_by, \
                        scope, project_path, agent_id, expires_at \
                 FROM permission_rules \
                 WHERE scope != 'session' \
                 ORDER BY id ASC"
                    .to_string(),
                Vec::new(),
            ),
        };

        let mut stmt = self.db.prepare(&sql)?;
        let rules = stmt
            .query_map(rusqlite::params_from_iter(params.iter()), row_to_rule)?
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .collect::<Result<Vec<_>, PermissionError>>()?;
        Ok(rules)
    }

    // ─────────────────────────────────────────────
    // Private
    // ─────────────────────────────────────────────

    fn match_in_db_project(
        &self,
        tool_name: &str,
        first_arg: Option<&str>,
        project_path: &Path,
        now: i64,
    ) -> Result<Option<(i64, RuleAction)>, PermissionError> {
        let path_str = project_path.to_string_lossy().to_string();
        let mut stmt = self.db.prepare_cached(
            "SELECT id, arg_prefix, action, expires_at FROM permission_rules \
             WHERE tool_name = ? AND scope = 'project' AND project_path = ? \
             ORDER BY CASE WHEN arg_prefix IS NULL THEN 0 ELSE LENGTH(arg_prefix) END DESC",
        )?;
        let mut rows = stmt.query(params![tool_name, path_str])?;
        scan_rows(&mut rows, tool_name, "project", first_arg, now)
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
        self.db.execute_batch(
            "CREATE TABLE IF NOT EXISTS permission_rules (
                id           INTEGER PRIMARY KEY AUTOINCREMENT,
                tool_name    TEXT NOT NULL,
                arg_prefix   TEXT,
                action       TEXT NOT NULL,
                created_at   INTEGER NOT NULL,
                created_by   TEXT,
                scope        TEXT NOT NULL DEFAULT 'global',
                project_path TEXT,
                expires_at   INTEGER
            );
            CREATE INDEX IF NOT EXISTS idx_rules_tool ON permission_rules(tool_name);
            CREATE INDEX IF NOT EXISTS idx_rules_scope_project
                ON permission_rules(scope, project_path);",
        )?;

        add_column_if_missing(
            &self.db,
            "permission_rules",
            "scope",
            "TEXT NOT NULL DEFAULT 'global'",
        )?;
        add_column_if_missing(&self.db, "permission_rules", "project_path", "TEXT")?;
        add_column_if_missing(&self.db, "permission_rules", "agent_id", "TEXT")?;
        add_column_if_missing(&self.db, "permission_rules", "expires_at", "INTEGER")?;

        Ok(())
    }
}

// ─────────────────────────────────────────────
// Free helpers
// ─────────────────────────────────────────────

fn current_unix_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn is_expired(expires_at: Option<i64>, now: i64) -> bool {
    matches!(expires_at, Some(deadline) if deadline <= now)
}

fn prefix_matches(tool_name: &str, arg_prefix: Option<&str>, first_arg: Option<&str>) -> bool {
    if crate::executor_guard::is_code_executor(tool_name) {
        return code_executor_prefix_matches(arg_prefix, first_arg);
    }
    match (arg_prefix, first_arg) {
        (None, _) => true,
        (Some(prefix), Some(arg)) => arg.starts_with(prefix),
        (Some(_), None) => false,
    }
}

/// Prefix matching restricted for arbitrary-code executors (`bash_executor`,
/// `python_executor`).
///
/// A no-prefix rule never grants a blanket allow over an entire interpreter,
/// and a prefix rule matches only when the argument is a single simple command
/// (no chaining/redirection/substitution), so an approved prefix cannot be
/// escaped by appending `; rm -rf ...`.
fn code_executor_prefix_matches(arg_prefix: Option<&str>, command: Option<&str>) -> bool {
    match (arg_prefix, command) {
        (None, _) => false,
        (Some(prefix), Some(cmd)) => {
            cmd.starts_with(prefix) && crate::executor_guard::is_single_simple_command(cmd)
        }
        (Some(_), None) => false,
    }
}

fn match_in_session(
    tool_name: &str,
    first_arg: Option<&str>,
    session_rules: &[PrefixRule],
    now: i64,
) -> Option<(i64, RuleAction)> {
    let mut candidates: Vec<&PrefixRule> = session_rules
        .iter()
        .filter(|r| r.tool_name == tool_name)
        .filter(|r| {
            if is_expired(r.expires_at, now) {
                tracing::warn!(
                    rule_id = r.id,
                    tool = %tool_name,
                    "expired session rule encountered - ignored"
                );
                false
            } else {
                true
            }
        })
        .collect();

    candidates.sort_by(|a, b| {
        let la = a.arg_prefix.as_deref().map(str::len).unwrap_or(0);
        let lb = b.arg_prefix.as_deref().map(str::len).unwrap_or(0);
        lb.cmp(&la)
    });

    for rule in candidates {
        if prefix_matches(tool_name, rule.arg_prefix.as_deref(), first_arg) {
            return Some((rule.id, rule.action.clone()));
        }
    }
    None
}

fn row_to_rule(row: &rusqlite::Row<'_>) -> rusqlite::Result<Result<PrefixRule, PermissionError>> {
    let id: i64 = row.get(0)?;
    let tool_name: String = row.get(1)?;
    let arg_prefix: Option<String> = row.get(2)?;
    let action_str: String = row.get(3)?;
    let created_at: i64 = row.get(4)?;
    let created_by_agent: Option<String> = row.get(5)?;
    let scope_str: String = row.get(6)?;
    let project_path_str: Option<String> = row.get(7)?;
    let agent_id: Option<String> = row.get(8)?;
    let expires_at: Option<i64> = row.get(9)?;

    Ok((|| -> Result<PrefixRule, PermissionError> {
        Ok(PrefixRule {
            id,
            tool_name,
            arg_prefix,
            action: RuleAction::from_str(&action_str)?,
            created_at,
            created_by_agent,
            scope: PermissionScope::from_db_str(&scope_str)?,
            project_path: project_path_str.map(PathBuf::from),
            agent_id,
            expires_at,
        })
    })())
}

fn scan_rows(
    rows: &mut rusqlite::Rows<'_>,
    tool_name: &str,
    scope: &str,
    first_arg: Option<&str>,
    now: i64,
) -> Result<Option<(i64, RuleAction)>, PermissionError> {
    while let Some(row) = rows.next()? {
        let id: i64 = row.get(0)?;
        let arg_prefix: Option<String> = row.get(1)?;
        let action_str: String = row.get(2)?;
        let expires_at: Option<i64> = row.get(3)?;

        if is_expired(expires_at, now) {
            tracing::warn!(
                rule_id = id,
                tool = %tool_name,
                scope = %scope,
                "expired prefix rule encountered - ignored"
            );
            continue;
        }

        let action = RuleAction::from_str(&action_str)?;
        if prefix_matches(tool_name, arg_prefix.as_deref(), first_arg) {
            tracing::debug!(
                tool = %tool_name,
                rule_id = id,
                scope = %scope,
                action = ?action,
                "prefix rule matched"
            );
            return Ok(Some((id, action)));
        }
    }
    Ok(None)
}

// ─────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
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
        let (mut engine, _tmp) = tmp_engine();
        engine
            .add_rule(&rule("bash_executor", Some("git"), RuleAction::Allow))
            .expect("add_rule");
        let result = engine
            .check("bash_executor", Some("git push origin main"))
            .expect("check");
        assert_eq!(result, Some(RuleAction::Allow));
    }

    #[test]
    fn prefix_matches_rejects_non_matching_and_missing_arg() {
        // A concrete prefix that the first argument does not start with must not
        // match, and a required prefix with no argument must not match either.
        // Pins the function against a mutant that unconditionally returns true,
        // which would make a scoped rule apply to every call.
        assert!(!prefix_matches("file_read", Some("git"), Some("rm -rf /")));
        assert!(!prefix_matches("file_read", Some("git"), None));

        // The matching and wildcard (no-prefix) cases still hold for an
        // ordinary, argument-scoped tool.
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
        let (engine, _tmp) = tmp_engine();
        let result = engine
            .check("bash_executor", Some("git status"))
            .expect("check");
        assert!(result.is_none());
    }

    #[test]
    fn prefix_rule_deny_action_returned() {
        let (mut engine, _tmp) = tmp_engine();
        engine
            .add_rule(&rule("file_write", Some("/etc"), RuleAction::Deny))
            .expect("add_rule");
        let result = engine
            .check("file_write", Some("/etc/passwd"))
            .expect("check");
        assert_eq!(result, Some(RuleAction::Deny));
    }

    #[test]
    fn prefix_rule_none_prefix_matches_any_arg() {
        let (mut engine, _tmp) = tmp_engine();
        engine
            .add_rule(&rule("file_read", None, RuleAction::Allow))
            .expect("add_rule");
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
        let (mut engine, _tmp) = tmp_engine();
        let id = engine
            .add_rule(&rule("bash_executor", Some("git"), RuleAction::Allow))
            .expect("add_rule");
        engine.remove_rule(id).expect("remove_rule");
        let result = engine
            .check("bash_executor", Some("git status"))
            .expect("check");
        assert!(result.is_none());
    }

    #[test]
    fn remove_rule_checked_reports_existence() {
        let (mut engine, _tmp) = tmp_engine();
        let id = engine
            .add_rule(&rule("bash_executor", None, RuleAction::Allow))
            .expect("add_rule");
        assert!(engine.remove_rule_checked(id).expect("remove_rule_checked"));
        assert!(!engine.remove_rule_checked(id).expect("remove_rule_checked"));
    }

    #[test]
    fn list_rules_returns_all() {
        let (mut engine, _tmp) = tmp_engine();
        engine
            .add_rule(&rule("tool_a", Some("prefix_x"), RuleAction::Allow))
            .expect("add_rule");
        engine
            .add_rule(&rule("tool_b", Some("prefix_y"), RuleAction::Allow))
            .expect("add_rule");
        let rules = engine.list_rules().expect("list_rules");
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
    fn list_rules_by_creator_returns_only_matching() {
        // GIVEN three rules: two by "onboarding-agent", one by "user-hitl"
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

        // WHEN listing by creator
        let rules = engine
            .list_rules_by_creator("onboarding-agent")
            .expect("list_rules_by_creator");

        // THEN only the two onboarding-agent rules are returned
        assert_eq!(rules.len(), 2);
        assert!(rules
            .iter()
            .all(|r| r.created_by_agent.as_deref() == Some("onboarding-agent")));
    }

    #[test]
    fn list_rules_by_creator_unknown_returns_empty() {
        // GIVEN an empty engine
        let (engine, _tmp) = tmp_engine();
        // WHEN listing an unknown creator
        let rules = engine
            .list_rules_by_creator("ghost")
            .expect("list_rules_by_creator");
        // THEN no rules
        assert!(rules.is_empty());
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
        let r = PrefixRule {
            tool_name: String::new(),
            ..PrefixRule::default()
        };
        assert!(matches!(
            engine.add_rule(&r),
            Err(PermissionError::InvalidRule(_))
        ));
    }

    #[test]
    fn add_rule_rejects_session_scope() {
        let (mut engine, _tmp) = tmp_engine();
        let r = PrefixRule {
            tool_name: "bash_executor".into(),
            scope: PermissionScope::Session,
            ..PrefixRule::default()
        };
        assert!(matches!(
            engine.add_rule(&r),
            Err(PermissionError::InvalidRule(_))
        ));
    }

    #[test]
    fn add_rule_rejects_project_scope_without_path() {
        let (mut engine, _tmp) = tmp_engine();
        let r = PrefixRule {
            tool_name: "bash_executor".into(),
            scope: PermissionScope::Project,
            project_path: None,
            ..PrefixRule::default()
        };
        assert!(matches!(
            engine.add_rule(&r),
            Err(PermissionError::InvalidRule(_))
        ));
    }

    #[test]
    fn test_session_rule_found_in_memory() {
        let (engine, _tmp) = tmp_engine();
        let session = vec![PrefixRule {
            id: 7,
            tool_name: "bash_executor".into(),
            arg_prefix: Some("git".into()),
            action: RuleAction::Allow,
            scope: PermissionScope::Session,
            ..PrefixRule::default()
        }];
        let ctx = ScopeContext::default();
        let hit = engine
            .check_with_scope("bash_executor", Some("git status"), &ctx, &session)
            .expect("check_with_scope");
        assert_eq!(hit, Some((7, RuleAction::Allow)));
    }

    #[test]
    fn test_agent_rule_filtered_by_id() {
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
        let ctx_other = ScopeContext {
            agent_id: Some("apollia:other".into()),
            ..ScopeContext::default()
        };
        let hit = engine
            .check_with_scope("bash_executor", Some("git status"), &ctx_other, &[])
            .expect("check_with_scope");
        assert!(
            hit.is_none(),
            "rule for apollia:chat must not match apollia:other"
        );

        let ctx_chat = ScopeContext {
            agent_id: Some("apollia:chat".into()),
            ..ScopeContext::default()
        };
        let hit = engine
            .check_with_scope("bash_executor", Some("git status"), &ctx_chat, &[])
            .expect("check_with_scope");
        assert!(matches!(hit, Some((_, RuleAction::Allow))));
    }

    #[test]
    fn test_add_rule_rejects_agent_scope_without_id() {
        let (mut engine, _tmp) = tmp_engine();
        let r = PrefixRule {
            tool_name: "bash_executor".into(),
            scope: PermissionScope::Agent,
            agent_id: None,
            ..PrefixRule::default()
        };
        assert!(matches!(
            engine.add_rule(&r),
            Err(PermissionError::InvalidRule(_))
        ));
    }

    #[test]
    fn test_project_rule_filtered_by_path() {
        let (mut engine, _tmp) = tmp_engine();
        let project_a = PathBuf::from("/home/user/projet-a");
        engine
            .add_rule(&PrefixRule {
                tool_name: "bash_executor".into(),
                arg_prefix: Some("git".into()),
                action: RuleAction::Allow,
                created_at: now(),
                scope: PermissionScope::Project,
                project_path: Some(project_a.clone()),
                ..PrefixRule::default()
            })
            .expect("add_rule");
        let ctx_b = ScopeContext {
            scope: PermissionScope::Project,
            project_path: Some(PathBuf::from("/home/user/projet-b")),
            agent_id: None,
        };
        let hit = engine
            .check_with_scope("bash_executor", Some("git status"), &ctx_b, &[])
            .expect("check_with_scope");
        assert!(hit.is_none(), "rule from projet-a must not match projet-b");

        let ctx_a = ScopeContext {
            scope: PermissionScope::Project,
            project_path: Some(project_a),
            agent_id: None,
        };
        let hit = engine
            .check_with_scope("bash_executor", Some("git status"), &ctx_a, &[])
            .expect("check_with_scope");
        assert!(matches!(hit, Some((_, RuleAction::Allow))));
    }

    #[test]
    fn test_global_rule_applies_everywhere() {
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
        let ctx = ScopeContext {
            scope: PermissionScope::Project,
            project_path: Some(PathBuf::from("/home/user/anywhere")),
            agent_id: None,
        };
        let hit = engine
            .check_with_scope("bash_executor", Some("git status"), &ctx, &[])
            .expect("check_with_scope");
        assert!(matches!(hit, Some((_, RuleAction::Allow))));
    }

    #[test]
    fn test_expired_rule_ignored() {
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
        let ctx = ScopeContext {
            scope: PermissionScope::Global,
            project_path: None,
            agent_id: None,
        };
        let hit = engine
            .check_with_scope("bash_executor", Some("git status"), &ctx, &[])
            .expect("check_with_scope");
        assert!(hit.is_none());
        let hit_legacy = engine
            .check("bash_executor", Some("git status"))
            .expect("check");
        assert!(hit_legacy.is_none());
    }

    #[test]
    fn test_scope_priority_project_over_agent_over_session_over_global() {
        // Expected order: Project > Agent > Session > Global.
        let (mut engine, _tmp) = tmp_engine();
        let project = PathBuf::from("/home/user/projet");
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
                action: RuleAction::Deny,
                created_at: now(),
                scope: PermissionScope::Agent,
                agent_id: Some("apollia:chat".into()),
                ..PrefixRule::default()
            })
            .expect("add_rule agent");
        let project_id_rule = engine
            .add_rule(&PrefixRule {
                tool_name: "bash_executor".into(),
                arg_prefix: Some("git".into()),
                action: RuleAction::Allow,
                created_at: now(),
                scope: PermissionScope::Project,
                project_path: Some(project.clone()),
                ..PrefixRule::default()
            })
            .expect("add_rule project");

        let session = vec![PrefixRule {
            id: 99,
            tool_name: "bash_executor".into(),
            arg_prefix: Some("git".into()),
            action: RuleAction::Deny,
            scope: PermissionScope::Session,
            ..PrefixRule::default()
        }];

        // Project wins when project_path is set.
        let ctx = ScopeContext {
            scope: PermissionScope::Project,
            project_path: Some(project),
            agent_id: Some("apollia:chat".into()),
        };
        let hit = engine
            .check_with_scope("bash_executor", Some("git status"), &ctx, &session)
            .expect("check_with_scope");
        assert_eq!(hit, Some((project_id_rule, RuleAction::Allow)));

        // Without a project, Agent wins over Session and Global.
        let ctx_no_project = ScopeContext {
            scope: PermissionScope::Agent,
            project_path: None,
            agent_id: Some("apollia:chat".into()),
        };
        let hit = engine
            .check_with_scope(
                "bash_executor",
                Some("git status"),
                &ctx_no_project,
                &session,
            )
            .expect("check_with_scope");
        assert_eq!(hit, Some((agent_id_rule, RuleAction::Deny)));

        // Without a project or agent, Session wins over Global.
        let ctx_bare = ScopeContext::default();
        let hit = engine
            .check_with_scope("bash_executor", Some("git status"), &ctx_bare, &session)
            .expect("check_with_scope");
        assert_eq!(hit, Some((99, RuleAction::Deny)));

        // With nothing else, Global wins as a last resort.
        let hit = engine
            .check_with_scope("bash_executor", Some("git status"), &ctx_bare, &[])
            .expect("check_with_scope");
        assert_eq!(hit, Some((global_id, RuleAction::Deny)));
    }

    #[test]
    fn test_backward_compat_check_without_scope() {
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
        let result = engine
            .check("bash_executor", Some("git push"))
            .expect("check");
        assert_eq!(result, Some(RuleAction::Allow));
    }

    #[test]
    fn list_rules_filtered_by_scope_and_path() {
        let (mut engine, _tmp) = tmp_engine();
        let project = PathBuf::from("/home/user/projet-a");
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
                tool_name: "tool_p".into(),
                action: RuleAction::Allow,
                created_at: now(),
                scope: PermissionScope::Project,
                project_path: Some(project.clone()),
                ..PrefixRule::default()
            })
            .expect("add_rule");

        let globals = engine
            .list_rules_filtered(Some(PermissionScope::Global), None)
            .expect("list globals");
        assert_eq!(globals.len(), 1);
        assert_eq!(globals[0].tool_name, "tool_g");

        let projects = engine
            .list_rules_filtered(Some(PermissionScope::Project), Some(&project))
            .expect("list project");
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].tool_name, "tool_p");

        let none_for_session = engine
            .list_rules_filtered(Some(PermissionScope::Session), None)
            .expect("list session");
        assert!(none_for_session.is_empty());
    }
}
