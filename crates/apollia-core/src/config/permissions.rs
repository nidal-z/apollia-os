use super::{validate_bounds, ConfigError};

// ─────────────────────────────────────────────
// PermissionsConfig
// ─────────────────────────────────────────────

/// Permission engine configuration (`[permissions]` section in `apollia.toml`).
///
/// Controls the three layers of the permission engine:
/// - SafeList (layer 1): commands auto-approved without HITL.
/// - PrefixRuleEngine (layer 2): prefix rules persisted in SQLite.
/// - InjectionDetector (layer 3): detection of dangerous shell patterns.
///
/// The SafeList is **empty by default**: the operator explicitly defines what
/// is safe (least-privilege principle, OWASP ASVS V1.4, CWE-272).
#[derive(Debug, Clone, serde::Deserialize)]
pub struct PermissionsConfig {
    /// Commands auto-approved without HITL, configured by the operator.
    ///
    /// Format: `"tool_name(arg_text)"` or `"tool_name"`.
    /// Examples: `"bash_executor(git status)"`, `"bash_executor(git log)"`.
    /// **Empty by default**: no command is auto-approved without explicit
    /// configuration.
    #[serde(default)]
    pub safe_commands: Vec<String>,

    /// Enables shell-injection detection (layer 3, absolute priority).
    ///
    /// Default: `true`. Disable only for controlled test environments.
    #[serde(default = "default_injection_detection")]
    pub injection_detection: bool,

    /// Lifetime of SQLite prefix rules, in hours.
    ///
    /// Default: 168 (7 days). Older rules may be purged by maintenance.
    #[serde(default = "default_prefix_rule_ttl_hours")]
    pub prefix_rule_ttl_hours: u64,

    /// Path to the consolidated SQLite database (governance.db).
    ///
    /// This single database holds the `permission_rules`, `permission_audit`,
    /// `tools` and `tool_credentials` tables. It replaces the former
    /// `permissions.db`: on the first start with an existing `permissions.db`,
    /// the runtime migrates it automatically to `governance.db` and keeps a
    /// `permissions.db.bak` backup.
    ///
    /// Default: `~/.apollia/governance.db`.
    #[serde(default = "default_permissions_db_path")]
    pub db_path: std::path::PathBuf,
}

impl Default for PermissionsConfig {
    fn default() -> Self {
        Self {
            safe_commands: vec![],
            injection_detection: default_injection_detection(),
            prefix_rule_ttl_hours: default_prefix_rule_ttl_hours(),
            db_path: default_permissions_db_path(),
        }
    }
}

fn default_injection_detection() -> bool {
    true
}

fn default_prefix_rule_ttl_hours() -> u64 {
    168
}

fn default_permissions_db_path() -> std::path::PathBuf {
    std::path::PathBuf::from("~/.apollia/governance.db")
}

// ─────────────────────────────────────────────
// BashValidatorConfig
// ─────────────────────────────────────────────

/// Pre-execution bash validator configuration (`[tools.bash]` section in `apollia.toml`).
///
/// Controls two protection mechanisms `BashValidator` applies before each
/// `BashExecutor` invocation:
/// - Per-category risk classification (`RiskClassifier`), synchronous and fast.
/// - Syntax validation via `bash -n -c`, asynchronous with a timeout.
///
/// All categories are **enabled** (`block_* = true`) but the pattern lists are
/// **empty by default**: no blocking happens without explicit configuration
/// (opt-in: the operator defines what to block).
#[derive(Debug, Clone, serde::Deserialize)]
pub struct BashValidatorConfig {
    /// Enables blocking of outbound network-access commands.
    ///
    /// Reference: OWASP A10:2021 (SSRF) and the Apollia local-first principle.
    /// Default: `true`. No effective blocking without `network_egress_patterns`.
    #[serde(default = "default_block_flag")]
    pub block_network_egress: bool,

    /// Enables blocking of irreversible destructive operations.
    ///
    /// Reference: NIST SP 800-190 §4.4.
    /// Default: `true`. No effective blocking without `destructive_patterns`.
    #[serde(default = "default_block_flag")]
    pub block_destructive: bool,

    /// Enables blocking of privilege escalations.
    ///
    /// Reference: CWE-269 (Improper Privilege Management).
    /// Default: `true`. No effective blocking without `privilege_patterns`.
    #[serde(default = "default_block_flag")]
    pub block_privilege_escalation: bool,

    /// Enables blocking of resource-exhaustion commands.
    ///
    /// Reference: CWE-400 (Uncontrolled Resource Consumption).
    /// Default: `true`. No effective blocking without `exhaustion_patterns`.
    #[serde(default = "default_block_flag")]
    pub block_resource_exhaustion: bool,

    /// Patterns triggering the `NetworkEgress` category.
    ///
    /// Each entry is a substring searched in the command (e.g. `"curl"`, `"wget"`).
    /// Empty by default: the operator defines patterns based on installed tools.
    #[serde(default)]
    pub network_egress_patterns: Vec<String>,

    /// Patterns triggering the `DestructiveOp` category.
    ///
    /// Examples: `"rm -rf /"`, `"dd if="`, `"mkfs"`.
    /// Empty by default.
    #[serde(default)]
    pub destructive_patterns: Vec<String>,

    /// Patterns triggering the `PrivilegeEscalation` category.
    ///
    /// Examples: `"sudo"`, `"su "`, `"chmod 777 /"`.
    /// Empty by default.
    #[serde(default)]
    pub privilege_patterns: Vec<String>,

    /// Patterns triggering the `ResourceExhaustion` category.
    ///
    /// Examples: `":(){ :|:& };:"` (fork bomb).
    /// Empty by default.
    #[serde(default)]
    pub exhaustion_patterns: Vec<String>,

    /// Timeout for `bash -n -c` syntax validation, in milliseconds.
    ///
    /// Beyond this delay, `BashValidator::validate_syntax()` returns
    /// `SyntaxValidationTimeout`. Default: 1000ms. Bounds: [100, 10000].
    #[serde(default = "default_syntax_check_timeout_ms")]
    pub syntax_check_timeout_ms: u64,
}

impl Default for BashValidatorConfig {
    fn default() -> Self {
        Self {
            block_network_egress: default_block_flag(),
            block_destructive: default_block_flag(),
            block_privilege_escalation: default_block_flag(),
            block_resource_exhaustion: default_block_flag(),
            network_egress_patterns: vec![],
            destructive_patterns: vec![],
            privilege_patterns: vec![],
            exhaustion_patterns: vec![],
            syntax_check_timeout_ms: default_syntax_check_timeout_ms(),
        }
    }
}

impl BashValidatorConfig {
    /// Validates the bash validator configuration bounds at startup (fail-fast).
    ///
    /// - `syntax_check_timeout_ms`: must be in [100, 10000].
    pub fn validate(&self) -> Result<(), ConfigError> {
        validate_bounds(
            "tools.bash.syntax_check_timeout_ms",
            self.syntax_check_timeout_ms,
            100_u64,
            10_000_u64,
        )?;
        Ok(())
    }
}

fn default_block_flag() -> bool {
    true
}

fn default_syntax_check_timeout_ms() -> u64 {
    1000
}
