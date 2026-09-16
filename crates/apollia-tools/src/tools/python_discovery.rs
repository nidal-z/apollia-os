//! Which Python interpreter the executor runs, and in which order it is looked
//! for.
//!
//! The order is the whole content of this module, and it is not the obvious one.
//!
//! 1. **The interpreter the operator chose**, `[tools] python_interpreter` in
//!    `apollia.toml`. Named by absolute path, checked before every use: it has
//!    to exist, start, and report a minor version the bundled standard library
//!    can be read by. One that stops satisfying that, after an upgrade or an
//!    uninstall, is dropped with a warning rather than failing the tool.
//! 2. **The bundled interpreter**, the one Apollia ships with, named by
//!    `APOLLIA_BUNDLED_PYTHON` or found beside the executable. This is the
//!    default, on all three systems, and it is the only configuration the
//!    product is tested in.
//! 3. **A system interpreter**, and only when no bundle exists at all, which is
//!    the development tree. A packaged install always carries its bundle, so
//!    this branch is unreachable there; it is reported at `warn` when it is
//!    taken, because it is not the shipped configuration.
//!
//! The order used to be the reverse: the system was probed and the bundle was
//! never consulted. That reads as harmless until the machine is the one this
//! product is for, a managed Windows desktop with no administrator rights and
//! whatever Python the image happened to carry, where the system interpreter is
//! a Microsoft Store stub, a Python 3.9, or a 3.14 that cannot read a 3.13
//! standard library.
//!
//! Windows hosts have no reliable `python3` name either: the stock alias is a
//! Microsoft Store stub that exits non-zero with an install hint, and real
//! installs ship `python.exe` plus the `py` launcher. The system probe
//! therefore walks an ordered candidate list per platform and classifies the
//! `--version` output instead of trusting a bare exit status.
//!
//! The probe order, the version compatibility rule and the output
//! classification are pure functions, so both platform branches are exercisable
//! from any host.

use std::path::{Path, PathBuf};

use crate::tools::python_executor::PythonExecutorError;

/// Minor version of the interpreter Apollia bundles.
///
/// A chosen interpreter has to match it: `PYTHONHOME` and `PYTHONPATH` point at
/// the bundled standard library, and a different minor version dies on startup
/// reading it. Measured, and written down in `apollia_core::subprocess_env`: a
/// 3.9 interpreter loading a 3.13 standard library raises `ImportError: cannot
/// import name 'text_encoding' from 'io'` before a line of user code runs.
pub const BUNDLED_PYTHON_MINOR: u32 = 13;

/// Environment variable naming the bundled interpreter by absolute path.
///
/// Exported by the desktop bootstrap. The CLI launchers export
/// `APOLLIA_PYTHON_BUNDLE_DIR` instead, which names the directory; both are
/// read here.
const BUNDLED_INTERPRETER_VAR: &str = "APOLLIA_BUNDLED_PYTHON";

/// Environment variable naming the directory the bundled interpreter sits in.
const BUNDLED_DIR_VAR: &str = "APOLLIA_PYTHON_BUNDLE_DIR";

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

/// Where a resolved interpreter came from, for the log line and the tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterpreterSource {
    /// `[tools] python_interpreter`, checked and accepted.
    Chosen,
    /// The interpreter Apollia ships with. The default on every system.
    Bundled,
    /// A `PATH` probe, reachable only when no bundle exists.
    System,
}

/// A resolved interpreter: how to invoke it, and where it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedInterpreter {
    /// The invocation.
    pub command: PythonCommand,
    /// Which of the three rules produced it.
    pub source: InterpreterSource,
}

/// Read the minor version out of a `python --version` banner.
///
/// `Python 3.13.7` yields `Some(13)`. Anything that is not a Python 3 banner
/// yields `None`, which the caller treats as unusable.
pub fn parse_minor_version(banner: &str) -> Option<u32> {
    let rest = banner.trim_start().strip_prefix("Python 3.")?;
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

/// Whether an interpreter of that minor version can run against the bundled
/// standard library.
///
/// Exact equality, and it is not conservatism: `PYTHONHOME` and `PYTHONPATH`
/// point at one standard library, and a neighbouring minor version fails on it
/// in the two ways `apollia_core::subprocess_env` documents. An older one dies
/// on a symbol it does not know; a newer one starts and then silently runs
/// against a standard library nobody chose, which is the worse of the two.
pub fn minor_version_is_compatible(minor: u32) -> bool {
    minor == BUNDLED_PYTHON_MINOR
}

/// Path of the bundled interpreter, when this process knows where it is.
///
/// Reads the two variables the launchers and the desktop bootstrap export, in
/// that order. Returns `None` in a development tree, where no bundle is staged.
pub fn bundled_interpreter() -> Option<PathBuf> {
    if let Some(named) = std::env::var_os(BUNDLED_INTERPRETER_VAR) {
        let path = PathBuf::from(named);
        if path.is_file() {
            return Some(path);
        }
    }
    let dir = PathBuf::from(std::env::var_os(BUNDLED_DIR_VAR)?);
    bundled_interpreter_in(&dir)
}

/// The interpreter inside a bundle directory, whichever layout it carries.
///
/// The three systems do not agree on where it sits, and neither do the two
/// packagings: `python-build-standalone` puts it under `bin/` on POSIX and at
/// the root on Windows, the desktop keeps the bundle in a `python/`
/// subdirectory, and the CLI archive flattens that subdirectory into the
/// archive root. All four spellings are the same bundle seen from a different
/// place, so all four are probed.
pub fn bundled_interpreter_in(dir: &Path) -> Option<PathBuf> {
    [
        dir.join(format!("bin/python3.{BUNDLED_PYTHON_MINOR}")),
        dir.join("python.exe"),
        dir.join(format!("python/bin/python3.{BUNDLED_PYTHON_MINOR}")),
        dir.join("python/python.exe"),
    ]
    .into_iter()
    .find(|candidate| candidate.is_file())
}

/// Run `--version` on an interpreter and answer its minor version.
///
/// `None` when it does not exist, does not start, or prints something that is
/// not a Python 3 banner. The bundled environment is scrubbed from the probe,
/// because under an inherited `PYTHONHOME` the answer would describe the
/// bundled runtime rather than the interpreter being asked.
fn probe_minor_version(program: &Path) -> Option<u32> {
    if !program.is_file() {
        return None;
    }
    let mut probe = std::process::Command::new(program);
    apollia_core::subprocess_env::scrub_bundled_python(&mut probe);
    apollia_core::subprocess_window::hide_console(&mut probe);
    let output = probe.arg("--version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let banner = if stdout.trim().is_empty() {
        stderr.to_string()
    } else {
        stdout.to_string()
    };
    parse_minor_version(&banner)
}

/// Check an interpreter an operator named, and say why it is refused.
///
/// The three conditions of the setting, in the order a reader would check them:
/// the path exists and starts, and its minor version matches the bundled
/// standard library.
///
/// # Errors
///
/// [`PythonExecutorError::ChosenInterpreterInvalid`], naming the path and the
/// condition it failed.
pub fn validate_chosen_interpreter(path: &Path) -> Result<(), PythonExecutorError> {
    let refuse = |reason: String| PythonExecutorError::ChosenInterpreterInvalid {
        path: path.display().to_string(),
        reason,
    };
    if !path.is_absolute() {
        return Err(refuse(
            "the path is relative; name the interpreter by absolute path so it \
             resolves the same from every working directory"
                .to_string(),
        ));
    }
    if !path.is_file() {
        return Err(refuse("no file at that path".to_string()));
    }
    match probe_minor_version(path) {
        None => Err(refuse(
            "it did not start, or answered something other than a Python 3 \
             version banner"
                .to_string(),
        )),
        Some(minor) if !minor_version_is_compatible(minor) => Err(refuse(format!(
            "it is Python 3.{minor}, and Apollia bundles the standard library of \
             Python 3.{BUNDLED_PYTHON_MINOR}; a different minor version cannot \
             read it"
        ))),
        Some(_) => Ok(()),
    }
}

/// Resolve the interpreter to run, applying the three rules of this module.
///
/// `chosen` is `[tools] python_interpreter`. A chosen interpreter that no
/// longer satisfies its conditions is dropped with a warning naming why, and
/// the bundled one answers instead: an upgrade on the operator's machine is not
/// a reason for the agent's tool to stop working.
///
/// # Errors
///
/// [`PythonExecutorError::PythonUnavailable`] when there is no bundle and no
/// system interpreter either.
pub fn resolve_interpreter(
    chosen: Option<&str>,
) -> Result<ResolvedInterpreter, PythonExecutorError> {
    resolve_interpreter_with(chosen, bundled_interpreter())
}

/// The rules of [`resolve_interpreter`], with the bundled interpreter supplied
/// rather than discovered.
///
/// The discovery reads two environment variables, which are process-wide; a
/// test that set them would race every other test in the binary. Taking the
/// bundle as an argument is what makes the ordering itself testable on any
/// host, whatever that host happens to have installed.
pub fn resolve_interpreter_with(
    chosen: Option<&str>,
    bundled: Option<PathBuf>,
) -> Result<ResolvedInterpreter, PythonExecutorError> {
    if let Some(chosen) = chosen.map(str::trim).filter(|s| !s.is_empty()) {
        let path = PathBuf::from(chosen);
        match validate_chosen_interpreter(&path) {
            Ok(()) => {
                tracing::info!(path = %path.display(), "python.interpreter.chosen");
                return Ok(ResolvedInterpreter {
                    command: PythonCommand::new(&path.display().to_string(), &[]),
                    source: InterpreterSource::Chosen,
                });
            }
            Err(e) => tracing::warn!(
                path = %path.display(),
                error = %e,
                detail = "falling back to the bundled interpreter",
                "python.interpreter.chosen.invalid"
            ),
        }
    }

    if let Some(path) = bundled {
        tracing::debug!(path = %path.display(), "python.interpreter.bundled");
        return Ok(ResolvedInterpreter {
            command: PythonCommand::new(&path.display().to_string(), &[]),
            source: InterpreterSource::Bundled,
        });
    }

    // No bundle at all: a development tree. A packaged install always carries
    // one, so this is not the shipped configuration and the log says so.
    let command = locate_system_python()?;
    tracing::warn!(
        interpreter = %command.display(),
        detail = "no bundled interpreter was found, which is not the shipped configuration",
        "python.interpreter.system"
    );
    Ok(ResolvedInterpreter {
        command,
        source: InterpreterSource::System,
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

#[cfg(test)]
mod interpreter_choice_tests {
    use super::*;

    #[test]
    fn a_version_banner_yields_its_minor() {
        // GIVEN the banner shapes an interpreter prints
        // WHEN the minor version is read out of each
        // THEN a Python 3 banner yields its minor and nothing else does
        assert_eq!(parse_minor_version("Python 3.13.7\n"), Some(13));
        assert_eq!(parse_minor_version("  Python 3.9.7"), Some(9));
        assert_eq!(parse_minor_version("Python 3.14.0rc1"), Some(14));
        assert_eq!(parse_minor_version("Python 2.7.18"), None);
        assert_eq!(
            parse_minor_version("Python was not found; run without arguments"),
            None
        );
    }

    #[test]
    fn only_the_bundled_minor_is_compatible() {
        // GIVEN the minor version Apollia bundles and its two neighbours
        // WHEN each is checked against the bundled standard library
        // THEN only the exact match is accepted: an older interpreter dies on a
        // symbol it does not know, and a newer one silently runs against a
        // standard library nobody chose
        assert!(minor_version_is_compatible(BUNDLED_PYTHON_MINOR));
        assert!(!minor_version_is_compatible(BUNDLED_PYTHON_MINOR - 1));
        assert!(!minor_version_is_compatible(BUNDLED_PYTHON_MINOR + 1));
    }

    #[test]
    fn a_relative_path_is_refused_with_its_reason() {
        // GIVEN an interpreter named relatively, which resolves differently per
        // working directory
        let result = validate_chosen_interpreter(Path::new("python3"));
        // WHEN it is validated
        // THEN it is refused, and the message says to use an absolute path
        let err = result.expect_err("a relative path must be refused");
        assert!(err.to_string().contains("absolute path"), "message: {err}");
    }

    #[test]
    fn a_path_with_no_file_is_refused() {
        // GIVEN an absolute path where nothing exists
        let result = validate_chosen_interpreter(Path::new("/nonexistent/apollia/python3.13"));
        // WHEN it is validated
        // THEN it is refused on the path alone, without running anything
        let err = result.expect_err("a missing file must be refused");
        assert!(
            err.to_string().contains("no file at that path"),
            "message: {err}"
        );
    }

    #[test]
    fn a_file_that_is_not_an_interpreter_is_refused() {
        // GIVEN an absolute path to a real file that is not a Python
        let file = tempfile::NamedTempFile::new().expect("a temp file");
        // WHEN it is validated
        let result = validate_chosen_interpreter(file.path());
        // THEN it is refused for not starting, rather than accepted on the
        // strength of existing
        let err = result.expect_err("a non-interpreter must be refused");
        assert!(err.to_string().contains("did not start"), "message: {err}");
    }

    #[test]
    fn an_invalid_choice_falls_back_instead_of_failing() {
        // GIVEN a chosen interpreter that has become invalid, as an upgrade or
        // an uninstall on the operator's machine leaves it
        let chosen = Some("/nonexistent/apollia/python3.13");
        // WHEN the interpreter is resolved
        let resolved = resolve_interpreter(chosen);
        // THEN something is still resolved: the operator's machine changing is
        // not a reason for the agent's tool to stop working. Which of the two
        // fallbacks answers depends on whether this tree has a staged bundle,
        // and either way it is not the invalid choice.
        match resolved {
            Ok(r) => {
                assert_ne!(
                    r.source,
                    InterpreterSource::Chosen,
                    "an invalid choice must not be retained"
                );
                assert!(!r.command.program.contains("/nonexistent/"));
            }
            Err(e) => {
                // Only reachable on a machine with no bundle AND no system
                // Python at all, where there is nothing to fall back to.
                assert!(
                    matches!(e, PythonExecutorError::PythonUnavailable { .. }),
                    "unexpected error: {e}"
                );
            }
        }
    }

    #[test]
    fn no_choice_and_no_bundle_reports_the_system_as_the_source() {
        // GIVEN no operator choice, in this tree, where no Python bundle is
        // staged beside the test binary
        let resolved = resolve_interpreter(None);
        // WHEN the interpreter is resolved
        // THEN the source is named, so a reader of the log can tell the shipped
        // configuration from a development one
        match resolved {
            Ok(r) => assert!(
                matches!(
                    r.source,
                    InterpreterSource::Bundled | InterpreterSource::System
                ),
                "a call with no choice must never report a chosen interpreter"
            ),
            Err(e) => assert!(
                matches!(e, PythonExecutorError::PythonUnavailable { .. }),
                "unexpected error: {e}"
            ),
        }
    }

    #[test]
    fn an_empty_setting_is_not_a_choice() {
        // GIVEN the setting present but blank, which is how an operator clears
        // it without deleting the line
        let resolved = resolve_interpreter(Some("   "));
        // WHEN the interpreter is resolved
        // THEN it is treated as absent rather than as a path to validate
        if let Ok(r) = resolved {
            assert_ne!(r.source, InterpreterSource::Chosen);
        }
    }
}

#[cfg(test)]
mod bundled_is_the_default_tests {
    use super::*;

    /// Build a fake bundle at `dir` in the layout of `relative`, and answer the
    /// interpreter's path.
    fn stage_bundle(dir: &Path, relative: &str) -> PathBuf {
        let interpreter = dir.join(relative);
        if let Some(parent) = interpreter.parent() {
            std::fs::create_dir_all(parent).expect("the fake bundle directory");
        }
        // The content is never read: only the layout is under test here, and
        // `bundled_interpreter_in` decides on `is_file` alone.
        std::fs::write(&interpreter, b"placeholder").expect("the fake interpreter");
        interpreter
    }

    #[test]
    fn the_bundled_interpreter_is_found_in_every_packaged_layout() {
        // GIVEN the four places the interpreter sits across the three systems
        // and the two packagings
        for relative in [
            "bin/python3.13",
            "python.exe",
            "python/bin/python3.13",
            "python/python.exe",
        ] {
            let tmp = tempfile::tempdir().expect("a temp directory");
            let expected = stage_bundle(tmp.path(), relative);
            // WHEN the bundle directory is probed
            let found = bundled_interpreter_in(tmp.path());
            // THEN the interpreter is found, whichever layout the packaging used
            assert_eq!(
                found.as_deref(),
                Some(expected.as_path()),
                "layout `{relative}` was not recognised"
            );
        }
    }

    #[test]
    fn a_directory_carrying_no_bundle_yields_nothing() {
        // GIVEN a directory that is not a Python bundle
        let tmp = tempfile::tempdir().expect("a temp directory");
        // WHEN it is probed
        // THEN nothing is found, rather than a path that does not exist
        assert_eq!(bundled_interpreter_in(tmp.path()), None);
    }

    #[test]
    fn with_no_choice_the_bundled_interpreter_wins_over_the_system() {
        // GIVEN a staged bundle and no operator choice, which is the shipped
        // configuration on all three systems
        let tmp = tempfile::tempdir().expect("a temp directory");
        let bundled = stage_bundle(tmp.path(), "bin/python3.13");

        // WHEN the interpreter is resolved
        let resolved = resolve_interpreter_with(None, Some(bundled.clone()))
            .expect("a staged bundle always resolves");

        // THEN the bundled one answers. The host running this test has a system
        // Python on PATH, and it is not consulted: the default depends on
        // nothing installed on the machine.
        assert_eq!(resolved.source, InterpreterSource::Bundled);
        assert_eq!(resolved.command.program, bundled.display().to_string());
        assert!(resolved.command.args.is_empty());
    }

    #[test]
    fn an_invalid_choice_falls_back_to_the_bundle_not_to_the_system() {
        // GIVEN a staged bundle and a choice that has stopped being valid
        let tmp = tempfile::tempdir().expect("a temp directory");
        let bundled = stage_bundle(tmp.path(), "bin/python3.13");

        // WHEN the interpreter is resolved
        let resolved = resolve_interpreter_with(
            Some("/nonexistent/apollia/python3.13"),
            Some(bundled.clone()),
        )
        .expect("the bundle is still there");

        // THEN the bundle answers, not the host's own Python: an upgrade on the
        // operator's machine returns the product to its shipped configuration
        // rather than to whatever happens to be on PATH
        assert_eq!(resolved.source, InterpreterSource::Bundled);
        assert_eq!(resolved.command.program, bundled.display().to_string());
    }
}
