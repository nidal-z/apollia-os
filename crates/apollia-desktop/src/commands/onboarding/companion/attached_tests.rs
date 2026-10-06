use serde_json::json;
use wiremock::matchers::{body_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;
use crate::attached::{gate, LinkMode};

const TOKEN: &str = "token-of-this-installation";
const BEARER: &str = "Bearer token-of-this-installation";

#[tokio::test]
async fn test_the_preference_is_read_and_put_on_the_daemon_with_the_token() {
    // GIVEN a daemon holding the companion on, accepting a write, answering
    // only the token of this installation
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/companion/enabled"))
        .and(header("authorization", BEARER))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "enabled": true })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/api/v1/companion/enabled"))
        .and(header("authorization", BEARER))
        .and(body_json(json!({ "enabled": false })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "enabled": false })))
        .expect(1)
        .mount(&server)
        .await;
    let client = AttachedClient::new(server.uri(), TOKEN).unwrap();

    // WHEN the attached arm reads the preference, then turns it off
    let enabled = enabled_on(&client).await.unwrap();
    set_enabled_on(&client, false).await.unwrap();

    // THEN the daemon's value comes back and each route was reached once
    assert!(enabled);
}

#[tokio::test]
async fn test_a_guide_session_is_opened_on_the_daemon_with_the_route_hint() {
    // GIVEN a daemon opening guide sessions
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/companion/sessions"))
        .and(header("authorization", BEARER))
        .and(body_json(json!({ "context": "agents" })))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({ "session_id": "g-1" })))
        .expect(1)
        .mount(&server)
        .await;
    let client = AttachedClient::new(server.uri(), TOKEN).unwrap();

    // WHEN the attached arm opens one from the agents page
    let session = create_session_on(&client, Some("agents".to_owned()))
        .await
        .unwrap();

    // THEN the daemon's session identifier is what the panel receives
    assert_eq!(session.session_id, "g-1");
}

#[tokio::test]
async fn test_a_refusal_of_the_daemon_reaches_the_panel_as_its_own_words() {
    // GIVEN a daemon without the guide, and whose user memory did not open
    let server = MockServer::start().await;
    let guide = "apollia-guide agent not found - it powers the companion panel and is provisioned at startup, bundle it and restart the application";
    Mock::given(method("POST"))
        .and(path("/api/v1/companion/sessions"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({ "error": guide })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/companion/enabled"))
        .respond_with(
            ResponseTemplate::new(503)
                .set_body_json(json!({ "error": "UserMemory database not initialized" })),
        )
        .mount(&server)
        .await;
    let client = AttachedClient::new(server.uri(), TOKEN).unwrap();

    // WHEN the attached arm opens a session and reads the preference
    let session = create_session_on(&client, None).await.unwrap_err();
    let enabled = enabled_on(&client).await.unwrap_err();

    // THEN each error displays the daemon's message as it is, which the panel
    // classifies as it did when the runtime ran in this process
    assert_eq!(String::from(session), guide);
    assert_eq!(String::from(enabled), "UserMemory database not initialized");
}

#[test]
fn test_the_three_companion_commands_pass_the_gate_attached() {
    // GIVEN the application attached to a daemon
    // WHEN each command of the companion is gated
    // THEN none is refused, and none is a window exception
    for command in [
        "create_companion_session",
        "get_companion_enabled",
        "set_companion_enabled",
    ] {
        assert!(gate(LinkMode::Attached, command).is_ok(), "{command}");
        assert!(
            !crate::attached::WINDOW_COMMANDS
                .iter()
                .any(|(name, _)| *name == command),
            "{command}"
        );
    }
}
