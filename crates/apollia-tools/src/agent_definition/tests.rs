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

    let resolved = resolve(&agent, &[], None, AutonomyLevel::Assisted);

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

    let resolved = resolve(&agent, &overrides, None, AutonomyLevel::Assisted);

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

    let resolved = resolve(&agent, &overrides, None, AutonomyLevel::Assisted);

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

    let resolved = resolve(&agent, &overrides, None, AutonomyLevel::Assisted);

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

    let resolved = resolve(&agent, &[], Some(model.clone()), AutonomyLevel::Assisted);

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

// GIVEN the manifest JSON of a row stored before definitions existed, with a
// system prompt, tools and data sources
// WHEN its definition is read
// THEN the mission is the prompt, the access lists each tool once, and the
// data sources, files the agent carried, check no source
#[test]
fn test_definition_from_a_legacy_manifest() {
    let manifest = json!({
        "name": "notes",
        "version": "1.0.0",
        "description": "Takes notes.",
        "tools_required": ["file_write"],
        "tools_optional": ["file_write", "file_read"],
        "system_prompt": "You keep the notes tidy.",
        "datasources": ["documents"]
    })
    .to_string();

    let definition = definition_from_legacy_manifest_json("notes", &manifest);

    assert_eq!(
        definition,
        json!({
            "id": "notes",
            "name": "notes",
            "mission": "You keep the notes tidy.",
            "defaults": {"access": ["file_write", "file_read"]}
        })
    );
}

/// The stored agent with `autonomy` declared among its defaults.
fn stored_declaring(autonomy: &str) -> StoredAgent {
    let mut agent = stored();
    agent.definition["defaults"]["autonomy"] = json!(autonomy);
    agent
}

#[test]
fn test_resolve_reads_the_declared_autonomy() {
    // GIVEN an agent whose definition declares the supervised tier
    let agent = stored_declaring("supervised");

    // WHEN it is resolved under an assisted global default
    let resolved = resolve(&agent, &[], None, AutonomyLevel::Assisted);

    // THEN the tier is the declared one
    assert_eq!(resolved.autonomy, AutonomyLevel::Supervised);
    assert!(resolved.overridden.is_empty());
}

#[test]
fn test_resolve_autonomy_override_wins_over_the_declared_one() {
    // GIVEN an agent declaring the supervised tier, overridden to the long one
    let agent = stored_declaring("supervised");
    let overrides = [over(OverrideField::Autonomy, json!("long_autonomous"))];

    // WHEN it is resolved
    let resolved = resolve(&agent, &overrides, None, AutonomyLevel::Assisted);

    // THEN the tier is the user's and is marked overridden
    assert_eq!(resolved.autonomy, AutonomyLevel::LongAutonomous);
    assert_eq!(resolved.overridden, vec![OverrideField::Autonomy]);
}

#[test]
fn test_resolve_autonomy_falls_back_to_the_global_default() {
    // GIVEN an agent that declares no tier, and one that declares an unknown one
    let silent = stored();
    let garbled = stored_declaring("unbounded");

    // WHEN both are resolved under a bounded global default
    let from_silent = resolve(&silent, &[], None, AutonomyLevel::BoundedAutonomous);
    let from_garbled = resolve(&garbled, &[], None, AutonomyLevel::BoundedAutonomous);

    // THEN both run at the global default
    assert_eq!(from_silent.autonomy, AutonomyLevel::BoundedAutonomous);
    assert_eq!(from_garbled.autonomy, AutonomyLevel::BoundedAutonomous);
}

#[test]
fn test_autonomy_override_value_is_one_of_the_four_tiers() {
    // GIVEN the autonomy field

    // WHEN a tier, an unknown name and a list are checked
    let tier = OverrideField::Autonomy.check(&json!("supervised"));
    let unknown = OverrideField::Autonomy.check(&json!("unbounded"));
    let list = OverrideField::Autonomy.check(&json!(["supervised"]));

    // THEN only the tier is accepted, and the field is parsed by its name
    assert_eq!(tier, Ok(()));
    assert_eq!(unknown, Err(OverrideValueError::ExpectedAutonomyLevel));
    assert_eq!(list, Err(OverrideValueError::ExpectedAutonomyLevel));
    assert_eq!(
        OverrideField::parse("autonomy"),
        Ok(OverrideField::Autonomy)
    );
}

#[test]
fn test_definition_from_a_legacy_manifest_carries_the_declared_autonomy() {
    // GIVEN the manifest JSON of a row that declared a tier
    let manifest = json!({
        "name": "notes",
        "version": "1.0.0",
        "description": "Takes notes.",
        "tools_required": [],
        "autonomy_level": "supervised"
    })
    .to_string();

    // WHEN its definition is read and resolved
    let definition = definition_from_legacy_manifest_json("notes", &manifest);
    let mut agent = stored();
    agent.definition = definition.clone();
    let resolved = resolve(&agent, &[], None, AutonomyLevel::Assisted);

    // THEN the tier sits among the defaults and is the one the agent runs at
    assert_eq!(definition["defaults"]["autonomy"], json!("supervised"));
    assert_eq!(resolved.autonomy, AutonomyLevel::Supervised);
}

#[test]
fn test_the_registry_manifest_of_a_stored_agent_declares_no_access() {
    // GIVEN a stored agent with a mission, examples, an access and a tier
    let mut agent = stored();
    agent.definition = json!({
        "id": agent.name,
        "name": "Notes",
        "mission": "You keep the notes tidy.",
        "examples": ["File this note"],
        "defaults": {"access": ["file_write"], "autonomy": "supervised"}
    });

    // WHEN the registry manifest is built
    let manifest = manifest_of(&agent);

    // THEN it names the agent, its version, its mission, its examples and
    // its tier, and serialises without any key naming a tool
    assert_eq!(manifest.name, agent.name);
    assert_eq!(manifest.version, agent.version);
    assert_eq!(manifest.description, "You keep the notes tidy.");
    assert_eq!(manifest.examples, vec!["File this note".to_string()]);
    assert_eq!(manifest.autonomy_level, Some(AutonomyLevel::Supervised));
    let written = serde_json::to_string(&manifest).unwrap();
    assert!(!written.contains("file_write"), "{written}");
}

#[test]
fn test_resolve_plans_on_its_own_judgement_when_nothing_says_otherwise() {
    // GIVEN an agent whose definition says nothing about planning
    let agent = stored();

    // WHEN it is resolved
    let resolved = resolve(&agent, &[], None, AutonomyLevel::Assisted);

    // THEN it plans when the task reads as a multi-step one
    assert_eq!(resolved.planning, PlanningMode::Auto);
}

#[test]
fn test_resolve_planning_override_wins_over_the_declared_one() {
    // GIVEN an agent whose package asks it to always plan, overridden to never
    let mut agent = stored();
    agent.definition["defaults"]["planning"] = json!("always");
    let declared = resolve(&agent, &[], None, AutonomyLevel::Assisted);
    let overrides = [over(OverrideField::Planning, json!("never"))];

    // WHEN it is resolved with the override
    let resolved = resolve(&agent, &overrides, None, AutonomyLevel::Assisted);

    // THEN the package's mode applies without it, the user's with it
    assert_eq!(declared.planning, PlanningMode::Always);
    assert_eq!(resolved.planning, PlanningMode::Never);
    assert_eq!(resolved.overridden, vec![OverrideField::Planning]);
}

#[test]
fn test_planning_override_value_is_one_of_the_three_modes() {
    // GIVEN the planning field

    // WHEN each mode, an unknown name and a boolean are checked
    let modes: Vec<_> = ["never", "auto", "always"]
        .iter()
        .map(|mode| OverrideField::Planning.check(&json!(mode)))
        .collect();
    let unknown = OverrideField::Planning.check(&json!("sometimes"));
    let boolean = OverrideField::Planning.check(&json!(true));

    // THEN only the three modes are accepted, and the field reads by its name
    assert_eq!(modes, vec![Ok(()), Ok(()), Ok(())]);
    assert_eq!(unknown, Err(OverrideValueError::ExpectedPlanningMode));
    assert_eq!(boolean, Err(OverrideValueError::ExpectedPlanningMode));
    assert_eq!(
        OverrideField::parse("planning"),
        Ok(OverrideField::Planning)
    );
}

#[test]
fn test_the_registry_manifest_names_the_memory_space_the_agent_writes_to() {
    // GIVEN a stored agent, and Apollia
    let agent = stored();
    let mut apollia = stored();
    apollia.name = APOLLIA_AGENT_ID.to_string();

    // WHEN the registry manifest of each is built
    let of_agent = manifest_of(&agent);
    let of_apollia = manifest_of(&apollia);

    // THEN each names the space its memory tools read and write, so a screen
    // that lists the memory of an agent looks where the agent keeps it
    assert_eq!(of_agent.memory_namespace, Some(agent.name.clone()));
    assert_eq!(
        of_apollia.memory_namespace,
        Some(APOLLIA_MEMORY_NAMESPACE.to_string())
    );
}
