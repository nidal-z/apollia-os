use serde::{Deserialize, Serialize};

/// Sandbox isolation profile applied to the execution of a native tool.
///
/// Defined in apollia-core because it is a fundamental architectural constraint.
/// The effective isolation is implemented in apollia-tools via Linux namespaces.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SandboxProfile {
    /// tmpfs read-only + PID namespace. 128MB RAM, 30s timeout.
    /// Usage: reading files, pure computation.
    ReadOnly,
    /// Sandbox filesystem rw + PID namespace. 256MB RAM, 60s timeout.
    /// Usage: writing files in an isolated directory.
    FileSystem,
    /// FileSystem + network namespace + iptables allowlist.
    /// Network access limited to the manifest's network_allowlist.
    NetworkRestricted,
    /// Everything allowed, no sandbox restriction.
    /// REQUIRES dangerous=true in ToolDescriptor. Not recommended in production.
    Full,
}

impl SandboxProfile {
    /// Retourne true si ce profil exige `dangerous=true` dans ToolDescriptor.
    pub fn requires_dangerous_flag(&self) -> bool {
        matches!(self, SandboxProfile::Full)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ac3_sandbox_profile_variants() {
        // GIVEN / WHEN
        let profiles = [
            SandboxProfile::ReadOnly,
            SandboxProfile::FileSystem,
            SandboxProfile::NetworkRestricted,
            SandboxProfile::Full,
        ];
        // THEN
        assert_eq!(profiles.len(), 4);
    }

    #[test]
    fn test_ac4_only_full_requires_dangerous_flag() {
        // GIVEN / WHEN / THEN
        assert!(SandboxProfile::Full.requires_dangerous_flag());
        assert!(!SandboxProfile::ReadOnly.requires_dangerous_flag());
        assert!(!SandboxProfile::FileSystem.requires_dangerous_flag());
        assert!(!SandboxProfile::NetworkRestricted.requires_dangerous_flag());
    }
}
