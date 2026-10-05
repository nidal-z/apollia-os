//! The state of the journal a window reports without anything being refused.
//!
//! Two things degrade the journal and stop nothing: it opened without its
//! signing key, so its entries are chained and not signed; the daily export
//! the organisation policy asks for cannot be written. Each is said in the
//! window and recorded in the journal. Neither puts governance in fail-closed:
//! that is kept for a journal the policy requires and that is not open at all.

use serde::{Deserialize, Serialize};

/// Why the daily export could not be written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DailyExportFailure {
    /// The folder the policy designates.
    pub directory: String,
    /// What the write failed on.
    pub detail: String,
}

/// What the journal handle knows of its own state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JournalStatus {
    /// Whether the entries are signed.
    pub signed: bool,
    /// Set while the daily export is failing.
    pub daily_export_failure: Option<DailyExportFailure>,
}

impl JournalStatus {
    /// The notices of this state, in a stable order.
    #[must_use]
    pub fn notices(&self) -> Vec<JournalNotice> {
        let mut notices = Vec::new();
        if !self.signed {
            notices.push(JournalNotice::Unsigned);
        }
        if let Some(failure) = &self.daily_export_failure {
            notices.push(JournalNotice::DailyExportFailed {
                directory: failure.directory.clone(),
                detail: failure.detail.clone(),
            });
        }
        notices
    }
}

/// One thing a window says about a degraded journal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(tag = "notice", rename_all = "snake_case")]
pub enum JournalNotice {
    /// The journal opened without its signing key: its entries are chained,
    /// so an alteration still shows, and they are not signed.
    Unsigned,
    /// The daily export the organisation policy asks for could not be
    /// written.
    DailyExportFailed {
        /// The folder the policy designates.
        directory: String,
        /// What the write failed on.
        detail: String,
    },
}
