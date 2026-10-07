use std::path::PathBuf;
use std::sync::Arc;

use apollia_permissions::PolicyVerdict;
use axum::http::StatusCode;
use serde_json::json;
use tempfile::TempDir;

use crate::api::server::AppState;
use crate::api::test_support::{app_state, call, MockBackend};
use crate::governance::InferenceGovernance;

/// A runtime started on an `apollia.toml` holding `content`, under the
/// settings `settings` of an organisation policy (none when empty).
fn runtime_on(content: &str, settings: &str) -> (TempDir, PathBuf, AppState<MockBackend>) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("apollia.toml");
    std::fs::write(&path, content).unwrap();
    let mut state = app_state(dir.path().to_path_buf());
    state.config_path = Some(path.clone());
    if !settings.is_empty() {
        let text = format!(
            "schema_version = 1\nsigned_at = 2026-10-01T09:00:00Z\n[settings]\n{settings}\n"
        );
        let policy = apollia_permissions::parse_policy(&text).expect("policy parses");
        state.inference_governance = InferenceGovernance::new(Arc::new(PolicyVerdict::Valid(
            Box::new(apollia_permissions::ValidPolicy {
                signed_at: policy.signed_at,
                policy,
                key_fingerprint: "00".into(),
                policy_digest: "00".into(),
                publisher: "it".into(),
            }),
        )));
    }
    (dir, path, state)
}

fn observability_body(retention_days: u32, debug_log_prompt: bool) -> serde_json::Value {
    json!({
        "max_input_bytes": 1024,
        "max_output_bytes": 2048,
        "max_tool_output_bytes": 512,
        "capture_thoughts": false,
        "capture_tool_args": true,
        "capture_tool_outputs": false,
        "capture_agent_logs": true,
        "retention_days": retention_days,
        "debug_log_prompt": debug_log_prompt,
    })
}

#[tokio::test]
async fn test_the_observability_policy_is_written_to_its_two_sections_then_read_back() {
    // GIVEN a runtime on a file with a comment and no observability section
    let (_dir, path, state) = runtime_on("# mine\n", "");

    // WHEN a client saves a policy, then reads it
    let (saved_status, saved) = call(
        state.clone(),
        "PUT",
        "/api/v1/config/observability",
        Some(observability_body(30, true)),
    )
    .await;
    let (read_status, read) = call(state, "GET", "/api/v1/config/observability", None).await;

    // THEN the capture keys land under [observability], the prompt log under
    // [llm.observability], the comment is kept, and the read gives them back
    assert_eq!(saved_status, StatusCode::OK, "{saved}");
    assert_eq!(read_status, StatusCode::OK, "{read}");
    assert_eq!(read, observability_body(30, true));
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("# mine"), "{text}");
    let doc: toml::Value = text.parse().unwrap();
    assert_eq!(
        doc["observability"]["retention_days"].as_integer(),
        Some(30)
    );
    assert_eq!(
        doc["llm"]["observability"]["debug_log_prompt"].as_bool(),
        Some(true)
    );
    assert!(
        doc["observability"].get("debug_log_prompt").is_none(),
        "{text}"
    );
}

#[tokio::test]
async fn test_a_file_without_the_section_reads_as_the_defaults() {
    // GIVEN a runtime on an empty file
    let (_dir, _path, state) = runtime_on("", "");

    // WHEN a client reads the observability policy
    let (status, body) = call(state, "GET", "/api/v1/config/observability", None).await;

    // THEN every field is at its default
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["retention_days"], 90);
    assert_eq!(body["capture_thoughts"], true);
    assert_eq!(body["debug_log_prompt"], false);
}

#[tokio::test]
async fn test_saving_over_a_file_that_does_not_parse_is_refused_and_the_file_untouched() {
    // GIVEN a runtime on a file the hand of its user broke
    let broken = "[observability\nretention_days = 7\n";
    let (_dir, path, state) = runtime_on(broken, "");

    // WHEN a client saves a policy
    let (status, body) = call(
        state,
        "PUT",
        "/api/v1/config/observability",
        Some(observability_body(30, false)),
    )
    .await;

    // THEN the writer refuses with its sentence, and nothing moved
    assert!(!status.is_success(), "{status} {body}");
    assert!(
        !body["error"].as_str().unwrap_or_default().is_empty(),
        "{body}"
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), broken);
}

#[tokio::test]
async fn test_without_a_configuration_file_the_settings_are_unavailable() {
    // GIVEN a runtime started without a configuration file
    let (_dir, _path, mut state) = runtime_on("", "");
    state.config_path = None;

    // WHEN a client reads the observability policy and the interpreter
    let (obs, obs_body) = call(state.clone(), "GET", "/api/v1/config/observability", None).await;
    let (py, py_body) = call(state, "GET", "/api/v1/config/python-interpreter", None).await;

    // THEN both answer unavailable
    assert_eq!(obs, StatusCode::SERVICE_UNAVAILABLE, "{obs_body}");
    assert_eq!(py, StatusCode::SERVICE_UNAVAILABLE, "{py_body}");
    assert_eq!(obs_body["code"], "unavailable");
}

#[tokio::test]
async fn test_the_chosen_interpreter_is_read_then_removed_to_return_to_the_bundled_one() {
    // GIVEN a runtime on a file that names an interpreter
    let (_dir, path, state) = runtime_on(
        "# mine\n[tools]\npython_interpreter = \"/opt/python/bin/python3\"\n",
        "",
    );

    // WHEN a client reads the setting, then returns to the bundled interpreter
    let (read_status, read) = call(
        state.clone(),
        "GET",
        "/api/v1/config/python-interpreter",
        None,
    )
    .await;
    let (saved_status, saved) = call(
        state,
        "PUT",
        "/api/v1/config/python-interpreter",
        Some(json!({ "path": null })),
    )
    .await;

    // THEN the read names the chosen path and the required minor, and the
    // return removes the key, keeping the comment
    assert_eq!(read_status, StatusCode::OK, "{read}");
    assert_eq!(read["chosen"], "/opt/python/bin/python3");
    assert_eq!(
        read["required_minor"],
        apollia_tools::tools::python_discovery::BUNDLED_PYTHON_MINOR
    );
    assert_eq!(saved_status, StatusCode::OK, "{saved}");
    assert!(saved["chosen"].is_null(), "{saved}");
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(!text.contains("python_interpreter"), "{text}");
    assert!(text.contains("# mine"), "{text}");
}

#[tokio::test]
async fn test_an_interpreter_that_does_not_validate_is_refused_with_its_condition() {
    // GIVEN a runtime on a file with a comment
    let (_dir, path, state) = runtime_on("# mine\n", "");

    // WHEN a client chooses a relative path
    let (status, body) = call(
        state,
        "PUT",
        "/api/v1/config/python-interpreter",
        Some(json!({ "path": "bin/python3" })),
    )
    .await;

    // THEN it is refused as invalid with the sentence naming the condition,
    // and the file is untouched
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["code"], "invalid");
    assert!(
        body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("relative"),
        "{body}"
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "# mine\n");
}

#[tokio::test]
async fn test_the_security_posture_is_the_one_of_this_machine() {
    // GIVEN a runtime
    let (_dir, _path, state) = runtime_on("", "");

    // WHEN a client reads the security posture
    let (status, body) = call(state, "GET", "/api/v1/security-posture", None).await;

    // THEN it is the posture the probe detects here
    assert_eq!(status, StatusCode::OK, "{body}");
    let expected = serde_json::to_value(apollia_core::SecurityPosture::detect()).unwrap();
    assert_eq!(body, expected);
}
