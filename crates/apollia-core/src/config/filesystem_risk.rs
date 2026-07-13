// ─────────────────────────────────────────────
// FilesystemRiskConfig
// ─────────────────────────────────────────────

/// System path lists used by `RiskClassifier::classify_filesystem`.
///
/// Configurable via `apollia.toml` under `[tools.filesystem]`.
///
/// `credential_paths` are expanded relative to `$HOME` at runtime. Writing to a
/// system or credential path always produces `RiskLevel::High`. Reading
/// credential paths stays `RiskLevel::Low`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FilesystemRiskConfig {
    /// System paths: writing = High.
    ///
    /// Default: `["/etc", "/usr", "/bin", "/sbin", "/boot", "/var/log"]`.
    #[serde(default = "default_system_paths")]
    pub system_paths: Vec<std::path::PathBuf>,

    /// Credential paths: writing = High, reading stays Low.
    ///
    /// Default: `["$HOME/.ssh", "$HOME/.aws/credentials", "$HOME/.gnupg"]`
    /// (resolved relative to `$HOME` when the config is loaded).
    #[serde(default = "default_credential_paths")]
    pub credential_paths: Vec<std::path::PathBuf>,
}

fn default_system_paths() -> Vec<std::path::PathBuf> {
    ["/etc", "/usr", "/bin", "/sbin", "/boot", "/var/log"]
        .iter()
        .map(std::path::PathBuf::from)
        .collect()
}

fn default_credential_paths() -> Vec<std::path::PathBuf> {
    let home = std::env::var("HOME").unwrap_or_default();
    [".ssh", ".aws/credentials", ".gnupg", ".config/gh/hosts.yml"]
        .iter()
        .map(|rel| std::path::PathBuf::from(&home).join(rel))
        .collect()
}

impl Default for FilesystemRiskConfig {
    fn default() -> Self {
        Self {
            system_paths: default_system_paths(),
            credential_paths: default_credential_paths(),
        }
    }
}
