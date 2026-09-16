use apollia_connectors::google::operations as google_operations;
use apollia_tools::descriptor::ApprovalRiskLevel;

use super::*;

#[test]
fn google_descriptors_cover_every_operation() {
    // GIVEN the Google operations the bridge exposes
    let ops_count = google_operations().len();
    // WHEN the tool descriptors are built from them
    let descriptors = google_tool_descriptors();
    // THEN there is exactly one descriptor per operation
    assert_eq!(
        descriptors.len(),
        ops_count,
        "one descriptor per Google operation"
    );
}

#[test]
fn every_descriptor_validates() {
    // GIVEN every Google tool descriptor
    // WHEN each one goes through its own validation
    // THEN none of them is rejected
    for d in google_tool_descriptors() {
        d.validate().unwrap_or_else(|e| {
            panic!("descriptor `{}` failed validation: {e}", d.name);
        });
    }
}

#[test]
fn read_ops_are_marked_read_only() {
    // GIVEN the four Google operations that only read
    let descs = google_tool_descriptors();
    for name in [
        "gcal.list_events",
        "gcal.get_event",
        "gdrive.workspace_list",
        "gdrive.workspace_read",
    ] {
        // WHEN each descriptor is looked up
        let d = descs.iter().find(|d| d.name == name).expect(name);
        // THEN it declares itself read-only, so no approval is asked for
        assert!(d.is_read_only, "{name} should be is_read_only=true");
    }
}

#[test]
fn write_ops_have_medium_or_higher_risk() {
    // GIVEN the three Google operations that write
    let descs = google_tool_descriptors();
    for name in ["gmail.send", "gcal.create_event", "gdrive.workspace_write"] {
        // WHEN each descriptor is looked up
        let d = descs.iter().find(|d| d.name == name).expect(name);
        // THEN its risk level is at least medium, which is what gates approval
        assert!(
            matches!(
                d.approval_risk_level,
                Some(
                    ApprovalRiskLevel::Medium
                        | ApprovalRiskLevel::High
                        | ApprovalRiskLevel::Critical
                ),
            ),
            "{name} should require approval"
        );
    }
}

#[test]
fn delete_event_requires_confirm_phrase() {
    // GIVEN the calendar deletion, whose approval policy is a confirm phrase
    let descs = google_tool_descriptors();
    // WHEN its descriptor is looked up
    let d = descs
        .iter()
        .find(|d| d.name == "gcal.delete_event")
        .expect("delete_event");
    // THEN it demands a reject reason and carries the critical risk level
    assert!(
        d.reject_reason_required,
        "ConfirmPhrase approval policy must set reject_reason_required"
    );
    assert_eq!(d.approval_risk_level, Some(ApprovalRiskLevel::Critical));
}

#[test]
fn google_executors_come_from_the_runtime_not_the_interface() {
    // GIVEN a process with no desktop: this test binary links apollia-runtime
    // and nothing of apollia-desktop, and no Tauri context exists
    // WHEN the Google executors are built
    let executors = build_google_executors();
    // THEN the set is non-empty and every executor answers to its own op id
    assert!(
        !executors.is_empty(),
        "the runtime must provide the Google executors on its own"
    );
    let names: Vec<&str> = executors.iter().map(|e| e.name()).collect();
    for op in ["gmail.send", "gcal.list_events", "gdrive.workspace_write"] {
        assert!(names.contains(&op), "`{op}` has no executor in the runtime");
    }
}

#[test]
fn every_google_descriptor_has_a_runtime_executor() {
    // GIVEN the descriptors registered at supervisor boot, which is what an
    // installed agent sees in its tool catalogue
    let descriptors = google_tool_descriptors();
    // WHEN they are crossed with the executors the runtime builds
    let executors = build_google_executors();
    let names: Vec<&str> = executors.iter().map(|e| e.name()).collect();
    // THEN no descriptor is advertised without something able to run it: that
    // gap is what answered `UnknownTool` outside the desktop
    let orphans: Vec<&str> = descriptors
        .iter()
        .map(|d| d.name.as_str())
        .filter(|name| !names.contains(name))
        .collect();
    assert!(
        orphans.is_empty(),
        "descriptors advertised with no executor: {orphans:?}"
    );
}

#[test]
fn a_google_call_reaches_an_executor_with_no_desktop() {
    // GIVEN a dispatcher holding only what the runtime supplies, which is the
    // shape `apollia-os start` builds on a machine where no interface runs
    let dispatcher = apollia_tools::executor::ToolDispatcher::new(build_google_executors());
    // WHEN the tool surface is enumerated
    let names = dispatcher.tool_names();
    // THEN a Google op is a name the dispatcher resolves, so a call to it is
    // executed rather than refused as unknown
    assert!(
        names.contains(&"gmail.send"),
        "gmail.send must be dispatchable without the desktop"
    );
    assert!(
        names.contains(&"gsheets.read_values"),
        "the runtime set is the full one, not the twelve ops the desktop carried"
    );
}
