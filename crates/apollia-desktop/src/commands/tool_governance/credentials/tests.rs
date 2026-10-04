use serde_json::json;
use wiremock::matchers::{body_json, header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;
use crate::attached::{gate, AttachedClient, LinkMode};

const TOKEN: &str = "token-of-this-installation";

fn attached(server: &MockServer) -> RuntimeLink {
    RuntimeLink::Attached(AttachedClient::new(server.uri(), TOKEN).unwrap())
}

/// One answer of the daemon, on one verb and route, given only with the token.
async fn answer(
    server: &MockServer,
    verb: &str,
    route: &str,
    status: u16,
    body: serde_json::Value,
) {
    Mock::given(method(verb))
        .and(path(route))
        .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
        .respond_with(ResponseTemplate::new(status).set_body_json(body))
        .expect(1)
        .mount(server)
        .await;
}

fn owned(value: &str) -> String {
    value.to_owned()
}

#[test]
fn test_the_four_credential_commands_pass_the_gate_attached() {
    // GIVEN the commands the tools settings open to manage a credential
    let commands = [
        "governance_list_credentials",
        "governance_set_credential",
        "governance_delete_credential",
        "governance_test_credential",
    ];

    // WHEN each is gated while attached to a daemon
    let refused: Vec<&str> = commands
        .into_iter()
        .filter(|command| gate(LinkMode::Attached, command).is_err())
        .collect();

    // THEN none answers that the service makes it unavailable
    assert!(refused.is_empty(), "refused: {refused:?}");
}

#[tokio::test]
async fn test_each_command_reaches_its_route_with_its_verb_and_the_token() {
    // GIVEN a daemon answering the four routes of the credentials, the list
    // filtered by tool, the value in the body of the write, and a key name
    // that needs encoding on the delete
    let server = MockServer::start().await;
    let entry = json!({
        "tool_name": "web_search", "key_name": "brave.api_key",
        "created_at": "2026-10-03T00:00:00+00:00", "last_used_at": null,
    });
    Mock::given(method("GET"))
        .and(path("/api/v1/tool-credentials"))
        .and(query_param("tool_name", "web_search"))
        .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([entry])))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/api/v1/tool-credentials/web_search/brave.api_key"))
        .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
        .and(body_json(json!({ "value": "k-123" })))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({ "tool_name": "web_search", "key_name": "brave.api_key" })),
        )
        .expect(1)
        .mount(&server)
        .await;
    let delete_route = "/api/v1/tool-credentials/mailer/smtp%2Fpassword";
    answer(
        &server,
        "DELETE",
        delete_route,
        200,
        json!({ "removed": true }),
    )
    .await;
    let test_route = "/api/v1/tool-credential-tests/web_search";
    let outcome = json!({ "ok": true, "latency_ms": 42, "error": null });
    answer(&server, "POST", test_route, 200, outcome).await;
    let link = attached(&server);

    // WHEN the attached arm runs each command
    let listed = list(&link, Some(owned("web_search"))).await.unwrap();
    let stored = set(
        &link,
        owned("web_search"),
        owned("brave.api_key"),
        owned("k-123"),
    )
    .await;
    let deleted = delete(&link, owned("mailer"), owned("smtp/password")).await;
    let tested = test(&link, owned("web_search")).await.unwrap();

    // THEN each reached its own route once (the mocks expect it) and reads
    // as the settings page draws it
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].key_name, "brave.api_key");
    assert_eq!(stored, Ok(()));
    assert_eq!(deleted, Ok(()));
    assert!(tested.ok);
    assert_eq!(tested.latency_ms, Some(42));
}

#[tokio::test]
async fn test_each_refusal_of_the_daemon_shows_its_own_words() {
    // GIVEN a daemon refusing each of the four routes with its message
    let server = MockServer::start().await;
    let locked = "failed to open governance database: locked";
    let empty = "credential value must not be empty";
    let no_test = "no credential test implemented for tool 'mailer'";
    let route = "/api/v1/tool-credentials/web_search/brave.api_key";
    answer(
        &server,
        "GET",
        "/api/v1/tool-credentials",
        500,
        json!({ "error": locked }),
    )
    .await;
    answer(&server, "PUT", route, 400, json!({ "error": empty })).await;
    answer(&server, "DELETE", route, 500, json!({ "error": locked })).await;
    let test_route = "/api/v1/tool-credential-tests/mailer";
    answer(
        &server,
        "POST",
        test_route,
        400,
        json!({ "error": no_test }),
    )
    .await;
    let link = attached(&server);

    // WHEN the attached arm runs each command
    let errors = [
        list(&link, None).await.unwrap_err(),
        set(
            &link,
            owned("web_search"),
            owned("brave.api_key"),
            String::new(),
        )
        .await
        .unwrap_err(),
        delete(&link, owned("web_search"), owned("brave.api_key"))
            .await
            .unwrap_err(),
        test(&link, owned("mailer")).await.unwrap_err(),
    ];

    // THEN the interface reads the daemon's message as it is, never an
    // unavailability
    assert_eq!(errors, [locked, empty, locked, no_test]);
}

#[tokio::test]
async fn test_a_blank_key_is_refused_before_any_request() {
    // GIVEN a daemon that must not be called
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&server)
        .await;

    // WHEN the attached arm stores a value under a blank key
    let error = set(
        &attached(&server),
        owned("web_search"),
        owned(" "),
        owned("k"),
    )
    .await
    .unwrap_err();

    // THEN it answers with the daemon's own words for that refusal
    assert_eq!(error, "tool_name and key_name must not be empty");
}
