//! The runtime installed as a service of the session, brought up before the
//! startup decision.
//!
//! When "Keep Apollia running without the application" is on, a service
//! definition exists for this home. The application then never embeds the
//! runtime: it makes sure the service serves the port, starting it when the
//! manager does not run it, and waits for it to answer, so that [`super::decide`]
//! finds it and attaches. Without a definition nothing here runs a manager
//! command, and the decision is taken as before.

use std::net::SocketAddr;
use std::path::Path;
use std::time::Duration;

use apollia_runtime::service::ServiceTarget;
use tracing::Level;

use super::{classify, fetch_health, read_token, Holder};

/// How long the application waits for the service to answer.
const SERVING_TIMEOUT: Duration = Duration::from_secs(30);

/// Interval between two health requests while waiting.
const POLL: Duration = Duration::from_millis(250);

/// What the application does about the service before deciding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ServiceStep {
    /// No service, or an Apollia runtime already serves: decide at once.
    Decide,
    /// The manager runs the service: wait for it to answer.
    Await,
    /// The service is installed and not running: start it, then wait.
    StartThenAwait,
}

/// The step for a service `installed` and `running` as the manager reports,
/// and whether an Apollia runtime already `serves` the port.
pub(crate) fn service_step(installed: bool, running: bool, serves: bool) -> ServiceStep {
    match (installed, running, serves) {
        (false, _, _) | (true, _, true) => ServiceStep::Decide,
        (true, true, false) => ServiceStep::Await,
        (true, false, false) => ServiceStep::StartThenAwait,
    }
}

/// Whether an Apollia runtime answers on `addr` now.
fn serves(addr: SocketAddr, data_dir: &Path) -> bool {
    let token = read_token(data_dir);
    matches!(
        fetch_health(addr, token.as_deref()).map(|answer| classify(&answer)),
        Ok(Holder::Apollia { .. })
    )
}

/// Waits until an Apollia runtime answers on `addr`, at most `timeout`. The
/// pause between two health requests is a timer of the async runtime, not a
/// parked thread; each request itself stays the blocking probe of startup.
pub(crate) fn await_serving(addr: SocketAddr, data_dir: &Path, timeout: Duration) -> bool {
    tauri::async_runtime::block_on(async {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            if serves(addr, data_dir) {
                return true;
            }
            if tokio::time::Instant::now() >= deadline {
                return false;
            }
            tokio::time::sleep(POLL).await;
        }
    })
}

/// Brings the installed service up before the startup decision. A failure is
/// logged and leaves the decision to [`super::decide`].
pub(crate) fn bring_up(addr: SocketAddr, data_dir: &Path) {
    let Ok(target) = ServiceTarget::current() else {
        return;
    };
    if !target.is_installed() {
        return;
    }
    let state = target.observed_state();
    let step = service_step(state.installed, state.running, serves(addr, data_dir));
    tracing::event!(
        Level::INFO,
        running = state.running,
        step = ?step,
        "desktop.startup.service"
    );
    match step {
        ServiceStep::Decide => return,
        ServiceStep::StartThenAwait => {
            if let Err(error) = target.start() {
                tracing::event!(
                    Level::WARN,
                    error = %error,
                    "desktop.startup.service_start_failed"
                );
                return;
            }
        }
        ServiceStep::Await => {}
    }
    if !await_serving(addr, data_dir, SERVING_TIMEOUT) {
        tracing::event!(
            Level::WARN,
            port = addr.port(),
            "desktop.startup.service_not_serving"
        );
    }
}
