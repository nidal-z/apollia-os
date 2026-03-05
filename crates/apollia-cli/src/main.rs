//! Apollia OS — CLI binary entry point.
//!
//! Command structure follows noun-verb pattern (ADR-008):
//!   apollia-os <noun> <verb> [options]
//!
//! Level-1 commands (STORY-037): start, stop, status, run
//! Level-2 commands (STORY-038): agent, task, tools, memory, audit
//!
//! Global flags: --json (machine-readable output), TTY auto-detected.

fn main() {
    // CLI commands are implemented in STORY-037 and STORY-038.
    // This entry point will be replaced with the full clap command tree.
    eprintln!("apollia-os: not yet implemented — Sprint 5");
    std::process::exit(1);
}
