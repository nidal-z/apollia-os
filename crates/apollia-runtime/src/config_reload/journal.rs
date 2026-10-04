//! What the journal keeps of a change of `apollia.toml`: which keys moved,
//! where the change came from, and the fingerprint of the file before and
//! after. Never a value.
//!
//! The runtime is the one writer of the signed journal. A change made while
//! it runs is recorded when the reloader applies it, under the origin its
//! caller declared; a change made while it was stopped (a command line run
//! without the daemon, an editor) is recorded at the next start, when the
//! fingerprint of the file is not the one the journal last recorded.
//!
//! The keys of a change made during a stop are found by comparing the file
//! with the copy of the last text applied, kept beside it. The copy is
//! trusted only when its fingerprint is the one the journal last recorded;
//! otherwise the entry says the keys are not known rather than guess them.

use std::path::{Path, PathBuf};

use apollia_core::config::writer::fingerprint;
use apollia_core::config::{validate, ApolliaConfig};
use serde::{Deserialize, Serialize};

use super::diff::changed_keys;
use crate::audit_journal::{AuditJournalHandle, JournalEntryDraft, JournalEntryKind};

/// Journal run holding the changes of `apollia.toml`.
pub const CONFIG_RUN_ID: &str = "governance:configuration";

/// Where a change of the configuration came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigOrigin {
    /// The editor of the configuration file, in the settings.
    Editor,
    /// A settings page that writes its keys.
    Settings,
    /// The command line.
    Cli,
    /// A client of the native API that named no other origin.
    Api,
    /// The file changed on disk while the runtime ran, and nobody declared it.
    ExternalFile,
    /// The file changed while the runtime was stopped.
    AtStartup,
}

/// The origin a caller of the save route may declare. The two origins the
/// runtime finds by itself are not among them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DeclaredOrigin {
    /// The editor of the configuration file, in the settings.
    Editor,
    /// A settings page that writes its keys.
    Settings,
    /// The command line.
    Cli,
    /// A client of the native API.
    #[default]
    Api,
}

impl From<DeclaredOrigin> for ConfigOrigin {
    fn from(origin: DeclaredOrigin) -> Self {
        match origin {
            DeclaredOrigin::Editor => Self::Editor,
            DeclaredOrigin::Settings => Self::Settings,
            DeclaredOrigin::Cli => Self::Cli,
            DeclaredOrigin::Api => Self::Api,
        }
    }
}

/// Payload of a `configuration_changed` journal entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigChangeRecord {
    /// Always `configuration_changed`, as the other governance payloads name
    /// their event.
    pub event: String,
    /// Dotted paths of the keys whose value changed, in key order. Absent
    /// when the text applied before is not known, which only a change made
    /// during a stop can be.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keys: Option<Vec<String>>,
    /// Where the change came from.
    pub origin: ConfigOrigin,
    /// SHA-256 of the text in force before the change.
    pub sha256_before: String,
    /// SHA-256 of the text in force after the change.
    pub sha256_after: String,
}

impl ConfigChangeRecord {
    /// The record of a change from `before` to `after`.
    pub(super) fn new(
        keys: Option<Vec<String>>,
        origin: ConfigOrigin,
        sha256_before: String,
        sha256_after: String,
    ) -> Self {
        Self {
            event: JournalEntryKind::ConfigurationChanged.tag().to_owned(),
            keys,
            origin,
            sha256_before,
            sha256_after,
        }
    }

    /// The journal draft of this record, stamped now.
    fn draft(&self) -> Option<JournalEntryDraft> {
        let payload = serde_json::to_value(self).ok()?;
        Some(JournalEntryDraft {
            run_id: CONFIG_RUN_ID.to_owned(),
            ts: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            kind: JournalEntryKind::ConfigurationChanged,
            payload,
        })
    }
}

/// Appends `record` to `journal`.
pub(super) fn append(journal: &AuditJournalHandle, record: &ConfigChangeRecord) {
    match record.draft() {
        Some(draft) => {
            journal.append(draft);
            tracing::event!(
                tracing::Level::INFO,
                origin = ?record.origin,
                keys = record.keys.as_ref().map_or(0, Vec::len),
                "config.change.journaled"
            );
        }
        None => tracing::event!(tracing::Level::WARN, "config.change.unjournaled"),
    }
}

/// The fingerprint of the last text the journal says was applied.
pub(super) async fn last_applied(journal: &AuditJournalHandle) -> Option<String> {
    journal
        .query_run(CONFIG_RUN_ID)
        .await
        .into_iter()
        .rev()
        .find(|entry| entry.kind == JournalEntryKind::ConfigurationChanged)
        .and_then(|entry| {
            entry
                .payload
                .get("sha256_after")
                .and_then(|sha| sha.as_str())
                .map(str::to_owned)
        })
}

/// Where the copy of the last text applied is kept: beside the file.
pub(super) fn applied_copy_path(config: &Path) -> PathBuf {
    let mut name = config
        .file_name()
        .map_or_else(|| "apollia.toml".into(), std::ffi::OsStr::to_os_string);
    name.push(".applied");
    config.with_file_name(name)
}

/// Keeps `text` as the last text applied. A failure is logged: the next start
/// then records a change during a stop without its keys.
pub(super) async fn keep_applied_copy(config: &Path, text: String) {
    let copy = applied_copy_path(config);
    let written = tokio::task::spawn_blocking(move || std::fs::write(&copy, text)).await;
    match written {
        Ok(Ok(())) => {}
        Ok(Err(e)) => {
            tracing::event!(tracing::Level::WARN, error = %e, "config.applied_copy.failed");
        }
        Err(e) => {
            tracing::event!(tracing::Level::WARN, error = %e, "config.applied_copy.failed");
        }
    }
}

/// The change the journal has not seen: the text in force now against the
/// last one it recorded. `None` when the journal is up to date.
///
/// Without any earlier entry the change is measured from the defaults, the
/// configuration of a machine that never had a file.
pub(super) async fn change_during_stop(
    journal: &AuditJournalHandle,
    config: &Path,
    text: &str,
    in_force: &ApolliaConfig,
) -> Option<ConfigChangeRecord> {
    let now = fingerprint(text);
    let last = last_applied(journal).await;
    let before = last.clone().unwrap_or_else(|| fingerprint(""));
    if before == now {
        return None;
    }
    let previous = match last {
        None => Some(ApolliaConfig::default()),
        Some(sha) => read_applied_copy(config, sha).await,
    };
    let keys = previous.map(|previous| changed_keys(&previous, in_force));
    Some(ConfigChangeRecord::new(
        keys,
        ConfigOrigin::AtStartup,
        before,
        now,
    ))
}

/// The configuration of the copy kept beside the file, when its fingerprint
/// is `sha`.
async fn read_applied_copy(config: &Path, sha: String) -> Option<ApolliaConfig> {
    let copy = applied_copy_path(config);
    tokio::task::spawn_blocking(move || {
        let text = std::fs::read_to_string(copy).ok()?;
        if fingerprint(&text) != sha {
            return None;
        }
        validate(&text).ok()
    })
    .await
    .ok()
    .flatten()
}

#[cfg(test)]
mod tests;
