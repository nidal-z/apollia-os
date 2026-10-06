use std::path::PathBuf;
use std::sync::Arc;

use apollia_core::config::writer::{fingerprint, update_config, ConfigEdit, KeyChange};
use apollia_core::config::PolicySettings;
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

#[tokio::test]
async fn test_reading_returns_the_text_its_fingerprint_its_faults_and_the_locked_keys() {
    // GIVEN a file with an unknown key on line 2, under a policy that fixes the
    // developer mode off
    let content = "[api]\ntypo = 1\n";
    let (_dir, _path, state) = runtime_on(content, "\"desktop.developer_mode\" = false");

    // WHEN a client reads the configuration
    let (status, body) = call(state, "GET", "/api/v1/config", None).await;

    // THEN it gets the text, its fingerprint, the fault at line 2, the locked
    // key with its value, and no key waiting for a restart
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["text"], content);
    assert_eq!(body["sha256"], fingerprint(content));
    assert_eq!(body["exists"], true);
    assert_eq!(body["diagnostics"][0]["line"], 2, "{body}");
    assert_eq!(body["locked"][0]["key"], "desktop.developer_mode");
    assert_eq!(body["locked"][0]["value"], false);
    assert_eq!(body["pendingRestart"], json!([]));
}

#[tokio::test]
async fn test_validating_a_text_reports_its_faults_without_writing() {
    // GIVEN a runtime on a valid file
    let (_dir, path, state) = runtime_on("[api]\nport = 7771\n", "");

    // WHEN a client validates a text with a wrong type on line 2
    let (status, body) = call(
        state,
        "POST",
        "/api/v1/config/validate",
        Some(json!({ "text": "[api]\nport = \"x\"\n" })),
    )
    .await;

    // THEN the verdict is invalid at line 2, and the file is untouched
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["valid"], false);
    assert_eq!(body["diagnostics"][0]["line"], 2);
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "[api]\nport = 7771\n"
    );
}

#[tokio::test]
async fn test_saving_against_the_fingerprint_read_writes_the_text() {
    // GIVEN a runtime on a file
    let (_dir, path, state) = runtime_on("# mine\n[api]\nport = 7771\n", "");
    let (_, read) = call(state.clone(), "GET", "/api/v1/config", None).await;

    // WHEN a client saves a new text against the fingerprint it read
    let text = "# mine\n[api]\nport = 7773\n";
    let (status, body) = call(
        state,
        "PUT",
        "/api/v1/config",
        Some(json!({ "text": text, "sha256": read["sha256"] })),
    )
    .await;

    // THEN the file holds the text and the answer carries its new fingerprint
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
    assert_eq!(body["sha256"], fingerprint(text));
}

#[tokio::test]
async fn test_saving_over_a_file_changed_since_it_was_read_is_a_conflict() {
    // GIVEN a file read by a client, then changed by another surface
    let (_dir, path, state) = runtime_on("[api]\nport = 7771\n", "");
    let (_, read) = call(state.clone(), "GET", "/api/v1/config", None).await;
    std::fs::write(&path, "[api]\nport = 7773\n").unwrap();

    // WHEN the client saves against its reading
    let (status, body) = call(
        state,
        "PUT",
        "/api/v1/config",
        Some(json!({ "text": "[api]\nport = 7773\n", "sha256": read["sha256"] })),
    )
    .await;

    // THEN it is refused as modified, and the other change stays
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "modified");
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "[api]\nport = 7773\n"
    );
}

#[tokio::test]
async fn test_saving_an_invalid_text_or_a_locked_value_is_refused_with_the_writer_sentence() {
    // GIVEN a runtime under a policy that fixes the developer mode off
    let (_dir, path, state) = runtime_on("", "\"desktop.developer_mode\" = false");
    let sha = fingerprint("");

    // WHEN a client saves an invalid text, then a text that turns the mode on
    let (invalid_status, invalid) = call(
        state.clone(),
        "PUT",
        "/api/v1/config",
        Some(json!({ "text": "[api]\ntypo = 1\n", "sha256": sha })),
    )
    .await;
    let (locked_status, locked) = call(
        state,
        "PUT",
        "/api/v1/config",
        Some(json!({ "text": "[desktop]\ndeveloper_mode = true\n", "sha256": sha })),
    )
    .await;

    // THEN both are refused, the second at line 2 with the sentence the writer
    // gives every surface, and nothing is written
    assert_eq!(
        invalid_status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "{invalid}"
    );
    assert_eq!(invalid["code"], "invalid");
    assert_eq!(invalid["diagnostics"][0]["line"], 2);
    assert_eq!(locked_status, StatusCode::FORBIDDEN, "{locked}");
    assert_eq!(locked["code"], "locked");
    assert_eq!(locked["diagnostics"][0]["line"], 2);
    let policy = PolicySettings::none().impose("desktop.developer_mode", json!(false));
    let direct = update_config(
        &path,
        ConfigEdit::Text("[desktop]\ndeveloper_mode = true\n".to_owned()),
        &policy,
    )
    .unwrap_err();
    assert_eq!(locked["error"], direct.to_string());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "");
}

#[tokio::test]
async fn test_the_default_text_keeps_the_locked_keys_at_their_value() {
    // GIVEN a runtime under a policy that fixes the developer mode on
    let (_dir, _path, state) = runtime_on("", "\"desktop.developer_mode\" = true");

    // WHEN a client asks for the default text
    let (status, body) = call(state, "GET", "/api/v1/config/default", None).await;

    // THEN it is the documented default, the locked key written at its value
    assert_eq!(status, StatusCode::OK, "{body}");
    let text = body["text"].as_str().unwrap();
    assert!(text.starts_with("# apollia.toml, the configuration of Apollia OS."));
    assert!(text.contains("developer_mode = true"), "{text}");
    assert!(apollia_core::config::validate(text).is_ok());
}

#[tokio::test]
async fn test_a_setting_written_by_a_key_edit_is_in_the_text_the_route_reads() {
    // GIVEN a runtime on a file with a comment
    let (_dir, path, state) = runtime_on("# mine\n", "");

    // WHEN a surface writes a named setting through the writer
    update_config(
        &path,
        ConfigEdit::Keys(vec![KeyChange::set("desktop.developer_mode", json!(true))]),
        &PolicySettings::none(),
    )
    .unwrap();

    // THEN the route reads it, next to the comment
    let (_, body) = call(state, "GET", "/api/v1/config", None).await;
    let text = body["text"].as_str().unwrap();
    assert!(text.contains("# mine"), "{text}");
    assert!(text.contains("developer_mode = true"), "{text}");
}
