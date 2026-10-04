//! The deletion of a stored task and its delegation tree, attached to a
//! daemon: each reaches its route with the token of this installation, and a
//! refusal of the daemon reaches the interface in the daemon's own words.

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
fn test_the_deletion_and_the_tree_pass_the_gate_attached() {
    // GIVEN the two task commands of the family
    // WHEN each is gated while attached
    let refused: Vec<&str> = ["delete_task", "get_delegation_tree"]
        .into_iter()
        .filter(|command| gate(LinkMode::Attached, command).is_err())
        .collect();

    // THEN none answers that the service makes it unavailable
    assert!(refused.is_empty(), "refused: {refused:?}");
}

#[tokio::test]
async fn test_the_deletion_reads_whether_the_daemon_removed_a_record() {
    // GIVEN a daemon that removed the record of t-1
    let (_server, link) = daemon(
        "DELETE",
        "/api/v1/tasks/t-1/record",
        200,
        json!({ "deleted": true }),
    )
    .await;

    // WHEN the attached arm deletes it
    let deleted = delete_task_on(&link, "t-1").await;

    // THEN the window reads the removal
    assert_eq!(deleted, Ok(true));
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
    // GIVEN a daemon refusing each command with the message its route answers
    let refusals = [
        (
            "delete_task",
            "DELETE",
            "/api/v1/tasks/t-1/record",
            "task persistence is not available",
        ),
        (
            "get_delegation_tree",
            "GET",
            "/api/v1/tasks/t-1/delegation-tree",
            "sidechain logging not initialized",
        ),
    ];
    for (command, verb, route, message) in refusals {
        let (_server, link) = daemon(verb, route, 503, json!({ "error": message })).await;

        // WHEN the attached arm of the command calls it
        let error = match command {
            "delete_task" => delete_task_on(&link, "t-1").await.unwrap_err(),
            _ => delegation_tree_on(&link, "t-1").await.unwrap_err(),
        };

        // THEN the interface shows the daemon's message, word for word
        assert_eq!(error, message, "{command}");
    }
}
