//! The version an engine states about itself, read at its source.
//!
//! A binary is asked with `--version` and its banner is read; a Python
//! environment is read from the metadata its package installer wrote. Nothing
//! here comes from a tag compiled into Apollia: a tag says which build the
//! tables were derived from, not which build is on this machine.

use std::path::Path;
use std::time::Duration;

/// Longest wait for a binary to print its banner. An engine that ignores the
/// flag and starts serving instead is killed when the wait ends.
const BANNER_TIMEOUT: Duration = Duration::from_secs(5);

/// The version a `--version` banner states.
///
/// llama.cpp prints `version: 6543 (a1b2c3d)`, possibly after lines about the
/// backends it loaded, so the `version:` line is looked for first; its value
/// is returned as printed. A banner without one (Node.js prints `v22.11.0`)
/// gives its first non-empty line when that line starts like a version.
#[must_use]
pub fn banner_version(banner: &str) -> Option<String> {
    let lines = || banner.lines().map(str::trim).filter(|l| !l.is_empty());
    if let Some(value) = lines().find_map(|l| l.strip_prefix("version:")) {
        let value = value.trim();
        return (!value.is_empty()).then(|| value.to_owned());
    }
    lines()
        .next()
        .filter(|l| {
            let mut chars = l.chars();
            match chars.next() {
                Some('v') => chars.next().is_some_and(|c| c.is_ascii_digit()),
                Some(c) => c.is_ascii_digit(),
                None => false,
            }
        })
        .map(str::to_owned)
}

/// The version of the distribution `name` installed in `site_packages`, read
/// from the `Version:` field of its `<name>-<version>.dist-info/METADATA`.
///
/// `name` is the normalised distribution name (`vllm_mlx`).
#[must_use]
pub fn dist_info_version(site_packages: &Path, name: &str) -> Option<String> {
    let prefix = format!("{name}-");
    let mut dirs: Vec<_> = std::fs::read_dir(site_packages)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(&prefix) && n.ends_with(".dist-info"))
        })
        .collect();
    dirs.sort();
    let metadata = std::fs::read_to_string(dirs.first()?.join("METADATA")).ok()?;
    metadata
        .lines()
        .take_while(|l| !l.is_empty())
        .find_map(|l| l.strip_prefix("Version:"))
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_owned)
}

/// Run `binary --version` and read the version from its banner, on stdout or
/// stderr. `None` when it cannot run, fails, says nothing usable, or has not
/// answered within [`BANNER_TIMEOUT`].
pub async fn probe_banner(binary: &Path) -> Option<String> {
    let mut command = tokio::process::Command::new(binary);
    apollia_core::subprocess_window::hide_console_async(&mut command);
    command
        .arg("--version")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    let output = match tokio::time::timeout(BANNER_TIMEOUT, command.output()).await {
        Ok(Ok(output)) if output.status.success() => output,
        Ok(Ok(output)) => {
            tracing::event!(
                tracing::Level::DEBUG,
                binary = %binary.display(),
                status = %output.status,
                "engine_store.version.refused"
            );
            return None;
        }
        Ok(Err(error)) => {
            tracing::event!(
                tracing::Level::DEBUG,
                binary = %binary.display(),
                error = %error,
                "engine_store.version.unreachable"
            );
            return None;
        }
        Err(_) => {
            tracing::event!(
                tracing::Level::WARN,
                binary = %binary.display(),
                "engine_store.version.timed_out"
            );
            return None;
        }
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    banner_version(&stdout).or_else(|| banner_version(&stderr))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_banner_version_reads_the_llama_server_banner() {
        // GIVEN the banner llama-server prints, after a line about its backends
        let banner = "load_backend: loaded RPC backend from /x/libggml-rpc.so\n\
                      version: 6543 (a1b2c3d)\n\
                      built with Apple clang version 17.0.0 for arm64-apple-darwin25.0.0\n";

        // WHEN its version is read
        let version = banner_version(banner);

        // THEN it is the value of the version line, as printed
        assert_eq!(version.as_deref(), Some("6543 (a1b2c3d)"));
    }

    #[test]
    fn test_banner_version_reads_a_bare_version_line() {
        // GIVEN the banner Node.js prints
        // WHEN its version is read
        let version = banner_version("v22.11.0\n");

        // THEN the line itself is the version
        assert_eq!(version.as_deref(), Some("v22.11.0"));
    }

    #[test]
    fn test_banner_version_refuses_a_banner_that_names_no_version() {
        // GIVEN banners with no version in them
        // WHEN they are read
        // THEN nothing is invented, and an empty version line is no version
        assert_eq!(banner_version(""), None);
        assert_eq!(banner_version("usage: llama-server [options]\n"), None);
        assert_eq!(banner_version("version:   \n"), None);
    }

    #[test]
    fn test_dist_info_version_reads_the_installed_metadata() {
        // GIVEN a site-packages holding the vllm-mlx distribution and another
        let dir = tempfile::tempdir().unwrap();
        let info = dir.path().join("vllm_mlx-0.9.1.dist-info");
        std::fs::create_dir_all(&info).unwrap();
        std::fs::write(
            info.join("METADATA"),
            "Metadata-Version: 2.4\nName: vllm-mlx\nVersion: 0.9.1\n\nVersion: 9.9.9 in the body\n",
        )
        .unwrap();
        let other = dir.path().join("mlx-0.29.0.dist-info");
        std::fs::create_dir_all(&other).unwrap();
        std::fs::write(other.join("METADATA"), "Name: mlx\nVersion: 0.29.0\n").unwrap();

        // WHEN the version of vllm_mlx is read, then that of an absent one
        let found = dist_info_version(dir.path(), "vllm_mlx");
        let absent = dist_info_version(dir.path(), "torch");

        // THEN the header field of its own metadata answers, and absence is none
        assert_eq!(found.as_deref(), Some("0.9.1"));
        assert_eq!(absent, None);
    }

    #[tokio::test]
    async fn test_probe_banner_answers_none_for_a_missing_binary() {
        // GIVEN a path where no binary exists
        let dir = tempfile::tempdir().unwrap();

        // WHEN it is probed
        let version = probe_banner(&dir.path().join("absent")).await;

        // THEN there is no version, and no error escapes
        assert_eq!(version, None);
    }
}
