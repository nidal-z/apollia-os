//! Config loading, the backend of the registered agents and the shutdown wait.

use std::path::{Path, PathBuf};
use std::pin::Pin;

use apollia_core::{AIPResult, AIPTask, ResilientReceiver, RuntimeEvent, TaskStatus};
use apollia_runtime::coordinator::ExecutionBackend;

use super::probe::find_config_file;
use super::StartError;

/// Locates and parses `apollia.toml`, through the shared validation of
/// `apollia-core`. On a first start, without `--config`, the default file is
/// written at `~/.apollia/apollia.toml` first. Returns the parsed config and its
/// path, or `(None, None)` when no file could be read or written (defaults are
/// then used by the caller).
///
/// `explicit` is the path given with `--config`; it must exist. Without it the
/// file is `~/.apollia/apollia.toml`. A file left at a former location is
/// never read: it is named by a `config.legacy_location.ignored` warning.
pub(super) fn load_start_config(
    explicit: Option<&Path>,
) -> Result<(Option<crate::config::ApolliaConfig>, Option<PathBuf>), StartError> {
    let legacy = crate::config_location::legacy_files_here();
    let canonical = crate::config_location::resolve(None);
    load_start_config_from(explicit, find_config_file(), &canonical, &legacy)
}

/// [`load_start_config`] with every location decided by the caller, so a test
/// can play it under a throwaway home and working directory.
pub(super) fn load_start_config_from(
    explicit: Option<&Path>,
    canonical_if_present: Option<PathBuf>,
    canonical: &Path,
    legacy: &[PathBuf],
) -> Result<(Option<crate::config::ApolliaConfig>, Option<PathBuf>), StartError> {
    crate::config_location::warn_legacy_files(legacy, explicit, canonical);
    let found = match explicit {
        Some(p) if !p.is_file() => {
            return Err(StartError::Config {
                path: p.to_path_buf(),
                reason: "the file given with --config does not exist".to_string(),
            });
        }
        Some(p) => Some(p.to_path_buf()),
        None => canonical_if_present.or_else(|| write_default_file(canonical)),
    };
    let Some(path) = found else {
        tracing::info!(detail = "starting with the defaults", "config.file.absent");
        return Ok((None, None));
    };
    tracing::info!(config = %path.display(), "config.loading");
    // One validation for every surface: unknown keys, wrong types and values
    // out of their section's bounds are all refused here, each at its line
    // (principle 4). Withdrawn keys only warn.
    let cfg = crate::config::parse_apollia_toml(&path).map_err(|e| StartError::Config {
        path: path.clone(),
        reason: match e {
            crate::config::ConfigError::Invalid(diagnostics) => diagnostics
                .iter()
                .map(|d| format!("\n  {}:{d}", path.display()))
                .collect(),
            other => other.to_string(),
        },
    })?;
    Ok((Some(cfg), Some(path)))
}

/// Writes the default `apollia.toml` at the canonical location on a first
/// start, and returns it to be loaded. A failure to write leaves the start on
/// the defaults, as before the file existed.
fn write_default_file(canonical: &Path) -> Option<PathBuf> {
    match apollia_core::config::write_default_file_if_absent(canonical) {
        Ok(_) => Some(canonical.to_path_buf()).filter(|p| p.is_file()),
        Err(e) => {
            tracing::warn!(
                path = %canonical.display(),
                error = %e,
                detail = "starting with the defaults",
                "config.default_file.write_failed"
            );
            None
        }
    }
}

/// The TCP port the daemon listens on: `--port`, then `[api].port`, then
/// [`DEFAULT_API_PORT`](apollia_core::config::DEFAULT_API_PORT), which is the
/// default of `[api].port` when the file or the section is absent.
pub(super) fn resolve_tcp_port(flag: Option<u16>, api: &apollia_core::ApiConfig) -> u16 {
    flag.unwrap_or(api.port)
}

/// The backend of the coordinator each registered agent holds.
///
/// An agent is run by the agent executor on the access its definition and
/// the user's overrides grant it; nothing reaches this backend on that path.
/// A task that does reach it, sent to a coordinator directly, is answered
/// `Failed` rather than left without an answer.
#[derive(Clone)]
pub(super) struct NoopBackend;

impl ExecutionBackend for NoopBackend {
    fn execute(
        &self,
        task: AIPTask,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<AIPResult, String>> + Send>> {
        Box::pin(async move {
            Ok(AIPResult {
                task_id: task.task_id,
                status: TaskStatus::Failed,
                output: Vec::new(),
                error: Some(apollia_core::AIPError {
                    code: "NO_BACKEND".to_string(),
                    message: "no execution backend configured for this agent".to_string(),
                    details: None,
                }),
                artifacts: Vec::new(),
                input_required_data: None,
            })
        })
    }
}

/// The two settings of `apollia.toml` the daemon used to replace by constants:
/// `[runtime] startup_timeout_secs` and the `[observability]` section.
pub(super) struct DaemonFileSettings {
    /// Seconds the supervisor gives every actor to start.
    pub(super) startup_timeout_secs: u64,
    /// Truncation and capture of persisted run data.
    pub(super) observability: apollia_core::ObservabilityConfig,
}

/// Reads [`DaemonFileSettings`] from the validated file, or from the defaults
/// when there is none.
pub(super) fn daemon_file_settings(file: &crate::config::ApolliaConfig) -> DaemonFileSettings {
    DaemonFileSettings {
        startup_timeout_secs: file.runtime.startup_timeout_secs,
        observability: file.observability.clone(),
    }
}

/// Wait until a `RuntimeEvent::ShutdownRequested` event is received on the bus.
pub(super) async fn wait_for_shutdown_event(rx: &mut ResilientReceiver) {
    while let Some(event) = rx.recv().await {
        if matches!(event, RuntimeEvent::ShutdownRequested) {
            return;
        }
    }
}

#[cfg(test)]
mod tests;
