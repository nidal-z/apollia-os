use apollia_connectors::google::operations as google_operations;
use apollia_tools::descriptor::ApprovalRiskLevel;

use super::*;

#[test]
fn google_descriptors_cover_every_operation() {
    let ops_count = google_operations().len();
    let descriptors = google_tool_descriptors();
    assert_eq!(
        descriptors.len(),
        ops_count,
        "one descriptor per Google operation"
    );
}

#[test]
fn every_descriptor_validates() {
    for d in google_tool_descriptors() {
        d.validate().unwrap_or_else(|e| {
            panic!("descriptor `{}` failed validation: {e}", d.name);
        });
    }
}

#[test]
fn read_ops_are_marked_read_only() {
    let descs = google_tool_descriptors();
    for name in [
        "gcal.list_events",
        "gcal.get_event",
        "gdrive.workspace_list",
        "gdrive.workspace_read",
    ] {
        let d = descs.iter().find(|d| d.name == name).expect(name);
        assert!(d.is_read_only, "{name} should be is_read_only=true");
    }
}

#[test]
fn write_ops_have_medium_or_higher_risk() {
    let descs = google_tool_descriptors();
    for name in ["gmail.send", "gcal.create_event", "gdrive.workspace_write"] {
        let d = descs.iter().find(|d| d.name == name).expect(name);
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
    let descs = google_tool_descriptors();
    let d = descs
        .iter()
        .find(|d| d.name == "gcal.delete_event")
        .expect("delete_event");
    assert!(
        d.reject_reason_required,
        "ConfirmPhrase approval policy must set reject_reason_required"
    );
    assert_eq!(d.approval_risk_level, Some(ApprovalRiskLevel::Critical));
}
