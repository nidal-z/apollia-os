//! Building a connection's virtualenv and installing its package into it.

use std::path::{Path, PathBuf};

use apollia_tools::tools::python_executor::{
    agent_venv_site_packages, venv_script, PythonExecutor,
};
use serde::{Deserialize, Serialize};

use super::{
    connection_dir, normalize_name, render_requirements, PythonServerError, PythonServerPlan,
};

/// File, beside the venv, recording what was installed.
const RECORD_FILE: &str = "server.json";

/// File, beside the venv, holding the rendered hash lock pip read.
const LOCK_FILE: &str = "requirements.lock";

/// What a connection's venv holds, written once the installation succeeded.
///
/// A later addition of the same connection reads it back and, when it still
/// describes what is asked, reuses the venv without running pip.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledServer {
    /// Distribution name, normalized.
    pub package: String,
    /// The exact version installed, read from the venv after installing.
    pub version: String,
    /// The console script the connection runs.
    pub entry_point: String,
    /// `true` when the installation went through a hash lock.
    pub locked: bool,
    /// Absolute path of the entry point. Not persisted: derived from the venv.
    #[serde(skip)]
    pub entry_path: PathBuf,
}

/// Installs Python MCP servers, one virtualenv per connection.
#[derive(Debug, Clone)]
pub struct PythonServerInstaller {
    connections: PathBuf,
    /// A local directory of archives to install from, with the index turned
    /// off. Tests only: the product always installs from the package index.
    local_archives: Option<PathBuf>,
}

impl PythonServerInstaller {
    /// An installer rooted at `connections` (normally
    /// [`super::connections_dir`]).
    pub fn new(connections: PathBuf) -> Self {
        Self {
            connections,
            local_archives: None,
        }
    }

    /// An installer that never reaches the network: pip reads `archives`
    /// with the index disabled.
    #[cfg(test)]
    pub fn offline(connections: PathBuf, archives: PathBuf) -> Self {
        Self {
            connections,
            local_archives: Some(archives),
        }
    }

    /// The virtualenv of a connection.
    pub fn venv_path(&self, name: &str) -> PathBuf {
        connection_dir(&self.connections, name).join("venv")
    }

    /// Make sure `plan` is installed for connection `name`, and say where its
    /// entry point is.
    ///
    /// When the record beside the venv already matches the plan and the entry
    /// point is there, nothing runs: no interpreter, no pip, no network.
    /// Otherwise the connection directory is rebuilt from scratch; on any
    /// failure it is removed, so a refused installation leaves nothing that
    /// could be launched.
    ///
    /// # Errors
    ///
    /// Any [`PythonServerError`] from building the venv or installing.
    pub async fn provision(
        &self,
        name: &str,
        plan: &PythonServerPlan,
    ) -> Result<InstalledServer, PythonServerError> {
        if let Some(existing) = self.reusable(name, plan) {
            tracing::event!(
                tracing::Level::INFO,
                connection = %name,
                package = %existing.package,
                version = %existing.version,
                "mcp.python_server.reused"
            );
            return Ok(existing);
        }
        let dir = connection_dir(&self.connections, name);
        remove_dir(&dir)?;
        match self.install(name, plan, &dir).await {
            Ok(installed) => Ok(installed),
            Err(e) => {
                if let Err(cleanup) = remove_dir(&dir) {
                    tracing::event!(
                        tracing::Level::WARN,
                        connection = %name,
                        error = %cleanup,
                        "mcp.python_server.cleanup.failed"
                    );
                }
                tracing::event!(
                    tracing::Level::WARN,
                    connection = %name,
                    package = %plan.invocation.package,
                    error = %e,
                    "mcp.python_server.install.failed"
                );
                Err(e)
            }
        }
    }

    /// The recorded installation, when it satisfies `plan` as it stands.
    fn reusable(&self, name: &str, plan: &PythonServerPlan) -> Option<InstalledServer> {
        let dir = connection_dir(&self.connections, name);
        let raw = std::fs::read_to_string(dir.join(RECORD_FILE)).ok()?;
        let mut record: InstalledServer = serde_json::from_str(&raw).ok()?;
        let same_package = record.package == normalize_name(&plan.invocation.package);
        let same_entry = record.entry_point == plan.invocation.entry_point;
        let same_lock = record.locked != plan.lock.is_empty();
        let same_version = plan.version.as_ref().is_none_or(|v| *v == record.version);
        if !(same_package && same_entry && same_lock && same_version) {
            return None;
        }
        record.entry_path = venv_script(&self.venv_path(name), &record.entry_point);
        record.entry_path.is_file().then_some(record)
    }

    async fn install(
        &self,
        name: &str,
        plan: &PythonServerPlan,
        dir: &Path,
    ) -> Result<InstalledServer, PythonServerError> {
        let package = &plan.invocation.package;
        std::fs::create_dir_all(dir).map_err(|source| PythonServerError::Io {
            path: dir.to_path_buf(),
            source,
        })?;

        // The venv is built exactly as an agent's is: same interpreter
        // resolution (the bundled one first), same `-m venv`, same scrubbed
        // environment. Only the location differs.
        let executor = PythonExecutor::new(name, &self.connections)
            .map_err(|e| PythonServerError::Interpreter(e.to_string()))?;
        executor
            .setup_venv(&[])
            .await
            .map_err(|e| PythonServerError::Venv {
                name: name.to_string(),
                reason: e.to_string(),
            })?;

        let venv = self.venv_path(name);
        let mut args: Vec<String> = [
            "-m",
            "pip",
            "install",
            "--isolated",
            "--disable-pip-version-check",
            "--no-input",
            "--quiet",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        if let Some(archives) = &self.local_archives {
            args.push("--no-index".into());
            args.push("--find-links".into());
            args.push(archives.display().to_string());
        }
        if plan.lock.is_empty() {
            args.push(plan.invocation.requirement(plan.version.as_deref()));
        } else {
            let lock_path = dir.join(LOCK_FILE);
            let rendered = render_requirements(package, &plan.lock)?;
            std::fs::write(&lock_path, rendered).map_err(|source| PythonServerError::Io {
                path: lock_path.clone(),
                source,
            })?;
            args.push("--require-hashes".into());
            args.push("-r".into());
            args.push(lock_path.display().to_string());
        }

        tracing::event!(
            tracing::Level::INFO,
            connection = %name,
            package = %package,
            locked = !plan.lock.is_empty(),
            "mcp.python_server.installing"
        );
        let mut pip = tokio::process::Command::new(venv_script(&venv, "python"));
        apollia_core::subprocess_env::scrub_bundled_python_async(&mut pip);
        apollia_core::subprocess_window::hide_console_async(&mut pip);
        let output = pip
            .args(&args)
            .output()
            .await
            .map_err(|e| PythonServerError::Install {
                package: package.clone(),
                stderr: e.to_string(),
            })?;
        if !output.status.success() {
            return Err(PythonServerError::Install {
                package: package.clone(),
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            });
        }

        let unusable = |reason: String| PythonServerError::Installed {
            package: package.clone(),
            reason,
        };
        let version = installed_version(&self.connections, name, package)
            .ok_or_else(|| unusable("no installed metadata found in the venv".into()))?;
        if let Some(wanted) = &plan.version {
            if *wanted != version {
                return Err(unusable(format!("{version} installed, {wanted} expected")));
            }
        }
        let entry_path = venv_script(&venv, &plan.invocation.entry_point);
        if !entry_path.is_file() {
            return Err(unusable(format!(
                "no `{}` command in the venv",
                plan.invocation.entry_point
            )));
        }

        let record = InstalledServer {
            package: normalize_name(package),
            version,
            entry_point: plan.invocation.entry_point.clone(),
            locked: !plan.lock.is_empty(),
            entry_path,
        };
        let record_path = dir.join(RECORD_FILE);
        let json = serde_json::to_string_pretty(&record).map_err(|e| PythonServerError::Io {
            path: record_path.clone(),
            source: std::io::Error::other(e),
        })?;
        std::fs::write(&record_path, json).map_err(|source| PythonServerError::Io {
            path: record_path,
            source,
        })?;
        tracing::event!(
            tracing::Level::INFO,
            connection = %name,
            package = %record.package,
            version = %record.version,
            locked = record.locked,
            "mcp.python_server.installed"
        );
        Ok(record)
    }
}

/// The version of `package` installed in a connection's venv, read from its
/// `<name>-<version>.dist-info` directory rather than by running anything.
fn installed_version(connections: &Path, name: &str, package: &str) -> Option<String> {
    let wanted = normalize_name(package);
    agent_venv_site_packages(connections, name)
        .iter()
        .filter_map(|site| std::fs::read_dir(site).ok())
        .flatten()
        .flatten()
        .find_map(|entry| {
            let file_name = entry.file_name();
            let stem = file_name.to_str()?.strip_suffix(".dist-info")?;
            let (dist, version) = stem.rsplit_once('-')?;
            (normalize_name(dist) == wanted).then(|| version.to_string())
        })
}

/// `remove_dir_all`, with an absent directory counted as removed.
pub(super) fn remove_dir(dir: &Path) -> Result<(), PythonServerError> {
    match std::fs::remove_dir_all(dir) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(PythonServerError::Io {
            path: dir.to_path_buf(),
            source,
        }),
    }
}
