//! `apollia-os completions <shell>`: generate a shell completion script.
//!
//! Derives the script from the clap command tree, so it always tracks the real
//! command surface. Emits to stdout for redirection into the shell's completion
//! directory.

use std::io::Write;

use clap::CommandFactory;
use clap_complete::Shell;

use crate::exit_codes;

/// Generate a completion script for `shell` on stdout.
///
/// Generates into a buffer first, then writes it, so a reader that closes the
/// pipe early (e.g. `apollia-os completions bash | head`) yields a clean exit
/// instead of a broken-pipe panic from the generator.
pub fn run(shell: Shell) -> i32 {
    let mut cmd = crate::Cli::command();
    let mut buf: Vec<u8> = Vec::new();
    clap_complete::generate(shell, &mut cmd, "apollia-os", &mut buf);
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

    // GIVEN every supported shell WHEN a script is generated THEN it is not
    // empty, and nothing panics.
    #[test]
    fn test_generation_succeeds_for_each_shell() {
        for shell in [Shell::Bash, Shell::Zsh, Shell::Fish, Shell::PowerShell] {
            let mut cmd = crate::Cli::command();
            let mut buf: Vec<u8> = Vec::new();
            clap_complete::generate(shell, &mut cmd, "apollia-os", &mut buf);
            assert!(!buf.is_empty(), "empty completion script for {shell:?}");
        }
    }
}
