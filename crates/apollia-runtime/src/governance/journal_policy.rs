//! What the organisation policy asks of the journal, applied at each start
//! and then once a day: the daily export, then the retention.
//!
//! The export goes first, so a day is written to the folder the policy
//! designates before the retention can remove its entries. A folder that
//! cannot be used stops neither the runtime nor the retention: it is said in
//! the window and recorded in the journal.

use std::path::Path;

use apollia_permissions::org_policy::JournalPolicy;
use chrono::{DateTime, Utc};

use crate::audit_journal::{export_missing_days, AuditJournalHandle, DailyExportFailure};
use crate::governance::journal::{
    record_daily_export, record_daily_export_failed, record_unsigned,
};

/// Records, at startup, that the journal opened without its signing key.
/// Nothing for a signed journal.
pub(crate) fn record_journal_state_at_startup(journal: &AuditJournalHandle) {
    if !journal.status().signed {
        record_unsigned(journal);
    }
}

/// Applies `policy` to `journal` as of `now`: writes the daily files that are
/// missing, then removes the entries older than the retention. Without a
/// folder there is no export, and without a retention nothing is removed.
pub(crate) async fn apply_journal_policy(
    journal: &AuditJournalHandle,
    policy: &JournalPolicy,
    now: DateTime<Utc>,
) {
    if let Some(directory) = &policy.export.daily_summary_dir {
        match export_missing_days(journal, Path::new(directory), now.date_naive()).await {
            Ok(files) => {
                for file in &files {
                    record_daily_export(journal, file.day, file.entries);
                }
                journal.set_daily_export_failure(None);
                tracing::event!(
                    tracing::Level::INFO,
                    files = files.len(),
                    directory = %directory,
                    "audit.journal.daily_export.written"
                );
            }
            Err(e) => {
                let failure = DailyExportFailure {
                    directory: directory.clone(),
                    detail: e.to_string(),
                };
                tracing::event!(
                    tracing::Level::WARN,
                    error = %e,
                    directory = %directory,
                    "audit.journal.daily_export.failed"
                );
                // One entry per failure that starts, not one per day it lasts.
                if journal.status().daily_export_failure.as_ref() != Some(&failure) {
                    record_daily_export_failed(journal, &failure);
                }
                journal.set_daily_export_failure(Some(failure));
            }
        }
    }
    if let Some(retention_days) = policy.retention_days {
        if let Err(e) = journal.purge_by_retention(retention_days, now).await {
            tracing::event!(
                tracing::Level::ERROR,
                error = %e,
                retention_days,
                "audit.journal.purge.failed"
            );
        }
    }
}

/// Applies `policy` now, then once a day shortly after midnight UTC, for as
/// long as the runtime runs. A policy that asks neither an export nor a
/// retention starts nothing.
pub(crate) async fn start_journal_policy(journal: &AuditJournalHandle, policy: &JournalPolicy) {
    if policy.export.daily_summary_dir.is_none() && policy.retention_days.is_none() {
        return;
    }
    apply_journal_policy(journal, policy, Utc::now()).await;
    let journal = journal.clone();
    let policy = policy.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(until_next_day(Utc::now())).await;
            apply_journal_policy(&journal, &policy, Utc::now()).await;
        }
    });
}

/// How long to wait from `now` until one minute past the next midnight UTC.
fn until_next_day(now: DateTime<Utc>) -> std::time::Duration {
    let next = now
        .date_naive()
        .succ_opt()
        .and_then(|day| day.and_hms_opt(0, 1, 0))
        .map(|at| at.and_utc());
    next.and_then(|next| (next - now).to_std().ok())
        .unwrap_or(std::time::Duration::from_secs(24 * 60 * 60))
}

#[cfg(test)]
mod tests;
