use std::sync::Arc;

use apollia_core::ProcessState;
use axum::http::StatusCode;

use super::*;
use crate::api::test_support::{app_state, call, manifest};

/// A loader that names every module `veille`, the way the real one reads the
/// name from the manifest rather than from the file, or refuses with `refusal`.
struct VeilleLoader {
    refusal: Option<String>,
}

impl AgentLoader for VeilleLoader {
    fn load_and_validate(&self, _path: &Path) -> Result<apollia_core::AgentManifest, String> {
        match &self.refusal {
            Some(refusal) => Err(refusal.clone()),
            None => Ok(manifest("veille")),
        }
    }
}

/// A data directory where `veille` is installed from a module on disk, and the
/// source directory holding a second version of it.
async fn installed_veille() -> (tempfile::TempDir, tempfile::TempDir, PathBuf) {
    let data = tempfile::tempdir().unwrap();
    let source = tempfile::tempdir().unwrap();
    let module = source.path().join("veille.py");
    std::fs::write(&module, "VERSION = 1\n").unwrap();
    let state = app_state(
        Arc::new(VeilleLoader { refusal: None }),
        data.path().to_path_buf(),
    );
    let body = serde_json::json!({ "path": module.display().to_string() });
    let (status, answer) = call(state, "POST", "/api/v1/agents/installed", Some(body)).await;
    assert_eq!(status, StatusCode::CREATED, "{answer}");
    std::fs::write(&module, "VERSION = 2\n").unwrap();
    (data, source, module)
}

#[test]
fn test_is_running_covers_live_states_only() {
    // GIVEN every ProcessState variant
    // WHEN asked whether the agent still answers tasks
    // THEN only the live ones qualify, Stopping included in the negatives
    assert!(is_running(&ProcessState::Initializing));
    assert!(is_running(&ProcessState::Active));
    assert!(is_running(&ProcessState::Degraded));
    assert!(!is_running(&ProcessState::Stopping));
    assert!(!is_running(&ProcessState::Stopped));
}

#[tokio::test]
async fn test_an_unknown_agent_is_refused_with_the_message_of_the_embedded_command() {
    // GIVEN a daemon with nothing installed
    let data = tempfile::tempdir().unwrap();
    let state = app_state(
        Arc::new(VeilleLoader { refusal: None }),
        data.path().to_path_buf(),
    );

    // WHEN an update names an agent it does not hold
    let file = data.path().join("ghost.py");
    std::fs::write(&file, "X = 1\n").unwrap();
    let body = serde_json::json!({ "path": file.display().to_string() });
    let (update, update_refused) =
        call(state, "PUT", "/api/v1/agents/installed/ghost", Some(body)).await;

    // THEN it answers 404 with the words the window showed embedded
    assert_eq!(update, StatusCode::NOT_FOUND);
    assert_eq!(
        update_refused["error"],
        "agent 'ghost' not found in installed agents"
    );
}

#[tokio::test]
async fn test_an_update_of_an_agent_not_running_replaces_the_module_and_says_so() {
    // GIVEN veille installed, not started, and a second version on disk
    let (data, _source, module) = installed_veille().await;
    let state = app_state(
        Arc::new(VeilleLoader { refusal: None }),
        data.path().to_path_buf(),
    );

    // WHEN it is updated over HTTP
    let body = serde_json::json!({ "path": module.display().to_string() });
    let (status, answer) = call(state, "PUT", "/api/v1/agents/installed/veille", Some(body)).await;

    // THEN the installed module is the new one, and nothing was cycled
    assert_eq!(status, StatusCode::OK, "{answer}");
    let updated: UpdateAgentResponse = serde_json::from_value(answer).unwrap();
    assert_eq!(updated.restart_outcome, restart_outcome::NOT_RUNNING);
    assert!(updated.restart_error.is_none());
    let installed = std::fs::read_to_string(data.path().join("agents/veille/agent.py")).unwrap();
    assert_eq!(installed, "VERSION = 2\n");
}

#[tokio::test]
async fn test_an_update_of_a_running_agent_cycles_it() {
    // GIVEN veille installed and running
    let (data, _source, module) = installed_veille().await;
    let state = app_state(
        Arc::new(VeilleLoader { refusal: None }),
        data.path().to_path_buf(),
    );
    let agent_path = data.path().join("agents/veille/agent.py");
    let body = serde_json::json!({ "agent_path": agent_path.display().to_string() });
    let (started, _) = call(state.clone(), "POST", "/api/v1/agents", Some(body)).await;
    assert_eq!(started, StatusCode::CREATED);

    // WHEN it is updated over HTTP
    let body = serde_json::json!({ "path": module.display().to_string() });
    let (status, answer) = call(
        state.clone(),
        "PUT",
        "/api/v1/agents/installed/veille",
        Some(body),
    )
    .await;

    // THEN the answer says it restarted, and it is active again
    assert_eq!(status, StatusCode::OK, "{answer}");
    assert_eq!(answer["restart_outcome"], restart_outcome::RESTARTED);
    let id = state
        .registry_handle
        .find_by_name("veille")
        .await
        .unwrap()
        .unwrap();
    let entry = state
        .registry_handle
        .get_agent(id.as_str())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(entry.process_state, ProcessState::Active);
}

#[tokio::test]
async fn test_a_refused_module_answers_400_and_leaves_the_install_alone() {
    // GIVEN veille installed, and a loader that refuses the new module
    let (data, _source, module) = installed_veille().await;
    let state = app_state(
        Arc::new(VeilleLoader {
            refusal: Some("missing @agent decorator".to_owned()),
        }),
        data.path().to_path_buf(),
    );

    // WHEN it is updated over HTTP
    let body = serde_json::json!({ "path": module.display().to_string() });
    let (status, answer) = call(state, "PUT", "/api/v1/agents/installed/veille", Some(body)).await;

    // THEN the Python cause comes back and the installed module is untouched
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(answer["error"], "missing @agent decorator");
    let installed = std::fs::read_to_string(data.path().join("agents/veille/agent.py")).unwrap();
    assert_eq!(installed, "VERSION = 1\n");
}
