use std::path::Path;
use std::sync::Arc;

use axum::http::StatusCode;
use serde_json::json;
use tempfile::TempDir;

use crate::api::server::AppState;
use crate::api::test_support::{app_state, call, MockBackend, StemLoader};

/// Writes a package folder `veille` with one agent, a cron trigger, and the
/// Python dependencies `deps`.
fn package(dir: &Path, deps: &[&str]) -> String {
    let root = dir.join("veille-src");
    std::fs::create_dir_all(&root).unwrap();
    let deps = deps
        .iter()
        .map(|d| format!("\"{d}\""))
        .collect::<Vec<_>>()
        .join(", ");
    let manifest = format!(
        r#"
[package]
name        = "veille"
version     = "1.2.0"
description = "Morning watch"
author      = "Ada"

[[agents]]
name     = "veille-agent"
entry    = "veille-agent.py"
role     = "director"
packages = [{deps}]

[[triggers]]
id    = "daily"
agent = "veille-agent"

[triggers.source]
type     = "cron"
schedule = "0 7 * * 1-5"

[[triggers]]
id    = "inbound"
agent = "veille-agent"

[triggers.source]
type = "webhook"
"#
    );
    std::fs::write(root.join("agent.toml"), manifest).unwrap();
    std::fs::write(root.join("veille-agent.py"), "class Agent: pass\n").unwrap();
    root.to_string_lossy().to_string()
}

fn state(dir: &Path, refusal: Option<&str>) -> AppState<MockBackend> {
    let data_dir = dir.join("data");
    std::fs::create_dir_all(&data_dir).unwrap();
    let loader = StemLoader {
        refusal: refusal.map(str::to_owned),
    };
    app_state(Arc::new(loader), data_dir)
}

#[tokio::test]
async fn test_a_folder_is_previewed_without_a_write_or_refused_when_unreadable() {
    // GIVEN a package folder declaring one dependency and a webhook with no secret
    let dir = TempDir::new().unwrap();
    let path = package(dir.path(), &["httpx>=0.27"]);
    let state = state(dir.path(), None);

    // WHEN it is previewed, then a folder without agent.toml is
    let (ok, preview) = call(
        state.clone(),
        "POST",
        "/api/v1/agent-packages/preview",
        Some(json!({ "path": path })),
    )
    .await;
    let missing = dir.path().join("nothing").to_string_lossy().to_string();
    let (refused, body) = call(
        state.clone(),
        "POST",
        "/api/v1/agent-packages/preview",
        Some(json!({ "path": missing })),
    )
    .await;

    // THEN the preview names the agent, the triggers, the webhook to
    // configure and the dependency, and nothing was installed; the missing
    // manifest is a 400 with the reason
    assert_eq!(ok, StatusCode::OK, "{preview}");
    assert_eq!(preview["valid"], true);
    assert_eq!(preview["agents"][0]["name"], "veille-agent");
    assert_eq!(preview["trigger_count"], 2);
    assert_eq!(preview["triggers"][1]["needs_config"], true);
    assert_eq!(preview["pip_packages"], json!(["httpx>=0.27"]));
    assert!(!state.data_dir.join("agents").exists());
    assert_eq!(refused, StatusCode::BAD_REQUEST, "{body}");
    assert!(body["error"]
        .as_str()
        .unwrap()
        .starts_with("cannot read agent.toml"));
}

#[tokio::test]
async fn test_a_package_is_installed_listed_and_read_in_full() {
    // GIVEN a package folder without dependencies
    let dir = TempDir::new().unwrap();
    let path = package(dir.path(), &[]);
    let state = state(dir.path(), None);

    // WHEN it is installed with a secret for its webhook, then listed and read
    let (created, report) = call(
        state.clone(),
        "POST",
        "/api/v1/agent-packages/installed",
        Some(json!({
            "path": path,
            "trigger_configs": [{ "id": "inbound", "secret": "0123456789abcdef0123456789abcdef" }],
        })),
    )
    .await;
    let (listed, list) = call(
        state.clone(),
        "GET",
        "/api/v1/agent-packages/installed",
        None,
    )
    .await;
    let (read, detail) = call(
        state.clone(),
        "GET",
        "/api/v1/agent-packages/installed/veille",
        None,
    )
    .await;

    // THEN the folder is copied, the agent and both triggers are recorded,
    // and the listing and the detail read the package back
    assert_eq!(created, StatusCode::CREATED, "{report}");
    assert_eq!(report["name"], "veille");
    assert_eq!(report["agent_count"], 1);
    assert_eq!(report["trigger_errors"], json!([]), "{report}");
    assert_eq!(report["trigger_count"], 2);
    let copied = state
        .data_dir
        .join("agents/packages/veille/veille-agent.py");
    assert!(copied.exists());
    assert_eq!(listed, StatusCode::OK, "{list}");
    assert_eq!(list[0]["name"], "veille");
    assert_eq!(list[0]["description"], "Morning watch");
    assert_eq!(list[0]["root_missing"], false);
    assert_eq!(read, StatusCode::OK, "{detail}");
    assert_eq!(detail["author"], "Ada");
    assert_eq!(detail["agents"][0]["role"], "director");
    assert_eq!(detail["manifest"]["package"]["version"], "1.2.0");
}

#[tokio::test]
async fn test_unconfirmed_dependencies_and_a_refused_import_leave_nothing_behind() {
    // GIVEN a package with a dependency, and a runtime whose loader refuses
    // every agent
    let dir = TempDir::new().unwrap();
    let with_deps = package(dir.path(), &["httpx>=0.27"]);
    let confirmable = state(dir.path(), None);
    let refusing = state(dir.path(), Some("no Agent class"));
    let plain = dir.path().join("plain");
    std::fs::create_dir_all(&plain).unwrap();
    let toml = std::fs::read_to_string(Path::new(&with_deps).join("agent.toml"))
        .unwrap()
        .replace("packages = [\"httpx>=0.27\"]", "packages = []");
    std::fs::write(plain.join("agent.toml"), toml).unwrap();
    std::fs::write(plain.join("veille-agent.py"), "class Agent: pass\n").unwrap();

    // WHEN the first is installed unconfirmed, the second through the refusing loader
    let (precondition, deps) = call(
        confirmable.clone(),
        "POST",
        "/api/v1/agent-packages/installed",
        Some(json!({ "path": with_deps })),
    )
    .await;
    let (unprocessable, import) = call(
        refusing.clone(),
        "POST",
        "/api/v1/agent-packages/installed",
        Some(json!({ "path": plain.to_string_lossy() })),
    )
    .await;

    // THEN the first answers the text the interface parses, the second the
    // import's cause, and neither left a folder
    assert_eq!(precondition, StatusCode::PRECONDITION_REQUIRED, "{deps}");
    assert_eq!(deps["error"], "DEPS_CONFIRMATION_REQUIRED:1:httpx>=0.27");
    assert_eq!(unprocessable, StatusCode::UNPROCESSABLE_ENTITY, "{import}");
    assert!(import["error"].as_str().unwrap().contains("no Agent class"));
    assert!(!refusing.data_dir.join("agents/packages/veille").exists());
}

#[tokio::test]
async fn test_an_unknown_package_is_a_404_and_an_unreadable_store_a_500() {
    // GIVEN a runtime with no package, and one whose data directory is a file
    let dir = TempDir::new().unwrap();
    let empty = state(dir.path(), None);
    let file = dir.path().join("not-a-dir");
    std::fs::write(&file, "x").unwrap();
    let broken = app_state(Arc::new(StemLoader { refusal: None }), file);

    // WHEN a package is read from the first and the list from the second
    let (missing, body) = call(
        empty,
        "GET",
        "/api/v1/agent-packages/installed/veille",
        None,
    )
    .await;
    let (failed, failure) = call(broken, "GET", "/api/v1/agent-packages/installed", None).await;

    // THEN the absent package is a 404 in the words of the embedded mode,
    // the store a 500
    assert_eq!(missing, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["error"], "Package 'veille' not found");
    assert_eq!(failed, StatusCode::INTERNAL_SERVER_ERROR, "{failure}");
}

#[tokio::test]
async fn test_a_package_is_uninstalled_and_a_name_that_is_a_path_refused() {
    // GIVEN an installed package
    let dir = TempDir::new().unwrap();
    let path = package(dir.path(), &[]);
    let state = state(dir.path(), None);
    let (created, _) = call(
        state.clone(),
        "POST",
        "/api/v1/agent-packages/installed",
        Some(json!({ "path": path, "trigger_configs": [{ "id": "inbound", "secret": "0123456789abcdef0123456789abcdef" }] })),
    )
    .await;
    assert_eq!(created, StatusCode::CREATED);

    // WHEN it is uninstalled, then a name climbing out of the data directory is
    let (removed, body) = call(
        state.clone(),
        "DELETE",
        "/api/v1/agent-packages/installed/veille",
        None,
    )
    .await;
    let (_, list) = call(
        state.clone(),
        "GET",
        "/api/v1/agent-packages/installed",
        None,
    )
    .await;
    let (refused, refusal) = call(
        state.clone(),
        "DELETE",
        "/api/v1/agent-packages/installed/%2E%2E",
        None,
    )
    .await;

    // THEN the package, its row and its folder are gone, and the path is a
    // 400 before anything is touched
    assert_eq!(removed, StatusCode::OK, "{body}");
    assert_eq!(body["uninstalled"], "veille");
    assert_eq!(list, json!([]));
    assert!(!state.data_dir.join("agents/packages/veille").exists());
    assert_eq!(refused, StatusCode::BAD_REQUEST, "{refusal}");
    assert!(state.data_dir.exists());
}
