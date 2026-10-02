use std::path::Path;

use serde_json::json;

use super::*;
use crate::agent_definition::OverrideValueError;

fn repo_with_agent() -> AgentRepository {
    let repo = AgentRepository::open(Path::new(":memory:")).unwrap();
    repo.save_definition(&StoredAgent {
        name: "invoices".into(),
        origin: AgentOrigin::Package,
        package: Some("office".into()),
        version: "1.0.0".into(),
        definition: json!({
            "id": "invoices",
            "name": "Invoices",
            "mission": "Extract the totals.",
            "defaults": {"access": ["file_read"]}
        }),
        enabled: true,
        installed_at: "2026-10-01T00:00:00Z".into(),
        updated_at: "2026-10-01T00:00:00Z".into(),
    })
    .unwrap();
    repo
}

// GIVEN a stored declarative agent
// WHEN it is read back
// THEN its origin, package, version and definition are those stored
#[test]
fn test_save_definition_round_trips() {
    let repo = repo_with_agent();

    let stored = repo.stored("invoices").unwrap().unwrap();

    assert_eq!(stored.origin, AgentOrigin::Package);
    assert_eq!(stored.package.as_deref(), Some("office"));
    assert_eq!(stored.version, "1.0.0");
    assert_eq!(stored.definition["mission"], "Extract the totals.");
    // The stored agents are this one and Apollia, which every file holds.
    assert_eq!(repo.list_stored().unwrap().len(), 2);
    // A declarative agent runs no code: it is not an installed Python agent.
    assert!(repo.get("invoices").unwrap().is_none());
    assert!(repo.list().unwrap().is_empty());
}

// GIVEN an agent with an overridden mission and access
// WHEN its definition is replaced by a newer version, as an update does
// THEN the definition is the new one and both overrides still apply
#[test]
fn test_replace_definition_keeps_overrides() {
    let repo = repo_with_agent();
    repo.set_override("invoices", OverrideField::Mission, &json!("Mine."))
        .unwrap();
    repo.set_override(
        "invoices",
        OverrideField::Access,
        &json!(["file_read", "web_fetch"]),
    )
    .unwrap();

    repo.replace_definition(
        "invoices",
        "2.0.0",
        &json!({
            "id": "invoices",
            "name": "Invoices v2",
            "mission": "Extract the totals and the VAT.",
            "defaults": {"access": ["file_read", "file_write"]}
        }),
    )
    .unwrap();

    let resolved = repo
        .resolved("invoices", None, AutonomyLevel::Assisted)
        .unwrap()
        .unwrap();
    assert_eq!(resolved.version, "2.0.0");
    assert_eq!(resolved.name, "Invoices v2");
    assert_eq!(resolved.mission, "Mine.");
    assert_eq!(resolved.access, vec!["file_read", "web_fetch"]);
    assert_eq!(repo.overrides("invoices").unwrap().len(), 2);
}

// GIVEN an agent with two overrides
// WHEN it goes back to defaults
// THEN the overrides are deleted, returned, and the agent is its definition
#[test]
fn test_reset_overrides_returns_to_defaults() {
    let repo = repo_with_agent();
    repo.set_override("invoices", OverrideField::Mission, &json!("Mine."))
        .unwrap();
    repo.set_override("invoices", OverrideField::Access, &json!([]))
        .unwrap();

    let removed = repo.reset_overrides("invoices").unwrap();

    assert_eq!(removed.len(), 2);
    assert!(repo.overrides("invoices").unwrap().is_empty());
    let resolved = repo
        .resolved("invoices", None, AutonomyLevel::Assisted)
        .unwrap()
        .unwrap();
    assert_eq!(resolved.mission, "Extract the totals.");
    assert_eq!(resolved.access, vec!["file_read"]);
    assert!(resolved.overridden.is_empty());
}

// GIVEN an overridden field
// WHEN it is set again, then removed
// THEN each call returns the value it replaced, and the field returns to
//   its default
#[test]
fn test_set_then_remove_override_returns_previous_values() {
    let repo = repo_with_agent();

    let first = repo
        .set_override("invoices", OverrideField::Mission, &json!("A"))
        .unwrap();
    let second = repo
        .set_override("invoices", OverrideField::Mission, &json!("B"))
        .unwrap();
    let removed = repo
        .remove_override("invoices", OverrideField::Mission)
        .unwrap();
    let again = repo
        .remove_override("invoices", OverrideField::Mission)
        .unwrap();

    assert_eq!(first, None);
    assert_eq!(second, Some(json!("A")));
    assert_eq!(removed, Some(json!("B")));
    assert_eq!(again, None);
    let resolved = repo
        .resolved("invoices", None, AutonomyLevel::Assisted)
        .unwrap()
        .unwrap();
    assert_eq!(resolved.mission, "Extract the totals.");
}

// GIVEN an unknown agent, and a value of the wrong shape
// WHEN an override is written
// THEN the first is not found and the second is refused, nothing stored
#[test]
fn test_set_override_refuses_unknown_agent_and_bad_value() {
    let repo = repo_with_agent();

    let missing = repo.set_override("ghost", OverrideField::Mission, &json!("x"));
    let bad = repo.set_override("invoices", OverrideField::Access, &json!("file_read"));

    assert!(matches!(missing, Err(AgentRepositoryError::NotFound(_))));
    assert!(matches!(
        bad,
        Err(AgentRepositoryError::InvalidOverride(
            OverrideValueError::ExpectedList("access")
        ))
    ));
    assert!(repo.overrides("invoices").unwrap().is_empty());
}

// GIVEN an agent with an override
// WHEN the agent is deleted, then stored again under the same name
// THEN the old override did not survive the deletion
#[test]
fn test_delete_removes_overrides() {
    let repo = repo_with_agent();
    repo.set_override("invoices", OverrideField::Mission, &json!("Mine."))
        .unwrap();

    repo.delete("invoices").unwrap();

    assert!(repo.stored("invoices").unwrap().is_none());
    assert!(repo.overrides("invoices").unwrap().is_empty());
}

// GIVEN a Python agent whose access the user overrode
// WHEN the agent is saved again with a newer version, as `agent update` does
// THEN the definition follows the new manifest and the override is kept
#[test]
fn test_saving_a_python_agent_again_keeps_overrides() {
    let repo = AgentRepository::open(Path::new(":memory:")).unwrap();
    let agent = |version: &str, tools: &[&str]| crate::InstalledAgent {
        name: "notes".into(),
        version: version.into(),
        install_path: "/a/notes/agent.py".into(),
        source_path: "/src/notes.py".into(),
        manifest: serde_json::from_value(json!({
            "name": "notes",
            "version": version,
            "description": "Takes notes.",
            "tools_required": tools
        }))
        .unwrap(),
        enabled: true,
        installed_at: "2026-10-01T00:00:00Z".into(),
        updated_at: "2026-10-01T00:00:00Z".into(),
    };
    repo.save(&agent("1.0.0", &["file_read"])).unwrap();
    repo.set_override("notes", OverrideField::Access, &json!(["web_fetch"]))
        .unwrap();

    repo.save(&agent("1.1.0", &["file_read", "file_write"]))
        .unwrap();

    let resolved = repo
        .resolved("notes", None, AutonomyLevel::Assisted)
        .unwrap()
        .unwrap();
    assert_eq!(resolved.version, "1.1.0");
    assert_eq!(
        resolved.definition["defaults"]["access"],
        json!(["file_read", "file_write"])
    );
    assert_eq!(resolved.access, vec!["web_fetch"]);
}

// GIVEN a Python agent the user disabled and whose mission they overrode
// WHEN a package adopts it as one of its declarative agents
// THEN it runs no code any more, belongs to the package, and keeps both the
// override and the disabled state; adopting it again changes nothing
#[test]
fn test_adopt_into_package_drops_the_code_and_keeps_the_user_state() {
    let repo = AgentRepository::open(Path::new(":memory:")).unwrap();
    repo.save(&crate::InstalledAgent {
        name: "guide".into(),
        version: "0.2.0".into(),
        install_path: "/a/guide/agent.py".into(),
        source_path: "/a/guide/agent.py".into(),
        manifest: serde_json::from_value(json!({
            "name": "guide",
            "version": "0.2.0",
            "description": "Helps.",
            "tools_required": []
        }))
        .unwrap(),
        enabled: true,
        installed_at: "2026-10-01T00:00:00Z".into(),
        updated_at: "2026-10-01T00:00:00Z".into(),
    })
    .unwrap();
    repo.set_override("guide", OverrideField::Mission, &json!("Mine."))
        .unwrap();
    repo.set_enabled("guide", false).unwrap();

    let adopted = repo.adopt_into_package("guide", "guide-pkg").unwrap();
    let again = repo.adopt_into_package("guide", "guide-pkg").unwrap();

    assert!(adopted);
    assert!(!again, "an agent without code is left alone");
    assert!(repo.get("guide").unwrap().is_none(), "no code any more");
    let stored = repo.stored("guide").unwrap().unwrap();
    assert_eq!(stored.origin, AgentOrigin::Package);
    assert_eq!(stored.package.as_deref(), Some("guide-pkg"));
    assert!(!stored.enabled);
    let resolved = repo
        .resolved("guide", None, AutonomyLevel::Assisted)
        .unwrap()
        .unwrap();
    assert_eq!(resolved.mission, "Mine.");
    assert!(!repo.adopt_into_package("unknown", "guide-pkg").unwrap());
}
