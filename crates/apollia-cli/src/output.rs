//! Shared output layer for every CLI leaf: errors, non-essential lines, and
//! the confirmation a destruction owes its operator.
//!
//! The published contract (`AGENTS.md` sections 2 and 6 of this crate,
//! `docs/site/docs/architecture/08-decisions.md#cli`) promises one shape for
//! machine-readable errors: a single `{"error": {"code": ..., "message": ...}}`
//! document on stdout, paired with the exit code the `code` field names. Every
//! error path of every leaf goes through [`emit_error`] so the shape exists in
//! exactly one place; `scripts/check_cli_json_contract.py` drives the binary to
//! hold it.
//!
//! The same file holds `--quiet`. The flag used to be handed leaf by leaf, so
//! two nouns honoured it and the rest printed their headers, separators and
//! hints whatever the operator asked. It is now recorded once by `main` and
//! read by exactly one place, the [`note!`](crate::note) macro, which every
//! non-essential line goes through.

use crate::exit_codes;
use std::io::{IsTerminal, Write};
use std::sync::atomic::{AtomicBool, Ordering};

/// The global `--quiet`, recorded once by `main` and read by [`note!`].
static QUIET: AtomicBool = AtomicBool::new(false);

/// Record the global `--quiet` flag. Called once, before any command runs.
pub fn set_quiet(value: bool) {
    QUIET.store(value, Ordering::Relaxed);
}

/// True when the operator asked for `--quiet`.
///
/// Read by [`note!`]; a leaf whose whole rendering changes shape under the flag
/// (`agent list`, `inspect`, `run`) reads it too, rather than carrying the flag
/// down its call chain.
pub fn is_quiet() -> bool {
    QUIET.load(Ordering::Relaxed)
}

/// One non-essential line on stdout: a header, a separator, a blank spacer, a
/// hint, a confirmation sentence.
///
/// Dropped under `--quiet`, which promises stdout carries the requested data
/// and nothing else. A line that IS the requested data stays a `println!`.
#[macro_export]
macro_rules! note {
    () => {
        if !$crate::output::is_quiet() {
            println!();
        }
    };
    ($($arg:tt)*) => {
        if !$crate::output::is_quiet() {
            println!($($arg)*);
        }
    };
}

/// The confirmation a destructive leaf owes its operator, in one place.
///
/// The rule is published in `crates/apollia-cli/AGENTS.md` section 2: a leaf
/// that deletes or overwrites acts on `--confirm`, asks when stdin and stderr
/// are a terminal, and refuses anywhere else rather than destroying silently
/// in a script.
///
/// `action` names the destruction in the infinitive, without a final period
/// (`"delete secret 'brave.api_key'"`): it is read both in the refusal message
/// and in the question.
///
/// Returns `None` when the leaf may act, `Some(exit_code)` when it must stop.
/// The question and the cancellation go to stderr, so stdout stays the data.
pub fn require_confirmation(confirm: bool, json: bool, action: &str) -> Option<i32> {
    if confirm {
        return None;
    }
    let interactive = !json && std::io::stdin().is_terminal() && std::io::stderr().is_terminal();
    if !interactive {
        return Some(emit_error(
            json,
            exit_codes::GENERAL_ERROR,
            &format!("use --confirm to {action}"),
        ));
    }
    eprint!("{action}? [y/N] ");
    let _ = std::io::stderr().flush();
    let mut answer = String::new();
    if std::io::stdin().read_line(&mut answer).is_err() {
        return Some(emit_error(
            json,
            exit_codes::GENERAL_ERROR,
            &format!("use --confirm to {action}"),
        ));
    }
    if answer.trim().eq_ignore_ascii_case("y") {
        None
    } else {
        eprintln!("cancelled");
        Some(exit_codes::SUCCESS)
    }
}

/// Stable machine name for an exit code, published as `error.code`.
pub fn code_label(exit_code: i32) -> &'static str {
    match exit_code {
        exit_codes::RUNTIME_ERROR => "runtime_error",
        exit_codes::TASK_FAILED => "task_failed",
        exit_codes::TIMEOUT => "timeout",
        exit_codes::INTERRUPTED => "interrupted",
        _ => "general_error",
    }
}

/// Print `message` in the mode the caller selected and return `exit_code`.
///
/// JSON mode: one `{"error": {"code", "message"}}` document on stdout, so a
/// script branches on `error.code` as the reference invites it to. Human mode:
/// `Error: <message>` on stderr, so results and diagnostics stay separable.
pub fn emit_error(json: bool, exit_code: i32, message: &str) -> i32 {
    if json {
        let envelope = serde_json::json!({
            "error": {"code": code_label(exit_code), "message": message}
        });
        println!("{envelope}");
    } else {
        eprintln!("Error: {message}");
    }
    exit_code
}

/// Map a runtime-client error onto the contract: an unreachable runtime is
/// exit 2 (`runtime_error`), anything else exit 1 (`general_error`).
pub fn emit_client_error(json: bool, err: &crate::client::ClientError) -> i32 {
    let exit_code = match err {
        crate::client::ClientError::ConnectionRefused => exit_codes::RUNTIME_ERROR,
        _ => exit_codes::GENERAL_ERROR,
    };
    emit_error(json, exit_code, &err.to_string())
}

#[cfg(test)]
mod tests {
    use super::{code_label, emit_error, is_quiet, require_confirmation, set_quiet};
    use crate::exit_codes;

    // GIVEN the five contractual exit codes
    // WHEN mapping each to its envelope code
    // THEN every label names its exit code and nothing else
    #[test]
    fn test_code_label_names_every_exit_code() {
        assert_eq!(code_label(exit_codes::GENERAL_ERROR), "general_error");
        assert_eq!(code_label(exit_codes::RUNTIME_ERROR), "runtime_error");
        assert_eq!(code_label(exit_codes::TASK_FAILED), "task_failed");
        assert_eq!(code_label(exit_codes::TIMEOUT), "timeout");
        assert_eq!(code_label(exit_codes::INTERRUPTED), "interrupted");
    }

    // GIVEN an error in JSON mode
    // WHEN emitting it
    // THEN the returned exit code is the one passed in
    #[test]
    fn test_emit_error_returns_the_exit_code() {
        assert_eq!(
            emit_error(true, exit_codes::RUNTIME_ERROR, "runtime not started"),
            exit_codes::RUNTIME_ERROR
        );
        assert_eq!(
            emit_error(false, exit_codes::GENERAL_ERROR, "nope"),
            exit_codes::GENERAL_ERROR
        );
    }

    // GIVEN the envelope the JSON mode builds
    // WHEN serializing it
    // THEN it is exactly {"error": {"code", "message"}}
    #[test]
    fn test_envelope_shape_is_the_published_contract() {
        let envelope = serde_json::json!({
            "error": {"code": code_label(exit_codes::GENERAL_ERROR), "message": "m"}
        });
        let doc: serde_json::Value = serde_json::from_str(&envelope.to_string()).unwrap();
        let obj = doc.as_object().unwrap();
        assert_eq!(obj.len(), 1);
        let err = obj["error"].as_object().unwrap();
        assert_eq!(err.len(), 2);
        assert_eq!(err["code"], "general_error");
        assert_eq!(err["message"], "m");
    }

    // GIVEN the global quiet flag, owned by this one test so no ordering
    // between tests can decide its value
    // WHEN it is set and restored
    // THEN the output layer reads back what main recorded
    #[test]
    fn test_quiet_flag_round_trips_through_the_output_layer() {
        let previous = is_quiet();
        set_quiet(true);
        assert!(is_quiet());
        set_quiet(false);
        assert!(!is_quiet());
        set_quiet(previous);
    }

    // GIVEN a destructive leaf whose operator passed --confirm
    // WHEN the confirmation rule is applied
    // THEN the leaf may act
    #[test]
    fn test_confirmed_destruction_proceeds() {
        assert_eq!(require_confirmation(true, false, "delete 'x'"), None);
        assert_eq!(require_confirmation(true, true, "delete 'x'"), None);
    }

    // GIVEN a destructive leaf without --confirm, in JSON mode (never a
    // question: a script is reading)
    // WHEN the confirmation rule is applied
    // THEN it refuses with the usage exit code rather than destroying
    #[test]
    fn test_unconfirmed_destruction_is_refused_in_machine_mode() {
        assert_eq!(
            require_confirmation(false, true, "delete 'x'"),
            Some(exit_codes::GENERAL_ERROR)
        );
    }
}
