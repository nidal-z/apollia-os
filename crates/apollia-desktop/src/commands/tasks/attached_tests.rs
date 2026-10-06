//! The delegation tree of a task, attached to a daemon: it reaches its route
//! with the token of this installation, and a refusal of the daemon reaches
//! the interface in the daemon's own words.

use serde_json::{json, Value};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;
use crate::attached::{gate, AttachedClient, LinkMode};

const TOKEN: &str = "token-of-this-installation";

/// A daemon answering `verb route` once with `status` and `body`, only when
/// the token of this installation comes with it.
async fn daemon(verb: &str, route: &str, status: u16, body: Value) -> (MockServer, RuntimeLink) {
    let server = MockServer::start().await;
    Mock::given(method(verb))
        .and(path(route))
        .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
        .respond_with(ResponseTemplate::new(status).set_body_json(body))
        .expect(1)
        .mount(&server)
        .await;
    let link = RuntimeLink::Attached(AttachedClient::new(server.uri(), TOKEN).unwrap());
    (server, link)
}

#[test]
fn test_the_tree_passes_the_gate_attached() {
    // GIVEN the delegation tree command
    // WHEN it is gated while attached
    let gated = gate(LinkMode::Attached, "get_delegation_tree");

    // THEN it does not answer that the service makes it unavailable
    assert!(gated.is_ok());
}

#[tokio::test]
async fn test_the_tree_is_read_from_the_daemon_with_the_token() {
    // GIVEN a daemon holding the task t-1 of veille, which delegated once
    let tree = json!({
        "agent_id": "a-1",
        "agent_name": "veille",
        "status": "completed",
        "started_at": "2026-10-03T10:00:00Z",
        "children": [{
            "agent_id": "recherche",
            "agent_name": "recherche",
            "status": "completed",
            "started_at": "2026-10-03T10:00:01Z",
            "children": [],
        }],
    });
    let (_server, link) = daemon("GET", "/api/v1/tasks/t-1/delegation-tree", 200, tree).await;

    // WHEN the attached arm reads its tree
    let node = delegation_tree_on(&link, "t-1").await.unwrap();

    // THEN the root and its delegation are those of the daemon
    assert_eq!(node.agent_name, "veille");
    assert_eq!(node.children.len(), 1);
    assert_eq!(node.children[0].agent_name, "recherche");
}

#[tokio::test]
async fn test_a_refusal_of_the_daemon_is_shown_as_it_is() {
    // GIVEN a daemon refusing the tree with the message its route answers
    let message = "sidechain logging not initialized";
    let (_server, link) = daemon(
        "GET",
        "/api/v1/tasks/t-1/delegation-tree",
        503,
        json!({ "error": message }),
    )
    .await;

    // WHEN the attached arm of the command calls it
    let error = delegation_tree_on(&link, "t-1").await.unwrap_err();

    // THEN the interface shows the daemon's message, word for word
    assert_eq!(error, message);
}
