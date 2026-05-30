//! Permission engine layer 1: operator SafeList.
//!
//! The SafeList holds commands auto-approved without HITL, configured
//! explicitly by the operator in `apollia.toml`. It is empty by default:
//! no command is auto-approved without explicit configuration (least
//! privilege, OWASP ASVS V1.4).
//!
//! Pattern format: `"tool_name(arg_text)"` or `"tool_name"`.
//! - `"bash_executor(git status)"`: approves bash_executor only when first_arg == "git status".
//! - `"bash_executor"`: approves bash_executor for any argument.

use apollia_core::config::PermissionsConfig;

// ─────────────────────────────────────────────
// SafePattern
// ─────────────────────────────────────────────

/// SafeList pattern parsed from the operator configuration.
///
/// Represents a `"tool_name(arg_text)"` or `"tool_name"` entry.
#[derive(Debug, Clone)]
struct SafePattern {
    /// Targeted tool name.
    tool_name: String,
    /// Exact argument to match (None means any argument is accepted).
    arg: Option<String>,
}

impl SafePattern {
    /// Parses a configuration string into a `SafePattern`.
    ///
    /// Expected format:
    /// - `"bash_executor(git status)"` gives tool="bash_executor", arg=Some("git status")
    /// - `"bash_executor"` gives tool="bash_executor", arg=None
    fn parse(s: &str) -> Self {
        if let Some(open_pos) = s.find('(') {
            let tool_name = s[..open_pos].trim().to_string();
            let rest = &s[open_pos + 1..];
            let arg = if let Some(close_pos) = rest.rfind(')') {
                rest[..close_pos].to_string()
            } else {
                rest.to_string()
            };
            Self {
                tool_name,
                arg: Some(arg),
            }
        } else {
            Self {
                tool_name: s.trim().to_string(),
                arg: None,
            }
        }
    }

    /// Returns `true` if this pattern matches the invocation (`tool_name`, `first_arg`).
    fn matches(&self, tool_name: &str, first_arg: Option<&str>) -> bool {
        if self.tool_name != tool_name {
            return false;
        }
        match (&self.arg, first_arg) {
            // No argument filter: any argument is accepted.
            (None, _) => true,
            // Argument filter: the argument must match exactly.
            (Some(expected), Some(actual)) => actual == expected.as_str(),
            // Argument filter but no argument supplied: deny.
            (Some(_), None) => false,
        }
    }
}

// ─────────────────────────────────────────────
// SafeList
// ─────────────────────────────────────────────

/// Operator-configured list of auto-approved invocations.
///
/// Empty by default: the operator explicitly defines what is safe (least
/// privilege, OWASP ASVS V1.4, CWE-272). Built from
/// [`PermissionsConfig::safe_commands`] at runtime startup.
pub struct SafeList {
    patterns: Vec<SafePattern>,
}

impl SafeList {
    /// Builds a `SafeList` from the `[permissions]` configuration section.
    ///
    /// Invalid patterns (empty strings) are silently ignored.
    pub fn from_config(config: &PermissionsConfig) -> Self {
        let patterns = config
            .safe_commands
            .iter()
            .filter(|s| !s.trim().is_empty())
            .map(|s| SafePattern::parse(s))
            .collect();
        Self { patterns }
    }

    /// Builds an empty `SafeList` (conservative default behavior).
    pub fn empty() -> Self {
        Self {
            patterns: Vec::new(),
        }
    }

    /// Returns `true` if the invocation (`tool_name`, `first_arg`) is allow-listed.
    pub fn matches(&self, tool_name: &str, first_arg: Option<&str>) -> bool {
        self.patterns
            .iter()
            .any(|p| p.matches(tool_name, first_arg))
    }

    /// Returns the parsed patterns as `(tool_name, arg)` pairs to enable
    /// migration into `governance.db` at engine startup.
    ///
    /// Used exclusively by `PermissionEngine::new()` to ingest the TOML
    /// `SafeList` into the `permission_rules` table with
    /// `created_by="config-import"`.
    pub fn parsed_patterns(&self) -> Vec<(String, Option<String>)> {
        self.patterns
            .iter()
            .map(|p| (p.tool_name.clone(), p.arg.clone()))
            .collect()
    }
}

// ─────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_core::config::PermissionsConfig;
    use std::path::PathBuf;

    fn config_with_safe_commands(cmds: Vec<&str>) -> PermissionsConfig {
        PermissionsConfig {
            safe_commands: cmds.into_iter().map(String::from).collect(),
            injection_detection: true,
            prefix_rule_ttl_hours: 168,
            db_path: PathBuf::from("/tmp/test_permissions.db"),
        }
    }

    #[test]
    fn safe_list_matches_configured_command() {
        let config = config_with_safe_commands(vec!["bash_executor(git status)"]);
        let list = SafeList::from_config(&config);
        let result = list.matches("bash_executor", Some("git status"));
        assert!(result);
    }

    #[test]
    fn safe_list_empty_by_default_rejects_all() {
        let list = SafeList::empty();
        let result = list.matches("bash_executor", Some("git status"));
        assert!(!result);
    }

    #[test]
    fn safe_list_rejects_unconfigured_command() {
        let config = config_with_safe_commands(vec!["bash_executor(git status)"]);
        let list = SafeList::from_config(&config);
        let result = list.matches("bash_executor", Some("rm -rf /"));
        assert!(!result);
    }

    #[test]
    fn safe_list_tool_without_arg_filter_matches_any_arg() {
        let config = config_with_safe_commands(vec!["file_read"]);
        let list = SafeList::from_config(&config);
        assert!(list.matches("file_read", Some("any/path.txt")));
        assert!(list.matches("file_read", None));
    }

    #[test]
    fn safe_list_rejects_different_tool_name() {
        let config = config_with_safe_commands(vec!["bash_executor(git status)"]);
        let list = SafeList::from_config(&config);
        let result = list.matches("file_read", Some("git status"));
        assert!(!result);
    }

    #[test]
    fn safe_list_from_config_ignores_empty_patterns() {
        let config = config_with_safe_commands(vec!["", "  ", "bash_executor(pwd)"]);
        let list = SafeList::from_config(&config);
        // WHEN
        assert!(list.matches("bash_executor", Some("pwd")));
        assert!(!list.matches("bash_executor", Some("rm -rf /")));
    }
}
