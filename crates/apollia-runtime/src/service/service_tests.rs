use std::path::Path;

use super::definition::Manager;
use super::{dedup_path, ServiceError, ServiceState, ServiceTarget};

/// A launchd target under a throwaway home. launchd is chosen because every
/// manager call it makes on a machine that is not this home's is read only:
/// `launchctl print` matches the definition path, which no real service holds.
fn launchd_target(home: &Path) -> ServiceTarget {
    ServiceTarget::under(Manager::Launchd, home.to_path_buf(), None)
}

#[cfg(unix)]
#[test]
fn test_dedup_path_keeps_the_first_occurrence_of_each_entry() {
    // GIVEN a PATH whose profile was sourced twice, with an empty entry
    let path = "/opt/bin:/usr/bin::/opt/bin:/bin:/usr/bin";
    // WHEN it is prepared for a service definition
    let kept = dedup_path(path);
    // THEN each directory appears once, in its first position
    assert_eq!(kept, "/opt/bin:/usr/bin:/bin");
}

#[test]
fn test_state_word_names_the_three_states() {
    // GIVEN the three combinations a status can observe
    let absent = ServiceState::default();
    let stopped = ServiceState {
        installed: true,
        loaded: true,
        running: false,
    };
    let running = ServiceState {
        installed: true,
        loaded: true,
        running: true,
    };
    // WHEN each is named
    // THEN absent, stopped and running read apart
    assert_eq!(absent.word(), "not_installed");
    assert_eq!(stopped.word(), "stopped");
    assert_eq!(running.word(), "running");
}

#[test]
fn test_install_without_load_writes_a_definition_that_runs_the_given_binary() {
    // GIVEN a throwaway home with no service, and the command line the
    // desktop application bundles
    let home = tempfile::tempdir().unwrap();
    let target = launchd_target(home.path());
    let bundled = home
        .path()
        .join("Apollia.app/Contents/Resources/apollia-os");
    assert!(!target.is_installed());

    // WHEN the service is installed without being handed to the manager
    let state = target.install(&bundled, false).unwrap();

    // THEN the LaunchAgent exists, runs that binary with `start`, and nothing
    // was started
    assert!(target.is_installed());
    let plist = std::fs::read_to_string(&target.definition).unwrap();
    assert!(plist.contains(&format!("<string>{}</string>", bundled.display())));
    assert!(plist.contains("<string>start</string>"));
    assert!(state.installed);
    assert!(!state.running);
    assert!(home.path().join(".apollia/logs").is_dir());
}

#[test]
fn test_uninstall_removes_the_definition_and_reports_nothing_stopped() {
    // GIVEN a definition written under a throwaway home and never loaded
    let home = tempfile::tempdir().unwrap();
    let target = launchd_target(home.path());
    target
        .install(Path::new("/opt/apollia/apollia-os"), false)
        .unwrap();

    // WHEN the service is removed
    let stopped = target.uninstall().unwrap();

    // THEN the definition is gone, no service was stopped, and the state reads
    // not installed
    assert!(!stopped);
    assert!(!target.is_installed());
    assert_eq!(target.observed_state(), ServiceState::default());
}

#[test]
fn test_uninstall_and_start_refuse_a_home_without_a_service() {
    // GIVEN a throwaway home where nothing was installed
    let home = tempfile::tempdir().unwrap();
    let target = launchd_target(home.path());

    // WHEN the service is removed, then started
    let removed = target.uninstall();
    let started = target.start();

    // THEN both name the missing definition
    assert!(matches!(removed, Err(ServiceError::NotInstalled(ref p)) if *p == target.definition));
    assert!(matches!(started, Err(ServiceError::NotInstalled(_))));
}

#[test]
fn test_a_binary_path_with_a_control_character_is_refused_before_any_write() {
    // GIVEN a binary path carrying a newline
    let home = tempfile::tempdir().unwrap();
    let target = launchd_target(home.path());

    // WHEN the service is installed for it
    let result = target.install(Path::new("/opt/apollia\n/apollia-os"), false);

    // THEN the install is refused and no definition is written
    assert!(matches!(result, Err(ServiceError::UnrepresentablePath(_))));
    assert!(!target.is_installed());
}
