//! Security posture: the single source of truth for the active isolation level.
//!
//! Agent Python runs in-process with the rights of the runtime process; there is
//! no OS sandbox around agent code. Native tools are confined by OS-native
//! primitives whose effectiveness depends on the platform. This module computes,
//! per platform and runtime state, exactly what confinement is active, so the CLI
//! (`doctor`, `status`) and the desktop shell can surface it honestly rather than
//! implying isolation that is not there.
//!
//! The posture reflects the project's in-process trust model for agent code.

use serde::{Deserialize, Serialize};

/// The OS-native confinement applied to native tool child processes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolSandbox {
    /// Linux: tool commands run under a PID and mount namespace via `unshare`.
    LinuxNamespaces,
    /// macOS and other platforms: no OS sandbox around tool processes. A
    /// per-invocation warning is emitted and only trusted agents should run.
    DevNoSandbox,
}

/// How agent Python code is executed. A single honest variant in v0.1.0: agent
/// code is trusted and shares the runtime process. No trusted/untrusted tiers
/// exist; introducing one without real OS isolation would be security theater.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentExecution {
    /// In-process via the PyO3 bridge, with the full rights of the current user.
    InProcessTrusted,
}

/// A snapshot of the active isolation level on the running host.
///
/// Serializes to `snake_case` for the CLI `--json` surface and the Tauri IPC
/// consumed by the desktop shell.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecurityPosture {
    /// Human-readable platform identifier, e.g. `"macos aarch64"`.
    pub platform: String,
    /// The confinement applied to native tool child processes.
    pub tool_sandbox: ToolSandbox,
    /// How agent code itself is executed.
    pub agent_execution: AgentExecution,
    /// Whether per-process resource limits (`setrlimit`) are applied to tool
    /// child processes. True on Unix. This means the limits are set, not that
    /// the kernel enforces every one equally (macOS enforces address-space
    /// limits weakly, for instance).
    pub rlimits_active: bool,
    /// Whether `unshare` PID and mount namespaces are usable on this host.
    /// False on non-Linux platforms and on Linux hosts without the required
    /// capability (for example standard CI runners without `CAP_SYS_ADMIN`).
    pub unshare_available: bool,
}

impl SecurityPosture {
    /// Probes the live platform and runtime state.
    ///
    /// Cheap and on-demand: it spawns `unshare` once on Linux to confirm the
    /// namespace path is usable. Keep it off any hot path; callers are the CLI
    /// diagnostics and the desktop settings surface.
    pub fn detect() -> Self {
        let platform = format!("{} {}", std::env::consts::OS, std::env::consts::ARCH);
        let unshare_available = probe_unshare();
        let tool_sandbox = if cfg!(target_os = "linux") && unshare_available {
            ToolSandbox::LinuxNamespaces
        } else {
            ToolSandbox::DevNoSandbox
        };
        Self {
            platform,
            tool_sandbox,
            agent_execution: AgentExecution::InProcessTrusted,
            rlimits_active: cfg!(unix),
            unshare_available,
        }
    }
}

/// Returns true when `unshare --pid --mount --fork` can actually run on this
/// host. Non-Linux platforms and Linux hosts lacking `CAP_SYS_ADMIN` return
/// false. Mirrors the probe the bash executor uses to self-skip its tests.
#[cfg(target_os = "linux")]
fn probe_unshare() -> bool {
    std::process::Command::new("/usr/bin/unshare")
        .args(["--pid", "--mount", "--fork", "/bin/true"])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

#[cfg(not(target_os = "linux"))]
fn probe_unshare() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_reports_in_process_trusted_agent() {
        // GIVEN the running platform
        // WHEN detecting the posture
        let posture = SecurityPosture::detect();
        // THEN the agent execution model is always in-process trusted
        assert_eq!(posture.agent_execution, AgentExecution::InProcessTrusted);
        assert!(!posture.platform.is_empty());
    }

    #[test]
    fn test_detect_tool_sandbox_matches_platform() {
        // GIVEN the running platform
        // WHEN detecting the posture
        let posture = SecurityPosture::detect();
        // THEN a non-Linux host never reports Linux namespaces
        #[cfg(not(target_os = "linux"))]
        {
            assert_eq!(posture.tool_sandbox, ToolSandbox::DevNoSandbox);
            assert!(!posture.unshare_available);
        }
        // AND rlimits are marked active on any Unix host
        #[cfg(unix)]
        assert!(posture.rlimits_active);
        #[cfg(not(unix))]
        assert!(!posture.rlimits_active);
        // silence unused warning on Linux where the block above is empty
        let _ = &posture;
    }

    #[test]
    fn test_posture_serializes_to_snake_case() {
        // GIVEN a posture with known variants
        let posture = SecurityPosture {
            platform: "linux x86_64".to_string(),
            tool_sandbox: ToolSandbox::LinuxNamespaces,
            agent_execution: AgentExecution::InProcessTrusted,
            rlimits_active: true,
            unshare_available: true,
        };
        // WHEN serializing to JSON
        let json = serde_json::to_string(&posture).expect("serialize");
        // THEN enum values are snake_case and round-trip
        assert!(json.contains("\"linux_namespaces\""));
        assert!(json.contains("\"in_process_trusted\""));
        let back: SecurityPosture = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, posture);
    }
}
