//! The uninstall routes through the whole router.

use std::sync::{Arc, Mutex};

use apollia_tools::AgentRepository;
use apollia_triggers::{OnBusy, TriggerDefinitionRepository, TriggerDefinitionRow};
use axum::http::StatusCode;
use serde_json::json;
use tempfile::TempDir;

use crate::api::routes_packages::tests::{agents_db, archive, state, MANIFEST};
use crate::api::test_support::call;
use crate::audit_journal::JournalEntryKind;
use crate::governance::{governance_run_id, package_run_id};

fn trigger(id: &str, agent: &str) -> TriggerDefinitionRow {
    TriggerDefinitionRow {
        id: id.into(),
        agent: Some(agent.into()),
        workflow: None,
        workflow_inputs: None,
        enabled: true,
        on_busy: OnBusy::Queue,
        source_type: "interval".into(),
        source_config: json!({ "every": "1h" }),
        input_template: None,
        created_at: String::new(),
        updated_at: String::new(),
    }
}

#[tokio::test]
async fn test_uninstall_route_removes_the_package_its_trigger_and_journals_it() {
    // GIVEN a runtime with a journal and a trigger store, the crm package
    // installed through the API, and a trigger on its agent
    let dir = TempDir::new().unwrap();
    let (mut state, journal) = state(&dir, true).await;
    let triggers = TriggerDefinitionRepository::open(
        &apollia_core::paths::DataFile::Triggers.path(dir.path()),
    )
    .unwrap();
    triggers.insert(&trigger("crm-daily", "crm-agent")).unwrap();
    let triggers = Arc::new(Mutex::new(triggers));
    state.trigger_def_repo = Some(Arc::clone(&triggers));
    let apo = archive(dir.path(), "crm.apo", MANIFEST, None);
    let (installed, body) = call(
        state.clone(),
        "POST",
        "/api/v1/packages/install",
        Some(json!({ "path": apo })),
    )
    .await;
    assert_eq!(installed, StatusCode::CREATED, "{body}");

    // WHEN it is uninstalled, erasing the history
    let (status, body) = call(
        state,
        "POST",
        "/api/v1/packages/crm/uninstall",
        Some(json!({ "clearMemory": false, "clearHistory": true })),
    )
    .await;

    // THEN the report names what went, the agent and its trigger are gone
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["agents"], json!(["crm-agent"]));
    assert_eq!(body["triggersRemoved"], json!(["crm-daily"]));
    assert_eq!(body["historyCleared"], true);
    assert_eq!(body["memoryCleared"], false);
    let agents = AgentRepository::open(&agents_db(&dir)).unwrap();
    assert!(agents.stored("crm-agent").unwrap().is_none());
    assert!(triggers.lock().unwrap().list().unwrap().is_empty());
    assert!(!dir.path().join("agents/packages/crm").exists());
    // AND the journal holds the uninstallation of the package and of its agent
    let journal = journal.unwrap();
    let package_kinds: Vec<JournalEntryKind> = journal
        .query_run(&package_run_id("crm"))
        .await
        .into_iter()
        .map(|e| e.kind)
        .collect();
    assert_eq!(
        package_kinds,
        vec![
            JournalEntryKind::PackageInstalled,
            JournalEntryKind::PackageUninstalled
        ]
    );
    let agent_kinds: Vec<JournalEntryKind> = journal
        .query_run(&governance_run_id("crm-agent"))
        .await
        .into_iter()
        .map(|e| e.kind)
        .collect();
    assert_eq!(
        agent_kinds,
        vec![
            JournalEntryKind::AgentInstalled,
            JournalEntryKind::AgentUninstalled
        ]
    );
}

#[tokio::test]
async fn test_uninstall_routes_map_refusals_to_codes() {
    // GIVEN a runtime with a journal and the crm package installed
    let dir = TempDir::new().unwrap();
    let (state, _) = state(&dir, true).await;
    let apo = archive(dir.path(), "crm.apo", MANIFEST, None);
    call(
        state.clone(),
        "POST",
        "/api/v1/packages/install",
        Some(json!({ "path": apo })),
    )
    .await;
    let keep = json!({ "clearMemory": false, "clearHistory": false });

    // WHEN Apollia, an unknown package, and the package's agent alone are uninstalled
    let apollia = format!(
        "/api/v1/agents/installed/{}/uninstall",
        apollia_tools::agent_definition::APOLLIA_AGENT_ID
    );
    let (built_in, built_in_body) = call(state.clone(), "POST", &apollia, Some(keep.clone())).await;
    let (missing, missing_body) = call(
        state.clone(),
        "POST",
        "/api/v1/packages/ghost/uninstall",
        Some(keep.clone()),
    )
    .await;
    let (owned, owned_body) = call(
        state,
        "POST",
        "/api/v1/agents/installed/crm-agent/uninstall",
        Some(keep),
    )
    .await;

    // THEN each is refused with its code, and the agent stays
    assert_eq!(built_in, StatusCode::FORBIDDEN);
    assert_eq!(built_in_body["code"], "built_in");
    assert_eq!(missing, StatusCode::NOT_FOUND);
    assert_eq!(missing_body["code"], "not_found");
    assert_eq!(owned, StatusCode::CONFLICT);
    assert_eq!(owned_body["code"], "owned_by_package");
    let agents = AgentRepository::open(&agents_db(&dir)).unwrap();
    assert!(agents.stored("crm-agent").unwrap().is_some());
}

#[tokio::test]
async fn test_uninstall_route_refuses_without_a_journal() {
    // GIVEN a runtime without an audit journal
    let dir = TempDir::new().unwrap();
    let (state, _) = state(&dir, false).await;

    // WHEN a package is uninstalled
    let (status, body) = call(
        state,
        "POST",
        "/api/v1/packages/crm/uninstall",
        Some(json!({ "clearMemory": false, "clearHistory": false })),
    )
    .await;

    // THEN it is refused rather than left unrecorded
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["code"], "journal_unavailable");
}

fn lookup_tool() -> apollia_tools::ToolDescriptor {
    apollia_tools::ToolDescriptor {
        name: "pkg:crm/lookup".into(),
        version: "1.0.0".into(),
        description: "Look up a lead by name.".into(),
        kind: apollia_tools::ToolKind::Native,
        input_schema: json!({ "type": "object" }),
        output_schema: None,
        sandbox_profile: apollia_core::SandboxProfile::NetworkRestricted,
        tags: vec![],
        family: apollia_tools::ActionFamily::Read,
        family_source: apollia_tools::FamilySource::Builtin,
        is_read_only: false,
        risk_score: 1,
        approval_risk_level: None,
        impact_description: None,
        reject_reason_required: false,
    }
}

#[tokio::test]
async fn test_an_installed_package_reads_in_full_then_its_tool_leaves_the_running_runtime() {
    // GIVEN the crm package installed, requiring a connection; one automation
    // on its agent and one on another agent; its tool in the tool registry as
    // the runtime entered it at start
    let dir = TempDir::new().unwrap();
    let (mut state, _journal) = state(&dir, true).await;
    let triggers = TriggerDefinitionRepository::open(
        &apollia_core::paths::DataFile::Triggers.path(dir.path()),
    )
    .unwrap();
    triggers.insert(&trigger("crm-daily", "crm-agent")).unwrap();
    triggers.insert(&trigger("other-daily", "other")).unwrap();
    state.trigger_def_repo = Some(Arc::new(Mutex::new(triggers)));
    let registry = apollia_tools::ToolRegistryHandle::start();
    state.tool_registry_handle = Some(registry.clone());
    let manifest =
        format!("{MANIFEST}\n[[requires]]\ntype = \"gmail\"\nreason = \"Read the requests\"\n");
    let apo = archive(dir.path(), "crm.apo", &manifest, None);
    let (installed, body) = call(
        state.clone(),
        "POST",
        "/api/v1/packages/install",
        Some(json!({ "path": apo })),
    )
    .await;
    assert_eq!(installed, StatusCode::CREATED, "{body}");
    registry.register(lookup_tool()).await.unwrap();

    // WHEN the installed packages are listed, the package is read, then
    // uninstalled
    let (_, list) = call(
        state.clone(),
        "GET",
        "/api/v1/agent-packages/installed",
        None,
    )
    .await;
    let (read, detail) = call(
        state.clone(),
        "GET",
        "/api/v1/agent-packages/installed/crm",
        None,
    )
    .await;
    let (status, body) = call(
        state,
        "POST",
        "/api/v1/packages/crm/uninstall",
        Some(json!({ "clearMemory": false, "clearHistory": false })),
    )
    .await;

    // THEN the listing names the publisher, the signature and the counts
    assert_eq!(list[0]["name"], "crm");
    assert_eq!(
        list[0]["signature"],
        json!({ "publisher": "Acme", "signed": false, "trusted": false, "key_fingerprint": null })
    );
    assert_eq!(list[0]["embedded"], false);
    assert_eq!(
        list[0]["counts"],
        json!({ "agents": 1, "workflows": 0, "tools": 1, "connections": 1 })
    );
    // AND the detail describes the content by kind and the automation that
    // goes with the package, not the one of another agent
    assert_eq!(read, StatusCode::OK, "{detail}");
    assert_eq!(
        detail["content"]["agents"],
        json!([{ "id": "crm-agent", "name": "CRM", "description": "Track leads." }])
    );
    assert_eq!(
        detail["content"]["tools"],
        json!([{ "name": "lookup", "description": "Look up a lead by name.", "family": "read" }])
    );
    assert_eq!(
        detail["content"]["connections_required"],
        json!([{ "type": "gmail", "reason": "Read the requests" }])
    );
    assert_eq!(
        detail["automations"],
        json!([{ "id": "crm-daily", "target": "crm-agent" }])
    );
    // AND once uninstalled its tool is gone from the registry, with no restart
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["tools"], json!(["lookup"]));
    assert!(registry.get("pkg:crm/lookup").await.unwrap().is_none());
}
