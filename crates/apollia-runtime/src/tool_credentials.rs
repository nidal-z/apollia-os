//! The credentials of the native tools, kept encrypted in `governance.db`.
//!
//! The desktop running the runtime in its own process and the daemon serving
//! `/api/v1/tool-credentials` both call these functions, so a credential
//! listed, stored, removed or tested reads the same in either mode. A value
//! goes in and never comes out: the list carries the key names and their
//! dates only.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use apollia_core::paths::DataFile;
use apollia_tools::{GovernanceDb, GovernanceError, ToolCredentialStore, ToolGovernanceError};
use serde::{Deserialize, Serialize};

/// Name of the master key file beside `governance.db`.
const KEYFILE_NAME: &str = ".keyfile";

/// The tool whose credential has a live test, and the key it reads.
const TESTED_TOOL: &str = "web_search";
const TESTED_KEY: &str = "brave.api_key";
const BRAVE_TEST_URL: &str = "https://api.search.brave.com/res/v1/web/search";
const BRAVE_TEST_TIMEOUT: Duration = Duration::from_secs(10);

/// What went wrong with a credential operation. The messages are the ones the
/// desktop showed when it ran the runtime itself.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CredentialError {
    /// A tool or key name is blank.
    #[error("tool_name and key_name must not be empty")]
    EmptyName,
    /// The value to store is empty.
    #[error("credential value must not be empty")]
    EmptyValue,
    /// The tool has no live test.
    #[error("no credential test implemented for tool '{0}'")]
    NoTest(String),
    /// `governance.db` did not open.
    #[error("failed to open governance database: {0}")]
    Database(#[source] GovernanceError),
    /// The encrypted store failed.
    #[error("credential store error: {0}")]
    Store(#[from] ToolGovernanceError),
    /// The blocking task holding the store did not finish.
    #[error("credential task failed: {0}")]
    Task(#[from] tokio::task::JoinError),
}

/// A credential as listed: its owner, its key name and its dates, never its
/// value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CredentialEntryView {
    /// Name of the owning tool.
    pub tool_name: String,
    /// Logical key name (e.g. `brave.api_key`).
    pub key_name: String,
    /// Creation date, RFC 3339.
    pub created_at: String,
    /// Date of the last actual use, if any.
    pub last_used_at: Option<String>,
}

/// Result of a live credential validation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CredentialTestView {
    /// `true` if the call succeeded.
    pub ok: bool,
    /// Measured round-trip latency in milliseconds.
    pub latency_ms: Option<u64>,
    /// Diagnostic message when `ok` is `false`.
    pub error: Option<String>,
}

/// Opens the store under `data_dir`, migrating `governance.db` first if needed.
fn open_store(data_dir: &Path) -> Result<ToolCredentialStore, CredentialError> {
    GovernanceDb::open(data_dir).map_err(CredentialError::Database)?;
    Ok(ToolCredentialStore::new(
        &DataFile::Governance.path(data_dir),
        &data_dir.join(KEYFILE_NAME),
    )?)
}

fn rfc3339(secs: i64) -> String {
    chrono::DateTime::<chrono::Utc>::from_timestamp(secs, 0)
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_default()
}

/// Lists the credentials, of one tool or of all.
///
/// # Errors
///
/// [`CredentialError`] when the store does not open or read.
pub async fn list_credentials(
    data_dir: PathBuf,
    tool_name: Option<String>,
) -> Result<Vec<CredentialEntryView>, CredentialError> {
    tokio::task::spawn_blocking(move || {
        let entries = open_store(&data_dir)?.list(tool_name.as_deref())?;
        Ok(entries
            .into_iter()
            .map(|e| CredentialEntryView {
                tool_name: e.tool_name,
                key_name: e.key_name,
                created_at: rfc3339(e.created_at),
                last_used_at: e.last_used_at.map(rfc3339),
            })
            .collect())
    })
    .await?
}

/// Stores or replaces the value of `(tool_name, key_name)`, encrypted.
///
/// # Errors
///
/// [`CredentialError::EmptyName`] or [`CredentialError::EmptyValue`] before
/// anything is written; [`CredentialError`] when the store fails.
pub async fn set_credential(
    data_dir: PathBuf,
    tool_name: String,
    key_name: String,
    value: String,
) -> Result<(), CredentialError> {
    if tool_name.trim().is_empty() || key_name.trim().is_empty() {
        return Err(CredentialError::EmptyName);
    }
    if value.is_empty() {
        return Err(CredentialError::EmptyValue);
    }
    tokio::task::spawn_blocking(move || {
        open_store(&data_dir)?.set(&tool_name, &key_name, &value)?;
        tracing::event!(
            tracing::Level::INFO,
            tool = %tool_name,
            key = %key_name,
            "tool.credential.stored"
        );
        Ok(())
    })
    .await?
}

/// Removes `(tool_name, key_name)`. Returns whether a value was there.
///
/// # Errors
///
/// [`CredentialError`] when the store fails.
pub async fn delete_credential(
    data_dir: PathBuf,
    tool_name: String,
    key_name: String,
) -> Result<bool, CredentialError> {
    tokio::task::spawn_blocking(move || {
        let removed = open_store(&data_dir)?.delete(&tool_name, &key_name)?;
        tracing::event!(
            tracing::Level::INFO,
            tool = %tool_name,
            key = %key_name,
            removed,
            "tool.credential.deleted"
        );
        Ok(removed)
    })
    .await?
}

/// Validates the credential of `tool_name` with a live call. Only
/// `web_search` has one: a search on the Brave API with its key. A success
/// records the key as used.
///
/// # Errors
///
/// [`CredentialError::NoTest`] for any other tool; [`CredentialError`] when
/// the store fails.
pub async fn test_credential(
    data_dir: PathBuf,
    tool_name: String,
) -> Result<CredentialTestView, CredentialError> {
    if tool_name != TESTED_TOOL {
        return Err(CredentialError::NoTest(tool_name));
    }
    let dir = data_dir.clone();
    let key = tokio::task::spawn_blocking(move || {
        Ok::<_, CredentialError>(open_store(&dir)?.get(TESTED_TOOL, TESTED_KEY)?)
    })
    .await??;
    let Some(key) = key else {
        return Ok(CredentialTestView {
            ok: false,
            latency_ms: None,
            error: Some(format!("{TESTED_KEY} is not configured")),
        });
    };

    let outcome = test_brave_key(&key).await;
    if outcome.ok {
        let touched = tokio::task::spawn_blocking(move || {
            Ok::<_, CredentialError>(
                open_store(&data_dir)?.touch_last_used(TESTED_TOOL, TESTED_KEY)?,
            )
        })
        .await?;
        if let Err(error) = touched {
            tracing::event!(
                tracing::Level::WARN,
                error = %error,
                "tool.credential.last_used.update.failed"
            );
        }
    }
    Ok(outcome)
}

async fn test_brave_key(key: &str) -> CredentialTestView {
    let client = match apollia_core::net::safe_client_builder()
        .timeout(BRAVE_TEST_TIMEOUT)
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return CredentialTestView {
                ok: false,
                latency_ms: None,
                error: Some(format!("failed to build HTTP client: {e}")),
            };
        }
    };
    let started = Instant::now();
    let response = client
        .get(BRAVE_TEST_URL)
        .header("X-Subscription-Token", key)
        .header("Accept", "application/json")
        .query(&[("q", "apollia"), ("count", "1")])
        .send()
        .await;
    let latency_ms = Some(u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX));

    match response {
        Ok(r) if r.status().is_success() => CredentialTestView {
            ok: true,
            latency_ms,
            error: None,
        },
        Ok(r) => CredentialTestView {
            ok: false,
            latency_ms,
            error: Some(format!("brave returned HTTP {}", r.status().as_u16())),
        },
        Err(e) => CredentialTestView {
            ok: false,
            latency_ms,
            error: Some(e.to_string()),
        },
    }
}
