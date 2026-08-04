//! Keep child processes from opening a console window on Windows.
//!
//! The desktop binary is built for the GUI subsystem
//! (`windows_subsystem = "windows"`), so it owns no console. On Windows that is
//! precisely what makes its children visible: when a process without a console
//! starts a console-subsystem executable, `CreateProcess` allocates a brand new
//! console for the child and shows it. Every long-lived child the runtime
//! spawns is therefore a terminal window sitting behind the application.
//!
//! Measured on a Windows MSVC build rather than assumed: two windows stayed
//! open behind the app, one for `apollia-runner-cpu.exe` and one for
//! `llama-server.exe`. Closing either sent `CTRL_CLOSE_EVENT` to the child, the
//! child died, and the supervision loop respawned it with a fresh window. The
//! respawn was correct; the console was the defect.
//!
//! `CREATE_NO_WINDOW` rather than `DETACHED_PROCESS`: the child still gets a
//! console, so the console APIs keep working and nothing changes for a program
//! that queries them, but it is never displayed. Every call site redirects
//! stdout and stderr into pipes or reads them through `output()`, so no output
//! is lost by hiding the window.
//!
//! This module mirrors [`crate::subprocess_env`]: two functions that mutate a
//! command builder, one per `Command` flavour, called immediately before the
//! spawn. A no-op everywhere but Windows, so call sites stay free of `cfg`.

/// Windows process creation flag: run the child without a console window.
///
/// Value from the Win32 `CreateProcess` documentation. Defined here rather than
/// pulled from `windows-sys` so no new dependency, and no new sovereignty
/// surface, is added for a single constant.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// The creation flags applied to a spawned child on this platform.
///
/// [`CREATE_NO_WINDOW`] on Windows, `0` everywhere else, where the notion does
/// not exist. Exposed so the value is assertable without spawning anything.
#[must_use]
pub fn console_creation_flags() -> u32 {
    #[cfg(windows)]
    {
        CREATE_NO_WINDOW
    }
    #[cfg(not(windows))]
    {
        0
    }
}

/// Hide the console window of a child spawned from a [`std::process::Command`].
///
/// Call this on every command reachable from the desktop binary, right before
/// the spawn. A no-op outside Windows.
pub fn hide_console(command: &mut std::process::Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(console_creation_flags());
    }
    #[cfg(not(windows))]
    {
        let _ = command;
    }
}

/// The [`tokio::process::Command`] counterpart of [`hide_console`].
///
/// No trait import here, unlike [`hide_console`]: `tokio::process::Command`
/// carries its own inherent `creation_flags` under `cfg(windows)`, and pulling
/// in `std::os::windows::process::CommandExt` as well would be an unused import
/// that fails the `-D warnings` gate on a Windows build.
pub fn hide_console_async(command: &mut tokio::process::Command) {
    #[cfg(windows)]
    {
        command.creation_flags(console_creation_flags());
    }
    #[cfg(not(windows))]
    {
        let _ = command;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_asks_for_no_console_and_other_platforms_ask_for_nothing() {
        // GIVEN the platform this test was compiled for
        // WHEN the creation flags are resolved
        let flags = console_creation_flags();

        // THEN Windows gets CREATE_NO_WINDOW and nothing else does
        #[cfg(windows)]
        assert_eq!(
            flags, 0x0800_0000,
            "the Win32 CREATE_NO_WINDOW value must be passed verbatim"
        );
        #[cfg(not(windows))]
        assert_eq!(
            flags, 0,
            "no platform but Windows has console creation flags"
        );
    }

    #[cfg(unix)]
    #[test]
    fn hiding_the_console_leaves_the_command_runnable() {
        // GIVEN a command that succeeds
        let mut command = std::process::Command::new("/bin/sh");
        command.args(["-c", "exit 0"]);

        // WHEN its console is hidden
        hide_console(&mut command);

        // THEN it still runs, so the helper is safe on the platforms where it
        // does nothing. The flag itself is not readable back from a built
        // `Command`, so this asserts innocuity rather than the flag's presence;
        // the presence is asserted on the pure function above.
        assert!(command.status().expect("spawn /bin/sh").success());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn hiding_the_console_leaves_an_async_command_runnable() {
        // GIVEN an async command that succeeds
        let mut command = tokio::process::Command::new("/bin/sh");
        command.args(["-c", "exit 0"]);

        // WHEN its console is hidden
        hide_console_async(&mut command);

        // THEN it still runs
        assert!(command.status().await.expect("spawn /bin/sh").success());
    }
}
