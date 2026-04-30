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
use crate::prefix_rule_engine::{
    PermissionScope, PrefixRule, PrefixRuleEngine, RuleAction, ScopeContext,
};
use crate::safe_list::SafeList;

/// Marqueur `created_by` apposé aux règles ingérées depuis `PermissionsConfig.safe_commands`
/// au démarrage du moteur (ADR-086 — source unique `governance.db`).
pub const CONFIG_IMPORT_CREATOR: &str = "config-import";

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
    session_rules: Vec<PrefixRule>,
    scope_context: Option<ScopeContext>,
}

impl PermissionEngine {
    /// Construit un `PermissionEngine` depuis la configuration et le chemin SQLite.
    ///
    /// Le même fichier SQLite est utilisé par le `PrefixRuleEngine` et le `PermissionAuditLog`.
    ///
    /// # Errors
    ///
    /// - [`PermissionError::Database`] si l'initialisation SQLite échoue.
    pub fn new(config: &PermissionsConfig, db_path: &Path) -> Result<Self, PermissionError> {
        let mut prefix_rules = PrefixRuleEngine::new(db_path)?;
        let safe_list = SafeList::from_config(config);

        // ADR-086 — Migration idempotente de la SafeList TOML vers governance.db.
        // Au premier boot avec une SafeList non vide, on ingère chaque pattern en
        // tant que règle Allow scope=Global avec created_by="config-import". Les
        // boots suivants détectent les règles déjà présentes et n'en réécrivent
        // aucune.
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

    /// Ajoute une règle de session (mémoire uniquement, jamais persistée).
    ///
    /// Utilisé par le bouton "Toujours autoriser pour cette session" du dialog HITL.
    /// La règle disparaît à l'arrêt du process.
    ///
    /// La règle est forcée à `scope = Session` quel que soit le scope du `PrefixRule` reçu,
    /// pour éviter qu'une règle session entre par mégarde dans le chemin DB.
    pub fn add_session_rule(&mut self, mut rule: PrefixRule) {
        rule.scope = PermissionScope::Session;
        rule.project_path = None;
        self.session_rules.push(rule);
    }

    /// Vide la liste des règles de session (à appeler en fin de process si besoin).
    pub fn clear_session_rules(&mut self) {
        self.session_rules.clear();
    }

    /// Définit le contexte de scope courant utilisé par `decide()` lorsqu'une règle
    /// `Project` doit être filtrée par chemin.
    pub fn set_scope_context(&mut self, ctx: ScopeContext) {
        self.scope_context = Some(ctx);
    }

    /// Retourne une vue immuable du contexte de scope courant.
    pub fn scope_context(&self) -> Option<&ScopeContext> {
        self.scope_context.as_ref()
    }

    /// Retourne une vue immuable des règles de session en mémoire.
    pub fn session_rules(&self) -> &[PrefixRule] {
        &self.session_rules
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
// Migration SafeList → governance.db (ADR-086)
// ─────────────────────────────────────────────

/// Ingère les patterns `SafeList` parsés en règles `permission_rules` avec
/// `created_by="config-import"` lorsqu'aucune règle de cet auteur n'existe encore.
///
/// Idempotent : un second appel après une migration réussie est un no-op (la
/// présence d'au moins une règle `created_by="config-import"` court-circuite
/// l'import).
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
        "safe_list migrated to governance.db (ADR-086)"
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
        // GIVEN un PermissionEngine avec SafeList pour la commande injectée
        // ET une commande avec command substitution
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
        // GIVEN un PermissionEngine complet
        let (mut engine, _tmp) = engine_with_config(empty_config());
        let manifest = dummy_manifest();
        // WHEN commande avec command substitution
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

    // ─────────────────────────────────────────────
    // Migration SafeList → governance.db (ADR-086)
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
        // GIVEN une config avec deux entrées safe_commands et une DB fraîche
        let db_file = NamedTempFile::new().expect("tempfile");
        let config = config_with_safe_cmds(vec![
            "bash_executor(git status)",
            "file_read",
        ]);

        // WHEN on construit le moteur
        let engine = PermissionEngine::new(&config, db_file.path()).expect("engine init");

        // THEN deux règles config-import existent en DB
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
        // GIVEN une DB partagée et une config avec un pattern
        let db_file = NamedTempFile::new().expect("tempfile");
        let config = config_with_safe_cmds(vec!["bash_executor(pwd)"]);

        // WHEN on construit le moteur deux fois sur la même DB
        {
            let _engine = PermissionEngine::new(&config, db_file.path()).expect("init 1");
        }
        let engine2 = PermissionEngine::new(&config, db_file.path()).expect("init 2");

        // THEN une seule règle config-import existe (pas de doublons)
        let imported = engine2
            .prefix_rules
            .list_rules_by_creator(CONFIG_IMPORT_CREATOR)
            .expect("list");
        assert_eq!(imported.len(), 1);
    }

    #[test]
    fn migrate_safe_list_empty_config_creates_no_rule() {
        // GIVEN une config sans safe_commands
        let db_file = NamedTempFile::new().expect("tempfile");
        let config = empty_config();

        // WHEN on construit le moteur
        let engine = PermissionEngine::new(&config, db_file.path()).expect("init");

        // THEN aucune règle config-import
        let imported = engine
            .prefix_rules
            .list_rules_by_creator(CONFIG_IMPORT_CREATOR)
            .expect("list");
        assert!(imported.is_empty());
    }
}
