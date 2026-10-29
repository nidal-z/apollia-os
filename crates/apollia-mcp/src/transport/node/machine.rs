//! A Node.js the user installed on the machine, used when the engine store
//! holds none.
//!
//! The engine channel is the preferred source: its release is verified by
//! Apollia before it is extracted. When the channel cannot provide one (not
//! published, offline, refused verification), a Node.js already installed by
//! the user is accepted instead, because the user installed and chose it. The
//! search reads the `PATH` of the process, then the usual install locations
//! and the folders of the common version managers. The only program it runs
//! is the candidate `node` itself, with `--version`, to read its release.
//!
//! A candidate is kept only when it carries npm beside it (`npx-cli.js` and
//! `npm-cli.js` of the same release, the ones Apollia runs) and when its
//! major release is at least [`MIN_MACHINE_NODE_MAJOR`].

use std::collections::HashSet;
use std::ffi::OsString;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use super::{npx_cli_beside, release_numbers, NodeInstall};

/// Oldest major release of a machine's Node.js a server runs on.
///
/// It is the release line the engine channel publishes (22, an LTS line), and
/// the line the locks of the pinned packages are installed and tried with: a
/// package whose dependencies ask for a newer Node.js than the one running it
/// fails at its first call rather than at its addition.
pub const MIN_MACHINE_NODE_MAJOR: u64 = 22;

/// Longest wait for a candidate to answer `--version`. A shim that hangs is
/// skipped rather than blocking the addition.
const VERSION_DEADLINE: Duration = Duration::from_secs(5);

/// The name of the Node.js binary on this platform.
const NODE_BINARY: &str = if cfg!(windows) { "node.exe" } else { "node" };

/// Where a Node.js installed on the machine is looked for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MachineNodeSearch {
    /// The `PATH` searched first, in its order.
    path: Option<OsString>,
    /// The home directory the version managers install under.
    home: Option<PathBuf>,
    /// The usual install directories, searched after the `PATH`.
    install_dirs: Vec<PathBuf>,
}

/// A Node.js found on the machine and accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MachineNode {
    /// The `node` and the npm beside it.
    pub install: NodeInstall,
    /// The release it answered, `v22.11.0`.
    pub version: String,
}

/// A candidate seen and left out, with why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RejectedNode {
    /// The `node` seen.
    pub node: PathBuf,
    /// Why it was left out, readable as is.
    pub reason: String,
}

/// No Node.js on the machine was accepted.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MachineNodeMissing {
    /// Every candidate seen, with why it was left out; empty when none was
    /// found at all.
    pub rejected: Vec<RejectedNode>,
}

impl std::fmt::Display for MachineNodeMissing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.rejected.is_empty() {
            return write!(f, "no Node.js was found on this machine");
        }
        let seen: Vec<String> = self
            .rejected
            .iter()
            .map(|r| format!("{} ({})", r.node.display(), r.reason))
            .collect();
        write!(f, "no usable Node.js on this machine: {}", seen.join("; "))
    }
}

impl MachineNodeSearch {
    /// The search of this process: its `PATH`, its home directory, and the
    /// usual install directories of its platform.
    #[must_use]
    pub fn from_env() -> Self {
        let install_dirs = if cfg!(windows) {
            ["ProgramW6432", "ProgramFiles", "ProgramFiles(x86)"]
                .iter()
                .filter_map(std::env::var_os)
                .map(|dir| PathBuf::from(dir).join("nodejs"))
                .collect()
        } else {
            vec![
                PathBuf::from("/opt/homebrew/bin"),
                PathBuf::from("/usr/local/bin"),
            ]
        };
        Self {
            path: std::env::var_os("PATH"),
            home: apollia_core::paths::home_dir(),
            install_dirs,
        }
    }

    /// A search over the given `PATH`, home directory and install
    /// directories, nothing read from the process.
    #[must_use]
    pub fn new(path: Option<OsString>, home: Option<PathBuf>, install_dirs: Vec<PathBuf>) -> Self {
        Self {
            path,
            home,
            install_dirs,
        }
    }

    /// The directories searched, in order: the `PATH`, the install
    /// directories, then each release of the version managers, newest first.
    fn dirs(&self) -> Vec<PathBuf> {
        let mut dirs: Vec<PathBuf> = self
            .path
            .as_deref()
            .map(|p| std::env::split_paths(p).collect())
            .unwrap_or_default();
        dirs.extend(self.install_dirs.iter().cloned());
        if let Some(home) = &self.home {
            for (root, below) in version_manager_roots(home) {
                dirs.extend(releases_newest_first(&root).map(|r| r.join(below)));
            }
        }
        dirs
    }

    /// The first Node.js of the machine Apollia accepts.
    ///
    /// # Errors
    ///
    /// [`MachineNodeMissing`], with every candidate seen and why it was left
    /// out, when none is accepted.
    pub fn find(&self) -> Result<MachineNode, MachineNodeMissing> {
        let mut seen = HashSet::new();
        let mut rejected = Vec::new();
        for dir in self.dirs() {
            let node = dir.join(NODE_BINARY);
            if !node.is_file() {
                continue;
            }
            let real = std::fs::canonicalize(&node).unwrap_or_else(|_| node.clone());
            if !seen.insert(real.clone()) {
                continue;
            }
            match accept(&node, &real) {
                Ok(found) => {
                    tracing::event!(
                        tracing::Level::INFO,
                        node = %found.install.node.display(),
                        version = %found.version,
                        "mcp.node.machine.found"
                    );
                    return Ok(found);
                }
                Err(reason) => {
                    tracing::event!(
                        tracing::Level::DEBUG,
                        node = %node.display(),
                        reason = %reason,
                        "mcp.node.machine.rejected"
                    );
                    rejected.push(RejectedNode { node, reason });
                }
            }
        }
        Err(MachineNodeMissing { rejected })
    }
}

/// The folders the common version managers keep their releases in, under
/// `home`, each with the path from a release folder to its `node`.
fn version_manager_roots(home: &Path) -> Vec<(PathBuf, &'static str)> {
    if cfg!(windows) {
        let roaming = home.join("AppData").join("Roaming");
        let local = home.join("AppData").join("Local");
        vec![
            (roaming.join("nvm"), ""),
            (local.join("Volta").join("tools/image/node"), ""),
            (roaming.join("fnm").join("node-versions"), "installation"),
        ]
    } else {
        vec![
            (home.join(".nvm/versions/node"), "bin"),
            (home.join(".volta/tools/image/node"), "bin"),
            (home.join(".fnm/node-versions"), "installation/bin"),
            (
                home.join(".local/share/fnm/node-versions"),
                "installation/bin",
            ),
            (
                home.join("Library/Application Support/fnm/node-versions"),
                "installation/bin",
            ),
        ]
    }
}

/// The release folders under `root` (`v22.11.0` or `22.11.0`), newest first.
fn releases_newest_first(root: &Path) -> impl Iterator<Item = PathBuf> {
    let mut releases: Vec<(Vec<u64>, PathBuf)> = std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_str()?.to_owned();
            let tag = if name.starts_with('v') {
                name
            } else {
                format!("v{name}")
            };
            Some((release_numbers(&tag)?, entry.path()))
        })
        .collect();
    releases.sort();
    releases.into_iter().rev().map(|(_, path)| path)
}

/// `node` as a [`MachineNode`], or why it is left out. `real` is the file a
/// link at `node` points to, where a Homebrew release keeps its npm.
fn accept(node: &Path, real: &Path) -> Result<MachineNode, String> {
    let (node, npx_cli) = [node, real]
        .into_iter()
        .find_map(|n| npx_cli_beside(n).map(|cli| (n.to_path_buf(), cli)))
        .ok_or_else(|| "no npm beside it".to_owned())?;
    if !npx_cli.with_file_name("npm-cli.js").is_file() {
        return Err("no npm beside it".to_owned());
    }
    let version = node_version(&node).ok_or_else(|| "it did not answer `--version`".to_owned())?;
    let major = release_numbers(&version)
        .and_then(|n| n.first().copied())
        .ok_or_else(|| format!("it answered `{version}`, not a release"))?;
    if major < MIN_MACHINE_NODE_MAJOR {
        return Err(format!(
            "{version}, older than the {MIN_MACHINE_NODE_MAJOR} release Apollia needs"
        ));
    }
    Ok(MachineNode {
        install: NodeInstall {
            node,
            npx_cli: Some(npx_cli),
        },
        version,
    })
}

/// What `node --version` prints, trimmed, or `None` when it fails or does
/// not answer within [`VERSION_DEADLINE`].
fn node_version(node: &Path) -> Option<String> {
    let mut command = Command::new(node);
    command
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    apollia_core::subprocess_window::hide_console(&mut command);
    let mut child = command.spawn().ok()?;
    let mut stdout = child.stdout.take()?;
    // The output is read on its own thread so the wait for it can be bounded:
    // the read ends when `node` closes its output, that is when it exits.
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let mut out = String::new();
        let read = stdout.read_to_string(&mut out).map(|_| out);
        let _ = tx.send(read);
    });
    let Ok(Ok(out)) = rx.recv_timeout(VERSION_DEADLINE) else {
        // A child that does not answer is stopped and reaped, so no process
        // outlives the search.
        let _ = child.kill();
        let _ = child.wait();
        return None;
    };
    if !child.wait().ok()?.success() {
        return None;
    }
    let version = out.trim();
    (!version.is_empty()).then(|| version.to_owned())
}

#[cfg(test)]
mod tests;
