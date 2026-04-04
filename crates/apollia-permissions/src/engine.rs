//! Moteur de permissions 3 couches — point d'entrée principal.
//!
//! `PermissionEngine::decide()` évalue chaque invocation d'outil dans l'ordre suivant :
//!
//! 1. **Couche 3 — InjectionDetector** (priorité absolue, bloquant)
//!    Vérifie tous les arguments string pour détecter des patterns shell dangereux.
//!
//! 2. **Couche 1 — SafeList** (config opérateur, vide par défaut)
//!    Auto-approuve les invocations explicitement configurées par l'opérateur.
//!
//! 3. **Couche 2 — PrefixRuleEngine** (règles SQLite persistées)
//!    Auto-approuve ou auto-refuse selon les règles enregistrées par l'opérateur
//!    ou le bouton "Toujours autoriser" HITL desktop.
//!
//! 4. **Fallback** → `NeedsApproval`
//!    Toute invocation sans correspondance demande une approbation humaine.
//!
//! Toutes les décisions sont persistées dans `PermissionAuditLog` (immuable).

use std::path::Path;

use apollia_core::config::PermissionsConfig;
use apollia_core::manifest::AgentManifest;
use serde_json::Value;

use crate::audit_log::PermissionAuditLog;
use crate::error::PermissionError;
use crate::injection_detector::InjectionDetector;
use crate::prefix_rule_engine::PrefixRuleEngine;
use crate::safe_list::SafeList;

// ─────────────────────────────────────────────
// PermissionDecision
// ─────────────────────────────────────────────

/// Décision émise par `PermissionEngine::decide()`.
///
/// Le runtime utilise cette décision pour émettre un événement
/// `RuntimeEvent::PermissionRequired` (NeedsApproval) ou retourner
/// `ToolError::PermissionDenied` (AutoDenied*).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionDecision {
    /// Invocation auto-approuvée par la SafeList opérateur (couche 1).
    AutoAllowedSafeList,
    /// Invocation auto-approuvée par une règle préfixe SQLite (couche 2).
    AutoAllowedPrefixRule {
        /// Identifiant de la règle ayant déclenché l'approbation.
        rule_id: i64,
    },
    /// Invocation auto-refusée par une règle préfixe SQLite (couche 2).
    AutoDeniedPrefixRule {
        /// Identifiant de la règle ayant déclenché le refus.
        rule_id: i64,
    },
    /// Invocation bloquée par détection d'injection shell (couche 3).
    AutoDeniedInjection {
        /// Nom du pattern d'injection détecté (ex : `";"`, `"$("`, ...).
        pattern: String,
    },
    /// Aucune couche n'a tranché — l'approbation humaine est requise.
    NeedsApproval,
}

// ─────────────────────────────────────────────
// PermissionEngine
// ─────────────────────────────────────────────

/// Moteur de permissions 3 couches.
///
/// Doit être instancié une fois par runtime et partagé via `Arc<Mutex<PermissionEngine>>`
/// lorsque plusieurs acteurs en ont besoin concurrentiellement.
pub struct PermissionEngine {
    safe_list: SafeList,
    prefix_rules: PrefixRuleEngine,
    injection_detector: InjectionDetector,
    audit_log: PermissionAuditLog,
    injection_detection_enabled: bool,
}

impl PermissionEngine {
    /// Construit un `PermissionEngine` depuis la configuration et le chemin SQLite.
    ///
    /// Le même fichier SQLite est utilisé par le `PrefixRuleEngine` et le `PermissionAuditLog`.
    ///
    /// # Errors
    ///
    /// - [`PermissionError::Database`] si l'initialisation SQLite échoue.
    /// - [`PermissionError::Regex`] si la compilation des patterns regex échoue.
    pub fn new(config: &PermissionsConfig, db_path: &Path) -> Result<Self, PermissionError> {
        Ok(Self {
            safe_list: SafeList::from_config(config),
            prefix_rules: PrefixRuleEngine::new(db_path)?,
            injection_detector: InjectionDetector::new()?,
            audit_log: PermissionAuditLog::new(db_path)?,
            injection_detection_enabled: config.injection_detection,
        })
    }

    /// Évalue les 3 couches de permission pour une invocation d'outil.
    ///
    /// Ordre d'évaluation :
    /// 1. InjectionDetector (couche 3, priorité absolue)
    /// 2. SafeList (couche 1, config opérateur)
    /// 3. PrefixRuleEngine (couche 2, règles SQLite)
    /// 4. NeedsApproval (fallback)
    ///
    /// La décision est systématiquement enregistrée dans l'audit log.
    ///
    /// # Errors
    ///
    /// - [`PermissionError::Database`] si l'audit log ne peut pas être écrit.
    pub fn decide(
        &mut self,
        tool_name: &str,
        input: &Value,
        _agent_manifest: &AgentManifest,
    ) -> Result<PermissionDecision, PermissionError> {
        let first_arg = extract_first_arg(input);

        // ── Couche 3 : InjectionDetector (priorité absolue) ─────────────────
        if self.injection_detection_enabled {
            let suspicious_value = find_suspicious_string(input, &self.injection_detector);
            if let Some(pattern) = suspicious_value {
                let decision = PermissionDecision::AutoDeniedInjection {
                    pattern: pattern.clone(),
                };
                tracing::warn!(
                    tool = %tool_name,
                    injection_pattern = %pattern,
                    "injection detected — invocation blocked"
                );
                self.audit_log
                    .record(tool_name, first_arg.as_deref(), &decision)?;
                return Ok(decision);
            }
        }

        // ── Couche 1 : SafeList ──────────────────────────────────────────────
        if self.safe_list.matches(tool_name, first_arg.as_deref()) {
            let decision = PermissionDecision::AutoAllowedSafeList;
            tracing::debug!(tool = %tool_name, "auto-allowed by safe list");
            self.audit_log
                .record(tool_name, first_arg.as_deref(), &decision)?;
            return Ok(decision);
        }

        // ── Couche 2 : PrefixRuleEngine ──────────────────────────────────────
        if let Some((rule_id, action)) = self
            .prefix_rules
            .check_with_id(tool_name, first_arg.as_deref())?
        {
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

        // ── Fallback : NeedsApproval ─────────────────────────────────────────
        let decision = PermissionDecision::NeedsApproval;
        tracing::debug!(tool = %tool_name, "needs human approval");
        self.audit_log
            .record(tool_name, first_arg.as_deref(), &decision)?;
        Ok(decision)
    }

    /// Expose le `PrefixRuleEngine` pour permettre l'ajout de règles depuis l'extérieur
    /// (par exemple, via le bouton "Toujours autoriser" HITL desktop).
    pub fn prefix_rules_mut(&mut self) -> &mut PrefixRuleEngine {
        &mut self.prefix_rules
    }

    /// Expose le `PermissionAuditLog` en lecture seule pour les requêtes d'audit.
    pub fn audit_log(&self) -> &PermissionAuditLog {
        &self.audit_log
    }
}

// ─────────────────────────────────────────────
// Helpers privés
// ─────────────────────────────────────────────

/// Extrait le premier argument string de l'input JSON.
///
/// Stratégie : essaie d'abord les clés courantes des outils natifs
/// (`cmd`, `command`, `path`, `query`, `input`, `text`, `content`, `prompt`),
/// puis prend la première valeur string trouvée dans l'objet.
/// Si l'input est directement une string, la retourne telle quelle.
fn extract_first_arg(input: &Value) -> Option<String> {
    match input {
        Value::String(s) => Some(s.clone()),
        Value::Object(map) => {
            // Essayer les clés courantes des outils natifs en priorité.
            const PRIORITY_KEYS: &[&str] = &[
                "cmd", "command", "path", "query", "input", "text", "content", "prompt",
            ];

            for key in PRIORITY_KEYS {
                if let Some(Value::String(s)) = map.get(*key) {
                    return Some(s.clone());
                }
            }
            // Fallback : première valeur string trouvée dans l'objet.
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

/// Cherche un pattern d'injection dans toutes les valeurs string de l'input JSON.
///
/// Inspecte récursivement les strings dans les objets et les tableaux.
/// Retourne le nom du premier pattern détecté, ou `None`.
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
            system_prompt: None,
            tools_requiring_approval: vec![],
            llm_backend: None,
            packages: vec![],
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
        // GIVEN un PermissionEngine avec SafeList vide (défaut)
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
        // GIVEN un PermissionEngine avec SafeList configurée pour "git status"
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
        // GIVEN un PermissionEngine avec SafeList pour "git status"
        // ET une commande injectée
        let (mut engine, _tmp) =
            engine_with_config(config_with_safe_cmd("bash_executor(git status; rm -rf /)"));
        let manifest = dummy_manifest();
        // WHEN
        let decision = engine
            .decide(
                "bash_executor",
                &json!({"cmd": "git status; rm -rf /"}),
                &manifest,
            )
            .expect("decide");
        // THEN couche 3 a priorité absolue
        assert!(matches!(
            decision,
            PermissionDecision::AutoDeniedInjection { .. }
        ));
    }

    #[test]
    fn engine_unknown_tool_needs_approval() {
        // GIVEN un PermissionEngine sans règle applicable
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
        // GIVEN un PermissionEngine avec règle "bash_executor(git:*)" Allow
        let (mut engine, _tmp) = engine_with_config(empty_config());
        use crate::prefix_rule_engine::{PrefixRule, RuleAction};
        let rule = PrefixRule {
            id: 0,
            tool_name: "bash_executor".into(),
            arg_prefix: Some("git".into()),
            action: RuleAction::Allow,
            created_at: 0,
            created_by_agent: None,
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
        // GIVEN un PermissionEngine complet
        let (mut engine, _tmp) = engine_with_config(empty_config());
        let manifest = dummy_manifest();
        // WHEN commande avec injection
        let decision = engine
            .decide(
                "bash_executor",
                &json!({"cmd": "git status; rm -rf /"}),
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
        // GIVEN un PermissionEngine avec injection_detection = false
        let config = PermissionsConfig {
            safe_commands: vec![],
            injection_detection: false,
            prefix_rule_ttl_hours: 168,
            db_path: PathBuf::from("/tmp/test.db"),
        };
        let (mut engine, _tmp) = engine_with_config(config);
        let manifest = dummy_manifest();
        // WHEN commande avec injection mais detection désactivée
        let decision = engine
            .decide(
                "bash_executor",
                &json!({"cmd": "git status; rm -rf /"}),
                &manifest,
            )
            .expect("decide");
        // THEN NeedsApproval (pas AutoDeniedInjection)
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
}
