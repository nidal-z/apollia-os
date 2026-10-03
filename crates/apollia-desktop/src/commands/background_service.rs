//! "Keep Apollia running without the application", in Settings > Developer.
//!
//! Turning it on installs the runtime as a service of the user's session with
//! `apollia_runtime::service`, the same code `apollia-os service install` runs,
//! naming the command line bundled with this application as the binary the
//! service starts. The application then relaunches: at startup it finds the
//! service installed, starts it and attaches to it (`startup::service`).
//! Turning it off stops and removes the service, then relaunches, and the
//! relaunched application embeds the runtime again.
//!
//! The state shown is the one the OS service manager reports, never a stored
//! value. The setting `desktop.background_service_allowed` only says whether
//! installing is allowed, which the organization policy can impose.

use std::path::PathBuf;
use std::time::Duration;

use apollia_core::config::SettingSource;
use apollia_runtime::service::{ServiceError, ServiceState, ServiceTarget};
use serde::Serialize;
use tauri::Manager;

/// The dotted key of the authorization in `apollia.toml`.
pub(crate) const ALLOWED_KEY: &str = "desktop.background_service_allowed";

/// How long a relaunch waits for the stopped daemon to release its port, so
/// the relaunched application does not attach to a runtime on its way out.
const RELEASE_TIMEOUT: Duration = Duration::from_secs(20);

/// Interval between two looks at the port.
const RELEASE_POLL: Duration = Duration::from_millis(250);

/// The switch as the settings show it.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct BackgroundService {
    /// This OS has a service manager the runtime can be installed with.
    pub supported: bool,
    /// The command line bundled with this application was found.
    pub bundled: bool,
    /// The service definition exists: the switch is on.
    pub installed: bool,
    /// The service's process is running.
    pub running: bool,
    /// Installing is allowed.
    pub allowed: bool,
    /// Where the authorization comes from; `policy` locks the switch.
    pub allowed_source: SettingSource,
}

/// What turning the switch to a position does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SwitchPlan {
    /// Install the service running this binary, then relaunch.
    Install(PathBuf),
    /// Remove the service, then relaunch.
    Uninstall,
    /// The switch already stands there: nothing to do.
    Nothing,
}

/// Why the switch refuses to move.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub(crate) enum SwitchRefusal {
    /// The organization policy forbids installing the service.
    #[error(
        "keeping Apollia running without the application is not allowed by the organization policy"
    )]
    NotAllowed,
    /// No command line is bundled with this application.
    #[error("the command line bundled with this application was not found")]
    NoBundledCli,
}

/// What turning the switch `on` does, from the observed state, the
/// authorization in force and the bundled command line. Turning it off is
/// never refused: the policy forbids installing, not removing.
pub(crate) fn plan_switch(
    on: bool,
    installed: bool,
    allowed: bool,
    bundled: Option<PathBuf>,
) -> Result<SwitchPlan, SwitchRefusal> {
    match (on, installed) {
        (true, true) | (false, false) => Ok(SwitchPlan::Nothing),
        (false, true) => Ok(SwitchPlan::Uninstall),
        (true, false) if !allowed => Err(SwitchRefusal::NotAllowed),
        (true, false) => bundled
            .map(SwitchPlan::Install)
            .ok_or(SwitchRefusal::NoBundledCli),
    }
}

/// Carries out `plan` on `target`. The service is written, not loaded: the
/// embedded runtime still holds the port, and the relaunched application
/// starts it before it decides to attach.
pub(crate) fn apply(target: &ServiceTarget, plan: &SwitchPlan) -> Result<(), ServiceError> {
    match plan {
        SwitchPlan::Install(binary) => target.install(binary, false).map(|_| ()),
        SwitchPlan::Uninstall => target.uninstall().map(|_| ()),
        SwitchPlan::Nothing => Ok(()),
    }
}

/// The switch from the observed service and the authorization.
pub(crate) fn describe(
    state: Option<ServiceState>,
    bundled: bool,
    allowed: bool,
    allowed_source: SettingSource,
) -> BackgroundService {
    let observed = state.unwrap_or_default();
    BackgroundService {
        supported: state.is_some(),
        bundled,
        installed: observed.installed,
        running: observed.running,
        allowed,
        allowed_source,
    }
}

/// The authorization in force: allowed unless the file or the policy says
/// otherwise; an unreadable file reads as the default.
fn authorization() -> (bool, SettingSource) {
    match super::config::settings::resolve_in_force(ALLOWED_KEY) {
        Ok(resolved) => (
            resolved.value != serde_json::Value::Bool(false),
            resolved.source,
        ),
        Err(_) => (true, SettingSource::Default),
    }
}

/// Whether nothing accepts a connection on the runtime port any more.
async fn port_released() -> bool {
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], crate::startup::RUNTIME_PORT));
    !matches!(
        tokio::time::timeout(RELEASE_POLL, tokio::net::TcpStream::connect(addr)).await,
        Ok(Ok(_))
    )
}

/// Waits, bounded, for the stopped daemon to release the runtime port,
/// without holding a thread of the pool between two looks.
async fn wait_for_release() {
    let deadline = tokio::time::Instant::now() + RELEASE_TIMEOUT;
    while tokio::time::Instant::now() < deadline {
        if port_released().await {
            return;
        }
        tokio::time::sleep(RELEASE_POLL).await;
    }
    tracing::event!(
        tracing::Level::WARN,
        port = crate::startup::RUNTIME_PORT,
        "desktop.background_service.port_still_held"
    );
}

fn service_state(target: Option<&ServiceTarget>) -> Option<ServiceState> {
    target.map(ServiceTarget::observed_state)
}

fn bundled_cli(app: &tauri::AppHandle) -> Option<PathBuf> {
    super::cli::bundled_cli_path(app)
}

/// The switch: the service as the OS reports it, and whether it may be
/// installed.
#[tauri::command]
pub async fn get_background_service(app: tauri::AppHandle) -> Result<BackgroundService, String> {
    let bundled = bundled_cli(&app).is_some();
    tauri::async_runtime::spawn_blocking(move || {
        let target = ServiceTarget::current().ok();
        let (allowed, source) = authorization();
        describe(service_state(target.as_ref()), bundled, allowed, source)
    })
    .await
    .map_err(|e| e.to_string())
}

/// Turns the switch on or off, then relaunches the application, which
/// attaches to the service or embeds the runtime again. Returns without a
/// relaunch when the switch already stands there.
#[tauri::command]
pub async fn set_background_service(app: tauri::AppHandle, on: bool) -> Result<(), String> {
    let bundled = bundled_cli(&app);
    let embedded = app
        .try_state::<apollia_runtime::embedded::RuntimeHandle>()
        .is_some();
    let (relaunch, released) =
        tauri::async_runtime::spawn_blocking(move || -> Result<(bool, bool), String> {
            let target = ServiceTarget::current().map_err(|e| e.to_string())?;
            let (allowed, _) = authorization();
            let plan = plan_switch(on, target.is_installed(), allowed, bundled)
                .map_err(|e| e.to_string())?;
            apply(&target, &plan).map_err(|e| e.to_string())?;
            tracing::event!(
                tracing::Level::INFO,
                on,
                changed = plan != SwitchPlan::Nothing,
                definition = %target.definition.display(),
                "desktop.background_service.switched"
            );
            Ok((
                plan != SwitchPlan::Nothing,
                plan == SwitchPlan::Uninstall && !embedded,
            ))
        })
        .await
        .map_err(|e| e.to_string())??;
    if released {
        wait_for_release().await;
    }
    if relaunch {
        app.request_restart();
    }
    Ok(())
}

#[cfg(test)]
#[path = "background_service_tests.rs"]
mod tests;
