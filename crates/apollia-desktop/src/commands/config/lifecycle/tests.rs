use serde_json::json;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;
use crate::attached::{gate, LinkMode};

const TOKEN: &str = "token-of-this-installation";
const BEARER: &str = "Bearer token-of-this-installation";

/// A daemon answering `POST route` with `answer`, once, only with the token.
async fn daemon_answering(route: &str, answer: serde_json::Value) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(route))
        .and(header("authorization", BEARER))
        .respond_with(ResponseTemplate::new(200).set_body_json(answer))
        .expect(1)
        .mount(&server)
        .await;
    server
}

#[tokio::test]
async fn test_the_onboarding_is_marked_and_reset_on_the_daemon() {
    // GIVEN a daemon that marks and resets the onboarding
    let marked =
        daemon_answering("/api/v1/onboarding/complete", json!({ "onboarded": true })).await;
    let reset = daemon_answering(
        "/api/v1/onboarding/reset",
        json!({ "removed_profile_entries": 4 }),
    )
    .await;

    // WHEN the attached arms mark it, then reset it
    mark_onboarded_on(&AttachedClient::new(marked.uri(), TOKEN).unwrap())
        .await
        .unwrap();
    let removed = reset_onboarding_on(&AttachedClient::new(reset.uri(), TOKEN).unwrap())
        .await
        .unwrap();

    // THEN each call reached its route with the token, and the count comes
    // back
    assert_eq!(removed, 4);
}

#[tokio::test]
async fn test_every_memory_is_wiped_on_the_daemon() {
    // GIVEN a daemon that wipes three databases
    let server = daemon_answering("/api/v1/memory/wipe", json!({ "removed_databases": 3 })).await;
    let client = AttachedClient::new(server.uri(), TOKEN).unwrap();

    // WHEN the attached arm wipes every memory
    let removed = clear_all_memories_on(&client).await.unwrap();

    // THEN the daemon's count comes back
    assert_eq!(removed, 3);
}

#[tokio::test]
async fn test_the_logs_and_the_factory_reset_reach_the_daemon() {
    // GIVEN a daemon that clears its logs and resets its data directory
    let logs = daemon_answering("/api/v1/logs/clear", json!({ "removed": true })).await;
    let reset = daemon_answering("/api/v1/factory-reset", json!({ "removed": true })).await;

    // WHEN the attached arms clear the logs, then reset the installation
    let cleared = clear_logs_on(&AttachedClient::new(logs.uri(), TOKEN).unwrap()).await;
    let wiped = factory_reset_on(&AttachedClient::new(reset.uri(), TOKEN).unwrap()).await;

    // THEN both complete; the mocks verify method, route and token on drop
    assert!(cleared.is_ok(), "{cleared:?}");
    assert!(wiped.is_ok(), "{wiped:?}");
}

#[tokio::test]
async fn test_a_refusal_of_the_daemon_is_shown_as_it_wrote_it_for_every_command() {
    // GIVEN a daemon refusing every lifecycle route with its own message
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(503)
                .set_body_json(json!({ "error": "user memory repository not initialized" })),
        )
        .mount(&server)
        .await;
    let client = AttachedClient::new(server.uri(), TOKEN).unwrap();

    // WHEN each attached call reaches it
    let refusals: Vec<String> = vec![
        mark_onboarded_on(&client).await.unwrap_err().into(),
        reset_onboarding_on(&client).await.unwrap_err().into(),
        clear_all_memories_on(&client).await.unwrap_err().into(),
        clear_logs_on(&client).await.unwrap_err().into(),
        factory_reset_on(&client).await.unwrap_err().into(),
    ];

    // THEN the interface shows the daemon's words, unchanged
    for refusal in refusals {
        assert_eq!(refusal, "user memory repository not initialized");
    }
}

#[test]
fn test_the_five_lifecycle_commands_pass_the_gate_attached() {
    // GIVEN the application attached to a daemon
    // WHEN the five reset commands are gated
    // THEN each passes
    for command in [
        "clear_all_memories",
        "clear_logs",
        "factory_reset",
        "mark_onboarded",
        "reset_onboarding",
    ] {
        assert!(gate(LinkMode::Attached, command).is_ok(), "{command}");
    }
}
