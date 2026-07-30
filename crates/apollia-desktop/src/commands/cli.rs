//! Tauri IPC commands for CLI installation management.
//!
//! Docker Desktop-style model: the `apollia-os` CLI binary is bundled inside
//! the `.app` bundle (via Tauri `resources`), and the user can create a symlink
//! in `/usr/local/bin/` from the Settings page.

use serde::Serialize;
use std::path::PathBuf;
use tauri::Manager;

/// Default symlink target on macOS and Linux.
const SYMLINK_DIR: &str = "/usr/local/bin";
/// Binary name used for the symlink, with the platform executable suffix.
const CLI_BINARY_NAME: &str = if cfg!(windows) {
    "apollia-os.exe"
} else {
    "apollia-os"
};

/// Status of the CLI installation.
#[derive(Debug, Serialize)]
pub struct CliStatus {
    /// Whether the CLI binary is bundled inside this app.
    pub bundled: bool,
    /// Absolute path to the bundled binary (inside .app/Contents/Resources/).
    pub bundled_path: Option<String>,
    /// Whether the symlink exists at the target location.
    pub installed: bool,
    /// Target symlink path (e.g. `/usr/local/bin/apollia-os`).
    pub symlink_path: String,
    /// Version of the bundled CLI (matches desktop version).
    pub version: String,
    /// If true, installation requires privilege escalation.
    pub needs_privilege: bool,
}

/// Resolves the path to the bundled CLI binary inside the app resources.
fn bundled_cli_path(app: &tauri::AppHandle) -> Option<PathBuf> {
    let resource_dir = app.path().resource_dir().ok()?;
    let candidate = resource_dir.join(CLI_BINARY_NAME);
    if candidate.exists() {
        Some(candidate)
    } else {
        None
    }
}

/// Returns the target symlink path.
fn symlink_target() -> PathBuf {
    PathBuf::from(SYMLINK_DIR).join(CLI_BINARY_NAME)
}

/// Checks whether the symlink directory is writable by the current user.
fn dir_is_writable(dir: &std::path::Path) -> bool {
    if !dir.exists() {
        return false;
    }
    // Try creating a temporary file to test write access.
    let probe = dir.join(".apollia-write-probe");
    match std::fs::File::create(&probe) {
        Ok(_) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

/// Returns the current CLI installation status.
#[tauri::command]
pub fn get_cli_status(app: tauri::AppHandle) -> Result<CliStatus, String> {
    let bundled = bundled_cli_path(&app);
    let target = symlink_target();
    let symlink_dir = PathBuf::from(SYMLINK_DIR);

    let installed = target.is_symlink();
    let needs_privilege = !dir_is_writable(&symlink_dir);

    Ok(CliStatus {
        bundled: bundled.is_some(),
        bundled_path: bundled.map(|p| p.display().to_string()),
        installed,
        symlink_path: target.display().to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        needs_privilege,
    })
}

/// Refuses off Unix: the mechanism is a symlink into `/usr/local/bin` with
/// `osascript`/`pkexec` escalation. Windows needs a different design (a PATH
/// entry or a shim), which is deliberately not guessed here. Present so the
/// crate compiles for that target and the command fails with a clear message
/// instead of not existing.
#[cfg(not(unix))]
#[tauri::command]
pub async fn install_cli(_app: tauri::AppHandle) -> Result<(), String> {
    Err(
        "installing the CLI from the app is only supported on macOS and Linux; \
         add the bundled apollia-os binary to your PATH manually"
            .to_string(),
    )
}

/// Creates a symlink from `/usr/local/bin/apollia-os` to the bundled CLI
/// binary. Escalates to admin privileges on macOS via `osascript` if the
/// target directory is not writable.
#[cfg(unix)]
#[tauri::command]
pub async fn install_cli(app: tauri::AppHandle) -> Result<(), String> {
    let source =
        bundled_cli_path(&app).ok_or_else(|| "CLI binary not found in app bundle".to_string())?;
    let target = symlink_target();
    let symlink_dir = PathBuf::from(SYMLINK_DIR);

    // Fast path: try without escalation.
    if dir_is_writable(&symlink_dir) {
        // Remove stale symlink if present.
        if target.is_symlink() || target.exists() {
            std::fs::remove_file(&target)
                .map_err(|e| format!("failed to remove existing file: {e}"))?;
        }
        std::os::unix::fs::symlink(&source, &target)
            .map_err(|e| format!("failed to create symlink: {e}"))?;
        tracing::info!(
            source = %source.display(),
            target = %target.display(),
            "CLI symlink created"
        );
        return Ok(());
    }

    // Slow path: privilege escalation.
    let cmd = format!(
        "mkdir -p '{}' && ln -sf '{}' '{}'",
        symlink_dir.display(),
        source.display(),
        target.display()
    );
    run_with_admin_privileges(&cmd).await?;
    tracing::info!(
        source = %source.display(),
        target = %target.display(),
        "CLI symlink created (with admin privileges)"
    );
    Ok(())
}

/// Removes the CLI symlink from `/usr/local/bin/apollia-os`.
///
/// Safety: only removes the file if it is a symlink, never deletes a real
/// binary.
#[tauri::command]
pub async fn uninstall_cli() -> Result<(), String> {
    let target = symlink_target();

    if !target.exists() && !target.is_symlink() {
        return Err("CLI is not installed".to_string());
    }

    if !target.is_symlink() {
        return Err(format!(
            "{} is not a symlink - refusing to delete",
            target.display()
        ));
    }

    // Fast path.
    match std::fs::remove_file(&target) {
        Ok(()) => {
            tracing::info!(path = %target.display(), "CLI symlink removed");
            return Ok(());
        }
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            // Fall through to escalation.
        }
        Err(e) => return Err(format!("failed to remove symlink: {e}")),
    }

    // Slow path: privilege escalation.
    let cmd = format!("rm -f '{}'", target.display());
    run_with_admin_privileges(&cmd).await?;
    tracing::info!(path = %target.display(), "CLI symlink removed (with admin privileges)");
    Ok(())
}

/// Executes a shell command with platform-appropriate privilege escalation.
///
/// - macOS: uses `osascript` which shows the native password dialog.
/// - Linux: uses `pkexec` (PolicyKit).
async fn run_with_admin_privileges(command: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let escaped = command.replace('\\', "\\\\").replace('"', "\\\"");
        let output = tokio::process::Command::new("osascript")
            .args([
                "-e",
                &format!("do shell script \"{escaped}\" with administrator privileges"),
            ])
            .output()
            .await
            .map_err(|e| format!("failed to run osascript: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // User cancelled the dialog.
            if stderr.contains("User canceled") || stderr.contains("-128") {
                return Err("Installation cancelled by user".to_string());
            }
            return Err(format!("privilege escalation failed: {stderr}"));
        }
        Ok(())
    }

    #[cfg(target_os = "linux")]
    {
        let output = tokio::process::Command::new("pkexec")
            .args(["sh", "-c", command])
            .output()
            .await
            .map_err(|e| format!("failed to run pkexec: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("privilege escalation failed: {stderr}"));
        }
        Ok(())
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = command;
        Err("CLI installation is only supported on macOS and Linux".to_string())
    }
}
