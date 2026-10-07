use serde_json::json;
use wiremock::matchers::{body_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;

const TOKEN: &str = "token-of-this-installation";

/// A daemon answering `status` and `body` to `verb route`, only with the token.
async fn daemon(verb: &str, route: &str, status: u16, body: serde_json::Value) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method(verb))
        .and(path(route))
        .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
        .respond_with(ResponseTemplate::new(status).set_body_json(body))
        .expect(1)
        .mount(&server)
        .await;
    server
}

/// A daemon refusing `verb route` with `status` and its own `message`.
async fn refusing(verb: &str, route: &str, status: u16, message: &str) -> MockServer {
    daemon(verb, route, status, json!({ "error": message })).await
}

fn client(server: &MockServer) -> AttachedClient {
    AttachedClient::new(server.uri(), TOKEN).unwrap()
}

fn accept() -> AuthorizeBody {
    AuthorizeBody {
        message_id: "a-1".to_owned(),
        tool_call_id: "call-1".to_owned(),
        tool_name: "bash_executor".to_owned(),
        decision: "accept".to_owned(),
        reason: None,
    }
}

#[tokio::test]
async fn test_an_answer_is_regenerated_and_an_edited_message_resent_on_the_daemon() {
    // GIVEN a daemon that runs a turn again, and one that replaces a user
    // message
    let processing = json!({ "status": "processing" });
    let regenerating = daemon(
        "POST",
        "/api/v1/sessions/s%201/messages/a-1/regenerate",
        202,
        processing,
    )
    .await;
    let resending = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/sessions/s-1/messages/u-1/resend"))
        .and(body_json(json!({ "content": "Bonsoir" })))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({ "message_id": "u-2" })))
        .expect(1)
        .mount(&resending)
        .await;

    // WHEN the attached arm regenerates (the session id encoded), and edits
    let regenerated = regenerate(&client(&regenerating), "s 1", "a-1").await;
    let new_id = edit_and_resend(&client(&resending), "s-1", "u-1", "Bonsoir").await;

    // THEN both reached the daemon, and the new user message id comes back
    assert!(regenerated.is_ok(), "{regenerated:?}");
    assert_eq!(new_id.unwrap(), "u-2");
}

#[tokio::test]
async fn test_a_refusal_is_posted_with_its_reason() {
    // GIVEN a daemon waiting for a decision on one tool call
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/sessions/s-1/authorize"))
        .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
        .and(body_json(json!({
            "message_id": "a-1", "tool_call_id": "call-1", "tool_name": "bash_executor",
            "decision": "refuse", "reason": "write a copy instead",
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "status": "resolved" })))
        .expect(1)
        .mount(&server)
        .await;
    let body = AuthorizeBody {
        decision: "refuse".to_owned(),
        reason: Some("write a copy instead".to_owned()),
        ..accept()
    };

    // WHEN the attached arm authorizes the tool
    let result = authorize(&client(&server), "s-1", &body).await;

    // THEN the daemon received the decision as its route reads it
    assert!(result.is_ok(), "{result:?}");
}

#[tokio::test]
async fn test_only_the_skills_of_worker_agents_are_listed_as_the_embedded_command_does() {
    // GIVEN a daemon listing one skill, one agent that takes a message and
    // one workflow
    let listing = |id: &str, kind: &str| {
        json!({ "skill_id": id, "agent_name": "veille", "skill_name": id,
                "description": "", "input_schema": null, "kind": kind })
    };
    let skills = json!({ "skills": [
        listing("digest", "skill"),
        listing("agent:veille", "agent"),
        listing("workflow:p/w", "workflow"),
    ] });
    let server = daemon("GET", "/api/v1/a2a/skills", 200, skills).await;

    // WHEN the attached arm lists the skills
    let skills = a2a_skills(&client(&server)).await.unwrap();

    // THEN only the skill a worker declares remains
    assert_eq!(skills.len(), 1);
    assert_eq!(skills[0].skill_id, "digest");
}

#[tokio::test]
async fn test_a_refusal_of_the_daemon_is_shown_in_its_own_words() {
    // GIVEN daemons that refuse each call with their own message
    let busy = "session busy: s-1";
    let not_user = "internal error: can only edit a user message";
    let not_waiting = "session not awaiting approval";
    let no_a2a = "A2A invoker not initialized";
    let base = "/api/v1/sessions/s-1";
    let a = refusing(
        "POST",
        &format!("{base}/messages/a-1/regenerate"),
        409,
        busy,
    )
    .await;
    let b = refusing(
        "POST",
        &format!("{base}/messages/a-1/resend"),
        500,
        not_user,
    )
    .await;
    let c = refusing("POST", &format!("{base}/authorize"), 409, not_waiting).await;
    let d = refusing("GET", "/api/v1/a2a/skills", 503, no_a2a).await;

    // WHEN each command's attached arm is called
    let shown: [String; 4] = [
        regenerate(&client(&a), "s-1", "a-1")
            .await
            .unwrap_err()
            .into(),
        edit_and_resend(&client(&b), "s-1", "a-1", "x")
            .await
            .unwrap_err()
            .into(),
        authorize(&client(&c), "s-1", &accept())
            .await
            .unwrap_err()
            .into(),
        a2a_skills(&client(&d)).await.unwrap_err().into(),
    ];

    // THEN the interface shows the daemon's words, never an unavailability
    assert_eq!(shown, [busy, not_user, not_waiting, no_a2a]);
}
