//! Prefix matching and row decoding.
//!
//! Split out of `prefix_rule_engine.rs`: the engine and its SQLite handle stay
//! in the parent, the pure predicates it consults on every decision live here.

use crate::error::PermissionError;

use crate::prefix_rule_engine::{PermissionScope, PrefixRule, RuleAction};

pub(super) fn current_unix_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
pub(super) fn is_expired(expires_at: Option<i64>, now: i64) -> bool {
    matches!(expires_at, Some(deadline) if deadline <= now)
}
pub(super) fn prefix_matches(
    tool_name: &str,
    arg_prefix: Option<&str>,
    first_arg: Option<&str>,
) -> bool {
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
pub(super) fn code_executor_prefix_matches(
    arg_prefix: Option<&str>,
    command: Option<&str>,
) -> bool {
    match (arg_prefix, command) {
        (None, _) => false,
        (Some(prefix), Some(cmd)) => {
            cmd.starts_with(prefix) && crate::executor_guard::is_single_simple_command(cmd)
        }
        (Some(_), None) => false,
    }
}
/// Whether `rule` decides a call of `tool_name` with `first_arg` made by
/// `agent_id` at Unix time `now`: same tool, a rule for every agent or for
/// this one, not expired, and a prefix that matches the argument, with the
/// stricter matching of the code executors.
pub fn rule_matches(
    rule: &PrefixRule,
    tool_name: &str,
    first_arg: Option<&str>,
    agent_id: &str,
    now: i64,
) -> bool {
    let reaches_agent = match rule.scope {
        PermissionScope::Global => true,
        PermissionScope::Agent => rule.agent_id.as_deref() == Some(agent_id),
    };
    rule.tool_name == tool_name
        && reaches_agent
        && !is_expired(rule.expires_at, now)
        && prefix_matches(tool_name, rule.arg_prefix.as_deref(), first_arg)
}
pub(super) fn row_to_rule(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<Result<PrefixRule, PermissionError>> {
    let id: i64 = row.get(0)?;
    let tool_name: String = row.get(1)?;
    let arg_prefix: Option<String> = row.get(2)?;
    let action_str: String = row.get(3)?;
    let created_at: i64 = row.get(4)?;
    let created_by_agent: Option<String> = row.get(5)?;
    let scope_str: String = row.get(6)?;
    let agent_id: Option<String> = row.get(7)?;
    let expires_at: Option<i64> = row.get(8)?;

    Ok((|| -> Result<PrefixRule, PermissionError> {
        Ok(PrefixRule {
            id,
            tool_name,
            arg_prefix,
            action: RuleAction::from_str(&action_str)?,
            created_at,
            created_by_agent,
            scope: PermissionScope::from_db_str(&scope_str)?,
            agent_id,
            expires_at,
        })
    })())
}
pub(super) fn scan_rows(
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
                "permission.prefix_rule.expired.ignored"
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
                "permission.prefix_rule.matched"
            );
            return Ok(Some((id, action)));
        }
    }
    Ok(None)
}
/// Extracts the first string argument from the JSON input.
///
/// Strategy: try the common native-tool keys first
/// (`cmd`, `command`, `path`, `url`, `query`, `input`, `text`, `content`,
/// `prompt`), then take the first string value found in the object.
/// If the input is itself a string, return it as is.
///
/// Public because the chat ReAct loop performs the same extraction before it
/// consults the prefix rules per invocation: the caller and the rule store
/// must agree on which argument a rule's prefix is matched against.
pub fn extract_first_arg(input: &serde_json::Value) -> Option<String> {
    use serde_json::Value;

    match input {
        Value::String(s) => Some(s.clone()),
        Value::Object(map) => {
            // Try the common native-tool keys first.
            const PRIORITY_KEYS: &[&str] = &[
                "cmd", "command", "path", "url", "query", "input", "text", "content", "prompt",
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
