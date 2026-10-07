//! Tauri IPC commands for the registry of sources.
//!
//! A source is what an agent may consult once it is checked on its sheet: a
//! folder, a file, a mailbox, the profile, its memory, a code folder. Each
//! command delegates to a route of the embedded runtime, or of the daemon in
//! attached mode, so the desktop and any other client read the same registry:
//!
//! - `list_sources`: `GET /api/v1/sources`;
//! - `add_source`: `POST /api/v1/sources`;
//! - `remove_source`: `DELETE /api/v1/sources/{id}`, which removes its index;
//! - `get_source_index`: `GET /api/v1/sources/{id}/index`, where the index of
//!   a source stands and the files it left out;
//! - `list_source_folder_kinds`: `GET /api/v1/sources/folder-kinds`, the kinds
//!   offered on a folder connection, the code folder in developer mode only.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::State;

use crate::attached::{segment, RuntimeLink};

/// What the sheet sends to add a source.
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceInput {
    /// `folder`, `file`, `mail`, `profile`, `memory` or `code_folder`.
    pub kind: String,
    /// The name the user sees.
    pub label: String,
    /// What the source points at: a folder connection, a file path, a
    /// mailbox.
    #[serde(default)]
    pub reference: Option<String>,
}

/// The body of the addition route for `input`.
fn add_body(input: &SourceInput) -> Value {
    json!({
        "kind": input.kind,
        "label": input.label.trim(),
        "reference": input
            .reference
            .as_deref()
            .map(str::trim)
            .filter(|reference| !reference.is_empty()),
    })
}

/// Route of the source `id`.
fn source_path(id: &str) -> String {
    format!("/api/v1/sources/{}", segment(id))
}

/// Route of the index of the source `id`.
fn index_path(id: &str) -> String {
    format!("{}/index", source_path(id))
}

/// Route of the kinds offered on a folder connection.
fn folder_kinds_path(developer_mode: bool) -> String {
    format!("/api/v1/sources/folder-kinds?developerMode={developer_mode}")
}

/// Every source of the registry, oldest first.
#[tauri::command]
pub async fn list_sources(link: State<'_, RuntimeLink>) -> Result<Value, String> {
    link.get_json("/api/v1/sources").await
}

/// Add a source to the registry; answers the source as stored.
#[tauri::command]
pub async fn add_source(
    link: State<'_, RuntimeLink>,
    source: SourceInput,
) -> Result<Value, String> {
    link.post_json("/api/v1/sources", &add_body(&source)).await
}

/// Remove the source `source_id` and its index; answers the source it was.
#[tauri::command]
pub async fn remove_source(
    link: State<'_, RuntimeLink>,
    source_id: String,
) -> Result<Value, String> {
    link.delete_json(&source_path(&source_id)).await
}

/// Where the index of the source `source_id` stands, its counts and the
/// files it left out.
#[tauri::command]
pub async fn get_source_index(
    link: State<'_, RuntimeLink>,
    source_id: String,
) -> Result<Value, String> {
    link.get_json(&index_path(&source_id)).await
}

/// The kinds a source made on a folder connection may take.
#[tauri::command]
pub async fn list_source_folder_kinds(
    link: State<'_, RuntimeLink>,
    developer_mode: bool,
) -> Result<Value, String> {
    link.get_json(&folder_kinds_path(developer_mode)).await
}

#[cfg(test)]
mod tests {
    use wiremock::matchers::{body_json, header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::attached::{gate, AttachedClient, LinkMode};

    const TOKEN: &str = "token-of-this-installation";

    fn attached(server: &MockServer) -> RuntimeLink {
        RuntimeLink::Attached(AttachedClient::new(server.uri(), TOKEN).unwrap())
    }

    #[test]
    fn test_the_added_source_is_sent_trimmed_and_a_blank_reference_as_none() {
        // GIVEN a profile source typed with spaces and a blank reference
        let input = SourceInput {
            kind: "profile".into(),
            label: "  Profile ".into(),
            reference: Some("   ".into()),
        };

        // WHEN the route body is built
        let body = add_body(&input);

        // THEN the label is trimmed and the reference absent
        assert_eq!(
            body,
            json!({ "kind": "profile", "label": "Profile", "reference": null })
        );
    }

    #[test]
    fn test_a_source_is_named_in_its_routes_by_its_escaped_identifier() {
        // GIVEN an identifier holding a character a path cannot carry
        // WHEN its routes are built
        // THEN the identifier is one escaped segment in both
        assert_eq!(source_path("src/1"), "/api/v1/sources/src%2F1");
        assert_eq!(index_path("src-9"), "/api/v1/sources/src-9/index");
        assert_eq!(
            folder_kinds_path(true),
            "/api/v1/sources/folder-kinds?developerMode=true"
        );
    }

    #[test]
    fn test_the_registry_commands_are_served_while_attached() {
        // GIVEN the application attached to a daemon
        // WHEN each command of the registry goes through the gate
        // THEN none is refused
        for command in [
            "list_sources",
            "add_source",
            "remove_source",
            "get_source_index",
            "list_source_folder_kinds",
        ] {
            assert!(gate(LinkMode::Attached, command).is_ok(), "{command}");
        }
    }

    #[tokio::test]
    async fn test_a_source_is_added_listed_and_removed_through_the_runtime_routes() {
        // GIVEN a daemon holding the registry
        let server = MockServer::start().await;
        let stored = json!({ "id": "src-1", "kind": "folder", "label": "Clients",
            "reference": "folder:4f1c", "indexState": "absent" });
        Mock::given(method("POST"))
            .and(path("/api/v1/sources"))
            .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
            .and(body_json(
                json!({ "kind": "folder", "label": "Clients", "reference": "folder:4f1c" }),
            ))
            .respond_with(ResponseTemplate::new(201).set_body_json(stored.clone()))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v1/sources/src-1/index"))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                json!({ "id": "src-1", "indexState": "ready", "documents": 3, "chunks": 9, "skipped": [] }),
            ))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v1/sources/folder-kinds"))
            .and(query_param("developerMode", "false"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!(["folder"])))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("DELETE"))
            .and(path("/api/v1/sources/src-1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(stored.clone()))
            .expect(1)
            .mount(&server)
            .await;
        let link = attached(&server);

        // WHEN the source is added, its index read, the folder kinds asked
        // for, and the source removed
        let input = SourceInput {
            kind: "folder".into(),
            label: "Clients".into(),
            reference: Some("folder:4f1c".into()),
        };
        let added = link.post_json("/api/v1/sources", &add_body(&input)).await;
        let index = link.get_json(&index_path("src-1")).await;
        let kinds = link.get_json(&folder_kinds_path(false)).await;
        let removed = link.delete_json(&source_path("src-1")).await;

        // THEN each answer is the route's
        assert_eq!(added.unwrap()["id"], "src-1");
        assert_eq!(index.unwrap()["documents"], 3);
        assert_eq!(kinds.unwrap(), json!(["folder"]));
        assert_eq!(removed.unwrap()["label"], "Clients");
    }
}
