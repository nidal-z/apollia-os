//! Handing a written definition to the OS service manager, and asking it what
//! it runs.
//!
//! Every query is matched against the definition file, not only the label:
//! the label is unique per user session, the definition is unique per home. A
//! `status` or `uninstall` run under a throwaway `HOME` (the end-to-end suite)
//! therefore never reports, nor stops, a service another home installed.

use std::path::Path;
use std::process::{Command, Output};

use tracing::Level;

use crate::service::definition::{Manager, LAUNCHD_LABEL, SYSTEMD_UNIT, TASK_NAME};
use crate::service::ServiceError;

/// What the service manager reports for this definition.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Observed {
    /// The manager holds the service, loaded from this definition file.
    pub loaded: bool,
    /// The service's process is running.
    pub running: bool,
}

/// Ask the manager about the service. A manager that cannot be reached (no
/// systemd user bus in a container, for instance) reports nothing loaded.
pub fn query_state(manager: Manager, definition: &Path) -> Observed {
    let definition = definition.to_string_lossy();
    match manager {
        Manager::Launchd => {
            let Some(target) = launchd_service_target() else {
                return Observed::default();
            };
            match run_quiet("launchctl", &["print", &target]) {
                Some(out) if out.status.success() => {
                    parse_launchctl_print(&String::from_utf8_lossy(&out.stdout), &definition)
                }
                _ => Observed::default(),
            }
        }
        Manager::Systemd => match run_quiet(
            "systemctl",
            &[
                "--user",
                "show",
                SYSTEMD_UNIT,
                "--property=FragmentPath,ActiveState",
            ],
        ) {
            Some(out) if out.status.success() => {
                parse_systemctl_show(&String::from_utf8_lossy(&out.stdout), &definition)
            }
            _ => Observed::default(),
        },
        Manager::TaskScheduler => {
            let script = format!(
                "$t = Get-ScheduledTask -TaskName '{TASK_NAME}' -ErrorAction Stop; $t.State; $t.Description"
            );
            match run_quiet(
                "powershell",
                &["-NoProfile", "-NonInteractive", "-Command", &script],
            ) {
                Some(out) if out.status.success() => {
                    parse_scheduled_task(&String::from_utf8_lossy(&out.stdout), &definition)
                }
                _ => Observed::default(),
            }
        }
    }
}

/// Load the definition and start the service. A copy already loaded from the
/// same definition is replaced, so `install` run twice picks up a new binary.
pub fn load(manager: Manager, definition: &Path) -> Result<(), ServiceError> {
    let path = definition.to_string_lossy().to_string();
    match manager {
        Manager::Launchd => {
            let domain = launchd_domain()?;
            if query_state(manager, definition).loaded {
                run_checked(
                    "launchctl",
                    &["bootout", &format!("{domain}/{LAUNCHD_LABEL}")],
                )?;
            }
            run_checked("launchctl", &["bootstrap", &domain, &path])
        }
        Manager::Systemd => {
            run_checked("systemctl", &["--user", "daemon-reload"])?;
            run_checked("systemctl", &["--user", "enable", SYSTEMD_UNIT])?;
            run_checked("systemctl", &["--user", "restart", SYSTEMD_UNIT])
        }
        Manager::TaskScheduler => {
            run_checked(
                "schtasks",
                &["/Create", "/TN", TASK_NAME, "/XML", &path, "/F"],
            )?;
            run_checked("schtasks", &["/Run", "/TN", TASK_NAME])
        }
    }
}

/// Stop the service and take it out of the manager, when it was loaded from
/// this definition. The definition file itself is removed by the caller.
pub fn unload(manager: Manager, definition: &Path) -> Result<bool, ServiceError> {
    if !query_state(manager, definition).loaded {
        return Ok(false);
    }
    match manager {
        Manager::Launchd => {
            let domain = launchd_domain()?;
            run_checked(
                "launchctl",
                &["bootout", &format!("{domain}/{LAUNCHD_LABEL}")],
            )?;
        }
        Manager::Systemd => {
            run_checked("systemctl", &["--user", "disable", "--now", SYSTEMD_UNIT])?;
        }
        Manager::TaskScheduler => {
            // `/End` fails when the task is not running, which is not an error here.
            let _ = run_quiet("schtasks", &["/End", "/TN", TASK_NAME]);
            run_checked("schtasks", &["/Delete", "/TN", TASK_NAME, "/F"])?;
        }
    }
    Ok(true)
}

/// Tell the manager a definition file disappeared. Only systemd caches unit
/// files; the two other managers hold nothing once the service is unloaded.
pub fn forget(manager: Manager) {
    if manager == Manager::Systemd {
        let _ = run_quiet("systemctl", &["--user", "daemon-reload"]);
    }
}

/// `gui/<uid>`, the launchd domain of the logged-in user.
fn launchd_domain() -> Result<String, ServiceError> {
    let out = run_quiet("id", &["-u"]).ok_or_else(|| ServiceError::Manager {
        command: "id -u".to_string(),
        detail: "could not run".to_string(),
    })?;
    let uid = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if !out.status.success() || uid.is_empty() || !uid.chars().all(|c| c.is_ascii_digit()) {
        return Err(ServiceError::Manager {
            command: "id -u".to_string(),
            detail: format!("unexpected answer {uid:?}"),
        });
    }
    Ok(format!("gui/{uid}"))
}

fn launchd_service_target() -> Option<String> {
    launchd_domain()
        .ok()
        .map(|domain| format!("{domain}/{LAUNCHD_LABEL}"))
}

/// Run a command for its answer; `None` when it could not be spawned.
fn run_quiet(program: &str, args: &[&str]) -> Option<Output> {
    let mut command = Command::new(program);
    apollia_core::subprocess_window::hide_console(&mut command);
    command.args(args).output().ok()
}

/// Run a command that must succeed; its stderr becomes the error detail.
fn run_checked(program: &str, args: &[&str]) -> Result<(), ServiceError> {
    let command = format!("{program} {}", args.join(" "));
    let mut child = Command::new(program);
    apollia_core::subprocess_window::hide_console(&mut child);
    let out = child
        .args(args)
        .output()
        .map_err(|e| ServiceError::Manager {
            command: command.clone(),
            detail: e.to_string(),
        })?;
    if out.status.success() {
        tracing::event!(Level::DEBUG, command = %command, "service.manager_call");
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
    Err(ServiceError::Manager {
        command,
        detail: if stderr.is_empty() { stdout } else { stderr },
    })
}

/// `launchctl print gui/<uid>/<label>`: the service's `path = ...` names the
/// plist it was loaded from, `state = running` that its process is up.
pub fn parse_launchctl_print(out: &str, definition: &str) -> Observed {
    let mut loaded = false;
    let mut running = false;
    for line in out.lines().map(str::trim) {
        if let Some(path) = line.strip_prefix("path = ") {
            loaded = loaded || path == definition;
        } else if let Some(state) = line.strip_prefix("state = ") {
            running = running || state == "running";
        }
    }
    Observed {
        loaded,
        running: loaded && running,
    }
}

/// `systemctl --user show <unit> --property=FragmentPath,ActiveState`.
pub fn parse_systemctl_show(out: &str, definition: &str) -> Observed {
    let mut loaded = false;
    let mut running = false;
    for line in out.lines().map(str::trim) {
        if let Some(path) = line.strip_prefix("FragmentPath=") {
            loaded = path == definition;
        } else if let Some(state) = line.strip_prefix("ActiveState=") {
            running = state == "active";
        }
    }
    Observed {
        loaded,
        running: loaded && running,
    }
}

/// `Get-ScheduledTask`: the first line is the state (an enum name, never
/// localised), the rest the description, which carries the definition path.
pub fn parse_scheduled_task(out: &str, definition: &str) -> Observed {
    let mut lines = out.lines().map(str::trim);
    let state = lines.next().unwrap_or_default();
    let loaded = lines.any(|line| line.contains(definition));
    Observed {
        loaded,
        running: loaded && state == "Running",
    }
}
