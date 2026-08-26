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
