//! The update of an agent, attached to a daemon: it reaches its route with the token of this installation, and a refusal of the
//! daemon reaches the interface in the daemon's own words.

use serde_json::{json, Value};
use wiremock::matchers::{body_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;
use crate::attached::{gate, AttachedClient, LinkMode};
use apollia_runtime::api::routes_agents::installed::lifecycle::restart_outcome;

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
fn test_the_uninstall_and_the_update_pass_the_gate_attached() {
    // GIVEN the uninstall, served by the packages, and the update
    // WHEN each is gated while attached
    let refused: Vec<&str> = ["uninstall_agent", "update_agent"]
        .into_iter()
        .filter(|command| gate(LinkMode::Attached, command).is_err())
        .collect();

    // THEN none answers that the service makes it unavailable
    assert!(refused.is_empty(), "refused: {refused:?}");
}

#[tokio::test]
async fn test_the_update_puts_the_new_module_and_reads_the_restart_outcome() {
    // GIVEN a daemon that cycled the running agent on its new module
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path("/api/v1/agents/installed/veille"))
        .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
        .and(body_json(json!({ "path": "/src/veille.py" })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "name": "veille",
            "version": "2.0.0",
            "install_path": "/data/agents/veille/agent.py",
            "restart_outcome": "restarted",
            "restart_error": null,
        })))
        .expect(1)
        .mount(&server)
        .await;
    let link = RuntimeLink::Attached(AttachedClient::new(server.uri(), TOKEN).unwrap());

    // WHEN the attached arm updates it
    let updated = update_on(&link, "veille", "/src/veille.py").await.unwrap();

    // THEN the window reads the version and what happened to the instance
    assert_eq!(updated.version, "2.0.0");
    assert_eq!(updated.restart_outcome, restart_outcome::RESTARTED);
    assert!(updated.restart_error.is_none());
}

#[tokio::test]
async fn test_a_refusal_of_the_daemon_is_shown_as_it_is() {
    // GIVEN a daemon refusing the update with the message its route answers
    let message = "missing @agent decorator";
    let (_server, link) = daemon(
        "PUT",
        "/api/v1/agents/installed/ghost",
        400,
        json!({ "error": message }),
    )
    .await;

    // WHEN the attached arm of the command calls it
    let error = update_on(&link, "ghost", "/src/ghost.py")
        .await
        .unwrap_err();

    // THEN the interface shows the daemon's message, word for word
    assert_eq!(error, message);
}
