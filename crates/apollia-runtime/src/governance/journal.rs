//! What governance writes to the journal on its own: the events the stores of
//! `governance.db` left in their outbox, the organisation policy applied at
//! each start, each setting it closed against the user's file, every entry
//! into fail-closed, and what happens to the journal
//! itself (an opening without its key, a daily export written or failed).
//!
//! The runtime is the one writer of the signed journal. A rule or a consent
//! changed by another process (the command line, the desktop) waits in the
//! outbox of `governance.db` until the runtime moves it here, with the date
//! the change was made.

use std::path::{Path, PathBuf};

use apollia_permissions::{JournalOutbox, PolicyVerdict};

use crate::audit_journal::{AuditJournalHandle, JournalEntryKind};
use crate::governance::fail_closed::FailClosed;
use crate::governance::record::{ExportFilters, GovernanceEvent, GovernanceRecord, POLICY_RUN_ID};

/// Moves the events waiting in the outbox of `db` to `journal`, oldest
/// first, and removes them from the outbox once the journal holds them.
/// Returns how many moved. A database that does not exist yet has none.
pub(crate) async fn drain_outbox(db: &Path, journal: &AuditJournalHandle) -> usize {
    if !db.exists() {
        return 0;
    }
    let path = db.to_path_buf();
    let pending = tokio::task::spawn_blocking(move || {
        JournalOutbox::new(&path).and_then(|outbox| outbox.pending())
    })
    .await;
    let entries = match pending {
        Ok(Ok(entries)) => entries,
        Ok(Err(e)) => {
            outbox_unreadable(&e.to_string());
            return 0;
        }
        Err(e) => {
            outbox_unreadable(&e.to_string());
            return 0;
        }
    };
    let Some(last) = entries.last().map(|entry| entry.id) else {
        return 0;
    };
    let mut last_run = None;
    for entry in &entries {
        let event = GovernanceEvent::from(entry.event.clone());
        let ts = chrono::DateTime::from_timestamp(entry.recorded_at, 0)
            .unwrap_or_default()
            .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        match GovernanceRecord::draft(&event, ts) {
            Ok(draft) => {
                last_run = Some(draft.run_id.clone());
                journal.append(draft);
            }
            Err(e) => tracing::event!(
                tracing::Level::WARN,
                id = entry.id,
                error = %e,
                "governance.outbox.unjournaled"
            ),
        }
    }
    // The journal actor handles its messages in order: once it answers a
    // query sent after the appends, the appends are written.
    if let Some(run) = last_run {
        let _ = journal.last_hash(&run).await;
    }
    acknowledge(db.to_path_buf(), last).await;
    tracing::event!(
        tracing::Level::DEBUG,
        moved = entries.len(),
        "governance.outbox.drained"
    );
    entries.len()
}

async fn acknowledge(db: PathBuf, last: i64) {
    let acknowledged = tokio::task::spawn_blocking(move || {
        JournalOutbox::new(&db).and_then(|mut outbox| outbox.acknowledge(last))
    })
    .await;
    match acknowledged {
        Ok(Ok(_)) => {}
        Ok(Err(e)) => outbox_unreadable(&e.to_string()),
        Err(e) => outbox_unreadable(&e.to_string()),
    }
}

fn outbox_unreadable(error: &str) {
    tracing::event!(
        tracing::Level::WARN,
        error = %error,
        "governance.outbox.unreadable"
    );
}

/// Records, at startup, the policy `verdict` read: applied, preceded by a
/// change when its digest differs from the one of the last policy the
/// journal says was applied, or unreadable with its reason. No policy, no
/// entry.
pub(crate) async fn record_policy_at_startup(
    journal: &AuditJournalHandle,
    verdict: &PolicyVerdict,
) {
    let ts = now();
    let events = match verdict {
        PolicyVerdict::Absent => return,
        PolicyVerdict::Invalid(invalid) => vec![GovernanceEvent::PolicyUnreadable {
            reason: invalid.reason.as_str().to_owned(),
            detail: invalid.detail.clone(),
        }],
        PolicyVerdict::Valid(valid) => {
            let previous = last_applied_digest(journal).await;
            let mut events = Vec::with_capacity(2);
            if let Some(previous) = previous.filter(|d| *d != valid.policy_digest) {
                events.push(GovernanceEvent::PolicyChanged {
                    key_fingerprint: valid.key_fingerprint.clone(),
                    policy_digest: valid.policy_digest.clone(),
                    previous_digest: previous,
                });
            }
            events.push(GovernanceEvent::PolicyApplied {
                key_fingerprint: valid.key_fingerprint.clone(),
                policy_digest: valid.policy_digest.clone(),
                publisher: valid.publisher.clone(),
                signed_at: valid.signed_at.to_string(),
            });
            events
        }
    };
    for event in &events {
        append(journal, event, ts.clone());
    }
}

/// The digest of the last policy the journal says was applied.
async fn last_applied_digest(journal: &AuditJournalHandle) -> Option<String> {
    journal
        .query_run(POLICY_RUN_ID)
        .await
        .into_iter()
        .rev()
        .find(|entry| {
            matches!(
                entry.kind,
                JournalEntryKind::PolicyApplied | JournalEntryKind::PolicyChanged
            )
        })
        .and_then(|entry| {
            entry
                .payload
                .get("policy_digest")
                .and_then(|d| d.as_str())
                .map(str::to_owned)
        })
}

/// Records that the organisation policy fixed the setting `key` against what
/// the user's file says, and that what it had opened was closed.
pub(crate) fn record_setting_enforced(journal: &AuditJournalHandle, key: &str) {
    append(
        journal,
        &GovernanceEvent::PolicySettingEnforced {
            key: key.to_owned(),
        },
        now(),
    );
}

/// Records that governance entered `state`.
pub(crate) fn record_fail_closed(journal: &AuditJournalHandle, state: &FailClosed) {
    append(
        journal,
        &GovernanceEvent::FailClosedTriggered {
            cause: state.cause().to_owned(),
            detail: state.detail(),
        },
        now(),
    );
}

/// Records that the journal was exported: in which format, how many entries,
/// narrowed to what, and whether the detail of each entry went with it.
pub(crate) fn record_export(
    journal: &AuditJournalHandle,
    format: &str,
    entries: u64,
    filters: ExportFilters,
    detail: bool,
) {
    append(
        journal,
        &GovernanceEvent::JournalExported {
            format: format.to_owned(),
            entries,
            actor: "user".to_owned(),
            filters,
            detail,
        },
        now(),
    );
}

/// Records that the journal opened without its signing key.
pub(crate) fn record_unsigned(journal: &AuditJournalHandle) {
    append(
        journal,
        &GovernanceEvent::JournalUnsigned {
            detail: "the signing key could not be read or created: entries are chained, not signed"
                .to_owned(),
        },
        now(),
    );
}

/// Records that the daily file of `day` was written, with `entries` entries,
/// at the request of the organisation policy.
pub(crate) fn record_daily_export(
    journal: &AuditJournalHandle,
    day: chrono::NaiveDate,
    entries: u64,
) {
    let instant = |day: chrono::NaiveDate| {
        day.and_hms_opt(0, 0, 0).map(|start| {
            start
                .and_utc()
                .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
        })
    };
    append(
        journal,
        &GovernanceEvent::JournalExported {
            format: "json".to_owned(),
            entries,
            actor: "policy".to_owned(),
            filters: ExportFilters {
                since: instant(day),
                until: day.succ_opt().and_then(instant),
                ..ExportFilters::default()
            },
            detail: false,
        },
        now(),
    );
}

/// Records that the daily export could not be written.
pub(crate) fn record_daily_export_failed(
    journal: &AuditJournalHandle,
    failure: &crate::audit_journal::DailyExportFailure,
) {
    append(
        journal,
        &GovernanceEvent::JournalExportFailed {
            directory: failure.directory.clone(),
            detail: failure.detail.clone(),
        },
        now(),
    );
}

fn append(journal: &AuditJournalHandle, event: &GovernanceEvent, ts: String) {
    match GovernanceRecord::draft(event, ts) {
        Ok(draft) => journal.append(draft),
        Err(e) => tracing::event!(
            tracing::Level::WARN,
            error = %e,
            "governance.record.unjournaled"
        ),
    }
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

#[cfg(test)]
mod tests;
