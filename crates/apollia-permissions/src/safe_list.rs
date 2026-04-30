//! Couche 1 du moteur de permissions — SafeList opérateur.
//!
//! La SafeList contient les commandes auto-approuvées sans HITL,
//! configurées explicitement par l'opérateur dans `apollia.toml`.
//! Elle est **vide par défaut** : aucune commande n'est auto-approuvée sans
//! configuration explicite (principe de moindre privilège, OWASP ASVS V1.4).
//!
//! Format de pattern : `"tool_name(arg_text)"` ou `"tool_name"`.
//! - `"bash_executor(git status)"` : approuve bash_executor uniquement quand first_arg == "git status".
//! - `"bash_executor"` : approuve bash_executor pour tout argument.

use apollia_core::config::PermissionsConfig;

// ─────────────────────────────────────────────
// SafePattern
// ─────────────────────────────────────────────

/// Pattern SafeList parsé depuis la configuration opérateur.
///
/// Représente une entrée `"tool_name(arg_text)"` ou `"tool_name"`.
#[derive(Debug, Clone)]
struct SafePattern {
    /// Nom de l'outil ciblé.
    tool_name: String,
    /// Argument exact à matcher (None = tout argument accepté).
    arg: Option<String>,
}

impl SafePattern {
    /// Parse une chaîne de configuration en `SafePattern`.
    ///
    /// Format attendu :
    /// - `"bash_executor(git status)"` → tool="bash_executor", arg=Some("git status")
    /// - `"bash_executor"` → tool="bash_executor", arg=None
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

    /// Retourne `true` si ce pattern correspond à l'invocation (`tool_name`, `first_arg`).
    fn matches(&self, tool_name: &str, first_arg: Option<&str>) -> bool {
        if self.tool_name != tool_name {
            return false;
        }
        match (&self.arg, first_arg) {
            // Pattern sans filtre d'argument → tout argument accepté.
            (None, _) => true,
            // Pattern avec filtre : l'argument doit correspondre exactement.
            (Some(expected), Some(actual)) => actual == expected.as_str(),
            // Pattern avec filtre mais aucun argument fourni → refus.
            (Some(_), None) => false,
        }
    }
}

// ─────────────────────────────────────────────
// SafeList
// ─────────────────────────────────────────────

/// Liste des invocations auto-approuvées configurée par l'opérateur.
///
/// Vide par défaut — l'opérateur définit explicitement ce qui est sûr
/// (principe de moindre privilège, OWASP ASVS V1.4, CWE-272).
/// Construite depuis [`PermissionsConfig::safe_commands`] au démarrage du runtime.
pub struct SafeList {
    patterns: Vec<SafePattern>,
}

impl SafeList {
    /// Construit une `SafeList` depuis la section `[permissions]` de la configuration.
    ///
    /// Les patterns invalides (chaînes vides) sont silencieusement ignorés.
    pub fn from_config(config: &PermissionsConfig) -> Self {
        let patterns = config
            .safe_commands
            .iter()
            .filter(|s| !s.trim().is_empty())
            .map(|s| SafePattern::parse(s))
            .collect();
        Self { patterns }
    }

    /// Construit une `SafeList` vide (comportement par défaut conservateur).
    pub fn empty() -> Self {
        Self {
            patterns: Vec::new(),
        }
    }

    /// Retourne `true` si l'invocation (`tool_name`, `first_arg`) est sur la liste blanche.
    pub fn matches(&self, tool_name: &str, first_arg: Option<&str>) -> bool {
        self.patterns
            .iter()
            .any(|p| p.matches(tool_name, first_arg))
    }

    /// Retourne les patterns parsés sous forme `(tool_name, arg)` pour permettre
    /// la migration vers `governance.db` au démarrage du moteur.
    ///
    /// Utilisé exclusivement par `PermissionEngine::new()` pour ingérer la `SafeList`
    /// TOML dans la table `permission_rules` avec `created_by="config-import"`
    /// (cf. ADR-086).
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
        // GIVEN une SafeList configurée avec "bash_executor(git status)"
        let config = config_with_safe_commands(vec!["bash_executor(git status)"]);
        let list = SafeList::from_config(&config);
        // WHEN
        let result = list.matches("bash_executor", Some("git status"));
        // THEN
        assert!(result);
    }

    #[test]
    fn safe_list_empty_by_default_rejects_all() {
        // GIVEN une SafeList vide (défaut — aucune config)
        let list = SafeList::empty();
        // WHEN
        let result = list.matches("bash_executor", Some("git status"));
        // THEN
        assert!(!result);
    }

    #[test]
    fn safe_list_rejects_unconfigured_command() {
        // GIVEN une SafeList configurée avec "bash_executor(git status)"
        let config = config_with_safe_commands(vec!["bash_executor(git status)"]);
        let list = SafeList::from_config(&config);
        // WHEN
        let result = list.matches("bash_executor", Some("rm -rf /"));
        // THEN
        assert!(!result);
    }

    #[test]
    fn safe_list_tool_without_arg_filter_matches_any_arg() {
        // GIVEN un pattern sans filtre d'argument
        let config = config_with_safe_commands(vec!["file_read"]);
        let list = SafeList::from_config(&config);
        // WHEN / THEN
        assert!(list.matches("file_read", Some("any/path.txt")));
        assert!(list.matches("file_read", None));
    }

    #[test]
    fn safe_list_rejects_different_tool_name() {
        // GIVEN une SafeList pour bash_executor seulement
        let config = config_with_safe_commands(vec!["bash_executor(git status)"]);
        let list = SafeList::from_config(&config);
        // WHEN on vérifie un outil différent
        let result = list.matches("file_read", Some("git status"));
        // THEN
        assert!(!result);
    }

    #[test]
    fn safe_list_from_config_ignores_empty_patterns() {
        // GIVEN une config avec des entrées vides
        let config = config_with_safe_commands(vec!["", "  ", "bash_executor(pwd)"]);
        let list = SafeList::from_config(&config);
        // WHEN
        assert!(list.matches("bash_executor", Some("pwd")));
        assert!(!list.matches("bash_executor", Some("rm -rf /")));
    }
}
