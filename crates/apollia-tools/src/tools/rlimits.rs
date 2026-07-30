//! Per-process resource limits for sandboxed tool child processes.
//!
//! Native tools (`bash_executor`, `python_executor`) spawn child processes on the
//! agent's behalf. Beyond the wall-clock timeout enforced by the executor, this
//! module applies POSIX `setrlimit` limits to each child so a single tool call
//! cannot exhaust CPU, memory, or file descriptors on the host.
//!
//! ## Honesty about what these limits do
//!
//! - `RLIMIT_CPU` is CPU-seconds per process (`SIGXCPU` then `SIGKILL`). It
//!   complements, and does not replace, the wall-clock timeout: a process that
//!   sleeps or blocks burns no CPU and is caught only by the wall-clock.
//! - `RLIMIT_AS` caps the address space of each process individually. It is NOT a
//!   total budget across a process tree: a child that forks many small processes
//!   can still use more host memory than one process's cap. It is applied on Linux
//!   only: macOS rejects setting it (`EINVAL`) and enforces it unreliably, so we do
//!   not claim it there rather than fail the spawn or pretend it is active.
//! - `RLIMIT_NOFILE` caps open file descriptors per process.
//! - `RLIMIT_NPROC` is enforced per real UID across all of the user's processes,
//!   not per sandbox. A low value can make unrelated user processes fail with
//!   `EAGAIN`, so it ships disabled (`None`) in v0.1.0.
//!
//! On non-Unix platforms these functions are honest no-ops (Windows is out of
//! scope).

/// Per-process resource limits applied to a sandboxed child process.
#[derive(Debug, Clone, Copy)]
pub struct ResourceLimits {
    /// `RLIMIT_CPU`: CPU-seconds before `SIGXCPU`.
    pub cpu_seconds: u64,
    /// `RLIMIT_AS`: per-process address space in bytes (not a tree total).
    pub address_space_bytes: u64,
    /// `RLIMIT_NOFILE`: maximum open file descriptors.
    pub open_files: u64,
    /// `RLIMIT_NPROC`: per-UID process cap. `None` leaves it unset (recommended
    /// in v0.1.0 because the limit is per-UID, not per-sandbox).
    pub max_processes: Option<u64>,
}

impl ResourceLimits {
    /// Fixed v0.1.0 defaults. Not operator-configurable yet; a `with_config`
    /// overload is a documented follow-up.
    ///
    /// `max_processes` is deliberately `None`: `RLIMIT_NPROC` is per-UID and a
    /// low value would interfere with unrelated user processes.
    pub const fn v0_defaults() -> Self {
        Self {
            cpu_seconds: 120,
            address_space_bytes: 2 * 1024 * 1024 * 1024,
            open_files: 256,
            max_processes: None,
        }
    }
}

/// Attaches a `pre_exec` hook that applies `limits` to the child spawned by
/// `cmd`. The hook runs in the child after `fork` and before `execve`, so the
/// limits are inherited by the executed image and by any process it forks
/// (for example `/bin/sh` under `unshare --fork`).
#[cfg(unix)]
pub fn apply_rlimits(cmd: &mut std::process::Command, limits: ResourceLimits) {
    use std::os::unix::process::CommandExt;

    let cpu = limits.cpu_seconds;
    #[cfg(target_os = "linux")]
    let address_space = limits.address_space_bytes;
    let nofile = limits.open_files;
    let nproc = limits.max_processes;

    // SAFETY: the closure runs in the forked child between fork() and execve().
    // Only async-signal-safe operations are permitted there. `setrlimit(2)` is on
    // the POSIX async-signal-safe list; every `rlimit` passed to it is a
    // fully-initialized stack value; the closure allocates nothing, takes no lock,
    // and performs no logging. It captures only `Copy` integers by move. The raw
    // FFI call borrows each `rlimit` only for the duration of the call, so it
    // cannot violate Rust aliasing.
    unsafe {
        cmd.pre_exec(move || {
            set_one(libc::RLIMIT_CPU, cpu)?;
            // RLIMIT_AS is Linux-only: macOS returns EINVAL when setting it.
            #[cfg(target_os = "linux")]
            set_one(libc::RLIMIT_AS, address_space)?;
            set_one(libc::RLIMIT_NOFILE, nofile)?;
            if let Some(n) = nproc {
                set_one(libc::RLIMIT_NPROC, n)?;
            }
            Ok(())
        });
    }
}

/// No-op on non-Unix platforms: POSIX `setrlimit` has no equivalent there. A
/// Windows build therefore gets no per-process resource caps, which
/// `SecurityPosture` reports honestly rather than implying it does.
#[cfg(not(unix))]
pub fn apply_rlimits(_cmd: &mut std::process::Command, _limits: ResourceLimits) {}

/// Sets a single soft-and-hard resource limit via `setrlimit`.
///
/// Called only from the `pre_exec` hook above; must stay async-signal-safe.
#[cfg(unix)]
fn set_one(resource: libc::c_int, value: u64) -> std::io::Result<()> {
    let rlim = libc::rlimit {
        rlim_cur: value as libc::rlim_t,
        rlim_max: value as libc::rlim_t,
    };
    // SAFETY: `resource` is a valid RLIMIT_* constant and `rlim` is a fully
    // initialized stack value borrowed only for this call. See the caller's
    // SAFETY note for the async-signal-safety invariant.
    let rc = unsafe { libc::setrlimit(resource, &rlim) };
    if rc != 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn test_v0_defaults_disable_nproc() {
        // GIVEN the shipped defaults
        let limits = ResourceLimits::v0_defaults();
        // THEN NPROC is disabled (per-UID semantics) and the rest are set
        assert_eq!(limits.max_processes, None);
        assert_eq!(limits.cpu_seconds, 120);
        assert_eq!(limits.open_files, 256);
        assert_eq!(limits.address_space_bytes, 2 * 1024 * 1024 * 1024);
    }

    #[test]
    fn test_apply_rlimits_caps_open_files() {
        // GIVEN a command that prints its file-descriptor soft limit
        let mut cmd = std::process::Command::new("/bin/sh");
        cmd.args(["-c", "ulimit -n"]);
        let limits = ResourceLimits {
            open_files: 64,
            ..ResourceLimits::v0_defaults()
        };
        apply_rlimits(&mut cmd, limits);
        // WHEN running it
        let output = cmd.output().expect("spawn with rlimits must succeed");
        // THEN the child observes the capped soft limit
        assert!(output.status.success(), "ulimit -n should run");
        let reported = String::from_utf8_lossy(&output.stdout);
        assert_eq!(reported.trim(), "64", "ulimit -n should reflect the cap");
    }

    #[test]
    fn test_apply_rlimits_cpu_limit_kills_busy_loop() {
        use std::os::unix::process::ExitStatusExt;

        // GIVEN a busy-looping command under a 1 CPU-second limit
        let mut cmd = std::process::Command::new("/bin/sh");
        cmd.args(["-c", "while :; do :; done"]);
        let limits = ResourceLimits {
            cpu_seconds: 1,
            ..ResourceLimits::v0_defaults()
        };
        apply_rlimits(&mut cmd, limits);
        // WHEN running it
        let output = match cmd.output() {
            Ok(o) => o,
            // Spawning may be forbidden in some sandboxed CI environments.
            Err(_) => return,
        };
        // THEN the kernel terminated it (SIGXCPU) rather than a clean exit
        assert!(
            !output.status.success(),
            "a busy loop under RLIMIT_CPU must not exit successfully"
        );
        assert!(
            output.status.signal().is_some() || output.status.code() != Some(0),
            "process should be terminated by a signal or non-zero exit"
        );
    }
}
