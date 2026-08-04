//! System Python discovery for the Python executor.
//!
//! Windows hosts have no reliable `python3` name: the stock alias is a
//! Microsoft Store stub that exits non-zero with an install hint, and real
//! installs ship `python.exe` plus the `py` launcher. Discovery therefore
//! probes an ordered candidate list per platform and classifies the
//! `--version` output instead of trusting a bare exit status of `python3`.
//!
//! The probe order and the output classification are pure functions so both
//! platform branches are exercisable from any host.

use crate::tools::python_executor::PythonExecutorError;

/// A system interpreter invocation: program plus fixed leading arguments
/// (the Windows `py` launcher needs `-3` to select Python 3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PythonCommand {
    /// Program name resolved via `PATH`.
    pub program: String,
    /// Fixed arguments inserted before any per-call arguments.
    pub args: Vec<String>,
}

impl PythonCommand {
    fn new(program: &str, args: &[&str]) -> Self {
        Self {
            program: program.to_string(),
            args: args.iter().map(|a| a.to_string()).collect(),
        }
    }

    /// Human-readable form for error messages, e.g. `py -3`.
    pub fn display(&self) -> String {
        if self.args.is_empty() {
            self.program.clone()
        } else {
            format!("{} {}", self.program, self.args.join(" "))
        }
    }
}

/// Probe order per platform.
///
/// Windows: `python` (the canonical installed name), then the `py -3`
/// launcher, then `python3` last because the stock alias is the Microsoft
/// Store stub. Unix: `python3` first per convention, then `python`.
pub fn candidates(windows: bool) -> Vec<PythonCommand> {
    if windows {
        vec![
            PythonCommand::new("python", &[]),
            PythonCommand::new("py", &["-3"]),
            PythonCommand::new("python3", &[]),
        ]
    } else {
        vec![
            PythonCommand::new("python3", &[]),
            PythonCommand::new("python", &[]),
        ]
    }
}

/// Classify one `--version` probe result.
///
/// A usable interpreter exits 0 AND prints a banner starting with
/// `Python 3` (on stdout for Python 3, on stderr for Python 2, which is
/// rejected). The Microsoft Store stub exits non-zero and prints an install
/// hint instead, so the exit status alone is not enough on Windows.
pub fn probe_output_is_python3(exit_success: bool, stdout: &str, stderr: &str) -> bool {
    if !exit_success {
        return false;
    }
    let banner = if stdout.trim().is_empty() {
        stderr
    } else {
        stdout
    };
    banner.trim_start().starts_with("Python 3")
}

/// Locate the first working system Python 3 for this platform.
///
/// Runs `<candidate> --version` for each entry of [`candidates`] in order
/// and returns the first that classifies as Python 3.
///
/// # Errors
///
/// Returns [`PythonExecutorError::PythonUnavailable`] naming every probed
/// candidate when none of them is a working Python 3.
pub fn locate_system_python() -> Result<PythonCommand, PythonExecutorError> {
    let probe_list = candidates(cfg!(windows));
    for candidate in &probe_list {
        // Probe the interpreter the operator actually has. Under the
        // desktop's inherited PYTHONHOME this check would answer for the
        // bundled runtime instead, and `--version` succeeds even when a
        // real run would not.
        let mut probe = std::process::Command::new(&candidate.program);
        apollia_core::subprocess_env::scrub_bundled_python(&mut probe);
        // Discovery runs at startup and probes several candidates in a row, so
        // an unhidden probe is not one window but a burst of them.
        apollia_core::subprocess_window::hide_console(&mut probe);
        let output = probe.args(&candidate.args).arg("--version").output();
        if let Ok(output) = output {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            if probe_output_is_python3(output.status.success(), &stdout, &stderr) {
                return Ok(candidate.clone());
            }
        }
    }
    Err(PythonExecutorError::PythonUnavailable {
        tried: probe_list
            .iter()
            .map(PythonCommand::display)
            .collect::<Vec<_>>()
            .join(", "),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_accepts_python3_banner() {
        // GIVEN a probe that exited 0 with a Python 3 banner on stdout
        // WHEN classifying it
        // THEN it is accepted
        assert!(probe_output_is_python3(true, "Python 3.12.4\n", ""));
    }

    #[test]
    fn probe_accepts_python3_banner_on_stderr() {
        // GIVEN a probe whose banner landed on stderr (some builds do this)
        // WHEN classifying it
        // THEN it is accepted
        assert!(probe_output_is_python3(true, "", "Python 3.9.7\n"));
    }

    #[test]
    fn probe_rejects_python2_banner_on_stderr() {
        // GIVEN a Python 2 interpreter, which prints its banner on stderr
        // WHEN classifying it
        // THEN it is rejected
        assert!(!probe_output_is_python3(true, "", "Python 2.7.18\n"));
    }

    #[test]
    fn probe_rejects_store_stub() {
        // GIVEN the Microsoft Store stub: non-zero exit and an install hint
        // WHEN classifying it
        // THEN it is rejected even though a naive existence check would pass
        assert!(!probe_output_is_python3(
            false,
            "Python was not found; run without arguments to install from the \
             Microsoft Store, or disable this shortcut from Settings.\n",
            ""
        ));
    }

    #[test]
    fn candidates_order_windows_and_unix() {
        // GIVEN both platform shapes
        let windows = candidates(true);
        let unix = candidates(false);

        // WHEN reading the probe order
        // THEN Windows tries python, then the py launcher, then python3 last
        // (Store stub), and Unix tries python3 then python
        assert_eq!(
            windows,
            vec![
                PythonCommand::new("python", &[]),
                PythonCommand::new("py", &["-3"]),
                PythonCommand::new("python3", &[]),
            ]
        );
        assert_eq!(
            unix,
            vec![
                PythonCommand::new("python3", &[]),
                PythonCommand::new("python", &[]),
            ]
        );
    }

    #[test]
    fn python_command_display_includes_fixed_args() {
        // GIVEN the launcher candidate
        // WHEN rendering it for an error message
        // THEN the fixed argument is visible
        assert_eq!(PythonCommand::new("py", &["-3"]).display(), "py -3");
        assert_eq!(PythonCommand::new("python", &[]).display(), "python");
    }
}
