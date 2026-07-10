use serde::{Deserialize, Serialize};

/// Declarative isolation intent for a native tool.
///
/// Defined in apollia-core because it is a fundamental architectural constraint.
///
/// IMPORTANT: in v0.1.0 these variants are metadata only. The executors do not
/// branch on the declared profile. On Linux, every command runs under a PID and
/// mount namespace via `unshare`; on macOS no OS sandbox is applied and a
/// per-invocation warning is emitted. RAM caps, network namespaces, `iptables`
/// allowlists, and `tmpfs` mounts are NOT YET ENFORCED (roadmap). Per-process
/// resource limits (CPU seconds, address space, file descriptors) are applied
/// via `setrlimit` on Unix; see the `rlimits` module in apollia-tools. The
/// variant a tool declares is recorded in the audit trail and drives the
/// `dangerous` opt-in check, not runtime confinement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SandboxProfile {
    /// Intent: reading files, pure computation.
    /// Enforced today: PID and mount namespace on Linux; nothing on macOS.
    /// NOT YET ENFORCED (roadmap): `tmpfs` read-only, RAM cap, per-profile timeout.
    ReadOnly,
    /// Intent: writing files in an isolated directory.
    /// Enforced today: PID and mount namespace on Linux; nothing on macOS.
    /// NOT YET ENFORCED (roadmap): scoped writable mount, RAM cap, per-profile timeout.
    FileSystem,
    /// Intent: network access limited to the manifest's `network_allowlist`.
    /// Enforced today: same as `FileSystem` (no network namespace is created).
    /// NOT YET ENFORCED (roadmap): network namespace, `iptables` allowlist.
    NetworkRestricted,
    /// Intent: everything allowed, no isolation restriction.
    /// REQUIRES `dangerous=true` in ToolDescriptor. Not recommended in production.
    Full,
}

impl SandboxProfile {
    /// Returns true when this profile requires `dangerous=true` in ToolDescriptor.
    pub fn requires_dangerous_flag(&self) -> bool {
        matches!(self, SandboxProfile::Full)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sandbox_profile_variants() {
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
    fn test_only_full_requires_dangerous_flag() {
        // GIVEN / WHEN / THEN
        assert!(SandboxProfile::Full.requires_dangerous_flag());
        assert!(!SandboxProfile::ReadOnly.requires_dangerous_flag());
        assert!(!SandboxProfile::FileSystem.requires_dangerous_flag());
        assert!(!SandboxProfile::NetworkRestricted.requires_dangerous_flag());
    }
}
