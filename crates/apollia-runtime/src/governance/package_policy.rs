//! The `[packages]` section of the organisation policy, answering the
//! signature question of the governance contract.
//!
//! Every other call is the governance without a policy: what a deposited
//! policy decides about accesses is not read here. The trusted publishers of
//! the same section are laid over the keyring by the package routes
//! ([`apollia_tools::TrustedKeyRepository::with_policy_publishers`]).

use apollia_permissions::PackagesPolicy;

use crate::audit_journal::AuditJournalHandle;
use crate::governance::contract::{
    AccessCall, AccessDecision, DeclaredAccess, EffectiveAccess, GovernanceContract,
    GovernanceError, InstallSubject,
};
use crate::governance::no_policy::NoPolicyGovernance;
use crate::governance::record::GovernanceEvent;

/// Governance whose signature question is answered by `packages.require_signature`.
///
/// Built from an empty section, it answers as [`NoPolicyGovernance`]: no
/// signature required, an unsigned package installs with its notice.
#[derive(Clone)]
pub struct PackagePolicyGovernance {
    inner: NoPolicyGovernance,
    require_signature: bool,
}

impl PackagePolicyGovernance {
    /// Builds the governance of `packages` over the instance audit journal.
    pub fn new(journal: AuditJournalHandle, packages: &PackagesPolicy) -> Self {
        Self {
            inner: NoPolicyGovernance::new(journal),
            require_signature: packages.require_signature,
        }
    }
}

impl GovernanceContract for PackagePolicyGovernance {
    fn effective_accesses(
        &self,
        agent_id: &str,
        declared: &[DeclaredAccess],
    ) -> Vec<EffectiveAccess> {
        self.inner.effective_accesses(agent_id, declared)
    }

    fn decide(&self, call: &AccessCall<'_>) -> AccessDecision {
        self.inner.decide(call)
    }

    fn signature_required(&self, subject: &InstallSubject<'_>) -> bool {
        if self.require_signature {
            tracing::event!(
                tracing::Level::DEBUG,
                package = %subject.package_id,
                "governance.signature.required"
            );
        }
        self.require_signature
    }

    fn record(&self, event: &GovernanceEvent) -> Result<(), GovernanceError> {
        self.inner.record(event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const KEY: &[u8] = b"package-policy-journal-key-0001";

    async fn journal(dir: &tempfile::TempDir) -> AuditJournalHandle {
        AuditJournalHandle::open_with_key_bytes(&dir.path().join("audit_journal.db"), KEY.to_vec())
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn test_require_signature_answers_the_signature_question() {
        // GIVEN a policy section requiring a signature, and an empty one
        let dir = tempfile::tempdir().unwrap();
        let journal = journal(&dir).await;
        let required = PackagesPolicy {
            require_signature: true,
            ..PackagesPolicy::default()
        };
        let subject = InstallSubject {
            package_id: "crm",
            publisher: Some("Acme"),
        };

        // WHEN the governance of each is asked
        let with_policy = PackagePolicyGovernance::new(journal.clone(), &required);
        let without = PackagePolicyGovernance::new(journal, &PackagesPolicy::default());

        // THEN only the first requires a signature
        assert!(with_policy.signature_required(&subject));
        assert!(!without.signature_required(&subject));
    }
}
