use serde_json::json;
use wiremock::matchers::{body_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;
use crate::attached::{gate, AttachedClient, LinkMode};

const TOKEN: &str = "token-of-this-installation";

/// The five commands of the agent packages.
const PACKAGE_COMMANDS: [&str; 3] = [
    "get_agent_package_detail",
    "list_agent_packages",
    "uninstall_agent_package",
];

/// The application attached to a daemon answering `verb route` once, with
/// the token, and `body` when given.
async fn daemon(
    verb: &str,
    route: &str,
    body: Option<Value>,
    answer: ResponseTemplate,
) -> (MockServer, RuntimeLink) {
    let server = MockServer::start().await;
    let mut mock = Mock::given(method(verb))
        .and(path(route))
        .and(header("authorization", format!("Bearer {TOKEN}").as_str()));
    if let Some(body) = body {
        mock = mock.and(body_json(body));
    }
    mock.respond_with(answer).expect(1).mount(&server).await;
    let client = AttachedClient::new(server.uri(), TOKEN).unwrap();
    (server, RuntimeLink::Attached(client))
}

fn ok(body: Value) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(body)
}

fn refused(status: u16, message: &str) -> ResponseTemplate {
    ResponseTemplate::new(status).set_body_json(json!({ "error": message }))
}

fn listed() -> Value {
    json!({
        "name": "veille",
        "version": "1.2.0",
        "description": "Morning watch",
        "agent_count": 1,
        "agents": [{ "name": "veille-agent", "role": "director", "entry": "veille-agent.py" }],
        "installed_at": "2026-10-03T07:00:00Z",
        "root_path": "/home/ada/.apollia/agents/packages/veille",
        "root_missing": false,
        "signature": {
            "publisher": "Ada",
            "signed": true,
            "trusted": false,
            "key_fingerprint": "0123abcd",
        },
        "embedded": false,
        "counts": { "agents": 1, "workflows": 0, "tools": 1, "connections": 1 },
    })
}

#[test]
fn test_the_package_commands_pass_the_gate_attached() {
    // GIVEN the application attached to a daemon
    // WHEN each package command is gated
    // THEN none is refused
    for command in PACKAGE_COMMANDS {
        assert!(gate(LinkMode::Attached, command).is_ok(), "{command}");
    }
}

#[tokio::test]
async fn test_the_packages_are_listed_by_the_daemon() {
    // GIVEN a daemon holding one installed package
    let (_server, link) = daemon(
        "GET",
        "/api/v1/agent-packages/installed",
        None,
        ok(json!([listed()])),
    )
    .await;

    // WHEN the attached arm lists them
    let packages = list(&link).await.unwrap();

    // THEN it reads the daemon's package, its publisher, its signature and
    // how much it holds
    assert_eq!(packages.len(), 1);
    assert_eq!(packages[0].name, "veille");
    assert_eq!(packages[0].agents[0].name, "veille-agent");
    assert_eq!(packages[0].signature.publisher, "Ada");
    assert!(packages[0].signature.signed);
    assert!(!packages[0].embedded);
    assert_eq!(packages[0].counts.tools, 1);
}

#[tokio::test]
async fn test_a_listing_refused_by_the_daemon_is_shown_as_written() {
    // GIVEN a daemon that cannot read its package registry
    let (_server, link) = daemon(
        "GET",
        "/api/v1/agent-packages/installed",
        None,
        refused(500, "agent package registry unavailable"),
    )
    .await;

    // WHEN the attached arm lists the packages
    let error = list(&link).await.unwrap_err();

    // THEN the daemon's message is the error, untouched
    assert_eq!(error, "agent package registry unavailable");
}

#[tokio::test]
async fn test_the_detail_is_read_with_the_name_encoded_and_a_404_shown_as_written() {
    // GIVEN a daemon that knows no package named `a/b`
    let (_server, link) = daemon(
        "GET",
        "/api/v1/agent-packages/installed/a%2Fb",
        None,
        refused(404, "package 'a/b' not found"),
    )
    .await;

    // WHEN the attached arm reads its detail
    let error = detail(&link, "a/b").await.unwrap_err();

    // THEN the name stayed one segment and the daemon's message is the error
    assert_eq!(error, "package 'a/b' not found");
}

#[tokio::test]
async fn test_the_detail_of_a_package_is_read_from_the_daemon() {
    // GIVEN a daemon holding the package `veille`
    let mut full = listed();
    full["author"] = json!("Ada");
    full["updated_at"] = json!("2026-10-03T07:00:00Z");
    full["manifest"] = json!({ "package": { "name": "veille" } });
    full["content"] = json!({
        "agents": [{ "id": "veille-agent", "name": "Veille", "description": "Watches the news." }],
        "workflows": [],
        "tools": [{ "name": "fetch", "description": "Fetches a page.", "family": "read" }],
        "connections_brought": [],
        "connections_required": [{ "type": "gmail", "reason": "Send the digest" }],
    });
    full["automations"] = json!([{ "id": "daily", "target": "veille-agent" }]);
    let (_server, link) = daemon(
        "GET",
        "/api/v1/agent-packages/installed/veille",
        None,
        ok(full),
    )
    .await;

    // WHEN the attached arm reads it
    let package = detail(&link, "veille").await.unwrap();

    // THEN it is the daemon's, its manifest included, with what it holds by
    // kind and the automation that goes with it
    assert_eq!(package.author, "Ada");
    assert_eq!(package.manifest["package"]["name"], "veille");
    assert_eq!(package.content.agents[0].name, "Veille");
    assert_eq!(package.content.tools[0].family, "read");
    assert_eq!(package.content.connections_required[0].kind, "gmail");
    assert_eq!(package.automations[0].id, "daily");
}

#[tokio::test]
async fn test_an_uninstall_takes_the_route_of_the_command_line_and_a_refusal_is_shown_as_written() {
    // GIVEN a daemon removing `veille` on the uninstall route the command line
    // takes, and knowing nothing of `ghost`
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/packages/veille/uninstall"))
        .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
        .and(body_json(
            json!({ "clearMemory": false, "clearHistory": true }),
        ))
        .respond_with(ok(json!({ "name": "veille", "historyCleared": true })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v1/packages/ghost/uninstall"))
        .respond_with(refused(404, "`ghost` is not installed"))
        .expect(1)
        .mount(&server)
        .await;
    let link = RuntimeLink::Attached(AttachedClient::new(server.uri(), TOKEN).unwrap());

    // WHEN the attached arm uninstalls both, erasing the history of the first
    let removed = uninstall::package(&link, "veille", None, Some(true)).await;
    let error = uninstall::package(&link, "ghost", None, None)
        .await
        .unwrap_err();

    // THEN the first is removed with the choice, the second refused with the
    // daemon's text
    assert_eq!(removed.unwrap()["historyCleared"], true);
    assert_eq!(error, "`ghost` is not installed");
}
