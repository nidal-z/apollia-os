//! The answer of an agent or a workflow, written as a model's answer to
//! `POST /v1/chat/completions`: one JSON body, or an event stream of chunks.
//!
//! The steps of the run go in `reasoning_content`, the field `llama-server`
//! uses for a model's reasoning, so a client that shows it shows the work and
//! one that does not ignores it; the final answer goes in `content`. A run that
//! waits for a decision ends on an `approval_pending` error that names the
//! pending item and the task, which the native API answers and follows.

use std::convert::Infallible;
use std::time::Duration;

use axum::body::{Body, Bytes};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::{json, Map, Value};

use super::agent_run::{Progress, RunOutcome, StartedRun, Usage};
use super::errors::{openai_error_body, openai_error_with, ErrorKind};

/// How long a stream stays silent before a comment line is sent: a client
/// that went away is noticed at the next write, and its run is stopped.
const KEEP_ALIVE: Duration = Duration::from_secs(5);

/// What every chunk and the body name: the answer's identifier, its creation
/// time and the model the client called.
#[derive(Debug, Clone)]
pub(crate) struct AnswerHeader {
    pub(crate) id: String,
    pub(crate) created: i64,
    pub(crate) model: String,
}

impl AnswerHeader {
    pub(crate) fn new(run_id: &str, model: &str) -> Self {
        Self {
            id: format!("chatcmpl-{run_id}"),
            created: chrono::Utc::now().timestamp(),
            model: model.to_owned(),
        }
    }
}

fn usage_body(usage: Usage) -> Value {
    json!({
        "prompt_tokens": usage.prompt,
        "completion_tokens": usage.completion,
        "total_tokens": usage.prompt + usage.completion,
    })
}

/// The error a run that did not answer ends on.
fn outcome_error(outcome: RunOutcome) -> (ErrorKind, String, Map<String, Value>) {
    match outcome {
        RunOutcome::Pending {
            pending_id,
            batch_id,
            task_id,
            access,
        } => {
            let mut extra = Map::new();
            extra.insert("pending_id".to_owned(), Value::String(pending_id.clone()));
            extra.insert("batch_id".to_owned(), Value::String(batch_id));
            extra.insert(
                "task_id".to_owned(),
                task_id.map_or(Value::Null, Value::String),
            );
            extra.insert(
                "access".to_owned(),
                access.map_or(Value::Null, Value::String),
            );
            (
                ErrorKind::ApprovalPending,
                format!(
                    "The agent waits for your approval to use an access nobody granted. \
                     Decide it in Apollia, or through the native API with \
                     POST /api/v1/pending-decisions/decide and the id '{pending_id}'; \
                     the run then goes on and its result is read from the task."
                ),
                extra,
            )
        }
        RunOutcome::Failed { message } => (ErrorKind::RunFailed, message, Map::new()),
        // Every caller answers this case itself before asking for an error.
        RunOutcome::Answered { .. } => (
            ErrorKind::RunFailed,
            "the run answered".to_owned(),
            Map::new(),
        ),
    }
}

/// Waits for the end of `run` and answers it as one JSON body. The steps are
/// joined in the message's `reasoning_content`.
pub(crate) async fn answer_whole(mut run: StartedRun, header: AnswerHeader) -> Response {
    let mut steps = String::new();
    while let Some(progress) = run.progress.recv().await {
        match progress {
            Progress::Step(step) => steps.push_str(&step),
            Progress::Ended(outcome) => {
                run.guard.release();
                return match outcome {
                    RunOutcome::Answered { content, usage } => {
                        let mut message = Map::new();
                        message.insert("role".to_owned(), json!("assistant"));
                        message.insert("content".to_owned(), Value::String(content));
                        if !steps.is_empty() {
                            message.insert("reasoning_content".to_owned(), Value::String(steps));
                        }
                        Json(json!({
                            "id": header.id,
                            "object": "chat.completion",
                            "created": header.created,
                            "model": header.model,
                            "choices": [{
                                "index": 0,
                                "message": message,
                                "finish_reason": "stop",
                            }],
                            "usage": usage_body(usage),
                        }))
                        .into_response()
                    }
                    other => {
                        let (kind, message, extra) = outcome_error(other);
                        openai_error_with(kind, message, extra)
                    }
                };
            }
        }
    }
    openai_error_with(
        ErrorKind::RunFailed,
        "the run ended without reporting its end".to_owned(),
        Map::new(),
    )
}

fn event(data: &Value) -> Bytes {
    Bytes::from(format!("data: {data}\n\n"))
}

fn chunk(header: &AnswerHeader, delta: Value, finish: Option<&str>, usage: Option<Usage>) -> Bytes {
    let mut body = json!({
        "id": header.id,
        "object": "chat.completion.chunk",
        "created": header.created,
        "model": header.model,
        "choices": [{
            "index": 0,
            "delta": delta,
            "finish_reason": finish,
        }],
    });
    if let Some(usage) = usage {
        body["usage"] = usage_body(usage);
    }
    event(&body)
}

/// The chunks that close a stream on `outcome`.
fn closing(header: &AnswerHeader, outcome: RunOutcome) -> Vec<Bytes> {
    match outcome {
        RunOutcome::Answered { content, usage } => vec![
            chunk(header, json!({ "content": content }), None, None),
            chunk(header, json!({}), Some("stop"), Some(usage)),
            Bytes::from_static(b"data: [DONE]\n\n"),
        ],
        other => {
            let (kind, message, extra) = outcome_error(other);
            vec![event(&openai_error_body(kind, message, extra))]
        }
    }
}

/// Where a stream stands.
struct Streaming {
    run: StartedRun,
    header: AnswerHeader,
    queued: std::collections::VecDeque<Bytes>,
    ended: bool,
}

impl Streaming {
    async fn next(mut self) -> Option<(Result<Bytes, Infallible>, Self)> {
        loop {
            if let Some(bytes) = self.queued.pop_front() {
                return Some((Ok(bytes), self));
            }
            if self.ended {
                return None;
            }
            let received = tokio::time::timeout(KEEP_ALIVE, self.run.progress.recv()).await;
            match received {
                Err(_) => return Some((Ok(Bytes::from_static(b": keep-alive\n\n")), self)),
                Ok(Some(Progress::Step(step))) => {
                    let bytes = chunk(
                        &self.header,
                        json!({ "reasoning_content": step }),
                        None,
                        None,
                    );
                    return Some((Ok(bytes), self));
                }
                Ok(Some(Progress::Ended(outcome))) => {
                    self.run.guard.release();
                    self.ended = true;
                    self.queued.extend(closing(&self.header, outcome));
                }
                Ok(None) => {
                    self.run.guard.release();
                    self.ended = true;
                    self.queued.push_back(event(&openai_error_body(
                        ErrorKind::RunFailed,
                        "the run ended without reporting its end".to_owned(),
                        Map::new(),
                    )));
                }
            }
        }
    }
}

/// Answers `run` as an event stream: a first chunk naming the assistant, a
/// `reasoning_content` chunk per step, then the answer in `content`, the
/// finish with the usage, and `[DONE]`; or the error the run ended on. Dropping
/// the stream before its end, as a client that goes away does, stops the run.
pub(crate) fn answer_streamed(run: StartedRun, header: AnswerHeader) -> Response {
    let opening = chunk(
        &header,
        json!({ "role": "assistant", "content": "" }),
        None,
        None,
    );
    let state = Streaming {
        run,
        header,
        queued: std::collections::VecDeque::from([opening]),
        ended: false,
    };
    let body = Body::from_stream(futures::stream::unfold(state, Streaming::next));
    (
        StatusCode::OK,
        [
            (
                header::CONTENT_TYPE,
                HeaderValue::from_static("text/event-stream"),
            ),
            (header::CACHE_CONTROL, HeaderValue::from_static("no-cache")),
        ],
        body,
    )
        .into_response()
}
