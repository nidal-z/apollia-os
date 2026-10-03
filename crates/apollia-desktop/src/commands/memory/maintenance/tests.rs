use serde_json::json;
use wiremock::matchers::{body_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;
use crate::attached::{gate, LinkMode};

const TOKEN: &str = "token-of-this-installation";
const BEARER: &str = "Bearer token-of-this-installation";

fn export_of(namespace: &str) -> MemoryExport {
    MemoryExport {
        format_version: 1,
        namespace: namespace.to_owned(),
        exported_at: "2026-10-03T00:00:00Z".to_owned(),
        episodic: Vec::new(),
        semantic: vec![json!({ "id": "s-1", "key": "client.budget", "value": "15000" })],
        procedural: Vec::new(),
    }
}

#[tokio::test]
async fn test_clear_and_purge_post_to_the_namespace_route_with_the_token() {
    // GIVEN a daemon that clears and purges a namespace whose name needs
    // encoding, and answers only the token
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/memory/namespaces/agent%20a/clear"))
        .and(header("authorization", BEARER))
        .and(body_json(json!({ "memoryType": "semantic" })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "removed": 3 })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v1/memory/namespaces/agent%20a/purge"))
        .and(header("authorization", BEARER))
        .and(body_json(
            json!({ "olderThanDays": 30, "memoryType": null }),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "removed": 1 })))
        .expect(1)
        .mount(&server)
        .await;
    let client = AttachedClient::new(server.uri(), TOKEN).unwrap();

    // WHEN the attached arm clears the facts, then purges every kind
    let cleared = clear_on(&client, "agent a", Some("semantic".to_owned()))
        .await
        .unwrap();
    let purged = purge_on(&client, "agent a", 30, None).await.unwrap();

    // THEN the daemon's counts come back
    assert_eq!(cleared, 3);
    assert_eq!(purged, 1);
}

#[tokio::test]
async fn test_an_entry_is_deleted_on_the_daemon_by_namespace_and_identifier() {
    // GIVEN a daemon holding the entry
    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path("/api/v1/memory/namespaces/agent-a/entries/s-1"))
        .and(header("authorization", BEARER))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "deleted": true })))
        .expect(1)
        .mount(&server)
        .await;
    let client = AttachedClient::new(server.uri(), TOKEN).unwrap();

    // WHEN the attached arm deletes it
    let deleted = delete_entry_on(&client, "agent-a", "s-1").await.unwrap();

    // THEN the daemon says it was there
    assert!(deleted);
}

#[tokio::test]
async fn test_an_export_is_read_from_the_daemon_and_written_on_this_machine() {
    // GIVEN a daemon answering the export of a namespace, and a file on the
    // machine of the window
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/memory/namespaces/agent-a/export"))
        .and(header("authorization", BEARER))
        .respond_with(ResponseTemplate::new(200).set_body_json(export_of("agent-a")))
        .expect(1)
        .mount(&server)
        .await;
    let client = AttachedClient::new(server.uri(), TOKEN).unwrap();
    let dir = tempfile::TempDir::new().unwrap();
    let file = dir.path().join("agent-a.apollia-memory");

    // WHEN the attached arm exports it to that file
    let export = export_on(&client, "agent-a").await.unwrap();
    let summary = write_export(&export, &file).unwrap();

    // THEN the file holds the daemon's export, and the summary counts it
    let written: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    assert_eq!(written["semantic"][0]["key"], "client.budget");
    assert!(
        summary.starts_with("Exported 0 episodic, 1 semantic, 0 procedural entries to "),
        "{summary}"
    );
}

#[tokio::test]
async fn test_an_import_read_on_this_machine_is_posted_to_the_daemon() {
    // GIVEN an export file on the machine of the window, and a daemon that
    // accepts it in replace mode
    let dir = tempfile::TempDir::new().unwrap();
    let file = dir.path().join("agent-a.apollia-memory");
    std::fs::write(&file, serde_json::to_string(&export_of("agent-a")).unwrap()).unwrap();
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/memory/namespaces/agent-b/import"))
        .and(header("authorization", BEARER))
        .and(body_json(
            json!({ "mode": "replace", "content": export_of("agent-a") }),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "imported": 1 })))
        .expect(1)
        .mount(&server)
        .await;
    let client = AttachedClient::new(server.uri(), TOKEN).unwrap();

    // WHEN the attached arm reads the file and imports it into another namespace
    let content = read_export(file.to_str().unwrap()).unwrap();
    let imported = import_on(&client, "agent-b", content, "replace".to_owned())
        .await
        .unwrap();

    // THEN the daemon's count comes back
    assert_eq!(imported, 1);
}

#[tokio::test]
async fn test_a_refusal_of_the_daemon_is_shown_as_it_wrote_it_for_every_command() {
    // GIVEN a daemon refusing every memory route with its own message
    let server = MockServer::start().await;
    Mock::given(wiremock::matchers::path_regex(
        "^/api/v1/memory/namespaces/",
    ))
    .respond_with(
        ResponseTemplate::new(404)
            .set_body_json(json!({ "error": "namespace 'nobody' not found" })),
    )
    .mount(&server)
    .await;
    let client = AttachedClient::new(server.uri(), TOKEN).unwrap();

    // WHEN each attached call reaches it
    let refusals: Vec<String> = vec![
        clear_on(&client, "nobody", None).await.unwrap_err().into(),
        purge_on(&client, "nobody", 1, None)
            .await
            .unwrap_err()
            .into(),
        delete_entry_on(&client, "nobody", "e-1")
            .await
            .unwrap_err()
            .into(),
        export_on(&client, "nobody").await.unwrap_err().into(),
        import_on(&client, "nobody", export_of("x"), "merge".to_owned())
            .await
            .unwrap_err()
            .into(),
    ];

    // THEN the interface shows the daemon's words, unchanged
    for refusal in refusals {
        assert_eq!(refusal, "namespace 'nobody' not found");
    }
}

#[test]
fn test_the_five_memory_commands_pass_the_gate_attached() {
    // GIVEN the application attached to a daemon
    // WHEN the five commands that change the memory are gated
    // THEN each passes
    for command in [
        "clear_memory",
        "purge_memory",
        "delete_memory_entry",
        "memory_export_namespace",
        "memory_import_namespace",
    ] {
        assert!(gate(LinkMode::Attached, command).is_ok(), "{command}");
    }
}
