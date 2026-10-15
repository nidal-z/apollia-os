//! Tauri IPC commands behind the view of a batch and the history of the
//! batches.
//!
//! - [`list_batches`]: `GET /api/v1/batches?limit=`
//! - [`get_batch`]: `GET /api/v1/batches/{id}`
//! - [`control_batch`]: `POST /api/v1/batches/{id}/pause`, `/resume`, `/stop`
//!   and `/retry`
//! - [`export_batch`]: `GET /api/v1/batches/{id}/export?format=`, then the
//!   file written where the user chose in the save dialog
//! - [`delete_batch`]: `DELETE /api/v1/batches/{id}`
//! - [`batch_item_decision`]: `GET /api/v1/pending-decisions`, narrowed to the
//!   decision one item waits on
//! - [`decide_batch_item`]: `POST /api/v1/pending-decisions/decide`
//!
//! Every command goes through the runtime's HTTP API, so each serves the
//! embedded runtime and the daemon alike.

use std::path::Path;

use serde_json::{json, Value};
use tauri::State;

use crate::attached::{segment, RuntimeLink};

/// Route of the batches.
const BATCHES_ROUTE: &str = "/api/v1/batches";

/// Route of the decisions waiting for the user.
const PENDING_ROUTE: &str = "/api/v1/pending-decisions";

/// What a batch can be told to do, each a route segment of its own.
const CONTROLS: [&str; 4] = ["pause", "resume", "stop", "retry"];

/// The formats a batch is exported in.
const FORMATS: [&str; 3] = ["xlsx", "csv", "json"];

/// The answers a batch item waiting for an access takes.
const ANSWERS: [&str; 3] = ["once", "always", "refuse"];

fn batch_path(batch_id: &str) -> String {
    format!("{BATCHES_ROUTE}/{}", segment(batch_id))
}

/// The route that applies `action` to a batch, or the reason `action` names
/// none.
fn control_path(batch_id: &str, action: &str) -> Result<String, String> {
    if !CONTROLS.contains(&action) {
        return Err(format!("unknown batch action '{action}'"));
    }
    Ok(format!("{}/{action}", batch_path(batch_id)))
}

/// The route that exports a batch as `format`, or the reason `format` is not
/// one a batch is exported in.
fn export_path(batch_id: &str, format: &str) -> Result<String, String> {
    if !FORMATS.contains(&format) {
        return Err(format!("unknown export format '{format}'"));
    }
    Ok(format!("{}/export?format={format}", batch_path(batch_id)))
}

/// The batches of an answer of the list route, an empty list when the answer
/// carries none.
fn batches_of(mut answer: Value) -> Value {
    answer
        .get_mut("batches")
        .map(Value::take)
        .unwrap_or_else(|| json!([]))
}

/// The decision the item `rank` of `batch_id` waits on, among every decision
/// waiting for the user.
fn decision_of(mut answer: Value, batch_id: &str, rank: u64) -> Option<Value> {
    let items = answer.get_mut("items")?.as_array_mut()?;
    let found = items.iter().position(|item| {
        let origin = &item["origin"];
        origin["batchId"].as_str() == Some(batch_id) && origin["rank"].as_u64() == Some(rank)
    })?;
    Some(items.swap_remove(found))
}

pub(crate) async fn list_on(link: &RuntimeLink, limit: u32) -> Result<Value, String> {
    let answer = link
        .get_json(&format!("{BATCHES_ROUTE}?limit={limit}"))
        .await?;
    Ok(batches_of(answer))
}

pub(crate) async fn control_on(
    link: &RuntimeLink,
    batch_id: &str,
    action: &str,
) -> Result<Value, String> {
    link.post_json(&control_path(batch_id, action)?, &json!({}))
        .await
}

pub(crate) async fn export_on(
    link: &RuntimeLink,
    batch_id: &str,
    format: &str,
    target: &Path,
) -> Result<Value, String> {
    let bytes = link.get_for_file(&export_path(batch_id, format)?).await?;
    std::fs::write(target, &bytes).map_err(|e| format!("{}: {e}", target.display()))?;
    tracing::event!(
        tracing::Level::INFO,
        batch_id = %batch_id,
        format = %format,
        bytes = bytes.len(),
        "batch.export.written"
    );
    Ok(json!({ "path": target.display().to_string(), "bytes": bytes.len() }))
}

pub(crate) async fn delete_on(link: &RuntimeLink, batch_id: &str) -> Result<Value, String> {
    let deleted = link.delete_json(&batch_path(batch_id)).await?;
    tracing::event!(
        tracing::Level::INFO,
        batch_id = %batch_id,
        "batch.delete.requested"
    );
    Ok(deleted)
}

pub(crate) async fn decision_on(
    link: &RuntimeLink,
    batch_id: &str,
    rank: u64,
) -> Result<Option<Value>, String> {
    Ok(decision_of(
        link.get_json(PENDING_ROUTE).await?,
        batch_id,
        rank,
    ))
}

pub(crate) async fn decide_on(link: &RuntimeLink, id: &str, answer: &str) -> Result<Value, String> {
    if !ANSWERS.contains(&answer) {
        return Err(format!("unknown answer '{answer}'"));
    }
    link.post_json(
        &format!("{PENDING_ROUTE}/decide"),
        &json!({ "id": id, "answer": answer }),
    )
    .await
}

/// The `limit` most recent batches, newest first. The history asks for more
/// as the user reads on, so no ceiling is set here.
#[tauri::command]
pub async fn list_batches(link: State<'_, RuntimeLink>, limit: u32) -> Result<Value, String> {
    list_on(link.inner(), limit).await
}

/// The batch `batch_id` with its items, `None` when no batch bears that id.
#[tauri::command]
pub async fn get_batch(
    link: State<'_, RuntimeLink>,
    batch_id: String,
) -> Result<Option<Value>, String> {
    link.get_json_or_none(&batch_path(&batch_id)).await
}

/// Pauses, resumes or stops the batch `batch_id`, or runs its failed and
/// waiting items again (`action` is `pause`, `resume`, `stop` or `retry`), and
/// returns the batch as changed. A refusal of the runtime comes back as its
/// message.
#[tauri::command]
pub async fn control_batch(
    link: State<'_, RuntimeLink>,
    batch_id: String,
    action: String,
) -> Result<Value, String> {
    control_on(link.inner(), &batch_id, &action).await
}

/// Writes the result of the batch `batch_id` to `path`, as `format` (`xlsx`,
/// `csv` or `json`), and answers the file written and its size.
#[tauri::command]
pub async fn export_batch(
    link: State<'_, RuntimeLink>,
    batch_id: String,
    format: String,
    path: String,
) -> Result<Value, String> {
    export_on(link.inner(), &batch_id, &format, Path::new(&path)).await
}

/// Deletes the batch `batch_id` and its items, and answers how many items
/// went with it. The runtime refuses an active batch, and its refusal comes
/// back as its message. Files the batch was exported to are not touched.
#[tauri::command]
pub async fn delete_batch(link: State<'_, RuntimeLink>, batch_id: String) -> Result<Value, String> {
    delete_on(link.inner(), &batch_id).await
}

/// The decision the item `rank` of the batch `batch_id` waits on, `None` when
/// it waits on none any more.
#[tauri::command]
pub async fn batch_item_decision(
    link: State<'_, RuntimeLink>,
    batch_id: String,
    rank: u64,
) -> Result<Option<Value>, String> {
    decision_on(link.inner(), &batch_id, rank).await
}

/// Answers the decision `id` of a batch item with `answer` (`once`, `always`
/// or `refuse`), and returns what the answer did.
#[tauri::command]
pub async fn decide_batch_item(
    link: State<'_, RuntimeLink>,
    id: String,
    answer: String,
) -> Result<Value, String> {
    decide_on(link.inner(), &id, &answer).await
}

#[cfg(test)]
mod tests {
    use wiremock::matchers::{body_json, header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::attached::AttachedClient;

    const TOKEN: &str = "token-of-this-installation";

    fn attached(server: &MockServer) -> RuntimeLink {
        RuntimeLink::Attached(AttachedClient::new(server.uri(), TOKEN).unwrap())
    }

    #[test]
    fn test_only_the_four_controls_of_a_batch_have_a_route() {
        // GIVEN a batch whose id would open a second path segment
        let batch_id = "b/1";

        // WHEN the routes of a retry and of an unknown action are built
        let retry = control_path(batch_id, "retry");
        let unknown = control_path(batch_id, "delete");

        // THEN the id stays one segment, and the unknown action has no route
        assert_eq!(retry.unwrap(), "/api/v1/batches/b%2F1/retry");
        assert!(unknown.unwrap_err().contains("delete"));
    }

    #[test]
    fn test_an_export_format_outside_the_three_is_refused_before_any_request() {
        // GIVEN the three formats and one that is not among them
        // WHEN their routes are built
        let csv = export_path("b-1", "csv");
        let pdf = export_path("b-1", "pdf");

        // THEN the known one has a route and the other a reason
        assert_eq!(csv.unwrap(), "/api/v1/batches/b-1/export?format=csv");
        assert!(pdf.unwrap_err().contains("pdf"));
    }

    #[test]
    fn test_the_decision_of_an_item_is_found_by_its_batch_and_rank() {
        // GIVEN decisions of two batches and of a conversation
        let listed = json!({ "items": [
            { "id": "consent:1", "origin": { "sessionId": "s-1" } },
            { "id": "batch:b-2:4", "origin": { "batchId": "b-2", "rank": 4 } },
            { "id": "batch:b-1:4", "origin": { "batchId": "b-1", "rank": 4 } },
        ]});

        // WHEN the decision of item 4 of b-1 is asked for, then of an item that waits on none
        let found = decision_of(listed.clone(), "b-1", 4);
        let absent = decision_of(listed, "b-1", 5);

        // THEN the one decision of that item comes back, and nothing for the other
        assert_eq!(found.unwrap()["id"], "batch:b-1:4");
        assert!(absent.is_none());
    }

    #[tokio::test]
    async fn test_the_history_asks_the_daemon_for_as_many_batches_as_the_screen_reads() {
        // GIVEN a daemon that lists two batches when asked for 101
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/batches"))
            .and(query_param("limit", "101"))
            .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(
                    json!({ "batches": [{ "batchId": "b-2" }, { "batchId": "b-1" }] }),
                ),
            )
            .expect(1)
            .mount(&server)
            .await;

        // WHEN the history reads its second page
        let batches = list_on(&attached(&server), 101).await.unwrap();

        // THEN it receives the list alone, in the order of the daemon
        assert_eq!(batches, json!([{ "batchId": "b-2" }, { "batchId": "b-1" }]));
    }

    #[tokio::test]
    async fn test_a_control_is_posted_to_its_route_and_a_refusal_comes_back() {
        // GIVEN a daemon that pauses b-1 and refuses to resume it
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/batches/b-1/pause"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({ "batchId": "b-1", "state": "paused" })),
            )
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/v1/batches/b-1/resume"))
            .respond_with(
                ResponseTemplate::new(409)
                    .set_body_json(json!({ "error": "cannot resume batch 'b-1': it is stopped" })),
            )
            .mount(&server)
            .await;
        let link = attached(&server);

        // WHEN the view pauses the batch, then resumes it
        let paused = control_on(&link, "b-1", "pause").await.unwrap();
        let refused = control_on(&link, "b-1", "resume").await;

        // THEN the batch comes back paused, and the refusal carries its reason
        assert_eq!(paused["state"], "paused");
        let reason = refused.unwrap_err();
        assert!(reason.contains("it is stopped"), "{reason}");
    }

    #[tokio::test]
    async fn test_an_export_is_written_to_the_file_the_user_chose() {
        // GIVEN a daemon that answers the CSV of b-1
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/batches/b-1/export"))
            .and(query_param("format", "csv"))
            .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/csv")
                    .set_body_string("client,output\nAcme,quote\n"),
            )
            .expect(1)
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("batch-b-1.csv");

        // WHEN the view exports the batch there
        let written = export_on(&attached(&server), "b-1", "csv", &target)
            .await
            .unwrap();

        // THEN the file holds the bytes of the daemon, and the answer names it
        assert_eq!(
            std::fs::read_to_string(&target).unwrap(),
            "client,output\nAcme,quote\n"
        );
        assert_eq!(written["bytes"], 25);
        assert_eq!(written["path"], target.display().to_string());
    }

    #[tokio::test]
    async fn test_a_deletion_is_sent_to_the_route_of_the_batch_and_a_refusal_comes_back() {
        // GIVEN a daemon that deletes b-1 and refuses to delete b-2, still running
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/api/v1/batches/b-1"))
            .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({ "batchId": "b-1", "items": 4 })),
            )
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("DELETE"))
            .and(path("/api/v1/batches/b-2"))
            .respond_with(ResponseTemplate::new(409).set_body_json(
                json!({ "error": "cannot delete batch 'b-2': it is running; stop it first" }),
            ))
            .mount(&server)
            .await;
        let link = attached(&server);

        // WHEN the history deletes both
        let deleted = delete_on(&link, "b-1").await.unwrap();
        let refused = delete_on(&link, "b-2").await;

        // THEN the first answers its items removed, the second carries the reason
        assert_eq!(deleted["items"], 4);
        let reason = refused.unwrap_err();
        assert!(reason.contains("it is running"), "{reason}");
    }

    #[tokio::test]
    async fn test_an_item_decision_is_answered_through_the_route_of_the_waiting_decisions() {
        // GIVEN a daemon where item 3 of b-1 waits on an access
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/pending-decisions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "items": [{
                "id": "batch:b-1:3", "kind": "batch_item", "access": "send_email",
                "answers": ["once", "always", "refuse"],
                "origin": { "batchId": "b-1", "rank": 3 },
            }]})))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/v1/pending-decisions/decide"))
            .and(body_json(
                json!({ "id": "batch:b-1:3", "answer": "always" }),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                json!({ "id": "batch:b-1:3", "answer": "always", "resumed": 1, "message": "ok" }),
            ))
            .expect(1)
            .mount(&server)
            .await;
        let link = attached(&server);

        // WHEN the view opens the decision, answers it, then sends an answer no item takes
        let decision = decision_on(&link, "b-1", 3).await.unwrap().unwrap();
        let decided = decide_on(&link, decision["id"].as_str().unwrap(), "always")
            .await
            .unwrap();
        let unknown = decide_on(&link, "batch:b-1:3", "answer").await;

        // THEN the item runs again, and the unknown answer never reaches the daemon
        assert_eq!(decision["access"], "send_email");
        assert_eq!(decided["resumed"], 1);
        assert!(unknown.unwrap_err().contains("answer"));
    }
}
