use serde_json::json;
use wiremock::matchers::{body_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;
use crate::attached::{gate, AttachedClient, LinkMode};

const TOKEN: &str = "token-of-this-installation";

/// The five commands of the agent packages.
const PACKAGE_COMMANDS: [&str; 5] = [
    "get_agent_package_detail",
    "install_agent_package",
    "list_agent_packages",
    "preview_agent_package",
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

    // THEN it reads the daemon's package
    assert_eq!(packages.len(), 1);
    assert_eq!(packages[0].name, "veille");
    assert_eq!(packages[0].agents[0].name, "veille-agent");
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
    let (_server, link) = daemon(
        "GET",
        "/api/v1/agent-packages/installed/veille",
        None,
        ok(full),
    )
    .await;

    // WHEN the attached arm reads it
    let package = detail(&link, "veille").await.unwrap();

    // THEN it is the daemon's, its manifest included
    assert_eq!(package.author, "Ada");
    assert_eq!(package.manifest["package"]["name"], "veille");
}

#[tokio::test]
async fn test_a_folder_is_previewed_by_the_daemon_and_an_unreadable_one_refused() {
    // GIVEN a daemon that previews one folder and cannot read another
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/agent-packages/preview"))
        .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
        .and(body_json(json!({ "path": "/pkgs/veille" })))
        .respond_with(ok(json!({
            "name": "veille", "version": "1.2.0", "description": "", "author": "",
            "agents": [], "triggers": [], "trigger_count": 0,
            "pip_packages": ["httpx>=0.27"], "valid": true, "error": null,
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v1/agent-packages/preview"))
        .and(body_json(json!({ "path": "/pkgs/none" })))
        .respond_with(refused(400, "cannot read agent.toml: No such file"))
        .expect(1)
        .mount(&server)
        .await;
    let link = RuntimeLink::Attached(AttachedClient::new(server.uri(), TOKEN).unwrap());

    // WHEN the attached arm previews both
    let preview_ok = preview(&link, "/pkgs/veille").await.unwrap();
    let error = preview(&link, "/pkgs/none").await.unwrap_err();

    // THEN the preview is the daemon's, and the refusal its message
    assert!(preview_ok.valid);
    assert_eq!(preview_ok.pip_packages, vec!["httpx>=0.27".to_owned()]);
    assert_eq!(error, "cannot read agent.toml: No such file");
}

#[tokio::test]
async fn test_an_install_posts_the_folder_the_secrets_and_the_consent() {
    // GIVEN a daemon installing a folder with a webhook secret and consent
    let secret = AgentPackageTriggerConfig {
        id: "inbound".to_owned(),
        secret: Some("s3cret".to_owned()),
    };
    let (_server, link) = daemon(
        "POST",
        "/api/v1/agent-packages/installed",
        Some(json!({
            "path": "/pkgs/veille",
            "trigger_configs": [{ "id": "inbound", "secret": "s3cret" }],
            "deps_confirmed": true,
        })),
        ResponseTemplate::new(201).set_body_json(json!({
            "name": "veille", "version": "1.2.0", "agent_count": 1,
            "trigger_count": 2, "trigger_errors": [],
        })),
    )
    .await;

    // WHEN the attached arm installs it
    let report = install(&link, "/pkgs/veille", &[secret], true)
        .await
        .unwrap();

    // THEN the daemon's report comes back
    assert_eq!(report.agent_count, 1);
    assert_eq!(report.trigger_count, 2);
}

#[tokio::test]
async fn test_unconfirmed_dependencies_come_back_as_the_text_the_dialog_parses() {
    // GIVEN a daemon refusing an install whose dependencies are not confirmed
    let (_server, link) = daemon(
        "POST",
        "/api/v1/agent-packages/installed",
        None,
        refused(428, "DEPS_CONFIRMATION_REQUIRED:1:httpx>=0.27"),
    )
    .await;

    // WHEN the attached arm installs without consent
    let error = install(&link, "/pkgs/veille", &[], false)
        .await
        .unwrap_err();

    // THEN the error is the daemon's text, untouched
    assert_eq!(error, "DEPS_CONFIRMATION_REQUIRED:1:httpx>=0.27");
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
