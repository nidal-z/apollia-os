//! Python MCP servers on the interpreter Apollia bundles.
//!
//! A catalogue or registry entry for a Python server arrives as
//! `uvx <package> [args...]`, the upstream convention. Apollia does not ship
//! `uv` and does not ask anyone to install it: when such a connection is added,
//! the package is installed once into a virtualenv of its own,
//! `<data>/connections/<name>/venv`, built by the bundled interpreter the same
//! way [`PythonExecutor`] builds an agent's, and the launch command is
//! rewritten to the absolute path of the package's entry point in that venv.
//!
//! What gets installed is fixed at that moment and recorded beside the venv:
//!
//! - a curated entry carries an exact version and a hash lock of its whole
//!   dependency closure ([`ConnectorEnrichment::package_lock`]); pip installs
//!   it with `--require-hashes`, so one altered digest refuses everything;
//! - any other entry is installed at the version pip resolves at that moment,
//!   or the one it names, and that version is written down.
//!
//! The rewritten command names the venv directly, so a later start spawns the
//! entry point and never runs pip or touches the network.
//!
//! [`PythonExecutor`]: apollia_tools::tools::python_executor::PythonExecutor

mod install;
#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};

use apollia_mcp::config::McpServerConfig;
use apollia_tools::tools::python_executor::{validate_package_spec, venv_dir_component};

use crate::mcp::enrichments::{ConnectorEnrichment, LockedRequirement};

pub use install::{InstalledServer, PythonServerInstaller};

/// The launcher name a Python entry arrives with.
const UVX: &str = "uvx";

/// Longest accepted distribution name or version.
const MAX_TOKEN_LEN: usize = 128;

/// Errors raised while turning a `uvx` connection into an installed one.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PythonServerError {
    /// The `uvx` argument list names no package, or uses an option Apollia
    /// does not translate.
    #[error("cannot read the Python server command: {0}")]
    Invocation(String),
    /// A lock line is malformed, or the lock does not pin the package it is
    /// supposed to install.
    #[error("invalid hash lock for '{package}': {reason}")]
    InvalidLock {
        /// The package the lock belongs to.
        package: String,
        /// What is wrong with it.
        reason: String,
    },
    /// No Python interpreter is available to build the virtualenv.
    #[error("no Python interpreter to install the server with: {0}")]
    Interpreter(String),
    /// The virtualenv could not be created.
    #[error("failed to create the virtualenv of '{name}': {reason}")]
    Venv {
        /// Connection name.
        name: String,
        /// The interpreter's error.
        reason: String,
    },
    /// pip refused or failed the installation (a digest mismatch lands here).
    #[error("failed to install '{package}': {stderr}")]
    Install {
        /// The package being installed.
        package: String,
        /// pip's error output.
        stderr: String,
    },
    /// The package installed but does not provide the expected entry point,
    /// or not at the expected version.
    #[error("installed package '{package}' is unusable: {reason}")]
    Installed {
        /// The package.
        package: String,
        /// What is missing.
        reason: String,
    },
    /// A filesystem operation on the connection directory failed.
    #[error("connection directory {path}: {source}")]
    Io {
        /// The path involved.
        path: PathBuf,
        /// The underlying error.
        #[source]
        source: std::io::Error,
    },
}

/// A `uvx` command line, read into what Apollia installs and runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UvxInvocation {
    /// Distribution name, as written.
    pub package: String,
    /// Exact version, when the command line pins one (`pkg==1.2`, `pkg@1.2`).
    pub version: Option<String>,
    /// Console script to run; the package name unless `--from` names another.
    pub entry_point: String,
    /// Arguments forwarded to the server.
    pub forwarded: Vec<String>,
}

impl UvxInvocation {
    /// Read the arguments that followed `uvx`.
    ///
    /// Accepts `<package> [args]`, `<package>==<version> [args]`,
    /// `<package>@<version> [args]` and `--from <package> <command> [args]`.
    /// Any other option before the package is refused rather than dropped:
    /// silently ignoring an index URL or an extra requirement would install
    /// something other than what the entry says.
    ///
    /// # Errors
    ///
    /// [`PythonServerError::Invocation`] when no package is named, an option
    /// is not understood, or the name or version is not a plain token.
    pub fn parse(args: &[String]) -> Result<Self, PythonServerError> {
        let (spec, entry, rest) = match args.first().map(String::as_str) {
            Some("--from") => {
                let spec = args.get(1).ok_or_else(|| {
                    PythonServerError::Invocation("`--from` names no package".into())
                })?;
                let entry = args.get(2).ok_or_else(|| {
                    PythonServerError::Invocation("`--from` names no command".into())
                })?;
                (spec, Some(entry.clone()), &args[3..])
            }
            Some(first) if first.starts_with('-') => {
                return Err(PythonServerError::Invocation(format!(
                    "option `{first}` is not supported"
                )));
            }
            Some(_) => (&args[0], None, &args[1..]),
            None => {
                return Err(PythonServerError::Invocation("no package named".into()));
            }
        };
        let (package, version) = split_spec(spec)?;
        let entry_point = entry.unwrap_or_else(|| package.clone());
        if !is_plain_token(&entry_point) {
            return Err(PythonServerError::Invocation(format!(
                "`{entry_point}` is not a command name"
            )));
        }
        Ok(Self {
            package,
            version,
            entry_point,
            forwarded: rest.to_vec(),
        })
    }

    /// The requirement handed to pip when no lock applies.
    pub fn requirement(&self, version: Option<&str>) -> String {
        match version {
            Some(v) => format!("{}=={v}", self.package),
            None => self.package.clone(),
        }
    }
}

/// Split `pkg`, `pkg==1.2` or `pkg@1.2` into name and optional version.
fn split_spec(spec: &str) -> Result<(String, Option<String>), PythonServerError> {
    let (name, version) = if let Some((n, v)) = spec.split_once("==") {
        (n, Some(v))
    } else if let Some((n, v)) = spec.split_once('@') {
        (n, Some(v))
    } else {
        (spec, None)
    };
    if !is_plain_token(name) {
        return Err(PythonServerError::Invocation(format!(
            "`{spec}` is not a package name"
        )));
    }
    if let Some(v) = version {
        if !is_plain_token(v) || v == "latest" {
            return Err(PythonServerError::Invocation(format!(
                "`{spec}` does not name one exact version"
            )));
        }
    }
    Ok((name.to_string(), version.map(str::to_string)))
}

/// A name or version made of `[A-Za-z0-9._+!-]`, starting alphanumeric.
///
/// Narrow on purpose: whatever passes ends up as an argv element of pip or as
/// a file name, so a leading `-` or a path separator must never get through.
fn is_plain_token(token: &str) -> bool {
    !token.is_empty()
        && token.len() <= MAX_TOKEN_LEN
        && token
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric())
        && token
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '+' | '!'))
}

/// PEP 503 normalisation: lowercase, runs of `-`, `_`, `.` folded to `-`.
pub fn normalize_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut in_separator = false;
    for c in name.chars() {
        if matches!(c, '-' | '_' | '.') {
            if !in_separator {
                out.push('-');
            }
            in_separator = true;
        } else {
            out.push(c.to_ascii_lowercase());
            in_separator = false;
        }
    }
    out
}

/// Render a hash lock as a pip requirements file, refusing any line that is
/// not exactly `name==version [; marker] --hash=sha256:<hex>...`.
///
/// The lock is data from the embedded catalogue, but every field still ends up
/// in a file pip parses, where an option such as `--index-url` would change
/// where packages come from. Each field is therefore checked for shape rather
/// than trusted.
///
/// # Errors
///
/// [`PythonServerError::InvalidLock`] on the first malformed line.
pub fn render_requirements(
    package: &str,
    lock: &[LockedRequirement],
) -> Result<String, PythonServerError> {
    let invalid = |reason: String| PythonServerError::InvalidLock {
        package: package.to_string(),
        reason,
    };
    let mut out = String::new();
    for req in lock {
        if !is_plain_token(&req.name) || !is_plain_token(&req.version) {
            return Err(invalid(format!(
                "`{}=={}` is not a name and one exact version",
                req.name, req.version
            )));
        }
        let mut spec = format!("{}=={}", req.name, req.version);
        if let Some(marker) = &req.marker {
            // pip reads a requirements line as whitespace-separated tokens and
            // takes the first one starting with `-` as an option, `#` as a
            // comment. A quoted marker literal is otherwise opaque, so either
            // character in it could smuggle an option past the checks below.
            if marker.contains(['-', '#', '\\']) {
                return Err(invalid(format!(
                    "the marker of `{}` carries `-`, `#` or `\\`",
                    req.name
                )));
            }
            spec = format!("{spec} ; {marker}");
        }
        validate_package_spec(&spec).map_err(|e| invalid(e.to_string()))?;
        if req.hashes.is_empty() {
            return Err(invalid(format!("`{}` carries no digest", req.name)));
        }
        out.push_str(&spec);
        for hash in &req.hashes {
            if !is_sha256_digest(hash) {
                return Err(invalid(format!(
                    "`{hash}` is not a sha256 digest of `{}`",
                    req.name
                )));
            }
            out.push_str(" --hash=");
            out.push_str(hash);
        }
        out.push('\n');
    }
    Ok(out)
}

/// `sha256:` followed by 64 lowercase hex digits.
fn is_sha256_digest(hash: &str) -> bool {
    hash.strip_prefix("sha256:").is_some_and(|hex| {
        hex.len() == 64 && hex.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f'))
    })
}

/// What a `uvx` connection resolves to before anything is installed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PythonServerPlan {
    /// The command line, read.
    pub invocation: UvxInvocation,
    /// The version to install, when one is known before installing.
    pub version: Option<String>,
    /// The hash lock to install from, for a curated entry.
    pub lock: Vec<LockedRequirement>,
}

/// Decide what to install for a `uvx` connection, `None` for anything else.
///
/// A curated entry for the same package brings its exact version and its lock;
/// a command line that pins another version than the curated one is refused,
/// since the lock only vouches for the version it was written for.
///
/// # Errors
///
/// [`PythonServerError::Invocation`] or [`PythonServerError::InvalidLock`].
pub fn plan(
    config: &McpServerConfig,
    enrichments: &[ConnectorEnrichment],
) -> Result<Option<PythonServerPlan>, PythonServerError> {
    if config.transport != "stdio" || !is_uvx(&config.command) {
        return Ok(None);
    }
    let invocation = UvxInvocation::parse(&config.args)?;
    let wanted = normalize_name(&invocation.package);
    let curated = enrichments.iter().find(|e| {
        e.package_registry_type.as_deref() == Some("pypi")
            && normalize_name(&e.package_identifier) == wanted
    });
    let Some(curated) = curated.filter(|e| !e.package_lock.is_empty()) else {
        let version = invocation
            .version
            .clone()
            .or_else(|| curated.and_then(|e| e.package_version.clone()));
        return Ok(Some(PythonServerPlan {
            invocation,
            version,
            lock: Vec::new(),
        }));
    };
    let invalid = |reason: String| PythonServerError::InvalidLock {
        package: invocation.package.clone(),
        reason,
    };
    let pinned = curated
        .package_version
        .clone()
        .ok_or_else(|| invalid("the entry has a lock but no version".into()))?;
    let locked_self = curated
        .package_lock
        .iter()
        .find(|r| normalize_name(&r.name) == wanted)
        .ok_or_else(|| invalid("the lock does not list the package itself".into()))?;
    if locked_self.version != pinned {
        return Err(invalid(format!(
            "the lock pins {} where the entry says {pinned}",
            locked_self.version
        )));
    }
    if let Some(asked) = &invocation.version {
        if *asked != pinned {
            return Err(PythonServerError::Invocation(format!(
                "the curated entry is pinned at {pinned}, not {asked}"
            )));
        }
    }
    Ok(Some(PythonServerPlan {
        lock: curated.package_lock.clone(),
        version: Some(pinned),
        invocation,
    }))
}

/// `uvx`, bare or as a path, with or without `.exe`.
fn is_uvx(command: &str) -> bool {
    Path::new(command)
        .file_stem()
        .and_then(|s| s.to_str())
        .is_some_and(|stem| stem.eq_ignore_ascii_case(UVX))
}

/// Install a `uvx` connection and rewrite its command; return any other
/// configuration unchanged.
///
/// Called on every path that hands a launch configuration to the runtime:
/// the test before adding, the addition, and the edit of an installed one.
/// On failure nothing is left behind and nothing is launched.
///
/// # Errors
///
/// Any [`PythonServerError`]; the caller reports it to the user.
pub async fn prepare(
    mut config: McpServerConfig,
    enrichments: &[ConnectorEnrichment],
    installer: &PythonServerInstaller,
) -> Result<McpServerConfig, PythonServerError> {
    let Some(plan) = plan(&config, enrichments)? else {
        return Ok(config);
    };
    let installed = installer.provision(&config.name, &plan).await?;
    config.command = installed.entry_path.display().to_string();
    config.args = plan.invocation.forwarded;
    Ok(config)
}

/// Directory holding every connection's own files: `<data>/connections`.
pub fn connections_dir() -> PathBuf {
    let home = apollia_core::paths::home_dir_or_temp();
    apollia_core::paths::data_dir_under(home).join("connections")
}

/// Directory of one connection: `<connections>/<name>`.
pub fn connection_dir(connections: &Path, name: &str) -> PathBuf {
    connections.join(venv_dir_component(name))
}

/// Delete a connection's directory, venv included. Absent is not an error.
///
/// # Errors
///
/// [`PythonServerError::Io`] when the directory exists and cannot be removed.
pub fn remove_connection_dir(connections: &Path, name: &str) -> Result<(), PythonServerError> {
    install::remove_dir(&connection_dir(connections, name))
}
