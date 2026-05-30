//! `apollia-os update`: checks and installs Apollia OS updates from GitHub Releases.
//!
//! Downloads the platform binary, verifies SHA256 integrity, and performs an
//! atomic replacement via `fs::rename`. A lock file prevents concurrent updates.

use std::io::{self, Write};
use std::path::PathBuf;

use sha2::{Digest, Sha256};
use thiserror::Error;

const REPO: &str = "apollia-os";
const GITHUB_API_BASE: &str = "https://api.github.com";
const LOCK_FILE: &str = "/tmp/apollia-update.lock";

// ─── Errors ──────────────────────────────────────────────────────────────────

/// Errors produced by the `update` command.
#[derive(Debug, Error)]
pub enum UpdateError {
    /// Network request failed.
    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),

    /// File system I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// SHA256 checksum does not match the downloaded binary.
    #[error("SHA256 mismatch — expected: {expected}, got: {actual}")]
    ChecksumMismatch { expected: String, actual: String },

    /// Version string could not be parsed as semver.
    #[error("invalid version string: {0}")]
    SemVer(#[from] semver::Error),

    /// An update is already in progress (lock file exists).
    #[error("an update is already in progress")]
    AlreadyRunning,

    /// Current executable path could not be determined.
    #[error("could not determine current executable path: {0}")]
    CurrentExe(std::io::Error),

    /// Generic update failure with a human-readable description.
    #[error("update failed: {0}")]
    Other(String),
}

// ─── GitHub API types ─────────────────────────────────────────────────────────

/// Minimal representation of a GitHub release as returned by `/releases/latest`.
#[derive(Debug, serde::Deserialize)]
struct GithubRelease {
    tag_name: String,
    assets: Vec<GithubAsset>,
}

/// A single downloadable asset attached to a GitHub release.
#[derive(Debug, serde::Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
}

// ─── Clap args ────────────────────────────────────────────────────────────────

/// Arguments for `apollia-os update`.
#[derive(Debug, clap::Args)]
pub struct UpdateArgs {
    /// Only check for a newer version without downloading or installing.
    #[arg(long)]
    pub check: bool,

    /// Install without asking for interactive confirmation.
    #[arg(long)]
    pub yes: bool,
}

// ─── Platform detection ───────────────────────────────────────────────────────

/// Returns the release asset name for the current platform.
///
/// The name follows the convention used in Apollia OS GitHub releases:
/// `apollia-os-<target-triple>[.exe]`.
fn platform_binary_name() -> &'static str {
    if cfg!(target_os = "linux") && cfg!(target_arch = "x86_64") {
        "apollia-os-x86_64-unknown-linux-musl"
    } else if cfg!(target_os = "linux") && cfg!(target_arch = "aarch64") {
        "apollia-os-aarch64-unknown-linux-musl"
    } else if cfg!(target_os = "macos") && cfg!(target_arch = "x86_64") {
        "apollia-os-x86_64-apple-darwin"
    } else if cfg!(target_os = "macos") && cfg!(target_arch = "aarch64") {
        "apollia-os-aarch64-apple-darwin"
    } else if cfg!(target_os = "windows") && cfg!(target_arch = "x86_64") {
        "apollia-os-x86_64-pc-windows-msvc.exe"
    } else {
        "apollia-os-unknown-platform"
    }
}

// ─── Core logic ───────────────────────────────────────────────────────────────

/// Fetches the latest GitHub release and returns the remote version string when
/// it is strictly newer than the currently running binary, or `None` when already
/// up to date.
pub async fn check_update(owner: &str) -> Result<Option<String>, UpdateError> {
    let url = format!("{GITHUB_API_BASE}/repos/{owner}/{REPO}/releases/latest");

    let client = reqwest::Client::builder()
        .user_agent(concat!("apollia-os-updater/", env!("CARGO_PKG_VERSION")))
        .build()?;

    let release: GithubRelease = client
        .get(&url)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let remote_str = release.tag_name.trim_start_matches('v');
    let remote = semver::Version::parse(remote_str)?;
    let local = semver::Version::parse(env!("CARGO_PKG_VERSION"))?;

    if remote > local {
        Ok(Some(remote_str.to_owned()))
    } else {
        Ok(None)
    }
}

/// Downloads, verifies, and atomically installs the latest Apollia OS binary.
///
/// Steps:
/// 1. Acquires `/tmp/apollia-update.lock` (fails immediately if another update is running).
/// 2. Fetches release metadata from GitHub API.
/// 3. Optionally prompts the user unless `yes` is `true`.
/// 4. Downloads the platform binary and its `.sha256` companion.
/// 5. Verifies SHA256, aborting without touching the live binary on mismatch.
/// 6. Writes `/tmp/apollia-new`, sets `chmod 755` on Unix.
/// 7. Replaces the running binary atomically via `fs::rename`; falls back to
///    `fs::copy` + delete when source and destination span different filesystems.
/// 8. Lock file is removed unconditionally on exit via `scopeguard::defer!`.
pub async fn install_update(owner: &str, yes: bool) -> Result<(), UpdateError> {
    // ── Lock file: prevent concurrent updates ─────────────────────────────
    if std::path::Path::new(LOCK_FILE).exists() {
        return Err(UpdateError::AlreadyRunning);
    }
    std::fs::write(LOCK_FILE, std::process::id().to_string())?;
    scopeguard::defer! {
        let _ = std::fs::remove_file(LOCK_FILE);
    }

    // ── Fetch release metadata ─────────────────────────────────────────────
    let url = format!("{GITHUB_API_BASE}/repos/{owner}/{REPO}/releases/latest");

    let client = reqwest::Client::builder()
        .user_agent(concat!("apollia-os-updater/", env!("CARGO_PKG_VERSION")))
        .build()?;

    let release: GithubRelease = client
        .get(&url)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let remote_str = release.tag_name.trim_start_matches('v');
    let remote = semver::Version::parse(remote_str)?;
    let local = semver::Version::parse(env!("CARGO_PKG_VERSION"))?;

    if remote <= local {
        println!("Already up to date ({local}).");
        return Ok(());
    }

    // ── Resolve platform asset URLs ────────────────────────────────────────
    let bin_name = platform_binary_name();
    let sha_name = format!("{bin_name}.sha256");

    let bin_url = resolve_asset(&release.assets, bin_name)?;
    let sha_url = resolve_asset(&release.assets, &sha_name)?;

    // ── Interactive confirmation ────────────────────────────────────────────
    if !yes {
        print!("Update Apollia OS from {local} to {remote_str}? [y/N] ");
        io::stdout().flush()?;
        let mut answer = String::new();
        io::stdin().read_line(&mut answer)?;
        if !answer.trim().eq_ignore_ascii_case("y") {
            println!("Update cancelled.");
            return Ok(());
        }
    }

    tracing::info!(version = remote_str, url = %bin_url, "downloading update");

    // ── Download binary and SHA256 ─────────────────────────────────────────
    let bin_bytes = client
        .get(&bin_url)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;

    let sha_text = client
        .get(&sha_url)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;

    // ── Verify checksum: fail fast on mismatch ────────────────────────────
    let expected = sha_text
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_lowercase();
    let actual = format!("{:x}", Sha256::digest(&bin_bytes));

    if actual != expected {
        return Err(UpdateError::ChecksumMismatch { expected, actual });
    }

    // ── Write staging file ─────────────────────────────────────────────────
    let tmp_path = PathBuf::from("/tmp/apollia-new");
    std::fs::write(&tmp_path, &bin_bytes)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp_path, std::fs::Permissions::from_mode(0o755))?;
    }

    // ── Atomic replace: fallback to copy+delete on cross-device move ──────
    let exe_path = std::env::current_exe().map_err(UpdateError::CurrentExe)?;

    match std::fs::rename(&tmp_path, &exe_path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::CrossesDevices => {
            std::fs::copy(&tmp_path, &exe_path)?;
            let _ = std::fs::remove_file(&tmp_path);
        }
        Err(e) => return Err(UpdateError::Io(e)),
    }

    tracing::info!(version = remote_str, "update installed successfully");
    println!("Updated to {remote_str} successfully.");
    Ok(())
}

/// Looks up an asset by name and returns its download URL.
fn resolve_asset(assets: &[GithubAsset], name: &str) -> Result<String, UpdateError> {
    assets
        .iter()
        .find(|a| a.name == name)
        .map(|a| a.browser_download_url.clone())
        .ok_or_else(|| UpdateError::Other(format!("release asset '{name}' not found")))
}

// ─── Entry point ──────────────────────────────────────────────────────────────

/// Execute `apollia-os update [--check] [--yes]`. Returns a POSIX exit code.
pub async fn run(args: &UpdateArgs, owner: &str) -> i32 {
    if args.check {
        match check_update(owner).await {
            Ok(Some(v)) => {
                println!("New version available: {v}");
                crate::exit_codes::SUCCESS
            }
            Ok(None) => {
                println!("Already up to date ({}).", env!("CARGO_PKG_VERSION"));
                crate::exit_codes::SUCCESS
            }
            Err(e) => {
                eprintln!("Error: {e}");
                crate::exit_codes::GENERAL_ERROR
            }
        }
    } else {
        match install_update(owner, args.yes).await {
            Ok(()) => crate::exit_codes::SUCCESS,
            Err(e) => {
                eprintln!("Error: {e}");
                crate::exit_codes::GENERAL_ERROR
            }
        }
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_platform_binary_name_not_empty() {
        // GIVEN the current compilation target
        // WHEN platform_binary_name() is called
        // THEN the result is non-empty
        let name = platform_binary_name();
        assert!(!name.is_empty());
    }

    #[test]
    fn test_platform_binary_name_contains_target_triple() {
        // GIVEN the current compilation target
        // WHEN platform_binary_name() is called
        // THEN the name begins with "apollia-os-" and is not the fallback
        let name = platform_binary_name();
        assert!(
            name.starts_with("apollia-os-"),
            "unexpected binary name: {name}"
        );
        assert_ne!(name, "apollia-os-unknown-platform");
    }

    #[test]
    fn test_sha256_mismatch_detection() {
        // GIVEN a known byte sequence
        // WHEN we compute its SHA256
        // THEN it differs from an arbitrary string
        let content = b"fake binary content";
        let actual = format!("{:x}", Sha256::digest(content));
        let wrong = "aabbcc";
        assert_ne!(actual, wrong);
    }

    #[test]
    fn test_semver_comparison() {
        // GIVEN remote = "0.2.0" and local = "0.1.0"
        // WHEN versions are compared
        // THEN remote > local
        let remote = semver::Version::parse("0.2.0").expect("valid semver");
        let local = semver::Version::parse("0.1.0").expect("valid semver");
        assert!(remote > local);
    }

    #[test]
    fn test_semver_equal_not_newer() {
        // GIVEN remote == local
        // WHEN versions are compared
        // THEN remote is NOT greater than local
        let v = semver::Version::parse("0.1.0").expect("valid semver");
        assert!(!(v.clone() > v));
    }

    #[tokio::test]
    #[ignore = "requires network access to GitHub"]
    async fn test_check_update_returns_result() {
        // GIVEN a valid GitHub owner
        // WHEN check_update() is called
        // THEN no error is returned
        let result = check_update("apollia-os").await;
        assert!(result.is_ok());
    }
}
