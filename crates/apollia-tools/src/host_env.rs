//! Best-effort host facts gathered for prompt injection.
//!
//! `apollia-prompts` renders the environment block but is a zero-I/O crate,
//! so the gathering (shell discovery, OS version probes) lives here. Callers
//! pass the result to `apollia_prompts::blocks::environment_block`.

use std::path::PathBuf;

use crate::tools::shell_discovery;

/// Facts about the host that an agent needs to stop guessing its
/// surroundings: which OS and version it runs on, whether a POSIX shell is
/// available for `bash_executor`, and which filesystem root the file tools
/// operate under.
#[derive(Debug, Clone)]
pub struct HostEnvironment {
    /// Host OS identifier (`std::env::consts::OS`: "macos", "linux", "windows").
    pub os: &'static str,
    /// Host CPU architecture (`std::env::consts::ARCH`).
    pub arch: &'static str,
    /// Human-readable OS version, `None` when the probe fails.
    pub os_version: Option<String>,
    /// The POSIX shell `bash_executor` will use, `None` when the host has none.
    pub posix_shell: Option<PathBuf>,
    /// Root the filesystem tools operate under, as provided by the caller.
    pub fs_root: Option<PathBuf>,
}

/// Gather the host facts. Never fails: every probe degrades to `None`.
///
/// `fs_root` is caller-provided because the workspace root is session state,
/// not host state.
pub fn gather_host_environment(fs_root: Option<PathBuf>) -> HostEnvironment {
    HostEnvironment {
        os: std::env::consts::OS,
        arch: std::env::consts::ARCH,
        os_version: os_version(),
        posix_shell: shell_discovery::resolve_posix_shell().ok(),
        fs_root,
    }
}

/// Human-readable OS version, hand-rolled per platform to avoid a dependency.
#[cfg(target_os = "macos")]
fn os_version() -> Option<String> {
    let output = std::process::Command::new("/usr/bin/sw_vers")
        .arg("-productVersion")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!version.is_empty()).then_some(version)
}

#[cfg(target_os = "linux")]
fn os_version() -> Option<String> {
    let contents = std::fs::read_to_string("/etc/os-release").ok()?;
    let line = contents
        .lines()
        .find(|l| l.starts_with("PRETTY_NAME="))?
        .trim_start_matches("PRETTY_NAME=");
    let version = line.trim().trim_matches('"').to_string();
    (!version.is_empty()).then_some(version)
}

#[cfg(target_os = "windows")]
fn os_version() -> Option<String> {
    // `cmd /c ver` prints e.g. `Microsoft Windows [Version 10.0.22631.3593]`.
    let mut probe = std::process::Command::new("cmd");
    apollia_core::subprocess_window::hide_console(&mut probe);
    let output = probe.args(["/c", "ver"]).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!version.is_empty()).then_some(version)
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn os_version() -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gather_host_environment_reports_running_os() {
        // GIVEN the running host
        // WHEN gathering the environment facts
        let env = gather_host_environment(Some(PathBuf::from("/workspace")));

        // THEN the OS and arch match the compile-time constants, the caller's
        // root is carried through, and on Unix the shell is /bin/sh
        assert_eq!(env.os, std::env::consts::OS);
        assert_eq!(env.arch, std::env::consts::ARCH);
        assert_eq!(env.fs_root, Some(PathBuf::from("/workspace")));
        #[cfg(unix)]
        assert_eq!(env.posix_shell, Some(PathBuf::from("/bin/sh")));
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn os_version_probe_returns_something_on_dev_hosts() {
        // GIVEN a macOS or Linux host (dev and CI machines)
        // WHEN probing the OS version
        let version = os_version();

        // THEN the probe yields a non-empty string
        assert!(
            version.as_deref().is_some_and(|v| !v.is_empty()),
            "expected a version string, got {version:?}"
        );
    }
}
