//! The Observability commands attached to a daemon: the timeline, the tool
//! audit and the mailbox each reach their route with the token of this
//! installation, and a refusal of the daemon reaches the interface in the
//! daemon's own words.

use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::audit::audit_trail_on;
use super::mailbox::mailbox_on;
use super::timeline_on;
use crate::attached::{gate, AttachedClient, LinkMode, RuntimeLink};

const TOKEN: &str = "token-of-this-installation";

fn attached(server: &MockServer) -> RuntimeLink {
    RuntimeLink::Attached(AttachedClient::new(server.uri(), TOKEN).unwrap())
}

/// Mounts `GET route?key=value` answering `body`, only with the token.
async fn answer(server: &MockServer, route: &str, query: (&str, &str), body: serde_json::Value) {
    Mock::given(method("GET"))
        .and(path(route))
        .and(query_param(query.0, query.1))
        .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .expect(1)
        .mount(server)
        .await;
}

#[tokio::test]
async fn test_the_timeline_is_read_from_the_daemon_with_the_token() {
    // GIVEN a daemon holding one tool call in the last hour
    let server = MockServer::start().await;
    answer(
        &server,
        "/api/v1/observability/timeline",
        ("window_minutes", "60"),
        serde_json::json!([{
            "event_type": "tool", "timestamp": "2026-10-03T09:00:00Z",
            "summary": "[scribe] Tool: file_io", "detail": { "id": "inv-1" },
        }]),
    )
    .await;

    // WHEN the attached arm reads the timeline of the last hour
    let events = timeline_on(&attached(&server), 60).await.unwrap();

    // THEN the daemon's event comes back as the view draws it
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].summary, "[scribe] Tool: file_io");
    assert_eq!(events[0].detail["id"], "inv-1");
}

#[tokio::test]
async fn test_the_mailbox_is_read_from_the_daemon_with_the_token() {
    // GIVEN a daemon holding one message between two agents
    let server = MockServer::start().await;
    answer(
        &server,
        "/api/v1/mailbox/messages",
        ("limit", "100"),
        serde_json::json!([{
            "message_id": "m-1", "from_agent": "apollia-guide", "to_agent": "scribe",
            "payload": "{\"kind\":\"ping\"}", "sent_at": "2026-10-03T09:00:00Z",
            "state": "pending",
        }]),
    )
    .await;

    // WHEN the attached arm lists the mailbox
    let rows = mailbox_on(&attached(&server), 100).await.unwrap();

    // THEN the message comes back with its payload as stored
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].from_agent, "apollia-guide");
    assert_eq!(rows[0].payload, r#"{"kind":"ping"}"#);
}

#[tokio::test]
async fn test_the_tool_audit_is_read_and_its_agents_named_by_the_daemon() {
    // GIVEN a daemon whose audit holds two calls, one by a registered agent
    // and one by an agent since removed
    let server = MockServer::start().await;
    answer(
        &server,
        "/api/v1/audit",
        ("limit", "25"),
        serde_json::json!({ "count": 2, "events": [
            { "id": "inv-1", "agent_id": "a-1", "tool_name": "file_io",
              "started_at": "2026-10-03T09:00:00Z", "duration_ms": 42, "exit_code": 0 },
            { "id": "inv-2", "agent_id": "a-gone", "tool_name": "shell",
              "started_at": "2026-10-03T08:00:00Z" },
        ]}),
    )
    .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/agents"))
        .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "agents": [{ "agent_id": "a-1", "name": "standup-scribe", "state": "active",
                         "supports_a2a": false, "skills": [] }],
        })))
        .expect(1)
        .mount(&server)
        .await;

    // WHEN the attached arm reads the audit trail
    let entries = audit_trail_on(&attached(&server), 25).await.unwrap();

    // THEN the registered agent is named, the removed one keeps its id
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].agent_name, "standup-scribe");
    assert_eq!(entries[0].duration_ms, Some(42));
    assert_eq!(entries[1].agent_name, "a-gone");
}

#[tokio::test]
async fn test_a_refusal_of_the_daemon_is_shown_in_its_own_words() {
    // GIVEN a daemon refusing each of the three reads with its own message
    let server = MockServer::start().await;
    for (route, message) in [
        (
            "/api/v1/observability/timeline",
            "window_minutes must be between 1 and 10080",
        ),
        ("/api/v1/mailbox/messages", "limit must be at least 1"),
        ("/api/v1/audit", "audit trail not available"),
    ] {
        Mock::given(method("GET"))
            .and(path(route))
            .respond_with(
                ResponseTemplate::new(400).set_body_json(serde_json::json!({ "error": message })),
            )
            .mount(&server)
            .await;
    }
    let link = attached(&server);

    // WHEN the attached arm calls each of them
    let timeline = timeline_on(&link, 0).await.unwrap_err();
    let mailbox = mailbox_on(&link, 0).await.unwrap_err();
    let audit = audit_trail_on(&link, 50).await.unwrap_err();

    // THEN each error is the daemon's message, unchanged
    assert_eq!(timeline, "window_minutes must be between 1 and 10080");
    assert_eq!(mailbox, "limit must be at least 1");
    assert_eq!(audit, "audit trail not available");
}

#[test]
fn test_the_three_observability_commands_pass_the_gate_attached() {
    // GIVEN the commands of the Observability view this change routes
    let commands = [
        "get_global_timeline",
        "get_tool_audit_trail",
        "list_mailbox_messages",
    ];

    // WHEN each is gated while attached
    let refused: Vec<&str> = commands
        .into_iter()
        .filter(|c| gate(LinkMode::Attached, c).is_err())
        .collect();

    // THEN none is refused
    assert!(refused.is_empty(), "refused: {refused:?}");
}
