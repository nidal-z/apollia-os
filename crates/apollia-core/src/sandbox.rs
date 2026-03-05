use serde::{Deserialize, Serialize};

/// Profil d'isolation sandbox appliqué à l'exécution d'un outil natif.
///
/// Défini dans apollia-core car c'est une contrainte architecturale fondamentale (ADR-005).
/// L'isolation effective est implémentée dans apollia-tools via Linux namespaces (STORY-013).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SandboxProfile {
    /// tmpfs read-only + PID namespace. 128MB RAM, 30s timeout.
    /// Usage: lecture de fichiers, calculs purs.
    ReadOnly,
    /// Sandbox filesystem rw + PID namespace. 256MB RAM, 60s timeout.
    /// Usage: écriture de fichiers dans un répertoire isolé.
    FileSystem,
    /// FileSystem + network namespace + iptables allowlist.
    /// Accès réseau limité à network_allowlist du manifest.
    NetworkRestricted,
    /// Tout autorisé — aucune restriction de sandbox.
    /// EXIGE dangerous=true dans ToolDescriptor. Non recommandé en production.
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
