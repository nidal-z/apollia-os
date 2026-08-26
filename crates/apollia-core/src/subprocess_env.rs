//! Keep the bundled Python's environment out of subprocesses that do not want it.
//!
//! The desktop binary embeds a Python interpreter and exports `PYTHONHOME` and
//! `PYTHONPATH` on the whole process, because PyO3 reads them lazily at first
//! use. `std::env::set_var` has no scope: every `Command` spawned afterwards
//! inherits them, including the ones that run a *different* Python, or a shell
//! that will.
//!
//! What that costs, measured rather than assumed:
//!
//! - When the outside interpreter's minor version differs from the bundled one,
//!   it dies on startup. `/usr/bin/python3` (3.9) loading a 3.13 standard
//!   library raises `ImportError: cannot import name 'text_encoding' from 'io'`
//!   before it runs a line of user code. Two seeded MCP servers failed exactly
//!   this way, and so would anyone's Python MCP server.
//! - When the versions happen to match, it is worse: the interpreter starts and
//!   silently uses the bundled standard library and site-packages instead of its
//!   own. Nothing fails, and the process runs against an environment nobody
//!   chose.
//!
//! `PATH` is prepended with the bundle only on Windows, so on macOS and Linux
//! `python3` never resolves to the bundled interpreter anyway. A subprocess that
//! genuinely wants it must name it by absolute path, and then it should also set
//! the variables itself rather than rely on inheritance.
//!
//! `DYLD_FALLBACK_LIBRARY_PATH` and `LD_LIBRARY_PATH` are deliberately left
//! alone: they are fallbacks, consulted only when the loader has not already
//! found the library, and an interpreter that resolves its own `libpython`
//! normally is unaffected. Verified on both a 3.9 and a 3.13 interpreter, each
//! kept its own `sys.prefix`.

/// The variables that make an inherited environment hijack an interpreter.
const BUNDLED_PYTHON_VARS: [&str; 2] = ["PYTHONHOME", "PYTHONPATH"];

/// Strip the bundled Python's variables from a `std::process::Command`.
///
/// Call this on every command that runs a shell, an interpreter, or anything an
/// operator supplied. It is a no-op when the variables are unset, so it is safe
/// on the CLI and in tests, where nothing exports them.
pub fn scrub_bundled_python(command: &mut std::process::Command) {
    for var in BUNDLED_PYTHON_VARS {
        command.env_remove(var);
    }
}

/// The `tokio::process::Command` counterpart of [`scrub_bundled_python`].
pub fn scrub_bundled_python_async(command: &mut tokio::process::Command) {
    for var in BUNDLED_PYTHON_VARS {
        command.env_remove(var);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrubbing_removes_the_variables_from_the_child() {
        // GIVEN a command that would otherwise pass the bundled Python on
        //
        // The variables are set on the command rather than on the process:
        // `std::env::set_var` is unsafe, denied in this crate, and would leak
        // into every other test in the binary. Setting them on the builder
        // exercises the same removal path, since `env_remove` overrides an
        // earlier `env` for the same key.
        let mut command = std::process::Command::new("/bin/sh");
        command
            .args(["-c", "echo home=[$PYTHONHOME] path=[$PYTHONPATH]"])
            .env("PYTHONHOME", "/nonexistent/bundle")
            .env("PYTHONPATH", "/nonexistent/bundle/lib");

        // WHEN it is scrubbed
        scrub_bundled_python(&mut command);
        let out = command.output().expect("spawn /bin/sh");

        // THEN the child sees neither variable
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            stdout.contains("home=[] path=[]"),
            "child still inherited the bundled Python: {stdout}"
        );
    }

    #[test]
    fn an_unscrubbed_command_does_pass_them_on() {
        // GIVEN the same command without the scrub, so the test above cannot
        // pass for the wrong reason (an empty echo, a shell that ignores them)
        // WHEN the child is spawned without the scrub and echoes the variable
        let out = std::process::Command::new("/bin/sh")
            .args(["-c", "echo home=[$PYTHONHOME]"])
            .env("PYTHONHOME", "/nonexistent/bundle")
            .output()
            .expect("spawn /bin/sh");

        // THEN the variable reaches the child
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            stdout.contains("home=[/nonexistent/bundle]"),
            "the control case did not inherit either: {stdout}"
        );
    }

    #[test]
    fn scrubbing_is_a_no_op_when_nothing_is_set() {
        // GIVEN no bundled Python on the command
        // WHEN it is scrubbed
        let mut command = std::process::Command::new("/bin/sh");
        command.args(["-c", "exit 0"]);
        scrub_bundled_python(&mut command);

        // THEN it still runs
        assert!(command.status().expect("spawn /bin/sh").success());
    }
}
