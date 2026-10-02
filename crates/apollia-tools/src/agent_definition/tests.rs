use super::*;
use apollia_core::model_preset::ModelSource;

fn stored() -> StoredAgent {
    StoredAgent {
        name: "invoices".into(),
        origin: AgentOrigin::Package,
        package: Some("office".into()),
        version: "1.0.0".into(),
        definition: json!({
            "id": "invoices",
            "name": "Invoices",
            "mission": "Extract the totals of the invoices.",
            "examples": ["Total of March"],
            "defaults": {
                "access": ["file_read", "pkg:office/extract_totals"],
                "sources": ["documents"]
            }
        }),
        enabled: true,
        installed_at: "2026-10-01T00:00:00Z".into(),
        updated_at: "2026-10-01T00:00:00Z".into(),
    }
}

fn over(field: OverrideField, value: Value) -> AgentOverride {
    AgentOverride { field, value }
}

// GIVEN an agent without any override
// WHEN it is resolved
// THEN every field is the definition's, and nothing is marked overridden
#[test]
fn test_resolve_without_override_is_the_definition() {
    let agent = stored();

    let resolved = resolve(&agent, &[], None);

    assert_eq!(resolved.id, "invoices");
    assert_eq!(resolved.name, "Invoices");
    assert_eq!(resolved.mission, "Extract the totals of the invoices.");
    assert_eq!(resolved.examples, vec!["Total of March"]);
    assert_eq!(
        resolved.access,
        vec!["file_read", "pkg:office/extract_totals"]
    );
    assert_eq!(resolved.sources, vec!["documents"]);
    assert!(resolved.overridden.is_empty());
    assert_eq!(resolved.model, None);
    assert_eq!(resolved.definition, agent.definition);
}

// GIVEN an agent whose mission is overridden
// WHEN it is resolved
// THEN the mission is the user's, the other fields the definition's
#[test]
fn test_resolve_overridden_mission() {
    let agent = stored();
    let overrides = [over(
        OverrideField::Mission,
        json!("Extract totals and flag duplicates."),
    )];

    let resolved = resolve(&agent, &overrides, None);

    assert_eq!(resolved.mission, "Extract totals and flag duplicates.");
    assert_eq!(resolved.name, "Invoices");
    assert_eq!(
        resolved.access,
        vec!["file_read", "pkg:office/extract_totals"]
    );
    assert_eq!(resolved.overridden, vec![OverrideField::Mission]);
}

// GIVEN an access added by the user (the default list plus one tool)
// WHEN the agent is resolved
// THEN the added tool is granted, with the defaults
#[test]
fn test_resolve_added_access() {
    let agent = stored();
    let overrides = [over(
        OverrideField::Access,
        json!(["file_read", "pkg:office/extract_totals", "web_fetch"]),
    )];

    let resolved = resolve(&agent, &overrides, None);

    assert_eq!(
        resolved.access,
        vec!["file_read", "pkg:office/extract_totals", "web_fetch"]
    );
    assert_eq!(resolved.overridden, vec![OverrideField::Access]);
}

// GIVEN an access the user took away (the default list minus one tool)
// WHEN the agent is resolved
// THEN the removed tool is no longer granted
#[test]
fn test_resolve_removed_access() {
    let agent = stored();
    let overrides = [over(
        OverrideField::Access,
        json!(["pkg:office/extract_totals"]),
    )];

    let resolved = resolve(&agent, &overrides, None);

    assert_eq!(resolved.access, vec!["pkg:office/extract_totals"]);
    assert!(!resolved.access.contains(&"file_read".to_owned()));
}

// GIVEN a model assigned to the agent in system.db
// WHEN the agent is resolved with it
// THEN the resolved agent carries that model
#[test]
fn test_resolve_carries_the_assigned_model() {
    let agent = stored();
    let model = AgentModel {
        agent: "invoices".into(),
        model_source: ModelSource::Local,
        model_id: "qwen3-8b".into(),
        preset_id: None,
    };

    let resolved = resolve(&agent, &[], Some(model.clone()));

    assert_eq!(resolved.model, Some(model));
}

// GIVEN a value of the wrong shape for each kind of field
// WHEN it is checked
// THEN it is refused with the field named
#[test]
fn test_override_value_shape_is_checked() {
    assert_eq!(
        OverrideField::Mission.check(&json!(["a"])),
        Err(OverrideValueError::ExpectedString("mission"))
    );
    assert_eq!(
        OverrideField::Access.check(&json!("file_read")),
        Err(OverrideValueError::ExpectedList("access"))
    );
    assert_eq!(
        OverrideField::Access.check(&json!(["file_read", 3])),
        Err(OverrideValueError::ExpectedList("access"))
    );
    assert!(OverrideField::Sources.check(&json!([])).is_ok());
    assert!(matches!(
        OverrideField::parse("model"),
        Err(OverrideValueError::UnknownField(_))
    ));
}

// GIVEN a manifest with a system prompt, tools and data sources
// WHEN its definition is built
// THEN the mission is the prompt and the access lists each tool once
#[test]
fn test_definition_from_manifest() {
    let manifest: AgentManifest = serde_json::from_value(json!({
        "name": "notes",
        "version": "1.0.0",
        "description": "Takes notes.",
        "tools_required": ["file_write"],
        "tools_optional": ["file_write", "file_read"],
        "system_prompt": "You keep the notes tidy.",
        "datasources": ["documents"]
    }))
    .unwrap();

    let definition = definition_from_manifest(&manifest);

    assert_eq!(
        definition,
        json!({
            "id": "notes",
            "name": "notes",
            "mission": "You keep the notes tidy.",
            "defaults": {"access": ["file_write", "file_read"], "sources": ["documents"]}
        })
    );
}
