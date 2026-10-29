//! Installing a connection's npm package into its own directory.

use std::path::{Component, Path, PathBuf};

use apollia_mcp::transport::node::{
    default_engines_root, installed_node, MachineNodeSearch, NodeInstall,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{NpmServerError, NpmServerPlan};
use crate::mcp::python_server::connection_dir;

/// File, in the connection directory, recording what was installed.
const RECORD_FILE: &str = "server.json";

/// The registry every installation reads from.
const DEFAULT_REGISTRY: &str = "https://registry.npmjs.org/";

/// What a connection's directory holds, written once the installation
/// succeeded.
///
/// A later addition of the same connection reads it back and, when it still
/// describes what is asked, reuses the installation without running npm.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledNpmServer {
    /// Package name.
    pub package: String,
    /// The exact version installed, read from the package after installing.
    pub version: String,
    /// The entry point, relative to the package directory.
    pub bin: String,
    /// `true` when the installation went through the curated lock.
    pub locked: bool,
    /// Absolute path of the entry point. Not persisted: derived from the
    /// connection directory.
    #[serde(skip)]
    pub entry_path: PathBuf,
}

/// Installs npm MCP servers, one directory per connection.
#[derive(Debug, Clone)]
pub struct NpmServerInstaller {
    connections: PathBuf,
    /// The Node.js to run npm with.
    node: NodeChoice,
    /// The registry npm reads.
    registry: String,
    /// How many times npm retries a fetch; npm's own default when `None`.
    fetch_retries: Option<u32>,
}

/// Which Node.js an installer runs npm with.
#[derive(Debug, Clone)]
enum NodeChoice {
    /// This one, whatever is installed. Tests only.
    #[cfg(test)]
    Fixed(NodeInstall),
    /// The engine store's under `engines_root`, or failing that the one the
    /// search finds on the machine, with the npm of the same release.
    Discover {
        engines_root: Option<PathBuf>,
        machine: MachineNodeSearch,
    },
}

impl NpmServerInstaller {
    /// An installer rooted at `connections` (normally
    /// [`crate::mcp::python_server::connections_dir`]), running the npm of the
    /// Node.js the engine store holds, or failing that of the Node.js installed
    /// on the machine, against the public registry.
    pub fn new(connections: PathBuf) -> Self {
        Self {
            connections,
            node: NodeChoice::Discover {
                engines_root: default_engines_root(),
                machine: MachineNodeSearch::from_env(),
            },
            registry: DEFAULT_REGISTRY.to_string(),
            fetch_retries: None,
        }
    }

    /// An installer running `node` against `registry`, without retrying a
    /// fetch. Tests only: the product always runs the engine store's Node.js
    /// on the public registry, and retries the way npm does.
    #[cfg(test)]
    pub fn using(connections: PathBuf, node: NodeInstall, registry: String) -> Self {
        Self {
            connections,
            node: NodeChoice::Fixed(node),
            registry,
            fetch_retries: Some(0),
        }
    }

    /// An installer that looks for Node.js the way the product does, in the
    /// store at `engines_root` then through `machine`, against `registry`.
    /// Tests only.
    #[cfg(test)]
    pub fn discovering(
        connections: PathBuf,
        engines_root: PathBuf,
        machine: MachineNodeSearch,
        registry: String,
    ) -> Self {
        Self {
            connections,
            node: NodeChoice::Discover {
                engines_root: Some(engines_root),
                machine,
            },
            registry,
            fetch_retries: Some(0),
        }
    }

    /// The directory of a connection.
    pub fn connection_path(&self, name: &str) -> PathBuf {
        connection_dir(&self.connections, name)
    }

    /// Make sure `plan` is installed for connection `name`, and say where its
    /// entry point is.
    ///
    /// When the record in the connection directory already matches the plan
    /// and the entry point is there, nothing runs: no Node.js, no npm, no
    /// network. Otherwise the directory is rebuilt from scratch; on any
    /// failure it is removed, so a refused installation leaves nothing that
    /// could be launched.
    ///
    /// # Errors
    ///
    /// Any [`NpmServerError`] from installing.
    pub async fn provision(
        &self,
        name: &str,
        plan: &NpmServerPlan,
    ) -> Result<InstalledNpmServer, NpmServerError> {
        if let Some(existing) = self.reusable(name, plan) {
            tracing::event!(
                tracing::Level::INFO,
                connection = %name,
                package = %existing.package,
                version = %existing.version,
                "mcp.npm_server.reused"
            );
            return Ok(existing);
        }
        let dir = self.connection_path(name);
        remove_dir(&dir)?;
        match self.install(name, plan, &dir).await {
            Ok(installed) => Ok(installed),
            Err(e) => {
                if let Err(cleanup) = remove_dir(&dir) {
                    tracing::event!(
                        tracing::Level::WARN,
                        connection = %name,
                        error = %cleanup,
                        "mcp.npm_server.cleanup.failed"
                    );
                }
                tracing::event!(
                    tracing::Level::WARN,
                    connection = %name,
                    package = %plan.invocation.package,
                    error = %e,
                    "mcp.npm_server.install.failed"
                );
                Err(e)
            }
        }
    }

    /// The recorded installation, when it satisfies `plan` as it stands.
    fn reusable(&self, name: &str, plan: &NpmServerPlan) -> Option<InstalledNpmServer> {
        let dir = self.connection_path(name);
        let raw = std::fs::read_to_string(dir.join(RECORD_FILE)).ok()?;
        let mut record: InstalledNpmServer = serde_json::from_str(&raw).ok()?;
        let same_package = record.package == plan.invocation.package;
        let same_lock = record.locked == plan.lock.is_some();
        let same_version = plan.version.as_ref().is_none_or(|v| *v == record.version);
        if !(same_package && same_lock && same_version) {
            return None;
        }
        record.entry_path = entry_path(&dir, &record.package, &record.bin)?;
        record.entry_path.is_file().then_some(record)
    }

    fn node(&self, package: &str) -> Result<(NodeInstall, PathBuf), NpmServerError> {
        let required = || NpmServerError::NodeRequired(package.to_string());
        let node = match &self.node {
            #[cfg(test)]
            NodeChoice::Fixed(node) => node.clone(),
            NodeChoice::Discover {
                engines_root,
                machine,
            } => match engines_root.as_deref().and_then(installed_node) {
                Some(node) => node,
                None => {
                    let found = machine.find().map_err(|_| required())?;
                    tracing::event!(
                        tracing::Level::INFO,
                        package = %package,
                        node = %found.install.node.display(),
                        version = %found.version,
                        "mcp.npm_server.machine_node"
                    );
                    found.install
                }
            },
        };
        let npm_cli = node
            .npx_cli
            .as_ref()
            .map(|npx| npx.with_file_name("npm-cli.js"))
            .filter(|p| p.is_file())
            .ok_or_else(required)?;
        Ok((node, npm_cli))
    }

    async fn install(
        &self,
        name: &str,
        plan: &NpmServerPlan,
        dir: &Path,
    ) -> Result<InstalledNpmServer, NpmServerError> {
        let package = &plan.invocation.package;
        let (node, npm_cli) = self.node(package)?;
        let io = |path: PathBuf| move |source| NpmServerError::Io { path, source };
        std::fs::create_dir_all(dir).map_err(io(dir.to_path_buf()))?;

        // Empty configuration files stand for the user's and the global one,
        // so a registry, a proxy or a script setting from the machine never
        // changes what gets installed. They and npm's cache live in a scratch
        // directory removed once npm is done.
        let scratch = dir.join(".npm-scratch");
        std::fs::create_dir_all(&scratch).map_err(io(scratch.clone()))?;
        let user_rc = scratch.join("user.npmrc");
        let global_rc = scratch.join("global.npmrc");
        for rc in [&user_rc, &global_rc] {
            std::fs::write(rc, "").map_err(io(rc.clone()))?;
        }
        let cache = scratch.join("cache");
        let mut args: Vec<String> = vec![npm_cli.display().to_string()];
        match &plan.lock {
            Some(lock) => {
                let manifest = dir.join("package.json");
                std::fs::write(&manifest, &lock.manifest).map_err(io(manifest))?;
                let lock_path = dir.join("package-lock.json");
                std::fs::write(&lock_path, &lock.raw).map_err(io(lock_path))?;
                args.push("ci".into());
            }
            None => {
                let manifest = dir.join("package.json");
                let body = "{\n  \"name\": \"apollia-connection\",\n  \"version\": \"0.0.0\",\n  \"private\": true\n}\n";
                std::fs::write(&manifest, body).map_err(io(manifest))?;
                args.extend([
                    "install".to_string(),
                    "--save-exact".to_string(),
                    plan.invocation.install_spec(plan.version.as_deref()),
                ]);
            }
        }
        args.extend([
            "--ignore-scripts".to_string(),
            "--no-audit".to_string(),
            "--no-fund".to_string(),
            "--no-update-notifier".to_string(),
            "--loglevel=error".to_string(),
            format!("--registry={}", self.registry),
            format!("--cache={}", cache.display()),
            format!("--userconfig={}", user_rc.display()),
            format!("--globalconfig={}", global_rc.display()),
        ]);
        if let Some(retries) = self.fetch_retries {
            args.push(format!("--fetch-retries={retries}"));
        }

        tracing::event!(
            tracing::Level::INFO,
            connection = %name,
            package = %package,
            locked = plan.lock.is_some(),
            "mcp.npm_server.installing"
        );
        let mut npm = tokio::process::Command::new(&node.node);
        apollia_core::subprocess_window::hide_console_async(&mut npm);
        for (key, _) in std::env::vars_os() {
            if key
                .to_str()
                .is_some_and(|k| k.to_ascii_lowercase().starts_with("npm_config_"))
            {
                npm.env_remove(key);
            }
        }
        if let Some(bin) = node.node.parent() {
            let path = std::env::var_os("PATH").unwrap_or_default();
            let dirs = std::iter::once(bin.to_path_buf()).chain(std::env::split_paths(&path));
            if let Ok(joined) = std::env::join_paths(dirs) {
                npm.env("PATH", joined);
            }
        }
        let output = npm
            .args(&args)
            .current_dir(dir)
            .output()
            .await
            .map_err(|e| NpmServerError::Install {
                package: package.clone(),
                stderr: e.to_string(),
            })?;
        if !output.status.success() {
            return Err(NpmServerError::Install {
                package: package.clone(),
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            });
        }
        remove_dir(&scratch)?;

        let unusable = |reason: String| NpmServerError::Installed {
            package: package.clone(),
            reason,
        };
        let manifest_path = dir.join("node_modules").join(package).join("package.json");
        let manifest: Value = std::fs::read_to_string(&manifest_path)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .ok_or_else(|| unusable("no package.json in node_modules".into()))?;
        let version = manifest
            .get("version")
            .and_then(Value::as_str)
            .ok_or_else(|| unusable("its package.json names no version".into()))?
            .to_string();
        if let Some(wanted) = &plan.version {
            if *wanted != version {
                return Err(unusable(format!("{version} installed, {wanted} expected")));
            }
        }
        let bin = entry_point(&manifest, package)
            .ok_or_else(|| unusable("its package.json declares no command".into()))?;
        let entry = entry_path(dir, package, &bin)
            .filter(|p| p.is_file())
            .ok_or_else(|| unusable(format!("no file at `{bin}`")))?;

        let record = InstalledNpmServer {
            package: package.clone(),
            version,
            bin,
            locked: plan.lock.is_some(),
            entry_path: entry,
        };
        let record_path = dir.join(RECORD_FILE);
        let json = serde_json::to_string_pretty(&record).map_err(|e| NpmServerError::Io {
            path: record_path.clone(),
            source: std::io::Error::other(e),
        })?;
        std::fs::write(&record_path, json).map_err(io(record_path))?;
        tracing::event!(
            tracing::Level::INFO,
            connection = %name,
            package = %record.package,
            version = %record.version,
            locked = record.locked,
            "mcp.npm_server.installed"
        );
        Ok(record)
    }
}

/// The command a package declares in its `bin`: the only one, or the one
/// named after the package without its scope.
fn entry_point(manifest: &Value, package: &str) -> Option<String> {
    match manifest.get("bin")? {
        Value::String(path) => Some(path.clone()),
        Value::Object(commands) => {
            let unscoped = package.rsplit('/').next().unwrap_or(package);
            let chosen = if commands.len() == 1 {
                commands.values().next()
            } else {
                commands.get(unscoped)
            };
            chosen.and_then(Value::as_str).map(str::to_string)
        }
        _ => None,
    }
}

/// `<dir>/node_modules/<package>/<bin>`, refused when `bin` is absolute or
/// climbs out of the package directory.
fn entry_path(dir: &Path, package: &str, bin: &str) -> Option<PathBuf> {
    let relative = Path::new(bin);
    let inside = relative
        .components()
        .all(|c| matches!(c, Component::Normal(_) | Component::CurDir));
    inside.then(|| dir.join("node_modules").join(package).join(relative))
}

/// `remove_dir_all`, with an absent directory counted as removed.
fn remove_dir(dir: &Path) -> Result<(), NpmServerError> {
    match std::fs::remove_dir_all(dir) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(NpmServerError::Io {
            path: dir.to_path_buf(),
            source,
        }),
    }
}
