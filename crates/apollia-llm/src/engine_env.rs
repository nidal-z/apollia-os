//! The variables a launched `llama-server` must not inherit.
//!
//! llama.cpp reads every one of its flags from a `LLAMA_ARG_*` variable as
//! well, `--connect` among them, which opens a peer-to-peer tunnel. A variable
//! inherited from the shell or the service unit must not be able to switch
//! that on, nor any other flag the runtime did not choose. It must not break a
//! short launch either: an invalid `LLAMA_ARG_*` makes `--list-devices` or
//! `--version` exit in error, and the machine then reads as having no device
//! or the engine as having no version. Every launch of the engine, long or
//! short, removes the names this module returns from the child's environment.
//!
//! The names alone are returned, never the values, which may hold a secret.

use std::ffi::OsString;

/// The prefix llama.cpp reads its flags from.
const ENGINE_FLAG_PREFIX: &str = "LLAMA_ARG_";

/// The names among `names` that would set an engine flag, sorted. Windows
/// reads a variable whatever the case of its name, so the prefix is matched
/// without case there.
#[must_use]
pub fn engine_flag_names(names: impl Iterator<Item = OsString>) -> Vec<OsString> {
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

/// The engine flag variables of this process, to remove from the environment
/// of every engine it launches.
#[must_use]
pub fn inherited_engine_flags() -> Vec<OsString> {
    engine_flag_names(std::env::vars_os().map(|(name, _)| name))
}

/// Remove every inherited engine flag variable from `command`, a short launch
/// of the engine (`--list-devices`, `--version`).
pub fn remove_engine_flags(command: &mut std::process::Command) {
    for name in inherited_engine_flags() {
        command.env_remove(name);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_engine_prefix_is_an_engine_flag() {
        // GIVEN variable names around the prefix
        let names = ["PATH", "LLAMA_ARG_PORT", "LLAMA_ARGS", "LLAMA_ARG_CONNECT"]
            .into_iter()
            .map(OsString::from);

        // WHEN the engine flags are picked out
        let found = engine_flag_names(names);

        // THEN the two `LLAMA_ARG_*` names come back sorted, and a name that
        //      only shares the start of the prefix is not one
        assert_eq!(
            found,
            vec![
                OsString::from("LLAMA_ARG_CONNECT"),
                OsString::from("LLAMA_ARG_PORT")
            ]
        );
    }
}
