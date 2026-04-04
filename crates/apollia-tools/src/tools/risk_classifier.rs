//! Classifieur de risque pour commandes shell pré-exécution.
//!
//! `RiskClassifier` inspecte la commande transmise à `BashExecutor` et retourne
//! la liste des catégories de risque détectées selon les patterns configurés
//! dans `apollia.toml` sous `[tools.bash]`.
//!
//! La détection est **synchrone et sans I/O** — elle s'exécute avant la validation
//! syntaxique et avant tout spawn de processus (Principe #4 — Fail fast).
//!
//! Chaque catégorie de risque est documentée par un standard public :
//! - `NetworkEgress`        → OWASP A10:2021 (SSRF)
//! - `DestructiveOp`        → NIST SP 800-190 §4.4
//! - `PrivilegeEscalation`  → CWE-269 (Improper Privilege Management)
//! - `ResourceExhaustion`   → CWE-400 (Uncontrolled Resource Consumption)

use apollia_core::BashValidatorConfig;

/// Catégorie de risque détectée sur une commande shell.
///
/// Chaque variante est documentée par un standard de sécurité reconnu.
/// Les listes concrètes de patterns sont configurables dans `apollia.toml`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RiskCategory {
    /// Accès réseau sortant non autorisé.
    ///
    /// Référence : OWASP A10:2021 (Server-Side Request Forgery) et
    /// Principe #1 Apollia (local-first — zéro octet sortant sans action explicite).
    NetworkEgress,

    /// Opération destructrice irréversible sur des données ou le système de fichiers.
    ///
    /// Référence : NIST SP 800-190 §4.4 (Container Security — destructive operations).
    DestructiveOp,

    /// Élévation de droits non autorisée.
    ///
    /// Référence : CWE-269 (Improper Privilege Management).
    PrivilegeEscalation,

    /// Consommation de ressources non contrôlée (CPU, mémoire, processus).
    ///
    /// Référence : CWE-400 (Uncontrolled Resource Consumption).
    ResourceExhaustion,
}

/// Classifieur de risque sans état pour commandes shell.
///
/// Toute la logique est statique — `RiskClassifier` n'a pas de champ propre.
/// La configuration est injectée à chaque appel depuis [`BashValidatorConfig`].
pub struct RiskClassifier;

impl RiskClassifier {
    /// Retourne les catégories de risque détectées pour `command`.
    ///
    /// La détection est synchrone et sans I/O. Les catégories dont le flag `block_*`
    /// est `false` dans `config` sont ignorées sans inspection.
    ///
    /// Un pattern correspond si la commande (après trim) **contient** le pattern
    /// en tant que sous-chaîne. Cette règle couvre les préfixes (`"rm -rf /home"`
    /// contient `"rm -rf /"`) et les noms de commande (`"curl https://…"` contient `"curl"`).
    ///
    /// La liste retournée est vide quand aucune catégorie n'est détectée.
    pub fn classify(command: &str, config: &BashValidatorConfig) -> Vec<RiskCategory> {
        let mut risks = Vec::new();

        // OWASP A10:2021 — network egress
        if config.block_network_egress
            && Self::command_matches(command, &config.network_egress_patterns)
        {
            risks.push(RiskCategory::NetworkEgress);
        }

        // NIST SP 800-190 §4.4 — destructive operations
        if config.block_destructive && Self::command_matches(command, &config.destructive_patterns)
        {
            risks.push(RiskCategory::DestructiveOp);
        }

        // CWE-269 — privilege escalation
        if config.block_privilege_escalation
            && Self::command_matches(command, &config.privilege_patterns)
        {
            risks.push(RiskCategory::PrivilegeEscalation);
        }

        // CWE-400 — resource exhaustion
        if config.block_resource_exhaustion
            && Self::command_matches(command, &config.exhaustion_patterns)
        {
            risks.push(RiskCategory::ResourceExhaustion);
        }

        risks
    }

    /// Retourne `true` si `command` contient au moins un pattern de `patterns`.
    ///
    /// La recherche est insensible à la casse des espaces initiaux (trim),
    /// mais sensible à la casse des caractères — les patterns sont opérateur-définis
    /// et doivent correspondre exactement à la casse des commandes attendues.
    fn command_matches(command: &str, patterns: &[String]) -> bool {
        let trimmed = command.trim();
        patterns
            .iter()
            .any(|pattern| trimmed.contains(pattern.as_str()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config_with_network(patterns: Vec<&str>) -> BashValidatorConfig {
        BashValidatorConfig {
            block_network_egress: true,
            network_egress_patterns: patterns.into_iter().map(str::to_owned).collect(),
            ..BashValidatorConfig::default()
        }
    }

    fn config_with_destructive(patterns: Vec<&str>) -> BashValidatorConfig {
        BashValidatorConfig {
            block_destructive: true,
            destructive_patterns: patterns.into_iter().map(str::to_owned).collect(),
            ..BashValidatorConfig::default()
        }
    }

    fn config_all_disabled() -> BashValidatorConfig {
        BashValidatorConfig {
            block_network_egress: false,
            block_destructive: false,
            block_privilege_escalation: false,
            block_resource_exhaustion: false,
            ..BashValidatorConfig::default()
        }
    }

    #[test]
    fn risk_classifier_detects_network_egress() {
        // GIVEN config avec block_network_egress=true et network_egress_patterns=["curl"]
        let config = config_with_network(vec!["curl"]);
        // WHEN
        let risks = RiskClassifier::classify("curl https://example.com", &config);
        // THEN
        assert_eq!(risks, vec![RiskCategory::NetworkEgress]);
    }

    #[test]
    fn risk_classifier_detects_destructive_op() {
        // GIVEN config avec block_destructive=true et destructive_patterns=["rm -rf /"]
        let config = config_with_destructive(vec!["rm -rf /"]);
        // WHEN
        let risks = RiskClassifier::classify("rm -rf /home", &config);
        // THEN
        assert_eq!(risks, vec![RiskCategory::DestructiveOp]);
    }

    #[test]
    fn risk_classifier_safe_command_no_risks() {
        // GIVEN config standard avec patterns non-vides mais commande safe
        let config = config_with_network(vec!["curl", "wget"]);
        // WHEN
        let risks = RiskClassifier::classify("git status", &config);
        // THEN
        assert!(risks.is_empty());
    }

    #[test]
    fn risk_classifier_all_blocks_false_no_risks() {
        // GIVEN config avec tous les block_* = false
        let config = config_all_disabled();
        // WHEN — même avec patterns qui correspondraient
        let risks = RiskClassifier::classify("curl evil.com", &config);
        // THEN — comportement opt-in
        assert!(risks.is_empty());
    }

    #[test]
    fn risk_classifier_default_config_no_risks_without_patterns() {
        // GIVEN config par défaut (blocks=true, patterns=[])
        let config = BashValidatorConfig::default();
        // WHEN
        let risks = RiskClassifier::classify("curl https://example.com", &config);
        // THEN — patterns vides → aucun blocage
        assert!(risks.is_empty());
    }

    #[test]
    fn risk_classifier_detects_privilege_escalation() {
        // GIVEN config avec block_privilege_escalation=true et patterns=["sudo"]
        let config = BashValidatorConfig {
            block_privilege_escalation: true,
            privilege_patterns: vec!["sudo".to_owned()],
            ..BashValidatorConfig::default()
        };
        // WHEN
        let risks = RiskClassifier::classify("sudo rm -rf /", &config);
        // THEN
        assert!(risks.contains(&RiskCategory::PrivilegeEscalation));
    }

    #[test]
    fn risk_classifier_detects_resource_exhaustion() {
        // GIVEN config avec block_resource_exhaustion=true et fork bomb pattern
        let config = BashValidatorConfig {
            block_resource_exhaustion: true,
            exhaustion_patterns: vec![":(){ :|:& };:".to_owned()],
            ..BashValidatorConfig::default()
        };
        // WHEN
        let risks = RiskClassifier::classify(":(){ :|:& };:", &config);
        // THEN
        assert!(risks.contains(&RiskCategory::ResourceExhaustion));
    }

    #[test]
    fn risk_classifier_returns_multiple_categories() {
        // GIVEN config qui bloque réseau et destruction
        let config = BashValidatorConfig {
            block_network_egress: true,
            block_destructive: true,
            network_egress_patterns: vec!["curl".to_owned()],
            destructive_patterns: vec!["rm".to_owned()],
            ..BashValidatorConfig::default()
        };
        // WHEN
        let risks = RiskClassifier::classify("curl evil.com && rm -rf /tmp", &config);
        // THEN — les deux catégories sont détectées
        assert!(risks.contains(&RiskCategory::NetworkEgress));
        assert!(risks.contains(&RiskCategory::DestructiveOp));
    }
}
