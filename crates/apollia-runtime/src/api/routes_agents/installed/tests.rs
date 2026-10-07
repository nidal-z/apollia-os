use std::time::Instant;

use apollia_core::{AgentId, ProcessState};
use axum::http::StatusCode;

use super::*;
use crate::api::test_support::{app_state, call, manifest};

fn stored(name: &str) -> StoredAgent {
    StoredAgent {
        name: name.to_owned(),
        origin: apollia_tools::AgentOrigin::Package,
        package: None,
        version: "1.0.0".to_owned(),
        definition: serde_json::json!({
            "id": name, "name": name, "mission": "", "defaults": {},
        }),
        enabled: true,
        installed_at: "2026-10-02T00:00:00Z".to_owned(),
        updated_at: "2026-10-02T00:00:00Z".to_owned(),
    }
}

fn running(name: &str) -> AgentEntry {
    AgentEntry {
        id: AgentId::new_v4(),
        manifest: manifest(name),
        process_state: ProcessState::Active,
        registered_at: Instant::now(),
    }
}

#[test]
fn test_the_view_lists_stored_agents_with_their_state_then_the_registered_ones() {
    // GIVEN one agent stored and registered, one stored only, and one
    // registered without being stored
    let rows = [stored("veille"), stored("docx")];
    let entries = [running("veille"), running("adhoc")];

    // WHEN the view is built
    let views = merge_agent_views(&rows, &entries);

    // THEN the stored rows come first, the registered one with what the
    // registry holds and its state, and the registered-only agent last
    let names: Vec<&str> = views.iter().map(|v| v.name.as_str()).collect();
    assert_eq!(names, ["veille", "docx", "adhoc"]);
    assert_eq!(views[0].runtime_status.as_deref(), Some("active"));
    assert_eq!(views[0].description.as_deref(), Some("veille description"));
    assert!(views[1].id.is_none() && views[1].runtime_status.is_none());
    assert!(views[1].description.is_none());
    assert!(views[2].installed_at.is_none());
}

#[tokio::test]
async fn test_the_stored_agents_are_listed_without_the_apollia_agent() {
    // GIVEN a data directory whose store holds one agent a package brought,
    // beside the Apollia agent every store holds
    let data = tempfile::tempdir().unwrap();
    let repo =
        AgentRepository::open(&apollia_core::paths::DataFile::Agents.path(data.path())).unwrap();
    let mut veille = stored("veille");
    veille.definition["mission"] = serde_json::json!("Watch the news.");
    repo.save_definition(&veille).unwrap();
    drop(repo);

    // WHEN the agents of the machine are listed
    let state = app_state(data.path().to_path_buf());
    let (status, listed) = call(state, "GET", "/api/v1/agents/installed", None).await;

    // THEN the agent is there, stored and not registered, with its mission,
    // and the Apollia agent is not
    assert_eq!(status, StatusCode::OK, "{listed}");
    let views: Vec<InstalledAgentView> = serde_json::from_value(listed).unwrap();
    assert_eq!(views.len(), 1);
    assert_eq!(views[0].name, "veille");
    assert_eq!(views[0].description.as_deref(), Some("Watch the news."));
    assert!(views[0].enabled && views[0].runtime_status.is_none());
}

#[tokio::test]
async fn test_installing_a_python_file_is_no_longer_a_route() {
    // GIVEN a daemon and a Python module on disk
    let data = tempfile::tempdir().unwrap();
    let source = tempfile::tempdir().unwrap();
    let module = source.path().join("veille.py");
    std::fs::write(&module, "class A: pass\n").unwrap();
    let state = app_state(data.path().to_path_buf());

    // WHEN the module is posted to the route that installed one
    let body = serde_json::json!({ "path": module.display().to_string() });
    let (status, _) = call(state, "POST", "/api/v1/agents/installed", Some(body)).await;

    // THEN the route does not take it, and nothing was written
    assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
    assert!(!data.path().join("agents").exists());
}
