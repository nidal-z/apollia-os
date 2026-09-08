//! `apollia-os completions <shell>`: generate a shell completion script.
//!
//! Derives the script from the clap command tree, so it always tracks the real
//! command surface. Emits to stdout for redirection into the shell's completion
//! directory.

use std::io::Write;

use clap::CommandFactory;
use clap_complete::Shell;

use crate::exit_codes;

/// Stack the generator runs on.
///
/// `clap_complete` walks the command tree recursively, and this tree is deep:
/// 199 leaf commands under a dozen nouns. The main thread of a Windows process
/// gets the stack its executable header declares, one mebibyte by default,
/// where Linux and macOS give eight, and the walk overflows it. Measured on
/// 2026-09-08 on Windows: every shell aborted with "thread 'main' has
/// overflowed its stack" and exit 127, on a binary whose other 197 commands
/// answered. Sizing the stack here rather than inheriting it means one script,
/// generated the same way on every system. The reservation is virtual address
/// space, committed only as it is used.
const GENERATOR_STACK_BYTES: usize = 16 * 1024 * 1024;

/// Render the completion script for `shell`, on a thread whose stack is ours.
fn render(shell: Shell) -> std::io::Result<Vec<u8>> {
    std::thread::Builder::new()
        .stack_size(GENERATOR_STACK_BYTES)
        .spawn(move || {
            let mut cmd = crate::Cli::command();
            let mut buf: Vec<u8> = Vec::new();
            clap_complete::generate(shell, &mut cmd, "apollia-os", &mut buf);
            buf
        })?
        .join()
        .map_err(|_| std::io::Error::other("the completion generator panicked"))
}

/// Generate a completion script for `shell` on stdout.
///
/// Generates into a buffer first, then writes it, so a reader that closes the
/// pipe early (e.g. `apollia-os completions bash | head`) yields a clean exit
/// instead of a broken-pipe panic from the generator.
pub fn run(shell: Shell) -> i32 {
    let buf = match render(shell) {
        Ok(buf) => buf,
        Err(e) => {
            eprintln!("Error: could not generate the completion script: {e}");
            return exit_codes::GENERAL_ERROR;
        }
    };
    let mut out = std::io::stdout();
    match out.write_all(&buf).and_then(|()| out.flush()) {
        Ok(()) => exit_codes::SUCCESS,
        // Pipe closed by the reader (head, less q, ...): exit cleanly.
        Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => exit_codes::SUCCESS,
        Err(_) => exit_codes::GENERAL_ERROR,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_shell_renders_a_script_through_the_sized_stack() {
        // GIVEN the four shells the command offers
        let shells = [Shell::Bash, Shell::Zsh, Shell::Fish, Shell::PowerShell];

        for shell in shells {
            // WHEN the script is rendered the way the command renders it
            let rendered = render(shell);

            // THEN it comes back, and it is not empty
            let buf = rendered.expect("the generator thread completes");
            assert!(!buf.is_empty(), "empty completion script for {shell:?}");
        }
    }
}
