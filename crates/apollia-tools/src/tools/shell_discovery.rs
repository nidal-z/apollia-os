//! POSIX shell discovery shared by the bash validator and executor.
//!
//! One resolved shell both validates (`-n -c`) and executes (`-c`) a command,
//! so a command can never pass validation under one parser and then run under
//! another. On Unix the shell is `/bin/sh`. Off Unix a POSIX shell must come
//! from Git Bash, MSYS2 or WSL; `cmd.exe` and PowerShell are never used
//! because the command validators guarding `bash_executor` encode POSIX
//! quoting and chaining semantics, and a different shell would silently
//! change the injection surface they were written for.

use std::ffi::OsStr;
use std::path::PathBuf;

use thiserror::Error;

/// Candidate program names probed on `PATH` when the host has no `/bin/sh`,
/// in preference order. Git for Windows ships `bash.exe`; MSYS2 and Cygwin
/// ship both `bash.exe` and `sh.exe`.
pub const WINDOWS_SHELL_CANDIDATES: &[&str] = &["bash.exe", "sh.exe", "bash", "sh"];

/// No POSIX shell is available on this host.
///
/// The message is the single source of the user-facing guidance: it names
/// what is missing and how to obtain it, and says why no native Windows
/// shell is substituted.
#[derive(Debug, Error)]
#[error(
    "no POSIX shell (bash or sh) found on PATH; bash_executor needs one to run \
     commands. On Windows, install Git Bash (gitforwindows.org) or MSYS2, or \
     enable WSL, then restart Apollia so PATH includes bash.exe. cmd.exe and \
     PowerShell are not supported: command validation encodes POSIX shell \
     semantics."
)]
pub struct ShellUnavailable;

/// Search the directories of `path_var` for the first of `candidates` that
/// exists as a file.
///
/// `PATH` is a parameter rather than read from the process environment, so
/// the Windows resolution logic is exercisable from any host.
pub fn find_program_in_path(candidates: &[&str], path_var: Option<&OsStr>) -> Option<PathBuf> {
    let path_var = path_var?;
    for name in candidates {
        let hit = std::env::split_paths(path_var)
            .map(|dir| dir.join(name))
            .find(|candidate| candidate.is_file());
        if hit.is_some() {
            return hit;
        }
    }
    None
}

/// Resolve a POSIX shell the way a non-Unix host must: from `PATH` alone.
pub fn resolve_windows_posix_shell(path_var: Option<&OsStr>) -> Result<PathBuf, ShellUnavailable> {
    find_program_in_path(WINDOWS_SHELL_CANDIDATES, path_var).ok_or(ShellUnavailable)
}

/// The single POSIX shell used to validate and execute commands on this host.
///
/// Unix always has `/bin/sh` (POSIX guarantees it); resolution cannot fail
/// there. Off Unix the shell comes from `PATH` or the call fails with the
/// actionable [`ShellUnavailable`] message.
#[cfg(unix)]
pub fn resolve_posix_shell() -> Result<PathBuf, ShellUnavailable> {
    Ok(PathBuf::from("/bin/sh"))
}

/// The single POSIX shell used to validate and execute commands on this host.
///
/// Off Unix the shell comes from `PATH` (Git Bash, MSYS2, WSL) or the call
/// fails with the actionable [`ShellUnavailable`] message.
#[cfg(not(unix))]
pub fn resolve_posix_shell() -> Result<PathBuf, ShellUnavailable> {
    resolve_windows_posix_shell(std::env::var_os("PATH").as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_path_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("apollia-shell-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("create fake PATH dir");
        dir
    }

    #[test]
    fn find_program_in_path_finds_candidate_in_fake_path() {
        // GIVEN a directory containing a file named bash.exe and a PATH value
        // pointing at it
        let dir = fake_path_dir();
        std::fs::write(dir.join("bash.exe"), b"").expect("seed bash.exe");
        let path_var = std::env::join_paths([&dir]).expect("join paths");

        // WHEN searching the Windows shell candidates against that PATH
        let found = find_program_in_path(WINDOWS_SHELL_CANDIDATES, Some(path_var.as_os_str()));

        // THEN the bash.exe path is returned
        assert_eq!(found, Some(dir.join("bash.exe")));

        // Cleanup
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn find_program_in_path_prefers_bash_over_sh() {
        // GIVEN both bash.exe and sh.exe present on the fake PATH
        let dir = fake_path_dir();
        std::fs::write(dir.join("bash.exe"), b"").expect("seed bash.exe");
        std::fs::write(dir.join("sh.exe"), b"").expect("seed sh.exe");
        let path_var = std::env::join_paths([&dir]).expect("join paths");

        // WHEN resolving
        let found = find_program_in_path(WINDOWS_SHELL_CANDIDATES, Some(path_var.as_os_str()));

        // THEN bash.exe wins per the candidate preference order
        assert_eq!(found, Some(dir.join("bash.exe")));

        // Cleanup
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn resolve_windows_posix_shell_errors_on_empty_path_and_names_providers() {
        // GIVEN no PATH value at all
        // WHEN resolving the Windows way
        let result = resolve_windows_posix_shell(None);

        // THEN the error is actionable: it names each provider of a POSIX
        // shell and states that native Windows shells are not substituted
        let message = result.expect_err("no PATH must fail").to_string();
        assert!(message.contains("Git Bash"), "missing Git Bash: {message}");
        assert!(message.contains("MSYS2"), "missing MSYS2: {message}");
        assert!(message.contains("WSL"), "missing WSL: {message}");
        assert!(
            message.contains("cmd.exe") && message.contains("PowerShell"),
            "must state why native shells are excluded: {message}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn resolve_posix_shell_is_bin_sh() {
        // GIVEN a Unix host
        // WHEN resolving the shell
        let shell = resolve_posix_shell().expect("Unix always has /bin/sh");

        // THEN it is /bin/sh, the same binary build_command executes with
        assert_eq!(shell, PathBuf::from("/bin/sh"));
    }
}
