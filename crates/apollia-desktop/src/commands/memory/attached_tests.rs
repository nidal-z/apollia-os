//! The memory commands, read side, attached to a daemon: each reaches its
//! route with the token of this installation, and a refusal of the daemon
//! reaches the interface in the daemon's own words.

use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;
use crate::attached::AttachedClient;
use crate::commands::user_memory::stats_on;

const TOKEN: &str = "token-of-this-installation";

/// A daemon answering `GET route` with `body`, only with the token.
async fn daemon_answering(route: &str, body: serde_json::Value) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(route))
        .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .expect(1)
        .mount(&server)
        .await;
    server
}

fn attached(server: &MockServer) -> RuntimeLink {
    RuntimeLink::Attached(AttachedClient::new(server.uri(), TOKEN).unwrap())
}

#[tokio::test]
async fn test_the_namespaces_are_read_from_the_daemon_with_the_token() {
    // GIVEN a daemon holding two namespaces
    let server = daemon_answering(
        "/api/v1/memory/namespaces",
        serde_json::json!(["agent-x", "agent-y"]),
    )
    .await;

    // WHEN the attached arm lists them
    let namespaces = namespaces_on(&attached(&server)).await.unwrap();

    // THEN the daemon's list comes back
    assert_eq!(namespaces, ["agent-x", "agent-y"]);
}

#[tokio::test]
async fn test_the_entries_of_a_namespace_are_read_by_its_encoded_name() {
    // GIVEN a daemon holding one entry in a namespace whose name has a space
    let server = daemon_answering(
        "/api/v1/memory/namespaces/agent%20x/entries",
        serde_json::json!([{
            "id": "e-1", "entry_type": "episodic", "key": "agent-x",
            "value": "Quote sent to Dupont", "created_at": "2026-10-03T00:00:00Z",
            "expires_at": null, "score": null,
        }]),
    )
    .await;

    // WHEN the attached arm lists the entries
    let entries = entries_on(&attached(&server), "agent x".to_owned())
        .await
        .unwrap();

    // THEN the entry comes back as the Memory screen draws it
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].value, "Quote sent to Dupont");
}

#[tokio::test]
async fn test_a_search_sends_the_query_to_the_daemon() {
    // GIVEN a daemon answering a search for "budget client"
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/memory/namespaces/agent-x/search"))
        .and(query_param("q", "budget client"))
        .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!([{
                "id": "s-1", "entry_type": "semantic", "content": "budget 15000",
                "score": 2.5, "relevance": 0.9, "created_at": "2026-10-03T00:00:00Z",
            }])),
        )
        .expect(1)
        .mount(&server)
        .await;

    // WHEN the attached arm searches
    let results = search_on(
        &attached(&server),
        "agent-x".to_owned(),
        "budget client".to_owned(),
    )
    .await
    .unwrap();

    // THEN the daemon's result comes back
    assert_eq!(results[0].id, "s-1");
    assert_eq!(results[0].score, 2.5);
}

#[tokio::test]
async fn test_the_entries_recalled_during_a_turn_are_read_from_the_daemon() {
    // GIVEN a daemon whose agent recalled one entry during a turn
    let server = daemon_answering(
        "/api/v1/memory/injections/turn-1",
        serde_json::json!([{
            "id": "sem-1", "content_preview": "budget 15000", "namespace": "agent-x",
            "injection_reason": "Matched query: budget", "relevance_score": 0.5,
        }]),
    )
    .await;

    // WHEN the attached arm reads that turn
    let entries = injected_on(&attached(&server), "turn-1".to_owned())
        .await
        .unwrap();

    // THEN the entry comes back with its reason
    assert_eq!(entries[0].injection_reason, "Matched query: budget");
}

#[tokio::test]
async fn test_the_stats_of_a_conversation_are_read_from_the_daemon() {
    // GIVEN a daemon holding the statistics of one conversation
    let server = daemon_answering(
        "/api/v1/sessions/s-1/stats",
        serde_json::json!({
            "message_count": 4, "summarized_count": 0, "context_usage_pct": 12.5,
            "user_memory_injected": true, "cross_sessions_referenced": 1,
        }),
    )
    .await;

    // WHEN the attached arm reads them
    let stats = stats_on(&attached(&server), "s-1".to_owned())
        .await
        .unwrap();

    // THEN they come back as the gauge draws them
    assert_eq!(stats.message_count, 4);
    assert!(stats.user_memory_injected);
}

#[tokio::test]
async fn test_a_refusal_of_the_daemon_is_shown_in_its_own_words() {
    // GIVEN a daemon refusing a namespace that names a path, and a session it
    // does not know
    let server = MockServer::start().await;
    for (route, status, words) in [
        (
            "/api/v1/memory/namespaces/..%2Fx/entries",
            400,
            "VALIDATION_ERROR: invalid memory namespace '../x'",
        ),
        (
            "/api/v1/sessions/ghost/stats",
            404,
            "NOT_FOUND: Chat session not found: ghost",
        ),
    ] {
        Mock::given(method("GET"))
            .and(path(route))
            .respond_with(
                ResponseTemplate::new(status).set_body_json(serde_json::json!({ "error": words })),
            )
            .mount(&server)
            .await;
    }
    let link = attached(&server);

    // WHEN the attached arm asks for each
    let entries = entries_on(&link, "../x".to_owned()).await.unwrap_err();
    let stats = stats_on(&link, "ghost".to_owned()).await.unwrap_err();

    // THEN the interface reads the daemon's words, not an unavailability
    assert_eq!(entries, "VALIDATION_ERROR: invalid memory namespace '../x'");
    assert_eq!(stats, "NOT_FOUND: Chat session not found: ghost");
}

#[test]
fn test_the_five_read_commands_pass_the_gate_attached() {
    // GIVEN the commands the Memory screen and the chat gauge open with
    let commands = [
        "list_memory_namespaces",
        "list_memory_entries",
        "search_memory",
        "get_injected_memory_entries",
        "get_conversation_stats",
    ];

    // WHEN each is gated while attached
    let refused: Vec<&str> = commands
        .into_iter()
        .filter(|c| crate::attached::gate(crate::attached::LinkMode::Attached, c).is_err())
        .collect();

    // THEN none answers that the service makes it unavailable
    assert!(refused.is_empty(), "refused: {refused:?}");
}
