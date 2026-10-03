use std::path::{Path, PathBuf};

use apollia_core::config::SettingSource;
use apollia_runtime::service::definition::Manager;
use apollia_runtime::service::{ServiceState, ServiceTarget};

use super::{apply, describe, plan_switch, SwitchPlan, SwitchRefusal};

fn bundled(root: &Path) -> PathBuf {
    root.join("Apollia.app/Contents/Resources/apollia-os")
}

/// A launchd target under a throwaway home: its manager calls are read only
/// for a definition no real service was loaded from.
fn target_under(home: &Path) -> ServiceTarget {
    ServiceTarget::under(Manager::Launchd, home.to_path_buf(), None)
}

#[test]
fn test_turning_on_installs_the_service_on_the_bundled_command_line() {
    // GIVEN a home without the service, installing allowed, and the command
    // line bundled with the application
    let home = tempfile::tempdir().unwrap();
    let target = target_under(home.path());
    let cli = bundled(home.path());

    // WHEN the switch is turned on and the plan carried out
    let plan = plan_switch(true, target.is_installed(), true, Some(cli.clone())).unwrap();
    apply(&target, &plan).unwrap();

    // THEN the plan installs that binary, and the definition the shared
    // service code wrote runs it with `start`
    assert_eq!(plan, SwitchPlan::Install(cli.clone()));
    assert!(target.is_installed());
    let definition = std::fs::read_to_string(&target.definition).unwrap();
    assert!(definition.contains(&format!("<string>{}</string>", cli.display())));
    assert!(definition.contains("<string>start</string>"));
}

#[test]
fn test_turning_off_removes_the_service_so_the_relaunch_embeds_the_runtime() {
    // GIVEN a home where the switch was turned on
    let home = tempfile::tempdir().unwrap();
    let target = target_under(home.path());
    let on = plan_switch(true, false, true, Some(bundled(home.path()))).unwrap();
    apply(&target, &on).unwrap();

    // WHEN the switch is turned off
    let plan = plan_switch(false, target.is_installed(), true, None).unwrap();
    apply(&target, &plan).unwrap();

    // THEN the service is gone: the relaunched application finds nothing
    // installed and claims the port itself
    assert_eq!(plan, SwitchPlan::Uninstall);
    assert!(!target.is_installed());
    assert_eq!(target.observed_state(), ServiceState::default());
}

#[test]
fn test_a_policy_that_forbids_the_service_refuses_on_but_never_off() {
    // GIVEN installing forbidden by the policy
    let cli = PathBuf::from("/Applications/Apollia.app/Contents/Resources/apollia-os");

    // WHEN the switch is turned on with nothing installed, and turned off with
    // a service installed
    let on = plan_switch(true, false, false, Some(cli));
    let off = plan_switch(false, true, false, None);

    // THEN on is refused, off removes the service
    assert_eq!(on, Err(SwitchRefusal::NotAllowed));
    assert_eq!(off, Ok(SwitchPlan::Uninstall));
}

#[test]
fn test_a_switch_already_in_place_does_nothing_and_no_cli_refuses_on() {
    // GIVEN the two positions the switch already holds, and an application
    // without its bundled command line
    // WHEN each is asked again, and on is asked without a command line
    let still_on = plan_switch(true, true, true, None);
    let still_off = plan_switch(false, false, true, None);
    let no_cli = plan_switch(true, false, true, None);

    // THEN nothing is done, and on is refused naming the missing binary
    assert_eq!(still_on, Ok(SwitchPlan::Nothing));
    assert_eq!(still_off, Ok(SwitchPlan::Nothing));
    assert_eq!(no_cli, Err(SwitchRefusal::NoBundledCli));
}

#[test]
fn test_the_switch_reads_the_observed_service_not_a_stored_value() {
    // GIVEN a running service under a policy that now forbids installing it,
    // and an OS with no service manager
    let running = ServiceState {
        installed: true,
        loaded: true,
        running: true,
    };

    // WHEN the switch is described for each
    let shown = describe(Some(running), true, false, SettingSource::Policy);
    let unsupported = describe(None, true, true, SettingSource::Default);

    // THEN the first shows on and running, locked by the policy; the second
    // says no service manager is there
    assert!(shown.supported && shown.installed && shown.running);
    assert!(!shown.allowed);
    assert_eq!(shown.allowed_source, SettingSource::Policy);
    assert!(!unsupported.supported);
    assert!(!unsupported.installed);
}
