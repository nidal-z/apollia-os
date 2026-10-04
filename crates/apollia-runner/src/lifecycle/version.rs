//! `apollia-runner --version`: the version of the dictation engine compiled in.
//!
//! The daemon lists the engines of the machine with the version each states,
//! and the runner states the whisper.cpp it links, as `version: <x.y.z>` on
//! stdout, the banner shape llama-server uses. A build without a dictation
//! backend states nothing.

use std::io::{self, Write};

/// Whether the arguments ask for the version rather than for a server.
pub fn requested<I: IntoIterator<Item = String>>(args: I) -> bool {
    args.into_iter().skip(1).any(|a| a == "--version")
}

/// The whisper.cpp version linked into this build, if any.
#[must_use]
pub fn engine_version() -> Option<&'static str> {
    #[cfg(feature = "local-cpu")]
    {
        Some(whisper_rs::get_whisper_version())
    }
    #[cfg(not(feature = "local-cpu"))]
    {
        None
    }
}

/// Write the version banner to stdout.
pub fn announce() -> io::Result<()> {
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    announce_to(&mut handle, engine_version())
}

/// Testable variant: writes `version: <version>` to an arbitrary writer, or
/// nothing when the build carries no engine.
pub fn announce_to<W: Write>(writer: &mut W, version: Option<&str>) -> io::Result<()> {
    if let Some(version) = version {
        writeln!(writer, "version: {version}")?;
    }
    writer.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_requested_reads_the_flag_after_the_program_name() {
        // GIVEN argument lists with and without the flag
        let args = |a: &[&str]| a.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();

        // WHEN they are read
        // THEN only a flag after the program name asks for the version
        assert!(requested(args(&["apollia-runner", "--version"])));
        assert!(!requested(args(&["apollia-runner"])));
        assert!(!requested(args(&["--version"])));
    }

    #[test]
    fn test_announce_writes_the_banner_the_daemon_parses() {
        // GIVEN a writable buffer standing in for stdout
        let mut with = Vec::new();
        let mut without = Vec::new();

        // WHEN the version is announced for a build with and without an engine
        announce_to(&mut with, Some("1.7.6")).unwrap();
        announce_to(&mut without, None).unwrap();

        // THEN the first writes the banner line, the second nothing
        assert_eq!(String::from_utf8(with).unwrap(), "version: 1.7.6\n");
        assert!(without.is_empty());
    }
}
