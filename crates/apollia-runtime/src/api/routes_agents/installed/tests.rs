use std::sync::Arc;
use std::time::Instant;

use apollia_core::{AgentId, ProcessState};
use axum::http::StatusCode;

use super::*;
use crate::api::test_support::{app_state, call, manifest, StemLoader};

fn installed(name: &str) -> InstalledAgent {
    let mut stored = manifest(name);
    stored.description = String::new();
    InstalledAgent {
        name: name.to_owned(),
        version: "1.0.0".to_owned(),
        install_path: PathBuf::from(format!("/agents/{name}/agent.py")),
        source_path: PathBuf::from(format!("/src/{name}.py")),
        manifest: stored,
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
fn test_the_view_lists_installed_agents_with_their_state_then_the_started_ones() {
    // GIVEN one agent installed and running, one installed and stopped, and
    // one started without being installed
    let rows = [installed("veille"), installed("docx")];
    let entries = [running("veille"), running("adhoc")];

    // WHEN the view is built
    let views = merge_agent_views(&rows, &entries);

    // THEN the installed rows come first, the running one with its runtime
    // manifest and state, and the started-only agent last with no install
    let names: Vec<&str> = views.iter().map(|v| v.name.as_str()).collect();
    assert_eq!(names, ["veille", "docx", "adhoc"]);
    assert_eq!(views[0].runtime_status.as_deref(), Some("active"));
    assert_eq!(views[0].description.as_deref(), Some("veille description"));
    assert!(views[1].id.is_none() && views[1].runtime_status.is_none());
    assert!(views[1].description.is_none());
    assert!(views[2].installed_at.is_none() && views[2].install_path.is_none());
}

#[tokio::test]
async fn test_an_agent_installed_over_http_is_copied_recorded_and_listed() {
    // GIVEN a daemon whose data directory is empty, and a module on disk
    let data = tempfile::tempdir().unwrap();
    let source = tempfile::tempdir().unwrap();
    let module = source.path().join("veille.py");
    std::fs::write(&module, "class A: pass\n").unwrap();
    std::fs::write(source.path().join("helpers.py"), "X = 1\n").unwrap();
    let loader = Arc::new(StemLoader { refusal: None });

    // WHEN the module is installed, then the installed agents are listed
    let state = app_state(loader.clone(), data.path().to_path_buf());
    let body = serde_json::json!({ "path": module.display().to_string() });
    let (status, installed) = call(state, "POST", "/api/v1/agents/installed", Some(body)).await;
    let state = app_state(loader, data.path().to_path_buf());
    let (list_status, listed) = call(state, "GET", "/api/v1/agents/installed", None).await;

    // THEN it lives under the data directory with its sibling, agents.db
    // holds it, and the list shows it installed but not started
    assert_eq!(status, StatusCode::CREATED, "{installed}");
    assert_eq!(installed["name"], "veille");
    let dir = data.path().join("agents").join("veille");
    assert!(dir.join("agent.py").is_file() && dir.join("helpers.py").is_file());
    assert_eq!(list_status, StatusCode::OK);
    let views: Vec<InstalledAgentView> = serde_json::from_value(listed).unwrap();
    assert_eq!(views.len(), 1);
    assert_eq!(views[0].name, "veille");
    assert!(views[0].enabled && views[0].runtime_status.is_none());
}

#[tokio::test]
async fn test_a_module_the_loader_refuses_answers_400_with_its_cause() {
    // GIVEN a module the loader refuses
    let data = tempfile::tempdir().unwrap();
    let source = tempfile::tempdir().unwrap();
    let module = source.path().join("broken.py");
    std::fs::write(&module, "nope\n").unwrap();
    let loader = Arc::new(StemLoader {
        refusal: Some("missing @agent decorator".to_owned()),
    });
    let state = app_state(loader, data.path().to_path_buf());

    // WHEN it is installed over HTTP
    let body = serde_json::json!({ "path": module.display().to_string() });
    let (status, answer) = call(state, "POST", "/api/v1/agents/installed", Some(body)).await;

    // THEN the refusal carries the Python cause and nothing was written
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(answer["error"], "missing @agent decorator");
    assert!(!data.path().join("agents").exists());
    assert!(!data.path().join("agents.db").exists());
}
