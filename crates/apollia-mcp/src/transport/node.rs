//! Node.js for `stdio` servers.
//!
//! A server launched by `node` or `npx` runs on the Node.js the engine store
//! of the runtime installed, reached by its absolute path. When the store
//! holds none, a Node.js the user installed on the machine is used instead,
//! found by [`MachineNodeSearch`] and kept only when its major release is
//! [`MIN_MACHINE_NODE_MAJOR`] or later. When neither is there, the launch
//! fails with [`TransportError::NodeRequired`] rather than with whatever the
//! operating system would say about a missing program.
//!
//! The store layout read here is the one the runtime's engine store writes:
//! `<data_dir>/engines/nodejs/<tag>/cpu/`, with its record
//! `.apollia-engine.json` written only once the archive was verified and
//! extracted. A directory without the record is not an install.

use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::transport::TransportError;

mod machine;

pub use machine::{
    MachineNode, MachineNodeMissing, MachineNodeSearch, RejectedNode, MIN_MACHINE_NODE_MAJOR,
};

/// Directory of the Node.js packages under the engine store root.
pub const NODE_ENGINE_DIR: &str = "nodejs";
/// Variant directory a Node.js package is installed under.
pub const NODE_VARIANT_DIR: &str = "cpu";
/// Record the engine store writes beside a verified install.
pub const RECORD_FILE: &str = ".apollia-engine.json";

/// Where `npx` lives inside the npm a Node.js release carries.
const NPX_CLI: &str = "node_modules/npm/bin/npx-cli.js";

/// A Node.js release: the one the engine store installed, or one found on the
/// machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeInstall {
    /// Absolute path of the `node` binary.
    pub node: PathBuf,
    /// Absolute path of `npx-cli.js`, when the release carries npm; `npm-cli.js`
    /// lies beside it.
    pub npx_cli: Option<PathBuf>,
}

/// How a `node` or `npx` server is launched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeLaunch {
    /// The program spawned: always the `node` binary of the store.
    pub program: PathBuf,
    /// Its arguments: those of the configuration, after `npx-cli.js` for `npx`.
    pub args: Vec<String>,
    /// The directory of `node`, put first on the child's `PATH` so the
    /// packages `npx` runs find the same Node.js.
    pub bin_dir: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NodeCommand {
    Node,
    Npx,
}

/// Whether `command` is `node` or `npx` by its bare name, the spelling the
/// launch resolves to the store's Node.js or, failing that, the machine's.
#[must_use]
pub fn runs_on_node(command: &str) -> bool {
    node_command(command).is_some()
}

/// Which Node.js command `command` names by its bare name, the only spelling
/// resolved to the store. A path is the operator's explicit choice and is kept.
fn node_command(command: &str) -> Option<NodeCommand> {
    if command.contains('/') || command.contains('\\') {
        return None;
    }
    let lower = command.to_ascii_lowercase();
    let stem = lower
        .strip_suffix(".exe")
        .or_else(|| lower.strip_suffix(".cmd"))
        .unwrap_or(&lower);
    match stem {
        "node" => Some(NodeCommand::Node),
        "npx" => Some(NodeCommand::Npx),
        _ => None,
    }
}

/// The engine store root of this machine, `<data_dir>/engines`.
#[must_use]
pub fn default_engines_root() -> Option<PathBuf> {
    apollia_core::paths::data_dir().map(|d| d.join("engines"))
}

/// The newest Node.js release installed under `engines_root`, or `None`.
#[must_use]
pub fn installed_node(engines_root: &Path) -> Option<NodeInstall> {
    let tags = std::fs::read_dir(engines_root.join(NODE_ENGINE_DIR)).ok()?;
    let mut installs: Vec<(Vec<u64>, PathBuf)> = tags
        .flatten()
        .filter_map(|entry| {
            let numbers = release_numbers(entry.file_name().to_str()?)?;
            let dir = entry.path().join(NODE_VARIANT_DIR);
            dir.join(RECORD_FILE).is_file().then_some((numbers, dir))
        })
        .collect();
    installs.sort();
    let (_, dir) = installs.pop()?;
    let node = find_node_binary(&dir)?;
    let npx_cli = npx_cli_beside(&node);
    Some(NodeInstall { node, npx_cli })
}

/// The `npx-cli.js` of the npm a Node.js release carries beside its `node`:
/// `bin/node` with `lib/node_modules` beside `bin` on macOS and Linux,
/// `node.exe` with `node_modules` beside it on Windows.
fn npx_cli_beside(node: &Path) -> Option<PathBuf> {
    let bin = node.parent()?;
    let unix = bin.parent().map(|prefix| prefix.join("lib").join(NPX_CLI));
    unix.into_iter()
        .chain(std::iter::once(bin.join(NPX_CLI)))
        .find(|p| p.is_file())
}

/// The numbers of a `v<x>.<y>.<z>` release tag.
fn release_numbers(tag: &str) -> Option<Vec<u64>> {
    let numbers: Option<Vec<u64>> = tag
        .strip_prefix('v')?
        .split('.')
        .map(|p| p.parse::<u64>().ok())
        .collect();
    numbers.filter(|n| n.len() == 3)
}

fn find_node_binary(dir: &Path) -> Option<PathBuf> {
    let names: &[&str] = if cfg!(windows) {
        &["node.exe"]
    } else {
        &["bin/node", "node"]
    };
    fn walk(dir: &Path, names: &[&str], depth: u8) -> Option<PathBuf> {
        if let Some(found) = names.iter().map(|n| dir.join(n)).find(|p| p.is_file()) {
            return Some(found);
        }
        if depth == 0 {
            return None;
        }
        let mut subdirs: Vec<PathBuf> = std::fs::read_dir(dir)
            .ok()?
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
            .map(|e| e.path())
            .collect();
        subdirs.sort();
        subdirs.into_iter().find_map(|d| walk(&d, names, depth - 1))
    }
    walk(dir, names, 2)
}

/// How to launch `command` with `args`: `None` when it does not name Node.js,
/// the store's Node.js otherwise.
///
/// # Errors
///
/// [`TransportError::NodeRequired`] when `command` is `node` or `npx` and
/// `engines_root` holds no Node.js (or, for `npx`, one without npm).
pub fn resolve(
    command: &str,
    args: &[String],
    engines_root: Option<&Path>,
) -> Result<Option<NodeLaunch>, TransportError> {
    resolve_or_machine(command, args, engines_root, None)
}

/// [`resolve`], falling back on the Node.js `machine` finds when the store
/// holds none. The store's Node.js is preferred whenever it is installed.
///
/// # Errors
///
/// [`TransportError::NodeRequired`] when `command` is `node` or `npx` and
/// neither the store nor the machine holds a Node.js Apollia accepts (or,
/// for `npx`, one without npm).
pub fn resolve_or_machine(
    command: &str,
    args: &[String],
    engines_root: Option<&Path>,
    machine: Option<&MachineNodeSearch>,
) -> Result<Option<NodeLaunch>, TransportError> {
    let Some(kind) = node_command(command) else {
        return Ok(None);
    };
    let required = || TransportError::NodeRequired {
        command: command.to_owned(),
    };
    let install = engines_root
        .and_then(installed_node)
        .or_else(|| {
            machine
                .and_then(|search| search.find().ok())
                .map(|found| found.install)
        })
        .ok_or_else(required)?;
    let mut launch_args = Vec::with_capacity(args.len() + 1);
    if kind == NodeCommand::Npx {
        let cli = install.npx_cli.as_ref().ok_or_else(required)?;
        launch_args.push(cli.display().to_string());
    }
    launch_args.extend(args.iter().cloned());
    let bin_dir = install
        .node
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(required)?;
    Ok(Some(NodeLaunch {
        program: install.node,
        args: launch_args,
        bin_dir,
    }))
}

/// `envs` with `bin_dir` first on `PATH`, after the `PATH` the configuration
/// sets or, failing that, the one this process runs with.
#[must_use]
pub fn with_node_on_path(
    mut envs: HashMap<String, String>,
    bin_dir: &Path,
) -> HashMap<String, String> {
    let key = envs
        .keys()
        .find(|k| k.eq_ignore_ascii_case("PATH"))
        .cloned()
        .unwrap_or_else(|| "PATH".to_owned());
    let current: OsString = envs
        .get(&key)
        .map(OsString::from)
        .or_else(|| std::env::var_os("PATH"))
        .unwrap_or_default();
    let mut dirs = vec![bin_dir.to_path_buf()];
    dirs.extend(std::env::split_paths(&current));
    if let Ok(joined) = std::env::join_paths(dirs) {
        envs.insert(key, joined.to_string_lossy().into_owned());
    }
    envs
}

#[cfg(test)]
mod tests;
