//! `apollia-os service`: install, remove and query the runtime as a service of
//! the user's session, so `apollia-os start` runs at logon without the desktop
//! application and without administrator rights.
//!
//! The work is `apollia_runtime::service`, which the desktop application
//! shares; this module parses the verbs, names this binary as the one the
//! service runs, and reports. `install --no-load` stops after
//! the write, which is how the end-to-end suite exercises the command under a
//! throwaway `HOME` without touching the session it runs in.

use std::path::PathBuf;

use clap::Subcommand;

use apollia_runtime::service::{ServiceState, ServiceTarget};

use crate::exit_codes;
use crate::output::{emit_error, require_confirmation};

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

/// Execute a `service` subcommand. Returns the process exit code.
pub fn run(cmd: &ServiceCommand, json: bool) -> i32 {
    let target = match ServiceTarget::current() {
        Ok(t) => t,
        Err(e) => return emit_error(json, exit_codes::GENERAL_ERROR, &e.to_string()),
    };
    match cmd {
        ServiceCommand::Install { no_load } => {
            let binary = match this_binary() {
                Ok(binary) => binary,
                Err(e) => {
                    return emit_error(
                        json,
                        exit_codes::GENERAL_ERROR,
                        &format!("cannot resolve the path of this binary: {e}"),
                    )
                }
            };
            match target.install(&binary, !*no_load) {
                Ok(state) => {
                    report_install(&target, state, *no_load, json);
                    exit_codes::SUCCESS
                }
                Err(e) => emit_error(json, exit_codes::GENERAL_ERROR, &e.to_string()),
            }
        }
        ServiceCommand::Uninstall { confirm } => uninstall(&target, *confirm, json),
        ServiceCommand::Status => {
            report_status(&target, json);
            exit_codes::SUCCESS
        }
    }
}

/// The absolute path of this binary, which the service runs.
fn this_binary() -> Result<PathBuf, std::io::Error> {
    let exe = std::env::current_exe()?;
    Ok(std::fs::canonicalize(&exe).unwrap_or(exe))
}

fn uninstall(target: &ServiceTarget, confirm: bool, json: bool) -> i32 {
    if !target.is_installed() {
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
    let stopped = match target.uninstall() {
        Ok(stopped) => stopped,
        Err(e) => return emit_error(json, exit_codes::GENERAL_ERROR, &e.to_string()),
    };
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

fn report_install(target: &ServiceTarget, state: ServiceState, no_load: bool, json: bool) {
    let word = state.word();
    if json {
        println!(
            "{}",
            serde_json::json!({
                "status": "installed",
                "manager": target.manager.name(),
                "label": target.manager.label(),
                "definition": target.definition.display().to_string(),
                "loaded": state.loaded,
                "state": word,
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
    println!("  State      : {word}");
    if no_load {
        crate::note!("Not loaded: it starts at your next logon.");
    }
}

fn report_status(target: &ServiceTarget, json: bool) {
    let state = target.observed_state();
    let word = state.word();
    if json {
        println!(
            "{}",
            serde_json::json!({
                "installed": state.installed,
                "loaded": state.loaded,
                "running": state.running,
                "state": word,
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
    println!("  State      : {}", word.replace('_', " "));
}
