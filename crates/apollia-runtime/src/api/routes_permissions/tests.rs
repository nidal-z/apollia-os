use std::sync::Arc;

use apollia_core::StepBudgetConfig;
use apollia_llm::LlmRouter;
use apollia_mcp::session::LoadingMode;
use apollia_tools::ToolRegistryHandle;
use axum::http::StatusCode;
use serde_json::json;
use tempfile::TempDir;

use crate::api::server::AppState;
use crate::api::test_support::{app_state, call, MockBackend, StemLoader};
use crate::chat::manager::ChatSessionManagerHandle;
use crate::chat::{AlwaysAcceptScope, FsHitlDecision};
use crate::permission_rules::seed_proposals;

fn state(dir: &TempDir) -> AppState<MockBackend> {
    app_state(
        Arc::new(StemLoader { refusal: None }),
        dir.path().to_path_buf(),
    )
}

fn with_chat(dir: &TempDir) -> (AppState<MockBackend>, ChatSessionManagerHandle) {
    let mut state = state(dir);
    let chat = ChatSessionManagerHandle::spawn(
        &dir.path().join("chat.db"),
        Some(Arc::new(LlmRouter::empty())),
        ToolRegistryHandle::start(),
        Arc::new(StemLoader { refusal: None }),
        None,
        state.event_sender.clone(),
        StepBudgetConfig::default(),
        None,
        state.registry_handle.clone(),
        None,
        None,
        None,
        None,
        None,
        LoadingMode::Eager,
        20,
        None,
        false,
    )
    .unwrap();
    state.chat_manager = Some(chat.clone());
    (state, chat)
}

#[tokio::test]
async fn test_the_proposals_are_listed_and_one_applied_over_http() {
    // GIVEN a daemon whose onboarding agent proposed two rules
    let dir = TempDir::new().unwrap();
    seed_proposals(
        dir.path(),
        &json!([
            {"tool_name": "file_read", "action": "allow", "scope": "global"},
            {"tool_name": "http_fetch", "action": "deny", "arg_prefix": "http://"}
        ]),
    );
    let state = state(&dir);

    // WHEN a client lists them, applies the second, then lists again
    let (listed, before) = call(state.clone(), "GET", "/api/v1/permissions/proposals", None).await;
    let (applied, rule) = call(
        state.clone(),
        "POST",
        "/api/v1/permissions/proposals/1/apply",
        None,
    )
    .await;
    let (_, after) = call(state, "GET", "/api/v1/permissions/proposals", None).await;

    // THEN both were listed as the card draws them, the rule has an id, and
    // the first proposal alone remains
    assert_eq!(listed, StatusCode::OK, "{before}");
    assert_eq!(before.as_array().unwrap().len(), 2);
    assert_eq!(before[1]["tool_name"], "http_fetch");
    assert_eq!(before[1]["arg_prefix"], "http://");
    assert_eq!(before[1]["scope"], "global");
    assert_eq!(applied, StatusCode::OK, "{rule}");
    assert!(rule["rule_id"].as_i64().unwrap() > 0);
    assert_eq!(after.as_array().unwrap().len(), 1);
    assert_eq!(after[0]["tool_name"], "file_read");
}

#[tokio::test]
async fn test_applying_past_the_end_is_refused_with_the_embedded_words() {
    // GIVEN one proposal
    let dir = TempDir::new().unwrap();
    seed_proposals(
        dir.path(),
        &json!([{"tool_name": "file_read", "action": "allow"}]),
    );

    // WHEN a client applies the proposal at cursor 3
    let (status, body) = call(
        state(&dir),
        "POST",
        "/api/v1/permissions/proposals/3/apply",
        None,
    )
    .await;

    // THEN it is a 404 that says why, as the embedded desktop says it
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"], "proposal index 3 out of range (len=1)");
}

#[tokio::test]
async fn test_a_proposal_is_dismissed_over_http_and_a_second_dismissal_refused() {
    // GIVEN one proposal
    let dir = TempDir::new().unwrap();
    seed_proposals(
        dir.path(),
        &json!([{"tool_name": "file_read", "action": "allow"}]),
    );

    // WHEN a client dismisses it twice
    let (first, gone) = call(
        state(&dir),
        "DELETE",
        "/api/v1/permissions/proposals/0",
        None,
    )
    .await;
    let (second, refusal) = call(
        state(&dir),
        "DELETE",
        "/api/v1/permissions/proposals/0",
        None,
    )
    .await;

    // THEN the first removes it, the second finds nothing left
    assert_eq!(first, StatusCode::OK, "{gone}");
    assert_eq!(gone["dismissed"], 0);
    assert_eq!(second, StatusCode::NOT_FOUND);
    assert_eq!(refusal["error"], "no proposed rules in memory");
}

#[tokio::test]
async fn test_a_global_rule_is_added_over_http() {
    // GIVEN a daemon with no rule yet
    let dir = TempDir::new().unwrap();

    // WHEN a client adds a rule for every agent
    let (status, body) = call(
        state(&dir),
        "POST",
        "/api/v1/permissions/rules",
        Some(json!({
            "tool_name": "file_write",
            "arg_prefix": "/tmp/",
            "action": "allow",
            "scope": "global"
        })),
    )
    .await;

    // THEN the rule is stored and its id answered
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body["rule_id"].as_i64().unwrap() > 0);
}

#[tokio::test]
async fn test_a_rule_scoped_to_a_project_is_refused_over_http() {
    // GIVEN a daemon with no rule yet
    let dir = TempDir::new().unwrap();

    // WHEN a client adds a rule scoped to a project
    let (status, body) = call(
        state(&dir),
        "POST",
        "/api/v1/permissions/rules",
        Some(json!({"tool_name": "file_write", "action": "allow", "scope": "project"})),
    )
    .await;

    // THEN it is a 400 with the embedded words, and nothing was stored
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        body["error"],
        "unknown scope 'project', expected 'global' (a rule targets one agent or every agent)"
    );
    assert!(!dir
        .path()
        .join(apollia_tools::governance_db::GOVERNANCE_DB_FILENAME)
        .exists());
}

#[tokio::test]
async fn test_a_pending_file_request_is_answered_over_http() {
    // GIVEN a turn waiting on a request to write a file
    let dir = TempDir::new().unwrap();
    let (state, chat) = with_chat(&dir);
    let waiting = chat.pending_fs_approvals().register("req-1".to_owned());

    // WHEN a client answers "always allow" for the project
    let (status, body) = call(
        state,
        "POST",
        "/api/v1/approvals/filesystem/req-1",
        Some(json!({
            "decision": "always_allow",
            "op": "write",
            "level": "high",
            "scope": "this_project"
        })),
    )
    .await;

    // THEN the route answers and the turn receives that decision
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["request_id"], "req-1");
    match waiting.await.unwrap() {
        FsHitlDecision::AlwaysAllow { scope, op, level } => {
            assert_eq!(scope, AlwaysAcceptScope::ThisProject);
            assert_eq!(op, "write");
            assert_eq!(level, "high");
        }
        other => panic!("expected AlwaysAllow, got {other:?}"),
    }
}

#[tokio::test]
async fn test_an_answer_to_an_unknown_file_request_is_refused_over_http() {
    // GIVEN a chat with no pending file request
    let dir = TempDir::new().unwrap();
    let (state, _chat) = with_chat(&dir);

    // WHEN a client answers a request id nobody waits on
    let (status, body) = call(
        state,
        "POST",
        "/api/v1/approvals/filesystem/gone",
        Some(json!({"decision": "deny", "reason": "not safe"})),
    )
    .await;

    // THEN it is a 404 naming the request
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(body["error"].as_str().unwrap().contains("'gone'"), "{body}");
}

#[tokio::test]
async fn test_an_answer_without_a_chat_is_unavailable() {
    // GIVEN a daemon whose chat subsystem did not start
    let dir = TempDir::new().unwrap();

    // WHEN a client answers a file request
    let (status, body) = call(
        state(&dir),
        "POST",
        "/api/v1/approvals/filesystem/req-1",
        Some(json!({"decision": "approve"})),
    )
    .await;

    // THEN it is a 503 with the embedded words
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["error"], "chat subsystem not available");
}

#[test]
fn test_the_legacy_session_answer_reads_as_always_allow_for_the_session() {
    // GIVEN the discriminant older frontends still send
    let body: super::FsHitlDecisionBody = serde_json::from_value(
        json!({"decision": "always_allow_session", "op": "delete", "level": "medium"}),
    )
    .unwrap();

    // WHEN it becomes the decision the chat manager resolves
    let decision = body.into_decision();

    // THEN it is an always-allow for this session
    assert!(matches!(
        decision,
        FsHitlDecision::AlwaysAllow {
            scope: AlwaysAcceptScope::ThisSession,
            ..
        }
    ));
}
