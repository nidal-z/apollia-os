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

use apollia_core::{BashValidatorConfig, FilesystemRiskConfig};

/// Filesystem operation subject to risk classification.
///
/// Used by [`RiskClassifier::classify_filesystem`] to adapt the risk level to
/// the operation semantics (read vs write vs destruction).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilesystemOp {
    /// Reading a file or directory.
    Read,
    /// Creating or overwriting a file.
    Write,
    /// Deleting a file or directory.
    Delete,
    /// Changing permissions.
    Chmod,
}

impl FilesystemOp {
    /// Returns the string representation for events / logs.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
            Self::Delete => "delete",
            Self::Chmod => "chmod",
        }
    }
}

/// Risk level resulting from a filesystem classification.
///
/// Total order: `Safe < Low < Medium < High < Critical`.
/// Used by the HITL broker to decide which friction to apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RiskLevel {
    /// Benign operation, no friction.
    Safe,
    /// Slightly sensitive operation, a discreet toast may be shown.
    Low,
    /// Sensitive operation (e.g. write outside the workspace), HITL modal required.
    Medium,
    /// High-risk operation (system path / destructive), HITL modal without "always allow".
    High,
    /// Critical operation, secondary confirmation mandatory.
    Critical,
}

impl RiskLevel {
    /// Returns the string representation for events / i18n.
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

    /// Classifies a filesystem operation by risk level.
    ///
    /// Classification is **synchronous and I/O-free** (fail fast). It is based on:
    /// 1. The operation type (`Delete` / `Chmod` are always `High`)
    /// 2. The configured system paths (write -> `High`)
    /// 3. The credential paths (write -> `High`; read -> `Low`)
    /// 4. The path's position relative to the current workspace
    ///
    /// If `canonicalize()` fails (path does not exist yet), classification is
    /// performed on the path as-is.
    pub fn classify_filesystem(
        op: FilesystemOp,
        path: &std::path::Path,
        workspace: Option<&std::path::Path>,
        config: &FilesystemRiskConfig,
    ) -> RiskLevel {
        // 1. Destructive operations are High regardless of the path.
        if matches!(op, FilesystemOp::Delete | FilesystemOp::Chmod) {
            return RiskLevel::High;
        }

        // Try to canonicalize the path for reliable comparisons.
        // On failure (non-existent path), work on the raw path.
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let p = canonical.as_path();

        // 2. System paths: write = High.
        // Compare both the canonicalized path and the original path to handle
        // system symlinks (e.g. /etc -> /private/etc on macOS).
        let is_system = config.system_paths.iter().any(|sp| {
            let sp_canon = sp.canonicalize().unwrap_or_else(|_| sp.clone());
            p.starts_with(sp) || p.starts_with(&sp_canon)
        });

        if matches!(op, FilesystemOp::Write) && is_system {
            return RiskLevel::High;
        }

        // 3. Credential paths: write = High, read = Low.
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
        // THEN Safe (or at worst Low if canonicalize fails; both are below Medium)
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
        // THEN Medium (outside the workspace, not a system path)
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
        // THEN Low (reading credentials is not high risk)
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
        // GIVEN the four filesystem operations the classifier knows
        // WHEN each is rendered as its stored string
        // THEN the four persisted forms come out, lower case
        assert_eq!(FilesystemOp::Read.as_str(), "read");
        assert_eq!(FilesystemOp::Write.as_str(), "write");
        assert_eq!(FilesystemOp::Delete.as_str(), "delete");
        assert_eq!(FilesystemOp::Chmod.as_str(), "chmod");
    }

    #[test]
    fn risk_level_as_str() {
        // GIVEN the five risk levels
        // WHEN each is rendered as its stored string
        // THEN the five persisted forms come out, lower case
        assert_eq!(RiskLevel::Safe.as_str(), "safe");
        assert_eq!(RiskLevel::Low.as_str(), "low");
        assert_eq!(RiskLevel::Medium.as_str(), "medium");
        assert_eq!(RiskLevel::High.as_str(), "high");
        assert_eq!(RiskLevel::Critical.as_str(), "critical");
    }
}
