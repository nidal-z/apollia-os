use serde_json::{json, Value};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;
use crate::attached::{gate, LinkMode};

const TOKEN: &str = "token-of-this-installation";
const START: &str = "/api/v1/connectors/oauth/start";
const COMPLETE: &str = "/api/v1/connectors/oauth/complete";
const FOLDERS: &str = "/api/v1/connectors/drive-folders";
const ADA_FOLDER: &str = "/api/v1/connectors/drive-folders/ada%40example.test";
const LOOPBACK: &str = "http://127.0.0.1:4321/callback";

/// A daemon answering each `verb route` once with `status` and `body` (none
/// when `Null`), only when the token of this installation comes with it.
async fn daemon(routes: Vec<(&str, &str, u16, Value)>) -> (MockServer, AttachedClient) {
    let server = MockServer::start().await;
    for (verb, route, status, body) in routes {
        let mut answer = ResponseTemplate::new(status);
        if !body.is_null() {
            answer = answer.set_body_json(body);
        }
        Mock::given(method(verb))
            .and(path(route))
            .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
            .respond_with(answer)
            .expect(1)
            .mount(&server)
            .await;
    }
    let client = AttachedClient::new(server.uri(), TOKEN).unwrap();
    (server, client)
}

fn folder(path: Option<&str>) -> Value {
    let effective = path.unwrap_or("Apollia");
    json!({ "account_id": "ada@example.test", "folder_path": path, "effective_folder_path": effective })
}

#[test]
fn test_every_command_of_the_family_passes_the_gate_attached() {
    // GIVEN the account and Drive folder commands
    let family = [
        "oauth_start_flow",
        "oauth_complete_flow",
        "oauth_list_drive_folders",
        "oauth_set_drive_folder",
        "oauth_reset_drive_folder",
    ];

    // WHEN each is gated while attached
    let refused: Vec<&str> = family
        .into_iter()
        .filter(|command| gate(LinkMode::Attached, command).is_err())
        .collect();

    // THEN none answers that the service makes it unavailable
    assert!(refused.is_empty(), "refused: {refused:?}");
}

#[tokio::test]
async fn test_each_command_reaches_its_route_on_the_daemon_with_the_token() {
    // GIVEN a daemon serving the five routes
    let started = json!({ "auth_url": "https://idp.test/auth?state=s-1", "state": "s-1" });
    let account =
        json!({ "provider": "google", "account_id": "ada@example.test", "granted_scopes": [] });
    let (_server, client) = daemon(vec![
        ("POST", START, 200, started),
        ("POST", COMPLETE, 200, account),
        ("GET", FOLDERS, 200, json!([folder(None)])),
        ("PUT", ADA_FOLDER, 200, folder(Some("Clients"))),
        ("DELETE", ADA_FOLDER, 200, folder(None)),
    ])
    .await;

    // WHEN the attached arm of each command runs
    let consent = start_consent(&client, "google", &[], LOOPBACK)
        .await
        .unwrap();
    let connected = complete_consent(&client, &consent.state, "c-1")
        .await
        .unwrap();
    let folders = drive_folders(&client).await.unwrap();
    let set = set_drive_folder(&client, " ada@example.test ", "Clients").await;
    let reset = reset_drive_folder(&client, "ada@example.test").await;

    // THEN each reached its route once, and the answers are the daemon's
    assert!(consent.auth_url.contains("state=s-1"));
    assert_eq!(connected.account_id, "ada@example.test");
    assert_eq!(folders[0].effective_folder_path, "Apollia");
    assert!(set.is_ok() && reset.is_ok(), "{set:?} {reset:?}");
}

#[tokio::test]
async fn test_a_refusal_of_the_daemon_reaches_the_interface_in_its_own_words() {
    // GIVEN a daemon refusing each of the five routes with its own reason
    let routes = [
        ("POST", START),
        ("POST", COMPLETE),
        ("GET", FOLDERS),
        ("PUT", ADA_FOLDER),
        ("DELETE", ADA_FOLDER),
    ];
    let reasons: Vec<String> = (0..routes.len()).map(|i| format!("reason {i}")).collect();
    let refusals = routes
        .iter()
        .zip(&reasons)
        .map(|((verb, route), reason)| (*verb, *route, 409, json!({ "error": reason })))
        .collect();
    let (_server, client) = daemon(refusals).await;

    // WHEN the attached arm of each command runs
    let ada = "ada@example.test";
    let results = [
        start_consent(&client, "google", &[], LOOPBACK)
            .await
            .map(drop),
        complete_consent(&client, "s", "c").await.map(drop),
        drive_folders(&client).await.map(drop),
        set_drive_folder(&client, ada, "x").await,
        reset_drive_folder(&client, ada).await,
    ];

    // THEN each error is the daemon's text, carried as such to the interface
    let texts: Vec<String> = results
        .into_iter()
        .map(|result| match result {
            Err(IntegrationsError::Service(text)) => text,
            other => panic!("expected the daemon's refusal, got {other:?}"),
        })
        .collect();
    assert_eq!(texts, reasons);
}

#[tokio::test]
async fn test_a_policy_refusal_of_the_daemon_keeps_its_kind() {
    // GIVEN a daemon whose connection admission refuses the consent
    let (_server, client) = daemon(vec![(
        "POST",
        START,
        403,
        json!({ "error": "refused by the organisation policy" }),
    )])
    .await;

    // WHEN the attached arm of the start command runs
    let err = start_consent(&client, "google", &[], LOOPBACK)
        .await
        .unwrap_err();

    // THEN the interface receives a policy refusal, not a generic one
    assert!(
        matches!(&err, IntegrationsError::RefusedByPolicy(reason)
            if reason == "refused by the organisation policy"),
        "{err:?}"
    );
}
