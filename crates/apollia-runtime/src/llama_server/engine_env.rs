//! The environment a launched `llama-server` receives.
//!
//! llama.cpp reads every one of its flags from a `LLAMA_ARG_*` variable as
//! well, `--connect` among them, which opens a peer-to-peer tunnel. A variable
//! inherited from the shell or the service unit must not be able to switch
//! that on, nor any other flag the runtime did not choose: the supervisor
//! passes everything as flags, and `APOLLIA_LLAMA_EXTRA_ARGS` is the explicit
//! way for an operator to add one. Every `LLAMA_ARG_*` is therefore removed
//! from the child's environment, the rest is inherited.

use std::ffi::OsString;
use std::path::Path;

use tokio::process::Command;

/// The prefix llama.cpp reads its flags from.
const ENGINE_FLAG_PREFIX: &str = "LLAMA_ARG_";

/// The inherited variables that would set an engine flag, sorted by name.
/// Windows reads a variable whatever the case of its name, so the prefix is
/// matched without case there.
fn engine_flag_names(names: impl Iterator<Item = OsString>) -> Vec<OsString> {
    let mut found: Vec<OsString> = names
        .filter(|name| {
            let name = name.to_string_lossy();
            if cfg!(windows) {
                name.to_ascii_uppercase().starts_with(ENGINE_FLAG_PREFIX)
            } else {
                name.starts_with(ENGINE_FLAG_PREFIX)
            }
        })
        .collect();
    found.sort();
    found
}

/// A command for `bin` whose child inherits the environment of this process
/// minus every `LLAMA_ARG_*` variable, and opens no console window. The launch
/// is logged with the count and the names removed, never their values, which
/// may hold a secret.
pub(super) fn engine_command(bin: &Path) -> Command {
    let mut command = Command::new(bin);
    // A console-subsystem child of a GUI parent gets its own terminal window
    // on Windows, one per resident model, and the supervision loop reopens it
    // on every respawn.
    apollia_core::subprocess_window::hide_console_async(&mut command);
    let removed = engine_flag_names(std::env::vars_os().map(|(name, _)| name));
    for name in &removed {
        command.env_remove(name);
    }
    let names: Vec<String> = removed
        .iter()
        .map(|name| name.to_string_lossy().into_owned())
        .collect();
    tracing::event!(
        tracing::Level::INFO,
        count = names.len(),
        names = %names.join(","),
        "llama.server.env.engine_flags_removed"
    );
    command
}

#[cfg(test)]
mod tests;
