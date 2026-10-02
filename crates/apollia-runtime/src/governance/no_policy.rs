//! Governance in force when no policy is deposited.

use crate::audit_journal::AuditJournalHandle;
use crate::governance::contract::{
    AccessCall, AccessDecision, DeclaredAccess, EffectiveAccess, GovernanceContract,
    GovernanceError, InstallSubject,
};
use crate::governance::record::{GovernanceEvent, GovernanceRecord};

/// Freedom by default: everything is allowed, an access is sensitive only when
/// its tool declares it so, no signature is required, and every record is
/// appended to the audit journal.
#[derive(Clone)]
pub struct NoPolicyGovernance {
    journal: AuditJournalHandle,
}

impl NoPolicyGovernance {
    /// Builds the default governance over the instance audit journal.
    pub fn new(journal: AuditJournalHandle) -> Self {
        Self { journal }
    }
}

impl GovernanceContract for NoPolicyGovernance {
    fn effective_accesses(
        &self,
        _agent_id: &str,
        declared: &[DeclaredAccess],
    ) -> Vec<EffectiveAccess> {
        declared
            .iter()
            .map(|access| EffectiveAccess {
                access_id: access.access_id.clone(),
                permits: access.permits.clone(),
                sensitive: access.declared_sensitive,
            })
            .collect()
    }

    fn decide(&self, _call: &AccessCall<'_>) -> AccessDecision {
        AccessDecision::Allowed
    }

    fn signature_required(&self, _subject: &InstallSubject<'_>) -> bool {
        false
    }

    fn record(&self, event: &GovernanceEvent) -> Result<(), GovernanceError> {
        let ts = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        let draft = GovernanceRecord::draft(event, ts)?;
        tracing::event!(
            tracing::Level::DEBUG,
            agent_id = %event.agent_id(),
            kind = %draft.kind.tag(),
            "governance.recorded"
        );
        self.journal.append(draft);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit_journal::verify::verify_entries;
    use crate::audit_journal::{HmacSigner, JournalEntryKind};
    use crate::governance::record::{governance_run_id, AccessChange};

    const KEY: &[u8] = b"governance-journal-key-0001";

    fn temp_db() -> std::path::PathBuf {
        std::env::temp_dir().join(format!("apollia_governance_{}.db", uuid::Uuid::new_v4()))
    }

    async fn governance(path: &std::path::Path) -> (NoPolicyGovernance, AuditJournalHandle) {
        let journal = AuditJournalHandle::open_with_key_bytes(path, KEY.to_vec())
            .await
            .expect("open signed journal");
        (NoPolicyGovernance::new(journal.clone()), journal)
    }

    fn declared(id: &str, sensitive: bool) -> DeclaredAccess {
        DeclaredAccess {
            access_id: id.into(),
            permits: format!("use {id}"),
            declared_sensitive: sensitive,
        }
    }

    // Effective accesses keep every declared access, sensitive only when declared.
    #[tokio::test]
    async fn test_effective_accesses_follow_declaration() {
        // GIVEN the default governance and one sensitive, one plain access
        let path = temp_db();
        let (gov, journal) = governance(&path).await;
        let accesses = [declared("send_email", true), declared("web_search", false)];

        // WHEN resolving the agent's effective accesses
        let effective = gov.effective_accesses("a1", &accesses);

        // THEN both are kept, with what they permit and the declared sensitivity
        assert_eq!(
            effective,
            vec![
                EffectiveAccess {
                    access_id: "send_email".into(),
                    permits: "use send_email".into(),
                    sensitive: true,
                },
                EffectiveAccess {
                    access_id: "web_search".into(),
                    permits: "use web_search".into(),
                    sensitive: false,
                },
            ]
        );
        journal.shutdown().await;
        tokio::fs::remove_file(&path).await.ok();
    }

    // Without a policy every call is allowed, sensitive or not.
    #[tokio::test]
    async fn test_decide_allows_everything() {
        // GIVEN the default governance
        let path = temp_db();
        let (gov, journal) = governance(&path).await;
        let calls = [
            AccessCall {
                agent_id: "a1",
                access_id: "bash_executor",
                argument: Some("rm -rf /tmp/x"),
            },
            AccessCall {
                agent_id: "a1",
                access_id: "web_search",
                argument: None,
            },
        ];

        for call in calls {
            // WHEN deciding a call
            let decision = gov.decide(&call);
            // THEN it is allowed
            assert_eq!(decision, AccessDecision::Allowed);
        }
        journal.shutdown().await;
        tokio::fs::remove_file(&path).await.ok();
    }

    // Without a policy no installation requires a signature.
    #[tokio::test]
    async fn test_signature_not_required() {
        // GIVEN the default governance and two packages, one naming a publisher
        let path = temp_db();
        let (gov, journal) = governance(&path).await;
        let subjects = [
            InstallSubject {
                package_id: "invoices",
                publisher: Some("acme"),
            },
            InstallSubject {
                package_id: "notes",
                publisher: None,
            },
        ];

        for subject in subjects {
            // WHEN asking whether a signature is required
            let required = gov.signature_required(&subject);
            // THEN it is not
            assert!(!required);
        }
        journal.shutdown().await;
        tokio::fs::remove_file(&path).await.ok();
    }

    // The four records land in the journal, chained and signed, and the chain
    // verifies with the journal's own verification.
    #[tokio::test]
    async fn test_record_writes_chained_signed_entries() {
        // GIVEN the default governance over a signed journal
        let path = temp_db();
        let (gov, journal) = governance(&path).await;
        let events = [
            GovernanceEvent::Installed {
                agent_id: "a1".into(),
                version: "1.0.0".into(),
                signed: false,
                actor: "user".into(),
            },
            GovernanceEvent::AccessChanged {
                agent_id: "a1".into(),
                access_id: "send_email".into(),
                change: AccessChange::Granted,
                actor: "user".into(),
                reason: None,
            },
            GovernanceEvent::Updated {
                agent_id: "a1".into(),
                from_version: "1.0.0".into(),
                to_version: "1.1.0".into(),
                signed: true,
                actor: "user".into(),
            },
            GovernanceEvent::Uninstalled {
                agent_id: "a1".into(),
                version: "1.1.0".into(),
                actor: "user".into(),
            },
        ];

        // WHEN each event is recorded
        for event in &events {
            gov.record(event).expect("record");
        }

        // THEN the agent run holds four entries of the four natures, in order,
        // each signed and linked to the previous one
        let run = governance_run_id("a1");
        let entries = journal.query_run(&run).await;
        let kinds: Vec<JournalEntryKind> = entries.iter().map(|e| e.kind.clone()).collect();
        assert_eq!(
            kinds,
            vec![
                JournalEntryKind::AgentInstalled,
                JournalEntryKind::AccessChanged,
                JournalEntryKind::AgentUpdated,
                JournalEntryKind::AgentUninstalled,
            ]
        );
        assert!(entries.iter().all(|e| e.signature.is_some()));
        for pair in entries.windows(2) {
            assert_eq!(pair[1].prev_hash, pair[0].hash);
        }
        // AND the existing verification accepts the chain with signatures required
        let signer = HmacSigner::from_key_bytes(KEY.to_vec()).expect("signer");
        let report = verify_entries(&run, &entries, Some(&signer), true);
        assert!(report.ok, "chain must verify: {report:?}");
        assert_eq!(report.entries_checked, 4);
        // AND the actor-side run and whole-journal verifications agree
        let actor_report = journal.verify_chain(&run).await.expect("verify chain");
        assert!(actor_report.ok);
        let whole = journal.verify_journal().await.expect("verify journal");
        assert!(whole.ok);
        assert_eq!(whole.entries_checked, 4);

        journal.shutdown().await;
        tokio::fs::remove_file(&path).await.ok();
    }

    // A tampered governance entry is caught by the same verification.
    #[tokio::test]
    async fn test_record_tamper_is_detected() {
        // GIVEN two recorded access changes
        let path = temp_db();
        let (gov, journal) = governance(&path).await;
        for change in [AccessChange::Granted, AccessChange::Revoked] {
            gov.record(&GovernanceEvent::AccessChanged {
                agent_id: "a3".into(),
                access_id: "send_email".into(),
                change,
                actor: "user".into(),
                reason: None,
            })
            .expect("record");
        }
        let run = governance_run_id("a3");
        let mut entries = journal.query_run(&run).await;

        // WHEN the revocation is rewritten into a grant after the fact
        entries[1].payload["change"] = serde_json::json!("granted");

        // THEN verification reports the rewritten entry
        let signer = HmacSigner::from_key_bytes(KEY.to_vec()).expect("signer");
        let report = verify_entries(&run, &entries, Some(&signer), true);
        assert!(!report.ok);
        assert_eq!(report.first_broken_link.map(|b| b.seq), Some(1));

        journal.shutdown().await;
        tokio::fs::remove_file(&path).await.ok();
    }
}
