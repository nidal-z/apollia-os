//! Pre-execution risk classifier for shell commands.
//!
//! `RiskClassifier` inspects the command passed to `BashExecutor` and returns
//! the list of risk categories detected against the patterns configured in
//! `apollia.toml` under `[tools.bash]`.
//!
//! Detection is **synchronous and I/O-free**: it runs before syntax validation
//! and before any process spawn (fail fast).
//!
//! Each risk category is documented by a public standard:
//! - `NetworkEgress`        -> OWASP A10:2021 (SSRF)
//! - `DestructiveOp`        -> NIST SP 800-190 §4.4
//! - `PrivilegeEscalation`  -> CWE-269 (Improper Privilege Management)
//! - `ResourceExhaustion`   -> CWE-400 (Uncontrolled Resource Consumption)

use apollia_core::BashValidatorConfig;

/// Risk category detected on a shell command.
///
/// Each variant is documented by a recognized security standard.
/// The concrete pattern lists are configurable in `apollia.toml`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RiskCategory {
    /// Unauthorized outbound network access.
    ///
    /// Reference: OWASP A10:2021 (Server-Side Request Forgery) and the Apollia
    /// local-first principle (zero outbound byte without an explicit action).
    NetworkEgress,

    /// Irreversible destructive operation on data or the filesystem.
    ///
    /// Reference: NIST SP 800-190 §4.4 (Container Security, destructive operations).
    DestructiveOp,

    /// Unauthorized privilege escalation.
    ///
    /// Reference: CWE-269 (Improper Privilege Management).
    PrivilegeEscalation,

    /// Uncontrolled resource consumption (CPU, memory, processes).
    ///
    /// Reference: CWE-400 (Uncontrolled Resource Consumption).
    ResourceExhaustion,
}

/// Stateless risk classifier for shell commands.
///
/// All the logic is static; `RiskClassifier` has no fields of its own.
/// The configuration is injected on each call from [`BashValidatorConfig`].
pub struct RiskClassifier;

impl RiskClassifier {
    /// Returns the risk categories detected for `command`.
    ///
    /// Detection is synchronous and I/O-free. Categories whose `block_*` flag is
    /// `false` in `config` are skipped without inspection.
    ///
    /// A pattern matches if the command (after trim) **contains** the pattern as
    /// a substring. This rule covers prefixes (`"rm -rf /home"` contains
    /// `"rm -rf /"`) and command names (`"curl https://…"` contains `"curl"`).
    ///
    /// The returned list is empty when no category is detected.
    pub fn classify(command: &str, config: &BashValidatorConfig) -> Vec<RiskCategory> {
        let mut risks = Vec::new();

        // OWASP A10:2021: network egress
        if config.block_network_egress
            && Self::command_matches(command, &config.network_egress_patterns)
        {
            risks.push(RiskCategory::NetworkEgress);
        }

        // NIST SP 800-190 §4.4: destructive operations
        if config.block_destructive && Self::command_matches(command, &config.destructive_patterns)
        {
            risks.push(RiskCategory::DestructiveOp);
        }

        // CWE-269: privilege escalation
        if config.block_privilege_escalation
            && Self::command_matches(command, &config.privilege_patterns)
        {
            risks.push(RiskCategory::PrivilegeEscalation);
        }

        // CWE-400: resource exhaustion
        if config.block_resource_exhaustion
            && Self::command_matches(command, &config.exhaustion_patterns)
        {
            risks.push(RiskCategory::ResourceExhaustion);
        }

        risks
    }

    /// Returns `true` if `command` contains at least one pattern from `patterns`.
    ///
    /// The search ignores leading whitespace (trim) but is case-sensitive on
    /// characters: patterns are operator-defined and must match the expected
    /// command case exactly.
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
        // GIVEN a config with block_network_egress=true and network_egress_patterns=["curl"]
        let config = config_with_network(vec!["curl"]);
        // WHEN
        let risks = RiskClassifier::classify("curl https://example.com", &config);
        // THEN
        assert_eq!(risks, vec![RiskCategory::NetworkEgress]);
    }

    #[test]
    fn risk_classifier_detects_destructive_op() {
        // GIVEN a config with block_destructive=true and destructive_patterns=["rm -rf /"]
        let config = config_with_destructive(vec!["rm -rf /"]);
        // WHEN
        let risks = RiskClassifier::classify("rm -rf /home", &config);
        // THEN
        assert_eq!(risks, vec![RiskCategory::DestructiveOp]);
    }

    #[test]
    fn risk_classifier_safe_command_no_risks() {
        // GIVEN a standard config with non-empty patterns but a safe command
        let config = config_with_network(vec!["curl", "wget"]);
        // WHEN
        let risks = RiskClassifier::classify("git status", &config);
        // THEN
        assert!(risks.is_empty());
    }

    #[test]
    fn risk_classifier_all_blocks_false_no_risks() {
        // GIVEN config with all block_* = false
        let config = config_all_disabled();
        // WHEN: even with patterns that would match
        let risks = RiskClassifier::classify("curl evil.com", &config);
        // THEN: opt-in behaviour
        assert!(risks.is_empty());
    }

    #[test]
    fn risk_classifier_default_config_no_risks_without_patterns() {
        // GIVEN default config (blocks=true, patterns=[])
        let config = BashValidatorConfig::default();
        // WHEN
        let risks = RiskClassifier::classify("curl https://example.com", &config);
        // THEN: empty patterns mean no blocking
        assert!(risks.is_empty());
    }

    #[test]
    fn risk_classifier_detects_privilege_escalation() {
        // GIVEN a config with block_privilege_escalation=true and patterns=["sudo"]
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
        // GIVEN a config with block_resource_exhaustion=true and a fork bomb pattern
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
        // GIVEN config that blocks network and destruction
        let config = BashValidatorConfig {
            block_network_egress: true,
            block_destructive: true,
            network_egress_patterns: vec!["curl".to_owned()],
            destructive_patterns: vec!["rm".to_owned()],
            ..BashValidatorConfig::default()
        };
        // WHEN
        let risks = RiskClassifier::classify("curl evil.com && rm -rf /tmp", &config);
        // THEN: both categories are detected
        assert!(risks.contains(&RiskCategory::NetworkEgress));
        assert!(risks.contains(&RiskCategory::DestructiveOp));
    }
}
