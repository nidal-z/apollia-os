use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use super::{load_start_config_from, StartError};
use crate::config_location::legacy_files;

/// A tracing writer that keeps what it is given, so a test can read the
/// events a call emitted.
#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl Write for Captured {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Captured {
    type Writer = Captured;
    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

fn write(path: &Path, body: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
}

type Loaded = Result<(Option<crate::config::ApolliaCConfig>, Option<PathBuf>), StartError>;

/// Plays the daemon's config loading for a home and a working directory,
/// returning its result and the log lines it emitted.
fn start_in(home: &Path, cwd: &Path, explicit: Option<&Path>) -> (Loaded, String) {
    let canonical = apollia_core::paths::config_file_under(home);
    let present = Some(canonical.clone()).filter(|p| p.is_file());
    let legacy = legacy_files(home, Some(cwd), None);
    let sink = Captured::default();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(sink.clone())
        .with_ansi(false)
        .finish();
    let result = tracing::subscriber::with_default(subscriber, || {
        load_start_config_from(explicit, present, &canonical, &legacy)
    });
    let logs = String::from_utf8(sink.0.lock().unwrap().clone()).unwrap();
    (result, logs)
}

#[test]
fn a_file_at_a_former_location_is_ignored_with_a_warning() {
    // GIVEN a configuration in the working directory and in ~/.config/apollia,
    // and none at the canonical location
    let home = tempfile::tempdir().unwrap();
    let cwd = tempfile::tempdir().unwrap();
    write(&cwd.path().join("apollia.toml"), "[api]\nport = 7001\n");
    write(
        &home.path().join(".config/apollia/apollia.toml"),
        "[api]\nport = 7002\n",
    );
    // WHEN the daemon loads its configuration
    let (result, logs) = start_in(home.path(), cwd.path(), None);
    // THEN it starts on the defaults, and each former file is named by a warning
    let (cfg, path) = result.unwrap();
    assert!(cfg.is_none() && path.is_none());
    assert_eq!(
        logs.matches("config.legacy_location.ignored").count(),
        2,
        "{logs}"
    );
    assert!(logs.contains("WARN"), "{logs}");
}

#[test]
fn a_package_manifest_in_the_working_directory_is_not_read() {
    // GIVEN a working directory holding an agent package manifest named
    // apollia.toml, and no file at the canonical location
    let home = tempfile::tempdir().unwrap();
    let cwd = tempfile::tempdir().unwrap();
    write(
        &cwd.path().join("apollia.toml"),
        "[package]\nid = \"demo\"\nversion = \"0.1.0\"\n\n[[tools]]\nid = \"ping\"\nmodule = \"tools/ping.py\"\n",
    );
    // WHEN `apollia-os start` loads its configuration from there
    let (result, logs) = start_in(home.path(), cwd.path(), None);
    // THEN the manifest is neither read nor reported as a misplaced config
    let (cfg, path) = result.unwrap();
    assert!(cfg.is_none() && path.is_none());
    assert!(!logs.contains("config.legacy_location.ignored"), "{logs}");
}

#[test]
fn the_canonical_file_is_the_one_loaded() {
    // GIVEN a configuration at ~/.apollia/apollia.toml and another in the
    // working directory
    let home = tempfile::tempdir().unwrap();
    let cwd = tempfile::tempdir().unwrap();
    let canonical = apollia_core::paths::config_file_under(home.path());
    write(&canonical, "[api]\nport = 7003\n");
    write(&cwd.path().join("apollia.toml"), "[api]\nport = 7004\n");
    // WHEN the daemon loads its configuration
    let (result, _) = start_in(home.path(), cwd.path(), None);
    // THEN the canonical file answers
    let (cfg, path) = result.unwrap();
    assert_eq!(path.as_deref(), Some(canonical.as_path()));
    assert_eq!(cfg.unwrap().api.unwrap().port, 7003);
}

#[test]
fn an_explicit_config_replaces_the_canonical_location() {
    // GIVEN a canonical file and another file named with --config
    let home = tempfile::tempdir().unwrap();
    let cwd = tempfile::tempdir().unwrap();
    write(
        &apollia_core::paths::config_file_under(home.path()),
        "[api]\nport = 7005\n",
    );
    let explicit = cwd.path().join("custom.toml");
    write(&explicit, "[api]\nport = 7006\n");
    // WHEN the daemon loads its configuration with --config
    let (result, _) = start_in(home.path(), cwd.path(), Some(&explicit));
    // THEN the explicit file is the one in force
    let (cfg, path) = result.unwrap();
    assert_eq!(path.as_deref(), Some(explicit.as_path()));
    assert_eq!(cfg.unwrap().api.unwrap().port, 7006);
}

#[test]
fn an_explicit_config_that_does_not_exist_stops_the_start() {
    // GIVEN a --config path with no file behind it
    let home = tempfile::tempdir().unwrap();
    let cwd = tempfile::tempdir().unwrap();
    let missing = cwd.path().join("missing.toml");
    // WHEN the daemon loads its configuration
    let (result, _) = start_in(home.path(), cwd.path(), Some(&missing));
    // THEN it fails at startup rather than running on the defaults
    assert!(matches!(result, Err(StartError::Config { .. })));
}
