//! Tauri IPC commands for the logs of the local server, on the developer
//! screen and in the window the logs detach into.
//!
//! - [`watch_server_logs`]: `GET /api/v1/server/logs?level=…`, an SSE stream
//!   relayed to the window that asked, batch by batch
//! - [`unwatch_server_logs`]: ends the stream of that window
//! - [`clear_server_logs`]: `DELETE /api/v1/server/logs`
//!
//! The logs live in the runtime: the stream is opened through the seam, so it
//! serves the embedded runtime and the daemon alike. The level is applied by
//! the route, never here: a change of level is a new stream.

use std::collections::BTreeMap;
use std::sync::{Mutex, PoisonError};

use serde::Serialize;
use tauri::async_runtime::JoinHandle;
use tauri::{AppHandle, Emitter, State, WebviewWindow};

use crate::attached::{Frame, RuntimeLink, SseReader};

const LOGS_PATH: &str = "/api/v1/server/logs";

/// Event carrying the lines one chunk of the stream completed.
const LINES_EVENT: &str = "server-logs:lines";

/// Event sent when a stream ends without having been asked to.
const CLOSED_EVENT: &str = "server-logs:closed";

/// SSE event name of a frame that carries one line, as the runtime names it.
const LOG_FRAME: &str = "log";

/// The levels the route filters on.
const LEVELS: [&str; 4] = ["debug", "info", "warn", "error"];

/// The stream each window reads, by window label: a window reads one stream,
/// and asking for another ends the one before. The map holds handles to end
/// tasks, no state of theirs; each task owns its stream alone.
static STREAMS: Mutex<BTreeMap<String, JoinHandle<()>>> = Mutex::new(BTreeMap::new());

/// What [`LINES_EVENT`] carries.
#[derive(Debug, Clone, Serialize)]
struct LogLines<'a> {
    /// The watch the lines answer, as the window named it.
    watch: &'a str,
    /// The lines, oldest first, each as the runtime sent it.
    lines: Vec<serde_json::Value>,
}

/// What [`CLOSED_EVENT`] carries.
#[derive(Debug, Clone, Serialize)]
struct LogsClosed<'a> {
    /// The watch that ended.
    watch: &'a str,
}

/// The path of the stream at `level`, refused for a level the route does not
/// know.
fn stream_path(level: &str) -> Result<String, String> {
    if LEVELS.contains(&level) {
        Ok(format!("{LOGS_PATH}?level={level}"))
    } else {
        Err(format!("unknown log level: {level}"))
    }
}

/// The lines `frames` carry, in order. A frame of another name, or one whose
/// data is not JSON, carries none.
fn lines_of(frames: &[Frame]) -> Vec<serde_json::Value> {
    frames
        .iter()
        .filter(|frame| frame.event == LOG_FRAME)
        .filter_map(|frame| serde_json::from_str(&frame.data).ok())
        .collect()
}

/// How many lines the route emptied, read from its answer.
fn cleared_count(json: &serde_json::Value) -> Result<u64, String> {
    json.get("cleared")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| "invalid server logs response: no cleared count".to_owned())
}

/// Ends the stream the window `label` reads, when it reads one.
pub(crate) fn stop_stream(label: &str) {
    let ended = STREAMS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .remove(label);
    if let Some(task) = ended {
        task.abort();
    }
}

fn keep_stream(label: String, task: JoinHandle<()>) {
    let replaced = STREAMS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(label, task);
    if let Some(previous) = replaced {
        previous.abort();
    }
}

/// Reads one open stream to its end, handing `sink` the lines of each chunk
/// that completed some. Answers `false` when `sink` asked to stop, `true` when
/// the stream ended or failed on its own.
async fn read_lines(
    mut response: reqwest::Response,
    mut sink: impl FnMut(Vec<serde_json::Value>) -> bool,
) -> bool {
    let mut reader = SseReader::default();
    loop {
        // SAFETY: an open-ended log stream has no total size to cap; each
        // frame is bounded instead, by the limit `SseReader` holds.
        let chunk = match response.chunk().await {
            Ok(Some(chunk)) => chunk,
            Ok(None) => return true,
            Err(error) => {
                tracing::event!(
                    tracing::Level::DEBUG,
                    error = %error,
                    "desktop.server_logs.interrupted"
                );
                return true;
            }
        };
        let Ok(frames) = reader.push(&chunk) else {
            tracing::event!(tracing::Level::WARN, "desktop.server_logs.unreadable");
            return true;
        };
        let lines = lines_of(&frames);
        if !lines.is_empty() && !sink(lines) {
            return false;
        }
    }
}

/// Relays one open stream to the window `label` until it ends, fails, or the
/// window stops listening. A stream that ends on its own tells the window.
async fn forward(response: reqwest::Response, app: &AppHandle, label: &str, watch: &str) {
    let ended = read_lines(response, |lines| {
        app.emit_to(label, LINES_EVENT, LogLines { watch, lines })
            .is_ok()
    })
    .await;
    if ended {
        tracing::event!(tracing::Level::INFO, window = %label, "desktop.server_logs.closed");
        let _ = app.emit_to(label, CLOSED_EVENT, LogsClosed { watch });
    }
}

/// Streams the logs of the local server, at `level` and above, to the window
/// that asks: what the runtime holds first, then each line as it is recorded.
/// The lines arrive as `server-logs:lines` events that name `watch`.
///
/// Delegates to `GET /api/v1/server/logs`. A window reads one stream: asking
/// again, at another level, ends the one before.
#[tauri::command]
pub async fn watch_server_logs(
    app: AppHandle,
    window: WebviewWindow,
    state: State<'_, RuntimeLink>,
    watch: String,
    level: String,
) -> Result<(), String> {
    let path = stream_path(&level)?;
    let label = window.label().to_owned();
    stop_stream(&label);
    let response = state.open_stream(&path).await?;
    let task_label = label.clone();
    let task = tauri::async_runtime::spawn(async move {
        forward(response, &app, &task_label, &watch).await;
    });
    keep_stream(label, task);
    Ok(())
}

/// Ends the stream the asking window reads.
#[tauri::command]
pub async fn unwatch_server_logs(window: WebviewWindow) -> Result<(), String> {
    stop_stream(window.label());
    Ok(())
}

/// Empties the logs of the local server and answers how many lines they
/// held. The audit journal is not touched.
///
/// Delegates to `DELETE /api/v1/server/logs`.
#[tauri::command]
pub async fn clear_server_logs(state: State<'_, RuntimeLink>) -> Result<u64, String> {
    cleared_count(&state.delete_json(LOGS_PATH).await?)
}

#[cfg(test)]
mod tests {
    use wiremock::matchers::{header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::attached::AttachedClient;

    const TOKEN: &str = "installation-token";

    /// The body of the logs route: two lines and a keep-alive between them.
    const STREAM: &str = "event: log\ndata: {\"seq\":7,\"level\":\"warn\",\"source\":\"request\"}\n\n\
                          : keep-alive\n\n\
                          event: log\ndata: {\"seq\":8,\"level\":\"error\",\"source\":\"engine\"}\n\n";

    async fn logs_route(server: &MockServer, level: &str) {
        Mock::given(method("GET"))
            .and(path(LOGS_PATH))
            .and(query_param("level", level))
            .and(header("accept", "text/event-stream"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(STREAM, "text/event-stream"))
            .expect(1)
            .mount(server)
            .await;
    }

    #[tokio::test]
    async fn test_the_stream_of_a_daemon_is_read_line_by_line_at_the_level_asked() {
        // GIVEN a daemon whose logs route streams two lines at `warn`, to the
        // token of this installation only
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(LOGS_PATH))
            .and(query_param("level", "warn"))
            .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
            .respond_with(ResponseTemplate::new(200).set_body_raw(STREAM, "text/event-stream"))
            .expect(1)
            .mount(&server)
            .await;
        let link = RuntimeLink::Attached(AttachedClient::new(server.uri(), TOKEN).unwrap());

        // WHEN the stream is opened through the seam at that level and read
        let response = link
            .open_stream(&stream_path("warn").unwrap())
            .await
            .unwrap();
        let mut seen = Vec::new();
        let ended = read_lines(response, |lines| {
            seen.extend(lines);
            true
        })
        .await;

        // THEN the two lines come out in order, and the end of the stream is
        //      told apart from a reader that stopped
        assert_eq!(seen.len(), 2);
        assert_eq!(seen[0]["seq"], 7);
        assert_eq!(seen[1]["source"], "engine");
        assert!(ended);
    }

    #[tokio::test]
    async fn test_the_stream_of_the_runtime_in_this_process_is_read_the_same_way() {
        // GIVEN the API of a runtime in this process, on its local port
        let server = MockServer::start().await;
        logs_route(&server, "debug").await;

        // WHEN the stream is opened on that port and read by a window that
        // stops listening at the first batch
        let response = crate::commands::http_open_stream(
            server.address().port(),
            &stream_path("debug").unwrap(),
        )
        .await
        .unwrap();
        let mut batches = 0;
        let ended = read_lines(response, |_| {
            batches += 1;
            false
        })
        .await;

        // THEN the reading stops there, and is not taken for a stream that ended
        assert_eq!(batches, 1);
        assert!(!ended);
    }

    #[tokio::test]
    async fn test_a_runtime_without_the_server_refuses_the_stream_in_its_own_words() {
        // GIVEN a runtime whose logs route answers that it has no server
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(LOGS_PATH))
            .respond_with(ResponseTemplate::new(503).set_body_json(
                serde_json::json!({ "error": "no compatible server in this runtime" }),
            ))
            .mount(&server)
            .await;
        let link = RuntimeLink::Attached(AttachedClient::new(server.uri(), TOKEN).unwrap());

        // WHEN the stream is opened in each mode
        let attached = link.open_stream(&stream_path("info").unwrap()).await;
        let embedded = crate::commands::http_open_stream(
            server.address().port(),
            &stream_path("info").unwrap(),
        )
        .await;

        // THEN both refuse with the words of the runtime, which the pane shows
        assert_eq!(
            attached.err().as_deref(),
            Some("no compatible server in this runtime")
        );
        assert_eq!(
            embedded.err().as_deref(),
            Some("no compatible server in this runtime")
        );
    }

    fn frame(event: &str, data: &str) -> Frame {
        Frame {
            event: event.to_owned(),
            data: data.to_owned(),
        }
    }

    #[test]
    fn test_the_level_is_handed_to_the_route_and_an_unknown_one_is_refused() {
        // GIVEN the four levels the route knows, and one it does not
        // WHEN the path of the stream is built for each
        let paths: Vec<String> = LEVELS
            .iter()
            .map(|level| stream_path(level).unwrap())
            .collect();
        let unknown = stream_path("verbose");

        // THEN the route receives the level in its query, so it filters
        assert_eq!(paths[0], "/api/v1/server/logs?level=debug");
        assert_eq!(paths[2], "/api/v1/server/logs?level=warn");
        assert_eq!(paths.len(), 4);
        // AND a level it does not know never reaches it
        assert_eq!(unknown, Err("unknown log level: verbose".to_owned()));
    }

    #[test]
    fn test_the_log_frames_of_a_chunk_become_lines_in_their_order() {
        // GIVEN a chunk of the stream: two lines around a keep-alive and a
        // frame of another name
        let mut reader = SseReader::default();
        let frames = reader
            .push(
                b"event: log\ndata: {\"seq\":4,\"level\":\"info\"}\n\n\
                  : keep-alive\n\n\
                  event: other\ndata: {\"seq\":99}\n\n\
                  event: log\ndata: {\"seq\":5,\"level\":\"error\"}\n\n",
            )
            .unwrap();

        // WHEN the lines are taken from it
        let lines = lines_of(&frames);

        // THEN the two lines come out, oldest first, as the runtime sent them
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0]["seq"], 4);
        assert_eq!(lines[1]["level"], "error");
    }

    #[test]
    fn test_a_log_frame_that_is_not_json_carries_no_line() {
        // GIVEN a log frame cut short
        let frames = [frame(LOG_FRAME, "{\"seq\":")];

        // WHEN the lines are taken from it
        let lines = lines_of(&frames);

        // THEN nothing is handed to the window
        assert!(lines.is_empty());
    }

    #[test]
    fn test_the_cleared_count_is_read_from_the_answer_of_the_route() {
        // GIVEN the answer of the route that empties the logs, and a body
        // without its count
        let answered = serde_json::json!({ "cleared": 12 });
        let silent = serde_json::json!({});

        // WHEN the count is read from each
        let count = cleared_count(&answered);
        let missing = cleared_count(&silent);

        // THEN the screen is told how many lines went, or that the answer
        //      could not be read
        assert_eq!(count, Ok(12));
        assert!(missing.unwrap_err().contains("no cleared count"));
    }

    #[test]
    fn test_the_lines_event_names_its_watch() {
        // GIVEN the lines of one chunk, for the watch a window named
        let payload = LogLines {
            watch: "main-3",
            lines: vec![serde_json::json!({ "seq": 1 })],
        };

        // WHEN the event is serialized for the webview
        let sent = serde_json::to_value(&payload).unwrap();

        // THEN the window can tell its own lines from a stream it ended
        assert_eq!(sent["watch"], "main-3");
        assert_eq!(sent["lines"][0]["seq"], 1);
    }
}
