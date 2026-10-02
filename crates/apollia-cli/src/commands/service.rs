//! `apollia-os service`: install, remove and query the runtime as a service of
//! the user's session, so `apollia-os start` runs at logon without the desktop
//! application and without administrator rights.
//!
//! The definition is written by this module (`definition`), then handed to
//! the OS service manager (`manager`): launchd on macOS, the systemd user
//! manager on Linux, Task Scheduler on Windows. `install --no-load` stops after
//! the write, which is how the end-to-end suite exercises the command under a
//! throwaway `HOME` without touching the session it runs in.

use std::path::{Path, PathBuf};

use clap::Subcommand;
use tracing::Level;

use crate::exit_codes;
use crate::output::{emit_error, require_confirmation};

pub mod definition;
pub mod manager;

use definition::{is_representable, Manager, ServiceSpec};
use manager::Observed;

/// Subcommands of the `service` noun: `apollia-os service <verb>`.
#[derive(Debug, Subcommand)]
pub enum ServiceCommand {
    /// Install the runtime as a service of your session and start it.
    ///
    /// macOS: a LaunchAgent; Linux: a systemd user unit; Windows: a task at
    /// logon. No administrator right is needed.
    Install {
        /// Write the service definition without handing it to the service
        /// manager: nothing starts now, the next logon starts it.
        #[arg(long)]
        no_load: bool,
    },
    /// Stop the service and remove its definition.
    ///
    /// Prompts for confirmation unless `--confirm` is passed.
    Uninstall {
        /// Skip the interactive confirmation prompt.
        #[arg(long)]
        confirm: bool,
    },
    /// Report whether the service is installed and running.
    Status,
}

/// Why a `service` leaf could not do its work.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ServiceError {
    /// This OS has none of the three supported service managers.
    #[error("no supported service manager on this OS (macOS, Linux and Windows are)")]
    UnsupportedPlatform,
    /// The home directory could not be resolved.
    #[error("cannot resolve the home directory")]
    NoHome,
    /// The running binary's own path could not be resolved.
    #[error("cannot resolve the path of this binary: {0}")]
    CurrentExe(#[source] std::io::Error),
    /// A path holds a character no definition format can carry.
    #[error("path cannot be written into a service definition: {0}")]
    UnrepresentablePath(String),
    /// Windows names the logon user by `USERDOMAIN` and `USERNAME`.
    #[error("cannot name the logon user: USERNAME is not set")]
    NoWindowsUser,
    /// Writing or removing a file failed.
    #[error("{action} {path}: {source}")]
    Io {
        action: &'static str,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// The service manager refused a command.
    #[error("`{command}` failed: {detail}")]
    Manager { command: String, detail: String },
}

/// Where this user's service lives.
struct Target {
    manager: Manager,
    home: PathBuf,
    definition: PathBuf,
    data_dir: PathBuf,
}

/// Execute a `service` subcommand. Returns the process exit code.
pub fn run(cmd: &ServiceCommand, json: bool) -> i32 {
    let target = match resolve_target() {
        Ok(t) => t,
        Err(e) => return emit_error(json, exit_codes::GENERAL_ERROR, &e.to_string()),
    };
    match cmd {
        ServiceCommand::Install { no_load } => match install(&target, *no_load) {
            Ok(observed) => {
                report_install(&target, observed, *no_load, json);
                exit_codes::SUCCESS
            }
            Err(e) => emit_error(json, exit_codes::GENERAL_ERROR, &e.to_string()),
        },
        ServiceCommand::Uninstall { confirm } => uninstall(&target, *confirm, json),
        ServiceCommand::Status => {
            report_status(&target, json);
            exit_codes::SUCCESS
        }
    }
}

fn resolve_target() -> Result<Target, ServiceError> {
    let manager = Manager::current().ok_or(ServiceError::UnsupportedPlatform)?;
    let home = apollia_core::paths::home_dir().ok_or(ServiceError::NoHome)?;
    let config_home = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute());
    let data_dir = apollia_core::paths::data_dir_under(&home);
    let definition = manager.definition_path(&home, config_home.as_deref(), &data_dir);
    Ok(Target {
        manager,
        home,
        definition,
        data_dir,
    })
}

fn representable(path: &Path) -> Result<String, ServiceError> {
    let text = path
        .to_str()
        .ok_or_else(|| ServiceError::UnrepresentablePath(path.to_string_lossy().to_string()))?;
    if is_representable(text) {
        Ok(text.to_string())
    } else {
        Err(ServiceError::UnrepresentablePath(text.to_string()))
    }
}

fn spec_for(target: &Target) -> Result<ServiceSpec, ServiceError> {
    let exe = std::env::current_exe().map_err(ServiceError::CurrentExe)?;
    let exe = std::fs::canonicalize(&exe).unwrap_or(exe);
    let log_file = target.data_dir.join("logs").join("service.log");
    let windows_user = if target.manager == Manager::TaskScheduler {
        let user = std::env::var("USERNAME").map_err(|_| ServiceError::NoWindowsUser)?;
        Some(match std::env::var("USERDOMAIN") {
            Ok(domain) if !domain.is_empty() => format!("{domain}\\{user}"),
            _ => user,
        })
    } else {
        None
    };
    let path_env = std::env::var("PATH")
        .ok()
        .map(|p| dedup_path(&p))
        .filter(|p| !p.is_empty() && is_representable(p));
    Ok(ServiceSpec {
        binary: representable(&exe)?,
        log_file: representable(&log_file)?,
        path_env,
        home: representable(&target.home)?,
        windows_user,
        definition: representable(&target.definition)?,
    })
}

/// `PATH` without its repeated entries, first occurrence kept: a shell that
/// sources its profile twice doubles every entry, and the definition should
/// not carry that.
fn dedup_path(path: &str) -> String {
    let separator = if cfg!(windows) { ';' } else { ':' };
    let mut seen = std::collections::HashSet::new();
    path.split(separator)
        .filter(|entry| !entry.is_empty() && seen.insert(*entry))
        .collect::<Vec<_>>()
        .join(&separator.to_string())
}

fn install(target: &Target, no_load: bool) -> Result<Observed, ServiceError> {
    let spec = spec_for(target)?;
    let logs = target.data_dir.join("logs");
    create_dir(&logs)?;
    if let Some(parent) = target.definition.parent() {
        create_dir(parent)?;
    }
    let bytes = target.manager.encode(&target.manager.render(&spec));
    std::fs::write(&target.definition, bytes).map_err(|source| ServiceError::Io {
        action: "cannot write",
        path: target.definition.clone(),
        source,
    })?;
    tracing::event!(
        Level::DEBUG,
        manager = target.manager.name(),
        definition = %target.definition.display(),
        binary = %spec.binary,
        "service.definition_written"
    );
    if !no_load {
        manager::load(target.manager, &target.definition)?;
        tracing::event!(
            Level::DEBUG,
            manager = target.manager.name(),
            "service.loaded"
        );
    }
    Ok(manager::query_state(target.manager, &target.definition))
}

fn create_dir(dir: &Path) -> Result<(), ServiceError> {
    std::fs::create_dir_all(dir).map_err(|source| ServiceError::Io {
        action: "cannot create",
        path: dir.to_path_buf(),
        source,
    })
}

fn uninstall(target: &Target, confirm: bool, json: bool) -> i32 {
    if !target.definition.is_file() {
        return emit_error(
            json,
            exit_codes::GENERAL_ERROR,
            &format!(
                "the service is not installed: no definition at {}",
                target.definition.display()
            ),
        );
    }
    if let Some(code) =
        require_confirmation(confirm, json, "stop and uninstall the Apollia service")
    {
        return code;
    }
    let stopped = match manager::unload(target.manager, &target.definition) {
        Ok(stopped) => stopped,
        Err(e) => return emit_error(json, exit_codes::GENERAL_ERROR, &e.to_string()),
    };
    if let Err(source) = std::fs::remove_file(&target.definition) {
        let e = ServiceError::Io {
            action: "cannot remove",
            path: target.definition.clone(),
            source,
        };
        return emit_error(json, exit_codes::GENERAL_ERROR, &e.to_string());
    }
    manager::forget(target.manager);
    tracing::event!(
        Level::DEBUG,
        manager = target.manager.name(),
        stopped,
        "service.uninstalled"
    );
    if json {
        println!(
            "{}",
            serde_json::json!({
                "status": "uninstalled",
                "manager": target.manager.name(),
                "label": target.manager.label(),
                "definition": target.definition.display().to_string(),
                "stopped": stopped,
            })
        );
    } else {
        println!(
            "Service {} uninstalled ({}).",
            target.manager.label(),
            if stopped { "stopped" } else { "was not loaded" }
        );
        crate::note!("Removed {}", target.definition.display());
    }
    exit_codes::SUCCESS
}

/// The state word published by `status` and `install`.
fn state_word(installed: bool, observed: Observed) -> &'static str {
    match (installed, observed.running) {
        (false, _) => "not_installed",
        (true, true) => "running",
        (true, false) => "stopped",
    }
}

fn report_install(target: &Target, observed: Observed, no_load: bool, json: bool) {
    let state = state_word(true, observed);
    if json {
        println!(
            "{}",
            serde_json::json!({
                "status": "installed",
                "manager": target.manager.name(),
                "label": target.manager.label(),
                "definition": target.definition.display().to_string(),
                "loaded": observed.loaded,
                "state": state,
            })
        );
        return;
    }
    println!(
        "Service {} installed ({}).",
        target.manager.label(),
        target.manager.name()
    );
    println!("  Definition : {}", target.definition.display());
    println!("  State      : {state}");
    if no_load {
        crate::note!("Not loaded: it starts at your next logon.");
    }
}

fn report_status(target: &Target, json: bool) {
    let installed = target.definition.is_file();
    let observed = if installed {
        manager::query_state(target.manager, &target.definition)
    } else {
        Observed::default()
    };
    let state = state_word(installed, observed);
    if json {
        println!(
            "{}",
            serde_json::json!({
                "installed": installed,
                "loaded": observed.loaded,
                "running": observed.running,
                "state": state,
                "manager": target.manager.name(),
                "label": target.manager.label(),
                "definition": target.definition.display().to_string(),
            })
        );
        return;
    }
    println!(
        "  Service    : {} ({})",
        target.manager.label(),
        target.manager.name()
    );
    println!("  Definition : {}", target.definition.display());
    println!("  State      : {}", state.replace('_', " "));
}

#[cfg(test)]
mod tests {
    use super::{dedup_path, state_word, Observed};

    #[cfg(unix)]
    #[test]
    fn test_dedup_path_keeps_the_first_occurrence_of_each_entry() {
        // GIVEN a PATH whose profile was sourced twice, with an empty entry
        let path = "/opt/bin:/usr/bin::/opt/bin:/bin:/usr/bin";
        // WHEN it is prepared for a service definition
        let kept = dedup_path(path);
        // THEN each directory appears once, in its first position
        assert_eq!(kept, "/opt/bin:/usr/bin:/bin");
    }

    #[test]
    fn test_state_word_names_the_three_states() {
        // GIVEN the three combinations a status can observe
        let stopped = Observed {
            loaded: true,
            running: false,
        };
        let running = Observed {
            loaded: true,
            running: true,
        };
        // WHEN each is named
        // THEN absent, stopped and running read apart
        assert_eq!(state_word(false, Observed::default()), "not_installed");
        assert_eq!(state_word(true, stopped), "stopped");
        assert_eq!(state_word(true, running), "running");
    }
}
