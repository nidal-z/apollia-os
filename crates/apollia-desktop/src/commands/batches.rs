//! Tauri IPC commands behind the launch of a batch: the sources a batch may
//! read, the sensitive accesses of the agent it runs, and the launch itself.
//!
//! - [`list_batch_sources`]: `GET /api/v1/sources`
//! - [`preview_batch_source`]: `POST /api/v1/sources/preview`
//! - [`list_batch_accesses`]: `GET /api/v1/batches/accesses?agent=`
//! - [`launch_batch`]: `POST /api/v1/batches`
//!
//! Every command goes through the runtime's HTTP API, so each serves the
//! embedded runtime and the daemon alike. The answers are the routes' own
//! JSON, typed on the interface side.

use serde_json::{json, Value};
use tauri::State;

use crate::attached::{segment, RuntimeLink};

/// Route of the registry of sources.
const SOURCES_ROUTE: &str = "/api/v1/sources";

/// Route of the preview of a workbook or a CSV file.
const PREVIEW_ROUTE: &str = "/api/v1/sources/preview";

/// Route of the batches.
const BATCHES_ROUTE: &str = "/api/v1/batches";

/// The route listing the sensitive accesses of `agent`, its name encoded as
/// one query value.
fn accesses_path(agent: &str) -> String {
    format!("{BATCHES_ROUTE}/accesses?agent={}", segment(agent))
}

/// The accesses of an answer of the accesses route, an empty list when the
/// answer carries none.
fn accesses_of(mut answer: Value) -> Value {
    answer
        .get_mut("accesses")
        .map(Value::take)
        .unwrap_or_else(|| json!([]))
}

pub(crate) async fn accesses_on(link: &RuntimeLink, agent: &str) -> Result<Value, String> {
    Ok(accesses_of(link.get_json(&accesses_path(agent)).await?))
}

/// The body of a preview: the file, then the sheet and the header row only
/// when the user picked them, so the runtime proposes whichever is absent.
fn preview_body(path: &str, sheet: Option<&str>, header_row: Option<u32>) -> Value {
    let mut body = json!({ "path": path });
    if let Some(sheet) = sheet {
        body["sheet"] = json!(sheet);
    }
    if let Some(header_row) = header_row {
        body["headerRow"] = json!(header_row);
    }
    body
}

pub(crate) async fn preview_on(
    link: &RuntimeLink,
    path: &str,
    sheet: Option<&str>,
    header_row: Option<u32>,
) -> Result<Value, String> {
    link.post_json(PREVIEW_ROUTE, &preview_body(path, sheet, header_row))
        .await
}

pub(crate) async fn launch_on(link: &RuntimeLink, launch: &Value) -> Result<Value, String> {
    link.post_json(BATCHES_ROUTE, launch).await
}

/// Every source of the registry, oldest first: what the launch of a batch
/// offers to read.
#[tauri::command]
pub async fn list_batch_sources(link: State<'_, RuntimeLink>) -> Result<Value, String> {
    link.get_json(SOURCES_ROUTE).await
}

/// What a batch on the workbook or CSV file at `path` would read: the sheet
/// and the header row proposed, the first rows, the columns and the number of
/// items. `sheet` and `header_row` follow the choice of the user; a file the
/// runtime cannot read comes back as its message.
#[tauri::command]
pub async fn preview_batch_source(
    link: State<'_, RuntimeLink>,
    path: String,
    sheet: Option<String>,
    header_row: Option<u32>,
) -> Result<Value, String> {
    preview_on(link.inner(), &path, sheet.as_deref(), header_row).await
}

/// The sensitive accesses of `agent`, those a launch may grant for the
/// duration of a batch.
#[tauri::command]
pub async fn list_batch_accesses(
    link: State<'_, RuntimeLink>,
    agent: String,
) -> Result<Value, String> {
    accesses_on(link.inner(), &agent).await
}

/// Launches a batch and returns it. `launch` is the body of the route: the
/// agent, the source, the accesses granted and the failures tolerated. A
/// launch the runtime refuses comes back as its message.
#[tauri::command]
pub async fn launch_batch(link: State<'_, RuntimeLink>, launch: Value) -> Result<Value, String> {
    launch_on(link.inner(), &launch).await
}

#[cfg(test)]
mod tests {
    use wiremock::matchers::{body_json, header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::attached::AttachedClient;

    const TOKEN: &str = "token-of-this-installation";

    #[test]
    fn test_an_agent_name_is_encoded_as_one_query_value() {
        // GIVEN an agent whose name would open a second query parameter
        let agent = "apollia:chat&agent=other";

        // WHEN the route of its accesses is built
        let route = accesses_path(agent);

        // THEN the name stays one value
        assert_eq!(
            route,
            "/api/v1/batches/accesses?agent=apollia%3Achat%26agent%3Dother"
        );
    }

    #[test]
    fn test_an_answer_without_accesses_reads_as_an_empty_list() {
        // GIVEN an answer that names the agent only
        let answer = json!({ "agent": "triage" });

        // WHEN its accesses are read
        let accesses = accesses_of(answer);

        // THEN there is none, rather than a missing field
        assert_eq!(accesses, json!([]));
    }

    #[test]
    fn test_a_preview_names_the_sheet_and_the_header_row_only_when_picked() {
        // GIVEN a file, first as opened, then with a sheet and a header row picked
        let path = "/data/requests.xlsx";

        // WHEN the two bodies are built
        let proposed = preview_body(path, None, None);
        let picked = preview_body(path, Some("March"), Some(3));

        // THEN the first leaves both to the runtime, the second carries both
        assert_eq!(proposed, json!({ "path": path }));
        assert_eq!(
            picked,
            json!({ "path": path, "sheet": "March", "headerRow": 3 })
        );
    }

    #[tokio::test]
    async fn test_a_preview_is_asked_of_the_daemon_and_its_refusal_comes_back() {
        // GIVEN a daemon that previews one workbook and refuses any other file
        let server = MockServer::start().await;
        let preview = json!({
            "format": "xlsx", "sourceType": "spreadsheet",
            "sheets": [{ "name": "March", "nonEmptyRows": 129 }],
            "proposedSheet": "March", "sheet": "March",
            "proposedHeaderRow": 1, "headerRow": 1,
            "columns": [{ "letter": "A", "name": "Customer" }],
            "rows": [{ "row": 1, "cells": ["Customer"] }],
            "estimatedItems": 128, "canModifyOriginal": true, "formattingNotKept": false,
        });
        Mock::given(method("POST"))
            .and(path("/api/v1/sources/preview"))
            .and(body_json(
                json!({ "path": "/data/requests.xlsx", "sheet": "March" }),
            ))
            .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
            .respond_with(ResponseTemplate::new(200).set_body_json(preview.clone()))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/v1/sources/preview"))
            .respond_with(
                ResponseTemplate::new(404)
                    .set_body_json(json!({ "error": "no file at '/data/absent.csv'" })),
            )
            .mount(&server)
            .await;
        let link = RuntimeLink::Attached(AttachedClient::new(server.uri(), TOKEN).unwrap());

        // WHEN the dialog previews the workbook, then a file that is not there
        let shown = preview_on(&link, "/data/requests.xlsx", Some("March"), None)
            .await
            .unwrap();
        let refused = preview_on(&link, "/data/absent.csv", None, None).await;

        // THEN the preview comes back whole, and the refusal carries the reason
        assert_eq!(shown, preview);
        let reason = refused.unwrap_err();
        assert!(reason.contains("no file at '/data/absent.csv'"), "{reason}");
    }

    #[tokio::test]
    async fn test_the_accesses_of_an_agent_are_read_from_the_daemon() {
        // GIVEN a daemon that lists one sensitive access for `triage`
        let server = MockServer::start().await;
        let access = json!({
            "accessId": "send_email", "permits": "Send an email",
            "family": "send_external", "grantable": true,
        });
        Mock::given(method("GET"))
            .and(path("/api/v1/batches/accesses"))
            .and(query_param("agent", "triage"))
            .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({ "agent": "triage", "accesses": [access.clone()] })),
            )
            .expect(1)
            .mount(&server)
            .await;
        let link = RuntimeLink::Attached(AttachedClient::new(server.uri(), TOKEN).unwrap());

        // WHEN the launch dialog asks for them
        let accesses = accesses_on(&link, "triage").await.unwrap();

        // THEN it receives the list alone
        assert_eq!(accesses, json!([access]));
    }

    #[tokio::test]
    async fn test_a_launch_is_posted_whole_and_its_refusal_comes_back() {
        // GIVEN a daemon that accepts one launch and refuses any other
        let server = MockServer::start().await;
        let launch = json!({
            "agent": "triage",
            "source": { "kind": "csv", "location": "/tmp/requests.csv", "options": {} },
            "grants": ["send_email"],
            "maxFailures": 10,
        });
        Mock::given(method("POST"))
            .and(path("/api/v1/batches"))
            .and(body_json(launch.clone()))
            .respond_with(
                ResponseTemplate::new(202)
                    .set_body_json(json!({ "batchId": "b-1", "state": "running", "total": 3 })),
            )
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/v1/batches"))
            .respond_with(
                ResponseTemplate::new(400)
                    .set_body_json(json!({ "error": "unknown source type 'folder'" })),
            )
            .mount(&server)
            .await;
        let link = RuntimeLink::Attached(AttachedClient::new(server.uri(), TOKEN).unwrap());

        // WHEN the dialog launches it, then launches one the daemon refuses
        let batch = launch_on(&link, &launch).await.unwrap();
        let refused = launch_on(&link, &json!({ "agent": "triage" })).await;

        // THEN the batch comes back, and the refusal carries the reason
        assert_eq!(batch["batchId"], "b-1");
        assert_eq!(batch["total"], 3);
        let reason = refused.unwrap_err();
        assert!(reason.contains("unknown source type 'folder'"), "{reason}");
    }
}
