use apollia_connectors::google::operations as google_operations;
use apollia_connectors::operation::ApprovalPolicy;
use apollia_tools::descriptor::{ActionFamily, ApprovalRiskLevel, FamilySource};

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

fn connector_descriptor(id: &str) -> apollia_tools::ToolDescriptor {
    all_connector_descriptors()
        .into_iter()
        .find(|d| d.name == id)
        .unwrap_or_else(|| panic!("no connector descriptor named {id}"))
}

fn connector_operation(id: &str) -> apollia_connectors::operation::OperationSpec {
    google_operations()
        .iter()
        .chain(apollia_connectors::microsoft::operations().iter())
        .find(|op| op.id == id)
        .cloned()
        .unwrap_or_else(|| panic!("no connector operation named {id}"))
}

#[test]
fn an_auto_approved_operation_reads_personal_data() {
    // GIVEN an operation the connector auto-approves
    let op = connector_operation("gcal.list_events");
    assert_eq!(op.approval, ApprovalPolicy::AutoApprove);
    // WHEN it is translated into a tool descriptor
    let d = connector_descriptor(op.id);
    // THEN it reads personal data, a classification fixed in Apollia's code
    assert_eq!(d.family, ActionFamily::ReadPersonal);
    assert_eq!(d.family_source, FamilySource::Builtin);
}

#[test]
fn a_send_operation_sends_outside_whatever_its_approval() {
    // GIVEN the two operations whose verb is `send`
    for id in ["gmail.send", "outlook.send"] {
        let op = connector_operation(id);
        assert_eq!(op.action, "send");
        // WHEN each is translated into a tool descriptor
        let d = connector_descriptor(id);
        // THEN it sends outside, even though its approval policy is a write's
        assert_eq!(d.family, ActionFamily::SendExternal, "{id}");
        assert_eq!(d.family_source, FamilySource::Builtin, "{id}");
    }
}

#[test]
fn an_operation_requiring_approval_writes_or_deletes() {
    // GIVEN an operation that always requires approval and does not send
    let op = connector_operation("gcal.create_event");
    assert_eq!(op.approval, ApprovalPolicy::AlwaysRequireApproval);
    assert_ne!(op.action, "send");
    // WHEN it is translated into a tool descriptor
    let d = connector_descriptor(op.id);
    // THEN it writes or deletes
    assert_eq!(d.family, ActionFamily::WriteOrDelete);
    assert_eq!(d.family_source, FamilySource::Builtin);
}

#[test]
fn an_operation_requiring_a_confirm_phrase_writes_or_deletes() {
    // GIVEN an operation guarded by a confirmation phrase
    let op = connector_operation("gcal.delete_event");
    assert_eq!(op.approval, ApprovalPolicy::ConfirmPhrase);
    // WHEN it is translated into a tool descriptor
    let d = connector_descriptor(op.id);
    // THEN it writes or deletes
    assert_eq!(d.family, ActionFamily::WriteOrDelete);
    assert_eq!(d.family_source, FamilySource::Builtin);
}

#[test]
fn every_connector_descriptor_is_classified() {
    // GIVEN every connector descriptor registered at boot
    // WHEN their families are read
    // THEN none is left unclassified nor exempt
    for d in all_connector_descriptors() {
        assert!(
            !matches!(d.family, ActionFamily::Unclassified | ActionFamily::Exempt),
            "{} has family {:?}",
            d.name,
            d.family
        );
    }
}
