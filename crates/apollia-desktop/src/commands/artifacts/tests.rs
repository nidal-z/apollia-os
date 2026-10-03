use apollia_runtime::artifacts::{Artifact, SaveArtifactRequest, UpdateArtifactRequest};
use wiremock::matchers::{body_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::{remote_delete, remote_list, remote_save, remote_update};
use crate::attached::{gate, AttachedClient, LinkMode};

const TOKEN: &str = "token-of-this-installation";

fn artifact(id: &str, title: &str) -> Artifact {
    Artifact {
        id: id.to_owned(),
        session_id: "s 1".to_owned(),
        source_message_id: Some("m-1".to_owned()),
        kind: "code".to_owned(),
        language: Some("rust".to_owned()),
        source_tool: Some("file_write".to_owned()),
        title: title.to_owned(),
        content: "fn main() {}".to_owned(),
        created_at: "2026-10-03T00:00:00Z".to_owned(),
        updated_at: "2026-10-03T00:00:00Z".to_owned(),
    }
}

#[tokio::test]
async fn test_each_artifact_command_reaches_its_daemon_route_with_the_token() {
    // GIVEN a daemon answering each artifact route once, only when the token
    // of this installation comes with the request and the body is the one
    // the command must send
    let server = MockServer::start().await;
    let auth = format!("Bearer {TOKEN}");
    let request = SaveArtifactRequest {
        session_id: "s 1".to_owned(),
        source_message_id: Some("m-1".to_owned()),
        kind: "code".to_owned(),
        language: Some("rust".to_owned()),
        source_tool: Some("file_write".to_owned()),
        title: "main.rs".to_owned(),
        content: "fn main() {}".to_owned(),
    };
    Mock::given(method("POST"))
        .and(path("/api/v1/artifacts"))
        .and(header("authorization", auth.as_str()))
        .and(body_json(&request))
        .respond_with(ResponseTemplate::new(200).set_body_json(artifact("a/1", "main.rs")))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/sessions/s%201/artifacts"))
        .and(header("authorization", auth.as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_json(vec![artifact("a/1", "main.rs")]))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PATCH"))
        .and(path("/api/v1/artifacts/a%2F1"))
        .and(header("authorization", auth.as_str()))
        .and(body_json(
            serde_json::json!({ "title": "lib.rs", "content": null }),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(artifact("a/1", "lib.rs")))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/api/v1/artifacts/a%2F1"))
        .and(header("authorization", auth.as_str()))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({ "deleted": "a/1" })),
        )
        .expect(1)
        .mount(&server)
        .await;
    let client = AttachedClient::new(server.uri(), TOKEN).unwrap();
    let change = UpdateArtifactRequest {
        title: Some("lib.rs".to_owned()),
        content: None,
    };

    // WHEN the attached arm of each command runs, with identifiers that need
    // encoding in a path
    let saved = remote_save(&client, &request).await.unwrap();
    let listed = remote_list(&client, "s 1").await.unwrap();
    let changed = remote_update(&client, "a/1", &change).await.unwrap();
    remote_delete(&client, "a/1").await.unwrap();

    // THEN each reached its own route once (the mocks verify it on drop), and
    // the artifacts read as the daemon sent them
    assert_eq!(saved, artifact("a/1", "main.rs"));
    assert_eq!(listed, vec![artifact("a/1", "main.rs")]);
    assert_eq!(changed.title, "lib.rs");
}

#[tokio::test]
async fn test_an_artifact_refused_by_the_daemon_shows_its_message() {
    // GIVEN a daemon that knows no artifact "ghost" and cannot open its store
    let server = MockServer::start().await;
    Mock::given(method("PATCH"))
        .and(path("/api/v1/artifacts/ghost"))
        .respond_with(
            ResponseTemplate::new(404)
                .set_body_json(serde_json::json!({ "error": "artifact not found: ghost" })),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/sessions/s-1/artifacts"))
        .respond_with(ResponseTemplate::new(500).set_body_json(
            serde_json::json!({ "error": "failed to open artifacts.db: disk I/O error" }),
        ))
        .mount(&server)
        .await;
    let client = AttachedClient::new(server.uri(), TOKEN).unwrap();

    // WHEN the attached arm changes the unknown artifact and lists a session
    let changed = remote_update(&client, "ghost", &UpdateArtifactRequest::default()).await;
    let listed = remote_list(&client, "s-1").await;

    // THEN the interface reads the daemon's own words, not an unavailability
    assert_eq!(changed.unwrap_err(), "artifact not found: ghost");
    assert_eq!(
        listed.unwrap_err(),
        "failed to open artifacts.db: disk I/O error"
    );
}

#[test]
fn test_the_artifact_commands_pass_the_gate_attached() {
    // GIVEN the commands that save, list, change and delete an artifact
    let commands = [
        "delete_artifact",
        "list_artifacts",
        "save_artifact",
        "update_artifact",
    ];

    // WHEN each is gated while attached
    let refused: Vec<&str> = commands
        .into_iter()
        .filter(|command| gate(LinkMode::Attached, command).is_err())
        .collect();

    // THEN none is refused
    assert!(refused.is_empty(), "refused: {refused:?}");
}
