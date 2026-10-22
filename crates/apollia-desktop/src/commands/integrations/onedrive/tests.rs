use serde_json::{json, Value};
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;
use crate::attached::{gate, LinkMode};

const TOKEN: &str = "token-of-this-installation";
const ADA: &str = "/api/v1/connectors/onedrive/ada%40example.test";

fn chosen() -> Value {
    json!([{ "id": "fold-c", "name": "Clients", "is_folder": true, "path": "/drive/root:/Clients" }])
}

async fn answer(server: &MockServer, verb: &str, route: &str, body: Value) {
    Mock::given(method(verb))
        .and(path(route))
        .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .expect(1)
        .mount(server)
        .await;
}

#[test]
fn test_every_onedrive_command_passes_the_gate_attached() {
    // GIVEN the four OneDrive choice commands
    let family = [
        "onedrive_list_choices",
        "onedrive_browse",
        "onedrive_choose",
        "onedrive_forget",
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
async fn test_each_command_reaches_its_route_on_the_daemon() {
    // GIVEN a daemon serving the four routes once each
    let server = MockServer::start().await;
    answer(&server, "GET", &format!("{ADA}/choices"), chosen()).await;
    Mock::given(method("GET"))
        .and(path(format!("{ADA}/browse")))
        .and(query_param("folder_id", "fold-c"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            { "id": "brief", "name": "brief.txt", "is_folder": false, "chosen": false }
        ])))
        .expect(1)
        .mount(&server)
        .await;
    answer(&server, "PUT", &format!("{ADA}/choices/fold-c"), chosen()).await;
    answer(
        &server,
        "DELETE",
        &format!("{ADA}/choices/fold-c"),
        json!([]),
    )
    .await;
    let client = AttachedClient::new(server.uri(), TOKEN).unwrap();

    // WHEN the attached arm of each command runs
    let listed = attached_list(&client, " ada@example.test ").await.unwrap();
    let browsed = attached_browse(&client, "ada@example.test", Some("fold-c"))
        .await
        .unwrap();
    let chose = attached_choose(&client, "ada@example.test", "fold-c")
        .await
        .unwrap();
    let forgot = attached_forget(&client, "ada@example.test", "fold-c")
        .await
        .unwrap();

    // THEN each answer is the daemon's
    assert_eq!(listed[0].name, "Clients");
    assert_eq!(browsed[0].id, "brief");
    assert!(chose[0].is_folder);
    assert!(forgot.is_empty());
}

#[tokio::test]
async fn test_a_refusal_of_the_daemon_reaches_the_interface_in_its_own_words() {
    // GIVEN a daemon refusing to choose the root of the drive
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path(format!("{ADA}/choices/root")))
        .respond_with(
            ResponseTemplate::new(400)
                .set_body_json(json!({ "error": "choose folders inside it" })),
        )
        .mount(&server)
        .await;
    let client = AttachedClient::new(server.uri(), TOKEN).unwrap();

    // WHEN the window asks for it
    let refused = attached_choose(&client, "ada@example.test", "root")
        .await
        .unwrap_err();

    // THEN the interface reads the daemon's reason
    assert!(
        refused.to_string().contains("choose folders inside it"),
        "{refused}"
    );
}
