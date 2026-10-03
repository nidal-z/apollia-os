//! The runtime as a service of the user's session: `apollia-os start` run at
//! logon by the OS service manager, without the desktop application and without
//! administrator rights.
//!
//! One code path serves the two surfaces that install it: the command line
//! (`apollia-os service`), which names its own binary, and the desktop
//! application ("Keep Apollia running without the application"), which names
//! the command line bundled with it. Each passes the binary the definition
//! runs; nothing here spawns `apollia-os` to install itself.
//!
//! The definition is rendered by [`definition`], then handed to the OS service
//! manager by [`manager`]: launchd on macOS, the systemd user manager on Linux,
//! Task Scheduler on Windows. What the manager reports is the state in force:
//! no setting mirrors it.

use std::path::{Path, PathBuf};

use tracing::Level;

pub mod definition;
pub mod manager;

use definition::{is_representable, Manager, ServiceSpec};
use manager::Observed;

/// Why the service could not be installed, removed or started.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ServiceError {
    /// This OS has none of the three supported service managers.
    #[error("no supported service manager on this OS (macOS, Linux and Windows are)")]
    UnsupportedPlatform,
    /// The home directory could not be resolved.
    #[error("cannot resolve the home directory")]
    NoHome,
    /// A path holds a character no definition format can carry.
    #[error("path cannot be written into a service definition: {0}")]
    UnrepresentablePath(String),
    /// Windows names the logon user by `USERDOMAIN` and `USERNAME`.
    #[error("cannot name the logon user: USERNAME is not set")]
    NoWindowsUser,
    /// The service is not installed for this home.
    #[error("the service is not installed: no definition at {}", .0.display())]
    NotInstalled(PathBuf),
    /// Writing or removing a file failed.
    #[error("{action} {path}: {source}")]
    Io {
        /// What was attempted.
        action: &'static str,
        /// The file or directory.
        path: PathBuf,
        /// The underlying error.
        #[source]
        source: std::io::Error,
    },
    /// The service manager refused a command.
    #[error("`{command}` failed: {detail}")]
    Manager {
        /// The command line run.
        command: String,
        /// What the manager answered.
        detail: String,
    },
}

/// What the service manager holds for one home, as observed now.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ServiceState {
    /// The definition file exists.
    pub installed: bool,
    /// The manager holds the service, loaded from this definition.
    pub loaded: bool,
    /// The service's process is running.
    pub running: bool,
}

impl ServiceState {
    /// The state word published by `apollia-os service status --json`.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match (self.installed, self.running) {
            (false, _) => "not_installed",
            (true, true) => "running",
            (true, false) => "stopped",
        }
    }
}

/// Where this user's service lives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceTarget {
    /// The manager of this OS.
    pub manager: Manager,
    /// The home the service serves.
    pub home: PathBuf,
    /// The definition file.
    pub definition: PathBuf,
    /// `~/.apollia` under `home`.
    pub data_dir: PathBuf,
}

impl ServiceTarget {
    /// The target of the current user, on the OS this binary was built for.
    ///
    /// # Errors
    ///
    /// [`ServiceError::UnsupportedPlatform`] off macOS, Linux and Windows;
    /// [`ServiceError::NoHome`] when no home directory resolves.
    pub fn current() -> Result<Self, ServiceError> {
        let manager = Manager::current().ok_or(ServiceError::UnsupportedPlatform)?;
        let home = apollia_core::paths::home_dir().ok_or(ServiceError::NoHome)?;
        let config_home = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute());
        Ok(Self::under(manager, home, config_home.as_deref()))
    }

    /// The target of `manager` for `home`, `config_home` being
    /// `$XDG_CONFIG_HOME` when it is absolute.
    #[must_use]
    pub fn under(manager: Manager, home: PathBuf, config_home: Option<&Path>) -> Self {
        let data_dir = apollia_core::paths::data_dir_under(&home);
        let definition = manager.definition_path(&home, config_home, &data_dir);
        Self {
            manager,
            home,
            definition,
            data_dir,
        }
    }

    /// Whether the definition file exists. Cheap: no manager is asked.
    #[must_use]
    pub fn is_installed(&self) -> bool {
        self.definition.is_file()
    }

    /// The state the manager reports for this definition.
    #[must_use]
    pub fn observed_state(&self) -> ServiceState {
        if !self.is_installed() {
            return ServiceState::default();
        }
        let Observed { loaded, running } = manager::query_state(self.manager, &self.definition);
        ServiceState {
            installed: true,
            loaded,
            running,
        }
    }

    /// The definition for a service that runs `binary start`.
    ///
    /// # Errors
    ///
    /// A path no definition can carry, or no Windows logon user.
    pub fn spec_for(&self, binary: &Path) -> Result<ServiceSpec, ServiceError> {
        let log_file = self.data_dir.join("logs").join("service.log");
        let windows_user = if self.manager == Manager::TaskScheduler {
            let user = std::env::var("USERNAME").map_err(|_| ServiceError::NoWindowsUser)?;
            Some(match std::env::var("USERDOMAIN") {
                Ok(domain) if !domain.is_empty() => format!("{domain}\\{user}"),
                _ => user,
            })
        } else {
            None
        };
        let path_env = std::env::var("PATH")
            .ok()
            .map(|p| dedup_path(&p))
            .filter(|p| !p.is_empty() && is_representable(p));
        Ok(ServiceSpec {
            binary: representable(binary)?,
            log_file: representable(&log_file)?,
            path_env,
            home: representable(&self.home)?,
            windows_user,
            definition: representable(&self.definition)?,
        })
    }

    /// Writes the definition of a service that runs `binary start`, then,
    /// when `load` is true, hands it to the manager, which starts it. Returns
    /// the state observed afterwards.
    ///
    /// `binary` is the `apollia-os` the service runs: the command line names
    /// itself, the desktop names the one bundled with it.
    ///
    /// # Errors
    ///
    /// A path no definition can carry, a file that cannot be written, or a
    /// manager that refuses the definition.
    pub fn install(&self, binary: &Path, load: bool) -> Result<ServiceState, ServiceError> {
        let spec = self.spec_for(binary)?;
        create_dir(&self.data_dir.join("logs"))?;
        if let Some(parent) = self.definition.parent() {
            create_dir(parent)?;
        }
        let bytes = self.manager.encode(&self.manager.render(&spec));
        std::fs::write(&self.definition, bytes).map_err(|source| ServiceError::Io {
            action: "cannot write",
            path: self.definition.clone(),
            source,
        })?;
        tracing::event!(
            Level::DEBUG,
            manager = self.manager.name(),
            definition = %self.definition.display(),
            binary = %spec.binary,
            "service.definition_written"
        );
        if load {
            self.start()?;
        }
        Ok(self.observed_state())
    }

    /// Hands the installed definition to the manager, which starts the
    /// service; a copy already loaded from it is replaced.
    ///
    /// # Errors
    ///
    /// [`ServiceError::NotInstalled`] without a definition, or a manager that
    /// refuses it.
    pub fn start(&self) -> Result<(), ServiceError> {
        if !self.is_installed() {
            return Err(ServiceError::NotInstalled(self.definition.clone()));
        }
        manager::load(self.manager, &self.definition)?;
        tracing::event!(
            Level::DEBUG,
            manager = self.manager.name(),
            "service.loaded"
        );
        Ok(())
    }

    /// Stops the service and removes its definition. Returns whether a loaded
    /// service was stopped.
    ///
    /// # Errors
    ///
    /// [`ServiceError::NotInstalled`] without a definition, a manager that
    /// refuses to stop it, or a definition that cannot be removed.
    pub fn uninstall(&self) -> Result<bool, ServiceError> {
        if !self.is_installed() {
            return Err(ServiceError::NotInstalled(self.definition.clone()));
        }
        let stopped = manager::unload(self.manager, &self.definition)?;
        std::fs::remove_file(&self.definition).map_err(|source| ServiceError::Io {
            action: "cannot remove",
            path: self.definition.clone(),
            source,
        })?;
        manager::forget(self.manager);
        tracing::event!(
            Level::DEBUG,
            manager = self.manager.name(),
            stopped,
            "service.uninstalled"
        );
        Ok(stopped)
    }
}

fn representable(path: &Path) -> Result<String, ServiceError> {
    let text = path
        .to_str()
        .ok_or_else(|| ServiceError::UnrepresentablePath(path.to_string_lossy().to_string()))?;
    if is_representable(text) {
        Ok(text.to_string())
    } else {
        Err(ServiceError::UnrepresentablePath(text.to_string()))
    }
}

/// `PATH` without its repeated entries, first occurrence kept: a shell that
/// sources its profile twice doubles every entry, and the definition should
/// not carry that.
fn dedup_path(path: &str) -> String {
    let separator = if cfg!(windows) { ';' } else { ':' };
    let mut seen = std::collections::HashSet::new();
    path.split(separator)
        .filter(|entry| !entry.is_empty() && seen.insert(*entry))
        .collect::<Vec<_>>()
        .join(&separator.to_string())
}

fn create_dir(dir: &Path) -> Result<(), ServiceError> {
    std::fs::create_dir_all(dir).map_err(|source| ServiceError::Io {
        action: "cannot create",
        path: dir.to_path_buf(),
        source,
    })
}

#[cfg(test)]
#[path = "service/service_tests.rs"]
mod tests;
