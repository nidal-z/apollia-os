//! The environment a launched `llama-server` receives.
//!
//! llama.cpp reads every one of its flags from a `LLAMA_ARG_*` variable as
//! well, `--connect` among them, which opens a peer-to-peer tunnel. A variable
//! inherited from the shell or the service unit must not be able to switch
//! that on, nor any other flag the runtime did not choose: the supervisor
//! passes everything as flags, and `APOLLIA_LLAMA_EXTRA_ARGS` is the explicit
//! way for an operator to add one. Every `LLAMA_ARG_*` is therefore removed
//! from the child's environment, the rest is inherited. The selection is
//! [`apollia_llm::engine_env`], which the short launches of the engine
//! (`--list-devices`, `--version`) go through as well.

use std::path::Path;

use tokio::process::Command;

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
    let removed = apollia_llm::engine_env::inherited_engine_flags();
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
