//! `apollia-os update`: checks and installs Apollia OS updates from GitHub Releases.
//!
//! Downloads the release archive named by the artifact contract, verifies its
//! SHA256 integrity, extracts the `apollia-os` binary and performs an atomic
//! replacement via `fs::rename`. A lock file prevents concurrent updates.

use std::io::{self, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use thiserror::Error;

const REPO: &str = "apollia-os";
const GITHUB_API_BASE: &str = "https://api.github.com";

/// The artifact naming contract, shared with the release workflow.
///
/// `release.yml` derives every published file name from this file, and this
/// command reads its expectations from the same place, so producer and consumer
/// cannot drift apart silently. `scripts/check_release_artifacts.py` guards the
/// crossing.
const ARTIFACTS_CONTRACT: &str = include_str!("../../../../packaging/artifacts.json");

/// Cap on the release metadata GitHub answers with (the `releases/latest`
/// document, and the `.sha256` companion of an archive).
const MAX_RELEASE_METADATA_BYTES: u64 = 8 * 1024 * 1024;

/// Cap on a downloaded release archive. The published archives sit well under
/// this; the point is that an unbounded `bytes()` on a remote answer is not a
/// download budget, it is the absence of one.
const MAX_RELEASE_ARCHIVE_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// The updater's HTTP client: shared user agent, shared SSRF redirect policy.
///
/// GitHub redirects a release asset onto its CDN, so the hops are followed, but
/// each one is re-checked against the public-destination policy.
fn updater_client() -> Result<reqwest::Client, UpdateError> {
    Ok(apollia_core::net::safe_client_builder()
        .user_agent(concat!("apollia-os-updater/", env!("CARGO_PKG_VERSION")))
        .build()?)
}

/// Where the concurrent-update lock lives: the platform temp directory, so the
/// path exists on Windows too.
fn lock_path() -> PathBuf {
    std::env::temp_dir().join("apollia-update.lock")
}

// ─── Errors ──────────────────────────────────────────────────────────────────

/// Errors produced by the `update` command.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum UpdateError {
    /// Network request failed.
    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),

    /// A response body was refused before being buffered.
    #[error("response body error: {0}")]
    Body(#[from] apollia_core::net::ReadCappedError),

    /// File system I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// SHA256 checksum does not match the downloaded binary.
    #[error("SHA256 mismatch - expected: {expected}, got: {actual}")]
    ChecksumMismatch { expected: String, actual: String },

    /// Version string could not be parsed as semver.
    #[error("invalid version string: {0}")]
    SemVer(#[from] semver::Error),

    /// An update is already in progress (lock file exists).
    #[error("an update is already in progress")]
    AlreadyRunning,

    /// The repository publishes no release yet, so there is nothing to compare against.
    ///
    /// GitHub answers `/releases/latest` with 404 both when no release exists and
    /// when every release is a draft or a prerelease. Without this variant the
    /// first public build reports a bare HTTP 404, which reads as a broken
    /// install rather than as an empty release feed.
    #[error("no release has been published yet")]
    NoRelease,

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

/// A CLI release archive as declared by the artifact contract.
#[derive(Debug, Clone, PartialEq, Eq)]
struct CliArtifact {
    /// The preset name, which is also the directory inside the archive.
    preset: String,
    /// The published archive file name, `apollia-os-<preset>.tar.gz|zip`.
    archive: String,
}

/// Reads the `self_update` CLI entry of the contract for one (os, arch) couple.
///
/// Returns `None` when no published archive covers the couple (macOS Intel
/// today), which the callers report as a state rather than invent a name the
/// release does not carry. The contract marks the CPU preset of each platform:
/// the `apollia-os` binary is identical across the presets of one target
/// triple, so the CPU archive updates a GPU install correctly.
fn platform_cli_artifact(os: &str, arch: &str) -> Option<CliArtifact> {
    let contract: serde_json::Value = serde_json::from_str(ARTIFACTS_CONTRACT).ok()?;
    contract.get("cli")?.as_array()?.iter().find_map(|entry| {
        let matches = entry.get("self_update")?.as_bool()?
            && entry.get("os")?.as_str()? == os
            && entry.get("arch")?.as_str()? == arch;
        if matches {
            Some(CliArtifact {
                preset: entry.get("preset")?.as_str()?.to_owned(),
                archive: entry.get("archive")?.as_str()?.to_owned(),
            })
        } else {
            None
        }
    })
}

/// The contract's `os` value for the running binary.
fn current_os() -> &'static str {
    if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "unsupported"
    }
}

/// The contract's `arch` value for the running binary.
fn current_arch() -> &'static str {
    if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "unsupported"
    }
}

// ─── Core logic ───────────────────────────────────────────────────────────────

/// Fetches the latest published release, mapping an empty feed to [`UpdateError::NoRelease`].
///
/// Shared by `check_update` and `install_update` so both report an empty release
/// feed the same way.
async fn fetch_latest_release(owner: &str) -> Result<GithubRelease, UpdateError> {
    let url = format!("{GITHUB_API_BASE}/repos/{owner}/{REPO}/releases/latest");

    let client = updater_client()?;

    let response = client.get(&url).send().await?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(UpdateError::NoRelease);
    }

    Ok(apollia_core::net::read_capped_json(
        response.error_for_status()?,
        MAX_RELEASE_METADATA_BYTES,
    )
    .await?)
}

/// Fetches the latest GitHub release and returns the remote version string when
/// it is strictly newer than the currently running binary, or `None` when already
/// up to date.
pub async fn check_update(owner: &str) -> Result<Option<String>, UpdateError> {
    let release = fetch_latest_release(owner).await?;

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
/// 1. Acquires the lock file in the temp directory (fails immediately if
///    another update is running).
/// 2. Fetches release metadata from GitHub API.
/// 3. Optionally prompts the user unless `yes` is `true`.
/// 4. Downloads the platform release archive (named by the artifact contract)
///    and its `.sha256` companion.
/// 5. Verifies SHA256, aborting without touching the live binary on mismatch.
/// 6. Extracts the archive in a staging directory with the system `tar`
///    (bsdtar reads the Windows `.zip` archives too) and takes its
///    `apollia-os` binary. The bundled Python and runners are left as they
///    are: the archive's binary is the one built against them.
/// 7. Replaces the running binary atomically via `fs::rename`; falls back to
///    `fs::copy` + delete when source and destination span different filesystems.
/// 8. Lock file and staging directory are removed unconditionally on exit via
///    `scopeguard::defer!`.
pub async fn install_update(owner: &str, yes: bool) -> Result<(), UpdateError> {
    // ── Lock file: prevent concurrent updates ─────────────────────────────
    let lock_file = lock_path();
    if lock_file.exists() {
        return Err(UpdateError::AlreadyRunning);
    }
    std::fs::write(&lock_file, std::process::id().to_string())?;
    scopeguard::defer! {
        let _ = std::fs::remove_file(lock_path());
    }

    // ── Fetch release metadata ─────────────────────────────────────────────
    let release = fetch_latest_release(owner).await?;

    let client = updater_client()?;

    let remote_str = release.tag_name.trim_start_matches('v');
    let remote = semver::Version::parse(remote_str)?;
    let local = semver::Version::parse(env!("CARGO_PKG_VERSION"))?;

    if remote <= local {
        println!("Already up to date ({local}).");
        return Ok(());
    }

    // ── Resolve platform asset URLs ────────────────────────────────────────
    let artifact = platform_cli_artifact(current_os(), current_arch()).ok_or_else(|| {
        UpdateError::Other(format!(
            "no release archive is published for {}/{} (see packaging/artifacts.json)",
            current_os(),
            current_arch()
        ))
    })?;
    let sha_name = format!("{}.sha256", artifact.archive);

    let bin_url = resolve_asset(&release.assets, &artifact.archive)?;
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
    let bin_bytes = apollia_core::net::read_capped_bytes(
        client.get(&bin_url).send().await?.error_for_status()?,
        MAX_RELEASE_ARCHIVE_BYTES,
    )
    .await?;

    let sha_text = apollia_core::net::read_capped_text(
        client.get(&sha_url).send().await?.error_for_status()?,
        MAX_RELEASE_METADATA_BYTES,
    )
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

    // ── Stage and extract the archive ─────────────────────────────────────
    let stage_dir = std::env::temp_dir().join(format!("apollia-update-{}", std::process::id()));
    std::fs::create_dir_all(&stage_dir)?;
    scopeguard::defer! {
        let _ = std::fs::remove_dir_all(
            std::env::temp_dir().join(format!("apollia-update-{}", std::process::id())),
        );
    }
    let archive_path = stage_dir.join(&artifact.archive);
    std::fs::write(&archive_path, &bin_bytes)?;
    extract_archive(&archive_path, &stage_dir)?;

    let bin_file = if cfg!(windows) {
        "apollia-os.exe"
    } else {
        "apollia-os"
    };
    let tmp_path = stage_dir
        .join(format!("apollia-os-{}", artifact.preset))
        .join(bin_file);
    if !tmp_path.is_file() {
        return Err(UpdateError::Other(format!(
            "the release archive does not carry {bin_file} at apollia-os-{}/{bin_file}",
            artifact.preset
        )));
    }

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

/// Extracts a release archive with the system `tar`.
///
/// `tar` reads `.tar.gz` everywhere; the `.zip` archives only exist for the
/// Windows presets, where the system `tar` is bsdtar and reads zip natively.
/// Shelling out keeps the updater free of archive-format dependencies, which
/// each would be a new sovereignty surface.
fn extract_archive(archive: &Path, dest: &Path) -> Result<(), UpdateError> {
    let mut cmd = std::process::Command::new("tar");
    if archive.extension().is_some_and(|e| e == "zip") {
        cmd.arg("-xf");
    } else {
        cmd.arg("-xzf");
    }
    let output = cmd.arg(archive).arg("-C").arg(dest).output()?;
    if !output.status.success() {
        return Err(UpdateError::Other(format!(
            "failed to extract {}: {}",
            archive.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
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
///
/// An empty release feed is a state, not a failure: `--check` reports it and
/// exits 0, so a first-day install does not look broken to the operator or to a
/// script polling the exit code.
pub async fn run(args: &UpdateArgs, owner: &str, json: bool) -> i32 {
    let current = env!("CARGO_PKG_VERSION");

    if args.check {
        let outcome = check_update(owner).await;

        if json {
            let (available, latest, channel) = match &outcome {
                Ok(Some(v)) => (true, Some(v.as_str()), true),
                Ok(None) => (false, None, true),
                Err(UpdateError::NoRelease) => (false, None, false),
                Err(e) => {
                    return crate::output::emit_error(
                        json,
                        crate::exit_codes::GENERAL_ERROR,
                        &e.to_string(),
                    );
                }
            };
            println!(
                "{}",
                serde_json::json!({
                    "current_version": current,
                    "update_available": available,
                    "latest_version": latest,
                    "channel_available": channel,
                })
            );
            return crate::exit_codes::SUCCESS;
        }

        match outcome {
            Ok(Some(v)) => {
                println!("Installed: {current}. New version available: {v}");
                crate::exit_codes::SUCCESS
            }
            Ok(None) => {
                println!("Installed: {current}. Already up to date.");
                crate::exit_codes::SUCCESS
            }
            Err(UpdateError::NoRelease) => {
                println!("Installed: {current}. No release has been published yet.");
                crate::exit_codes::SUCCESS
            }
            Err(e) => {
                crate::output::emit_error(json, crate::exit_codes::GENERAL_ERROR, &e.to_string())
            }
        }
    } else {
        match install_update(owner, args.yes).await {
            Ok(()) => crate::exit_codes::SUCCESS,
            Err(UpdateError::NoRelease) => {
                println!("Installed: {current}. No release has been published yet.");
                crate::exit_codes::SUCCESS
            }
            Err(e) => {
                crate::output::emit_error(json, crate::exit_codes::GENERAL_ERROR, &e.to_string())
            }
        }
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_artifacts_contract_parses() {
        // GIVEN the embedded artifact contract
        // WHEN it is parsed as JSON
        // THEN it carries a non-empty `cli` array
        let contract: serde_json::Value =
            serde_json::from_str(ARTIFACTS_CONTRACT).expect("valid contract JSON");
        let cli = contract["cli"].as_array().expect("cli array");
        assert!(!cli.is_empty());
    }

    #[test]
    fn test_one_self_update_archive_per_platform() {
        // GIVEN the embedded artifact contract
        // WHEN the self_update entries are grouped by (os, arch)
        // THEN each couple carries exactly one archive, so the lookup is
        //      unambiguous on every supported platform
        let contract: serde_json::Value =
            serde_json::from_str(ARTIFACTS_CONTRACT).expect("valid contract JSON");
        let mut couples: Vec<(String, String)> = contract["cli"]
            .as_array()
            .expect("cli array")
            .iter()
            .filter(|e| e["self_update"].as_bool() == Some(true))
            .map(|e| {
                (
                    e["os"].as_str().expect("os").to_owned(),
                    e["arch"].as_str().expect("arch").to_owned(),
                )
            })
            .collect();
        let total = couples.len();
        couples.sort();
        couples.dedup();
        assert_eq!(total, couples.len(), "duplicated (os, arch) couple");
        assert!(total >= 5, "expected at least 5 platforms, got {total}");
    }

    #[test]
    fn test_platform_lookup_returns_contract_names() {
        // GIVEN a couple the contract covers
        // WHEN the artifact is resolved
        // THEN the archive is the contract's name for that platform
        let artifact = platform_cli_artifact("linux", "x86_64").expect("covered couple");
        assert_eq!(artifact.preset, "linux-x86-cpu");
        assert_eq!(artifact.archive, "apollia-os-linux-x86-cpu.tar.gz");
    }

    #[test]
    fn test_platform_lookup_reports_uncovered_couple() {
        // GIVEN a couple no published archive covers (macOS Intel)
        // WHEN the artifact is resolved
        // THEN the lookup answers None instead of inventing a name
        assert!(platform_cli_artifact("macos", "x86_64").is_none());
    }

    #[test]
    fn test_current_platform_resolves_an_archive() {
        // GIVEN the platform this test compiles for (one of the release couples)
        // WHEN the artifact is resolved
        // THEN an archive of the expected shape is found
        let artifact =
            platform_cli_artifact(current_os(), current_arch()).expect("supported platform");
        assert!(
            artifact.archive.starts_with("apollia-os-"),
            "unexpected archive name: {}",
            artifact.archive
        );
        assert!(
            artifact.archive.ends_with(".tar.gz") || artifact.archive.ends_with(".zip"),
            "unexpected archive extension: {}",
            artifact.archive
        );
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
        assert!((v.clone() <= v));
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
