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

type Loaded = Result<(Option<crate::config::ApolliaConfig>, Option<PathBuf>), StartError>;

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
    // THEN it starts on the default file written at the canonical location, and
    // each former file is named by a warning
    let (cfg, path) = result.unwrap();
    let canonical = apollia_core::paths::config_file_under(home.path());
    assert_eq!(path.as_deref(), Some(canonical.as_path()));
    assert_eq!(cfg.unwrap().api.port, 7771);
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
    assert!(cfg.unwrap().tools.disabled.is_empty());
    assert_eq!(
        path.as_deref(),
        Some(apollia_core::paths::config_file_under(home.path()).as_path())
    );
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
    assert_eq!(cfg.unwrap().api.port, 7003);
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
    assert_eq!(cfg.unwrap().api.port, 7006);
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

#[test]
fn an_invalid_file_stops_the_start_with_its_faults_at_their_line() {
    // GIVEN a canonical configuration with an unknown key on line 3 and a
    // value out of bounds on line 6
    let home = tempfile::tempdir().unwrap();
    let cwd = tempfile::tempdir().unwrap();
    let canonical = apollia_core::paths::config_file_under(home.path());
    write(&canonical, "[api]\nbind = \"127.0.0.1\"\ntypo = 1\n");
    // WHEN the daemon loads its configuration
    let (result, _) = start_in(home.path(), cwd.path(), None);
    // THEN the start is refused, the message naming the file, line and key
    let Err(StartError::Config { path, reason }) = result else {
        panic!("an invalid file must stop the start");
    };
    assert_eq!(path, canonical);
    let expected = format!("{}:3:1: api.typo", canonical.display());
    assert!(reason.contains(&expected), "{reason}");
}

#[test]
fn a_value_out_of_its_section_bounds_stops_the_start() {
    // GIVEN a [hitl] section whose scan interval is below its bound
    let home = tempfile::tempdir().unwrap();
    let cwd = tempfile::tempdir().unwrap();
    let canonical = apollia_core::paths::config_file_under(home.path());
    write(&canonical, "[hitl]\nscan_interval_secs = 1\n");
    // WHEN the daemon loads its configuration
    let (result, _) = start_in(home.path(), cwd.path(), None);
    // THEN HitlConfig::validate refuses it, at line 2
    let Err(StartError::Config { reason, .. }) = result else {
        panic!("a value out of bounds must stop the start");
    };
    assert!(reason.contains(":2:1: hitl.scan_interval_secs"), "{reason}");
}

#[test]
fn a_first_start_writes_the_default_file_and_loads_it() {
    // GIVEN a home with no data folder and no configuration
    let home = tempfile::tempdir().unwrap();
    let cwd = tempfile::tempdir().unwrap();
    let canonical = apollia_core::paths::config_file_under(home.path());
    // WHEN the daemon loads its configuration
    let (result, _) = start_in(home.path(), cwd.path(), None);
    // THEN the documented default file exists at ~/.apollia/apollia.toml and is
    // the one loaded, on the defaults
    let (cfg, path) = result.unwrap();
    assert_eq!(path.as_deref(), Some(canonical.as_path()));
    let text = std::fs::read_to_string(&canonical).unwrap();
    assert!(text.starts_with("# apollia.toml, the configuration of Apollia OS."));
    assert_eq!(cfg.unwrap().api.port, 7771);
}

#[test]
fn a_start_over_an_existing_file_leaves_its_bytes() {
    // GIVEN a configuration the user wrote
    let home = tempfile::tempdir().unwrap();
    let cwd = tempfile::tempdir().unwrap();
    let canonical = apollia_core::paths::config_file_under(home.path());
    write(&canonical, "[api]\nport = 7005\n# mine\n");
    // WHEN the daemon loads its configuration
    let (result, _) = start_in(home.path(), cwd.path(), None);
    // THEN the file is read as it is and never rewritten
    assert_eq!(result.unwrap().0.unwrap().api.port, 7005);
    assert_eq!(
        std::fs::read_to_string(&canonical).unwrap(),
        "[api]\nport = 7005\n# mine\n"
    );
}
