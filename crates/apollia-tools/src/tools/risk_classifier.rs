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

use apollia_core::{BashValidatorConfig, FilesystemRiskConfig};

/// Opération filesystem soumise à classification de risque.
///
/// Utilisée par [`RiskClassifier::classify_filesystem`] pour adapter le niveau
/// de risque selon la sémantique de l'opération (lecture vs écriture vs destruction).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilesystemOp {
    /// Lecture d'un fichier ou d'un répertoire.
    Read,
    /// Création ou écrasement d'un fichier.
    Write,
    /// Suppression d'un fichier ou répertoire.
    Delete,
    /// Modification des permissions.
    Chmod,
}

impl FilesystemOp {
    /// Retourne la représentation string pour les events / logs.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
            Self::Delete => "delete",
            Self::Chmod => "chmod",
        }
    }
}

/// Niveau de risque résultant d'une classification filesystem.
///
/// Ordre total : `Safe < Low < Medium < High < Critical`.
/// Utilisé par le HITL broker pour décider de la friction à appliquer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RiskLevel {
    /// Opération bénigne, aucune friction.
    Safe,
    /// Opération légèrement sensible, toast discret possible.
    Low,
    /// Opération sensible (ex : write hors workspace), modal HITL requis.
    Medium,
    /// Opération à haut risque (path système / destructive), modal HITL sans "toujours autoriser".
    High,
    /// Opération critique, confirmation secondaire obligatoire.
    Critical,
}

impl RiskLevel {
    /// Retourne la représentation string pour les events / i18n.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Safe => "safe",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Critical => "critical",
        }
    }
}

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

    /// Classe une opération filesystem par niveau de risque.
    ///
    /// La classification est **synchrone, sans I/O** (Principe #4 — Fail fast).
    /// Elle se base sur :
    /// 1. Le type d'opération (`Delete` / `Chmod` → toujours `High`)
    /// 2. Les paths système configurés (écriture → `High`)
    /// 3. Les paths credentials (écriture → `High` ; lecture → `Low` — ADR-069)
    /// 4. La position du path par rapport au workspace courant
    ///
    /// Si `canonicalize()` échoue (path n'existe pas encore), la classification
    /// est effectuée sur le path tel quel.
    pub fn classify_filesystem(
        op: FilesystemOp,
        path: &std::path::Path,
        workspace: Option<&std::path::Path>,
        config: &FilesystemRiskConfig,
    ) -> RiskLevel {
        // 1. Opérations destructrices → High indépendamment du path.
        if matches!(op, FilesystemOp::Delete | FilesystemOp::Chmod) {
            return RiskLevel::High;
        }

        // Tenter de canonicaliser le path pour des comparaisons fiables.
        // En cas d'échec (path inexistant), on travaille sur le path brut.
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let p = canonical.as_path();

        // 2. Paths système : écriture = High.
        // On compare à la fois le path canonicalisé et le path original pour
        // gérer les symlinks systèmes (ex. /etc → /private/etc sur macOS).
        let is_system = config.system_paths.iter().any(|sp| {
            let sp_canon = sp.canonicalize().unwrap_or_else(|_| sp.clone());
            p.starts_with(sp) || p.starts_with(&sp_canon)
        });

        if matches!(op, FilesystemOp::Write) && is_system {
            return RiskLevel::High;
        }

        // 3. Paths credentials : écriture = High, lecture = Low (ADR-069).
        let is_credential = config.credential_paths.iter().any(|cp| {
            let cp_canon = cp.canonicalize().unwrap_or_else(|_| cp.clone());
            p.starts_with(cp) || p.starts_with(&cp_canon)
        });

        if matches!(op, FilesystemOp::Write) && is_credential {
            return RiskLevel::High;
        }
        if matches!(op, FilesystemOp::Read) && is_credential {
            return RiskLevel::Low;
        }

        // 4. In/out workspace.
        let in_workspace = workspace
            .map(|ws| {
                let canonical_ws = ws.canonicalize().unwrap_or_else(|_| ws.to_path_buf());
                p.starts_with(&canonical_ws)
            })
            .unwrap_or(false);

        match op {
            FilesystemOp::Read if in_workspace => RiskLevel::Safe,
            FilesystemOp::Read => RiskLevel::Low,
            FilesystemOp::Write if in_workspace => RiskLevel::Low,
            FilesystemOp::Write => RiskLevel::Medium,
            // Delete / Chmod handled above.
            FilesystemOp::Delete | FilesystemOp::Chmod => RiskLevel::High,
        }
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

#[cfg(test)]
mod tests_filesystem {
    use super::*;
    use std::path::{Path, PathBuf};

    fn default_fs_config() -> FilesystemRiskConfig {
        FilesystemRiskConfig {
            system_paths: vec![PathBuf::from("/etc"), PathBuf::from("/usr")],
            credential_paths: vec![PathBuf::from("/home/alice/.ssh")],
        }
    }

    #[test]
    fn risk_level_ordering() {
        // GIVEN the RiskLevel ordering
        // WHEN comparing levels
        // THEN Safe < Low < Medium < High < Critical
        assert!(RiskLevel::Safe < RiskLevel::Low);
        assert!(RiskLevel::Low < RiskLevel::Medium);
        assert!(RiskLevel::Medium < RiskLevel::High);
        assert!(RiskLevel::High < RiskLevel::Critical);
    }

    #[test]
    fn classify_read_in_workspace_is_safe() {
        // GIVEN workspace /home/alice/proj and path inside it
        let ws = Path::new("/home/alice/proj");
        let path = Path::new("/home/alice/proj/src/main.rs");
        // WHEN
        let level = RiskClassifier::classify_filesystem(
            FilesystemOp::Read,
            path,
            Some(ws),
            &default_fs_config(),
        );
        // THEN Safe (or at worst Low if canonicalize fails — both are below Medium)
        assert!(level <= RiskLevel::Low);
    }

    #[test]
    fn classify_write_out_workspace_is_medium() {
        // GIVEN workspace /home/alice/proj and path outside it
        let ws = Path::new("/home/alice/proj");
        let path = Path::new("/home/alice/other/notes.md");
        // WHEN
        let level = RiskClassifier::classify_filesystem(
            FilesystemOp::Write,
            path,
            Some(ws),
            &default_fs_config(),
        );
        // THEN Medium (hors workspace, pas système)
        assert_eq!(level, RiskLevel::Medium);
    }

    #[test]
    fn classify_write_system_path_is_high() {
        // GIVEN path in /etc (system path)
        let path = Path::new("/etc/hosts");
        // WHEN
        let level = RiskClassifier::classify_filesystem(
            FilesystemOp::Write,
            path,
            None,
            &default_fs_config(),
        );
        // THEN High
        assert_eq!(level, RiskLevel::High);
    }

    #[test]
    fn classify_delete_in_workspace_is_high() {
        // GIVEN path inside workspace but delete op
        let ws = Path::new("/home/alice/proj");
        let path = Path::new("/home/alice/proj/tmp.txt");
        // WHEN
        let level = RiskClassifier::classify_filesystem(
            FilesystemOp::Delete,
            path,
            Some(ws),
            &default_fs_config(),
        );
        // THEN High (delete is always high regardless of workspace)
        assert_eq!(level, RiskLevel::High);
    }

    #[test]
    fn classify_read_ssh_config_is_low_not_high() {
        // GIVEN a credential path (.ssh) and Read operation
        let path = Path::new("/home/alice/.ssh/config");
        // WHEN
        let level = RiskClassifier::classify_filesystem(
            FilesystemOp::Read,
            path,
            None,
            &default_fs_config(),
        );
        // THEN Low (reading credentials is not high risk — ADR-069)
        assert_eq!(level, RiskLevel::Low);
    }

    #[test]
    fn classify_write_ssh_key_is_high() {
        // GIVEN a credential path and Write operation
        let path = Path::new("/home/alice/.ssh/id_rsa");
        // WHEN
        let level = RiskClassifier::classify_filesystem(
            FilesystemOp::Write,
            path,
            None,
            &default_fs_config(),
        );
        // THEN High (writing credentials is always High)
        assert_eq!(level, RiskLevel::High);
    }

    #[test]
    fn classify_no_workspace_write_is_medium() {
        // GIVEN no workspace and a regular path
        let path = Path::new("/home/alice/docs/note.md");
        // WHEN
        let level = RiskClassifier::classify_filesystem(
            FilesystemOp::Write,
            path,
            None,
            &default_fs_config(),
        );
        // THEN Medium
        assert_eq!(level, RiskLevel::Medium);
    }

    #[test]
    fn classify_no_workspace_read_is_low() {
        // GIVEN no workspace and a regular path
        let path = Path::new("/home/alice/docs/note.md");
        // WHEN
        let level = RiskClassifier::classify_filesystem(
            FilesystemOp::Read,
            path,
            None,
            &default_fs_config(),
        );
        // THEN Low (no workspace = slightly elevated vs Safe)
        assert_eq!(level, RiskLevel::Low);
    }

    #[test]
    fn filesystem_op_as_str() {
        assert_eq!(FilesystemOp::Read.as_str(), "read");
        assert_eq!(FilesystemOp::Write.as_str(), "write");
        assert_eq!(FilesystemOp::Delete.as_str(), "delete");
        assert_eq!(FilesystemOp::Chmod.as_str(), "chmod");
    }

    #[test]
    fn risk_level_as_str() {
        assert_eq!(RiskLevel::Safe.as_str(), "safe");
        assert_eq!(RiskLevel::Low.as_str(), "low");
        assert_eq!(RiskLevel::Medium.as_str(), "medium");
        assert_eq!(RiskLevel::High.as_str(), "high");
        assert_eq!(RiskLevel::Critical.as_str(), "critical");
    }
}
