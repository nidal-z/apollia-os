//! Governance at boot: the organisation policy, the audit trail and journal,
//! the one-shot upgrades they record, and the connection governance.

use crate::supervisor::*;

use crate::supervisor::persistence::{open_audit_journal, open_audit_trail};

/// What the governance phases open, shared with `AppState` and the handles.
pub(super) struct Governance {
    pub(super) audit_trail: Option<AuditTrailHandle>,
    pub(super) audit_journal: Option<AuditJournalHandle>,
    pub(super) connection: crate::governance::ConnectionGovernance,
    /// The governance actor every tool call is decided by.
    pub(super) handle: crate::governance::GovernanceHandle,
}

/// The connection admission built at boot and the existing connections it
/// refused, both handed to the connection governance and the actor.
pub(super) struct Connections {
    pub(super) admission: Arc<crate::governance::PolicyAdmission>,
    pub(super) forbidden: Arc<crate::governance::ForbiddenConnections>,
}

impl Supervisor {
    /// Phases 8 to 8d: the audit trail, the hash-chained journal and its
    /// subscriber, the governance actor, the access upgrade, the chat settings carry-over, then the
    /// connection governance.
    pub(super) async fn open_governance(
        &self,
        tool_registry_handle: &ToolRegistryHandle,
        event_sender: &EventBusSender,
        org_policy: &Arc<apollia_permissions::PolicyVerdict>,
        connections: Connections,
    ) -> Governance {
        // Phase 8 (pos 9): AuditTrail, opened before APIServer so it's injectable into AppState.
        info!("supervisor.audit_trail.opening");
        let audit_trail_handle = open_audit_trail(&self.config.data_dir).await;

        // Phase 8b: hash-chained, signed AuditJournal + its EventBus subscriber.
        info!("supervisor.audit_journal.opening");
        let audit_journal_handle = open_audit_journal(&self.config.data_dir).await;
        if let Some(journal) = &audit_journal_handle {
            AuditJournalSubscriber::spawn(journal.clone(), event_sender.subscribe());
            info!("audit.journal.subscriber.started");
            // The policy read at boot, applied, changed or refused, opens
            // the governance history of this start.
            crate::governance::record_policy_at_startup(journal, org_policy).await;
        } else {
            warn!(detail = "subscriber not started", "audit.journal.disabled");
        }

        // Phase 8c: the governance actor, owner of what every tool call is
        // decided from; then the one-shot access upgrade of governance.db,
        // journaled through it.
        let governance =
            crate::governance::GovernanceHandle::spawn(crate::governance::GovernanceSources {
                governance_db: Some(
                    self.config
                        .data_dir
                        .join(apollia_core::paths::DataFile::Governance.file_name()),
                ),
                config_path: Some(
                    self.config
                        .config_path
                        .clone()
                        .unwrap_or_else(|| self.config.data_dir.join("apollia.toml")),
                ),
                policy: Arc::clone(org_policy),
                journal: audit_journal_handle.clone(),
                events: Some(event_sender.clone()),
                forbidden_connections: connections.forbidden.tool_connections(),
            });
        crate::governance::access_upgrade::run_at_startup(&self.config.data_dir, &governance);

        // Phase 8c': connection secrets kept outside the secret store move in.
        crate::supervisor::legacy_secrets::move_legacy_secrets_into_store(&self.config.data_dir);
        // Phase 8d: the settings the chat kept before it was the Apollia
        // agent, carried over once the access upgrade has read them.
        let reading = apollia_tools::reading_family(
            tool_registry_handle,
            &startup_disabled_tools(&self.config.data_dir, &self.config.tools_config),
        )
        .await;
        crate::agents::apollia::carry_over::run_at_startup(
            &self.config.data_dir,
            &governance,
            &reading,
        );
        governance.refresh().await;

        // Connections answer to governance: admitted by the organisation
        // policy read above before they are added, journaled when they change.
        let connection_governance = crate::governance::ConnectionGovernance::new(
            Arc::new(
                connections
                    .admission
                    .as_ref()
                    .clone()
                    .with_governance(governance.clone()),
            ),
            audit_journal_handle.clone(),
        )
        .with_forbidden(connections.forbidden);

        Governance {
            audit_trail: audit_trail_handle,
            audit_journal: audit_journal_handle,
            connection: connection_governance,
            handle: governance,
        }
    }
}

/// Reads the organisation policy and reports the verdict.
pub(super) fn read_org_policy() -> apollia_permissions::PolicyVerdict {
    use apollia_permissions::PolicyVerdict;

    let location = apollia_permissions::policy_location();
    let verdict = apollia_permissions::load_policy(&location.dir, location.owner);
    let dir = location.dir.display();
    match &verdict {
        PolicyVerdict::Absent => {
            tracing::event!(tracing::Level::INFO, verdict = verdict.kind(), dir = %dir, "policy.read");
        }
        PolicyVerdict::Valid(valid) => {
            tracing::event!(
                tracing::Level::INFO,
                verdict = verdict.kind(),
                dir = %dir,
                key_fingerprint = %valid.key_fingerprint,
                publisher = %valid.publisher,
                signed_at = %valid.signed_at,
                "policy.read"
            );
        }
        PolicyVerdict::Invalid(invalid) => {
            tracing::event!(
                tracing::Level::WARN,
                verdict = verdict.kind(),
                dir = %dir,
                reason = invalid.reason.as_str(),
                detail = %invalid.detail,
                "policy.read"
            );
        }
    }
    verdict
}

/// The tools switched off at this start: the operator's `[tools] disabled`
/// list and the tools disabled in `governance.db`. An unreadable governance
/// database leaves the static list alone, the tolerance the runners apply.
fn startup_disabled_tools(
    data_dir: &std::path::Path,
    tools_config: &apollia_core::ToolsConfig,
) -> Vec<String> {
    let mut disabled = apollia_tools::load_governance_snapshot(data_dir)
        .map(|snapshot| snapshot.disabled_tools)
        .unwrap_or_default();
    for name in &tools_config.disabled {
        if !disabled.contains(name) {
            disabled.push(name.clone());
        }
    }
    disabled
}
