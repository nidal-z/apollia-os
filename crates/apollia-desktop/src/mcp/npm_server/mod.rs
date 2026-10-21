//! npm MCP servers, installed once and launched offline.
//!
//! A catalogue or registry entry for a Node.js server arrives as
//! `npx -y <package>[@<version>] [args...]`, the upstream convention, which
//! fetches whatever the registry publishes at every start. Apollia does not
//! launch it that way. When such a connection is added, the package is
//! installed once into a directory of its own, `<data>/connections/<name>/`,
//! by the npm of the Node.js the engine store holds, and the launch command is
//! rewritten to `node <absolute path of the package's entry point>`. A later
//! start runs that file on the store's Node.js and never runs npm nor touches
//! the network.
//!
//! What gets installed is fixed at that moment and recorded beside it:
//!
//! - a curated entry carries an exact version and an npm `package-lock.json`,
//!   embedded as npm wrote it, in which every package carries the registry's
//!   `integrity` digest; it is installed with `npm ci`, which refuses the
//!   whole installation on one digest that does not match;
//! - any other entry is installed at the version it names, or the one the
//!   registry resolves at that moment, with `--save-exact`, and the lock npm
//!   writes stays beside it.
//!
//! No install script runs, for any entry: every npm invocation carries
//! `--ignore-scripts`.

mod install;
#[cfg(test)]
mod tests;

use std::path::Path;

use apollia_mcp::config::McpServerConfig;
use serde_json::Value;

use crate::mcp::enrichments::ConnectorEnrichment;

pub use apollia_mcp::npm_package::{installed_package, is_package_name};
pub use install::{InstalledNpmServer, NpmServerInstaller};

/// The launcher name an npm entry arrives with.
const NPX: &str = "npx";

/// The program an installed npm connection is launched with. Bare on
/// purpose: the stdio transport resolves it to the Node.js of the engine
/// store, never to one found on the machine's `PATH`.
pub const NODE: &str = "node";

/// Longest accepted package name or version.
const MAX_TOKEN_LEN: usize = 214;

/// The npm lock of each pinned curated entry, as npm wrote it, by package.
///
/// Regenerate one with `npm install --package-lock-only --ignore-scripts`
/// over a `package.json` naming the package at its exact version, then set
/// the same version as the entry's `package_version`.
const EMBEDDED_LOCKS: &[(&str, &str)] = &[
    (
        "@modelcontextprotocol/server-brave-search",
        include_str!("locks/server-brave-search/package-lock.json"),
    ),
    (
        "@modelcontextprotocol/server-filesystem",
        include_str!("locks/server-filesystem/package-lock.json"),
    ),
    (
        "@modelcontextprotocol/server-memory",
        include_str!("locks/server-memory/package-lock.json"),
    ),
    (
        "@modelcontextprotocol/server-postgres",
        include_str!("locks/server-postgres/package-lock.json"),
    ),
    (
        "@playwright/mcp",
        include_str!("locks/playwright-mcp/package-lock.json"),
    ),
];

/// The npm lock a pinned curated package is installed from: the one the
/// online catalogue this machine accepted gives for it, else the one compiled
/// into the application.
pub fn pinned_lock(package: &str) -> Option<std::borrow::Cow<'static, str>> {
    crate::mcp::remote_catalogue::kept_on_this_machine()
        .and_then(|remote| remote.npm_lock(package))
        .map(std::borrow::Cow::Owned)
        .or_else(|| embedded_lock(package).map(std::borrow::Cow::Borrowed))
}

/// The embedded npm lock of a curated package, if it has one.
pub fn embedded_lock(package: &str) -> Option<&'static str> {
    EMBEDDED_LOCKS
        .iter()
        .find(|(name, _)| *name == package)
        .map(|(_, lock)| *lock)
}

/// Errors raised while turning an `npx` connection into an installed one.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum NpmServerError {
    /// The `npx` argument list names no package, or uses an option Apollia
    /// does not translate.
    #[error("cannot read the npm server command: {0}")]
    Invocation(String),
    /// The lock is malformed, or does not pin the package it is supposed to
    /// install, or lets a package through without an integrity digest.
    #[error("invalid npm lock for '{package}': {reason}")]
    InvalidLock {
        /// The package the lock belongs to.
        package: String,
        /// What is wrong with it.
        reason: String,
    },
    /// The engine store holds no Node.js, or one without npm.
    #[error("installing '{0}' requires Node.js, which is not installed")]
    NodeRequired(String),
    /// npm refused or failed the installation (a digest mismatch lands here).
    #[error("failed to install '{package}': {stderr}")]
    Install {
        /// The package being installed.
        package: String,
        /// npm's error output.
        stderr: String,
    },
    /// The package installed but does not provide an entry point, or not at
    /// the expected version.
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
        path: std::path::PathBuf,
        /// The underlying error.
        #[source]
        source: std::io::Error,
    },
}

/// An `npx` command line, read into what Apollia installs and runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NpxInvocation {
    /// Package name, `@scope/name` or `name`.
    pub package: String,
    /// Exact version, when the command line pins one.
    pub version: Option<String>,
    /// Arguments forwarded to the server.
    pub forwarded: Vec<String>,
}

impl NpxInvocation {
    /// Read the arguments that followed `npx`.
    ///
    /// Accepts `[-y | --yes] <package>[@<version>] [args]`. Any other option
    /// before the package is refused rather than dropped: `--package` or
    /// `--registry` would install something other than what the entry says.
    /// A version is one exact version; a range or a tag such as `latest` is
    /// refused, since it would not say what runs.
    ///
    /// # Errors
    ///
    /// [`NpmServerError::Invocation`] when no package is named, an option is
    /// not understood, or the name or version is not acceptable.
    pub fn parse(args: &[String]) -> Result<Self, NpmServerError> {
        let mut rest = args
            .iter()
            .skip_while(|a| matches!(a.as_str(), "-y" | "--yes"));
        let spec = rest
            .next()
            .ok_or_else(|| NpmServerError::Invocation("no package named".into()))?;
        if spec.starts_with('-') {
            return Err(NpmServerError::Invocation(format!(
                "option `{spec}` is not supported"
            )));
        }
        let (package, version) = split_spec(spec);
        if !is_package_name(package) {
            return Err(NpmServerError::Invocation(format!(
                "`{spec}` is not an npm package name"
            )));
        }
        if let Some(v) = version {
            if !is_exact_version(v) {
                return Err(NpmServerError::Invocation(format!(
                    "`{spec}` does not name one exact version"
                )));
            }
        }
        Ok(Self {
            package: package.to_string(),
            version: version.map(str::to_string),
            forwarded: rest.cloned().collect(),
        })
    }

    /// The spec handed to `npm install` when no lock applies.
    pub fn install_spec(&self, version: Option<&str>) -> String {
        match version {
            Some(v) => format!("{}@{v}", self.package),
            None => self.package.clone(),
        }
    }
}

/// `@scope/name@1.2.3` into `@scope/name` and `1.2.3`.
fn split_spec(spec: &str) -> (&str, Option<&str>) {
    let search_from = usize::from(spec.starts_with('@'));
    match spec[search_from..].find('@') {
        Some(at) => (
            &spec[..search_from + at],
            Some(&spec[search_from + at + 1..]),
        ),
        None => (spec, None),
    }
}

/// One exact semantic version: `1.2.3`, with an optional `-pre` and `+build`.
pub fn is_exact_version(version: &str) -> bool {
    if version.is_empty() || version.len() > MAX_TOKEN_LEN {
        return false;
    }
    let (core, build) = match version.split_once('+') {
        Some((c, b)) => (c, Some(b)),
        None => (version, None),
    };
    let (core, pre) = match core.split_once('-') {
        Some((c, p)) => (c, Some(p)),
        None => (core, None),
    };
    let numbers: Vec<&str> = core.split('.').collect();
    let label = |s: &str| {
        !s.is_empty()
            && s.split('.').all(|id| {
                !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
            })
    };
    numbers.len() == 3
        && numbers
            .iter()
            .all(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
        && pre.is_none_or(label)
        && build.is_none_or(label)
}

/// An embedded npm lock, checked against the package it must install.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NpmLock {
    /// The lock, exactly as embedded; written as `package-lock.json`.
    pub raw: String,
    /// The `package.json` matching the lock's root, written beside it so that
    /// `npm ci` finds the two in agreement.
    pub manifest: String,
}

impl NpmLock {
    /// Check `raw` as the lock of `package` at `version`.
    ///
    /// The root must depend on `package` alone, the lock must install it at
    /// `version`, and every package it lists must carry a `sha512` integrity
    /// digest and an address to fetch it from: a package without one is a
    /// package `npm ci` would accept unverified.
    ///
    /// # Errors
    ///
    /// [`NpmServerError::InvalidLock`] naming the first defect.
    pub fn check(package: &str, version: &str, raw: &str) -> Result<Self, NpmServerError> {
        let invalid = |reason: String| NpmServerError::InvalidLock {
            package: package.to_string(),
            reason,
        };
        let lock: Value =
            serde_json::from_str(raw).map_err(|e| invalid(format!("not JSON: {e}")))?;
        if lock.get("lockfileVersion").and_then(Value::as_u64) < Some(2) {
            return Err(invalid("lockfileVersion 2 or later is required".into()));
        }
        let packages = lock
            .get("packages")
            .and_then(Value::as_object)
            .ok_or_else(|| invalid("no `packages` table".into()))?;
        let root = packages
            .get("")
            .and_then(Value::as_object)
            .ok_or_else(|| invalid("no root package".into()))?;
        let deps = root
            .get("dependencies")
            .and_then(Value::as_object)
            .ok_or_else(|| invalid("the root declares no dependency".into()))?;
        if deps.len() != 1 || !deps.contains_key(package) {
            return Err(invalid(format!(
                "the root must depend on `{package}` alone"
            )));
        }
        let own = packages
            .get(&format!("node_modules/{package}"))
            .and_then(|p| p.get("version"))
            .and_then(Value::as_str);
        if own != Some(version) {
            return Err(invalid(format!(
                "the lock installs {} where the entry says {version}",
                own.unwrap_or("nothing")
            )));
        }
        for (path, entry) in packages.iter().filter(|(path, _)| !path.is_empty()) {
            if entry.get("link").and_then(Value::as_bool) == Some(true) {
                return Err(invalid(format!("`{path}` is a link")));
            }
            let integrity = entry.get("integrity").and_then(Value::as_str);
            if !integrity.is_some_and(|i| i.starts_with("sha512-")) {
                return Err(invalid(format!("`{path}` carries no sha512 integrity")));
            }
            if entry
                .get("resolved")
                .and_then(Value::as_str)
                .is_none_or(str::is_empty)
            {
                return Err(invalid(format!("`{path}` says nowhere to fetch it from")));
            }
        }
        let manifest = serde_json::json!({
            "name": root.get("name").cloned().unwrap_or(Value::from("apollia-connection")),
            "version": root.get("version").cloned().unwrap_or(Value::from("0.0.0")),
            "private": true,
            "dependencies": deps,
        });
        let manifest = serde_json::to_string_pretty(&manifest)
            .map_err(|e| invalid(format!("cannot render the manifest: {e}")))?;
        Ok(Self {
            raw: raw.to_string(),
            manifest,
        })
    }
}

/// What an `npx` connection resolves to before anything is installed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NpmServerPlan {
    /// The command line, read.
    pub invocation: NpxInvocation,
    /// The version to install, when one is known before installing.
    pub version: Option<String>,
    /// The lock to install from, for a pinned curated entry.
    pub lock: Option<NpmLock>,
}

/// Decide what to install for an `npx` connection, `None` for anything else.
///
/// A pinned curated entry for the same package brings its exact version and
/// its lock; a command line that asks another version than the pinned one is
/// refused, since the lock only vouches for the version it was written for.
///
/// # Errors
///
/// [`NpmServerError::Invocation`] or [`NpmServerError::InvalidLock`].
pub fn plan(
    config: &McpServerConfig,
    enrichments: &[ConnectorEnrichment],
) -> Result<Option<NpmServerPlan>, NpmServerError> {
    if config.transport != "stdio" || !is_npx(&config.command) {
        return Ok(None);
    }
    let invocation = NpxInvocation::parse(&config.args)?;
    let curated = enrichments.iter().find(|e| {
        e.package_registry_type.as_deref() == Some("npm")
            && e.package_identifier == invocation.package
    });
    let Some(pinned) = curated.and_then(|e| e.package_version.clone()) else {
        let version = invocation.version.clone();
        return Ok(Some(NpmServerPlan {
            invocation,
            version,
            lock: None,
        }));
    };
    let raw = pinned_lock(&invocation.package).ok_or_else(|| NpmServerError::InvalidLock {
        package: invocation.package.clone(),
        reason: "the entry is pinned but carries no lock".into(),
    })?;
    if let Some(asked) = &invocation.version {
        if *asked != pinned {
            return Err(NpmServerError::Invocation(format!(
                "the curated entry is pinned at {pinned}, not {asked}"
            )));
        }
    }
    let lock = NpmLock::check(&invocation.package, &pinned, &raw)?;
    Ok(Some(NpmServerPlan {
        invocation,
        version: Some(pinned),
        lock: Some(lock),
    }))
}

/// `npx`, bare or as a path, with or without `.cmd` or `.exe`.
fn is_npx(command: &str) -> bool {
    Path::new(command)
        .file_stem()
        .and_then(|s| s.to_str())
        .is_some_and(|stem| stem.eq_ignore_ascii_case(NPX))
}

/// Install an `npx` connection and rewrite its command; return any other
/// configuration unchanged.
///
/// Called on every path that hands a launch configuration to the runtime:
/// the test before adding, the addition, and the edit of an installed one.
/// On failure nothing is left behind and nothing is launched.
///
/// # Errors
///
/// Any [`NpmServerError`]; the caller reports it to the user.
pub async fn prepare(
    mut config: McpServerConfig,
    enrichments: &[ConnectorEnrichment],
    installer: &NpmServerInstaller,
) -> Result<McpServerConfig, NpmServerError> {
    let Some(plan) = plan(&config, enrichments)? else {
        return Ok(config);
    };
    let installed = installer.provision(&config.name, &plan).await?;
    config.command = NODE.to_string();
    config.args = std::iter::once(installed.entry_path.display().to_string())
        .chain(plan.invocation.forwarded)
        .collect();
    Ok(config)
}

/// Whether `config` is launched by `npx` and so needs Node.js to install.
pub fn needs_install(config: &McpServerConfig) -> bool {
    config.transport == "stdio" && is_npx(&config.command)
}
