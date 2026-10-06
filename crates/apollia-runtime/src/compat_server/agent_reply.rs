//! The answer of an agent or a workflow, written as a model's answer in the
//! format of the endpoint the client called: one JSON body, or an event
//! stream.
//!
//! What every format shares lives here: waiting for the end of the run,
//! queueing the events of a stream, the error a run that did not answer ends
//! on. The steps of the run go where the format puts a model's reasoning, so a
//! client that shows it shows the work and one that does not ignores it; the
//! final answer goes where the format puts the text. A run that waits for a
//! decision ends on an `approval_pending` error that names the pending item
//! and the task, which the native API answers and follows.
//!
//! This module also holds the format of `POST /v1/chat/completions`
//! ([`ChatAnswer`]): the steps in `reasoning_content`, the field
//! `llama-server` uses for a model's reasoning, and the answer in `content`.

use std::collections::VecDeque;
use std::convert::Infallible;
use std::time::Duration;

use axum::body::{Body, Bytes};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::{json, Map, Value};

use super::agent_run::{Progress, RunOutcome, StartedRun, Usage};
use super::errors::{error_body, error_with, ErrorKind, Family};

/// How long a stream stays silent before a comment line is sent: a client
/// that went away is noticed at the next write, and its run is stopped.
const KEEP_ALIVE: Duration = Duration::from_secs(5);

/// What every answer names: the run it comes from, its creation time and the
/// model the client called.
#[derive(Debug, Clone)]
pub(crate) struct AnswerHeader {
    pub(crate) run_id: String,
    pub(crate) created: i64,
    pub(crate) model: String,
}

impl AnswerHeader {
    pub(crate) fn new(run_id: &str, model: &str) -> Self {
        Self {
            run_id: run_id.to_owned(),
            created: chrono::Utc::now().timestamp(),
            model: model.to_owned(),
        }
    }
}

/// How one endpoint writes the answer of a run.
///
/// A stream is written in order: [`Self::opening`] once, [`Self::step`] for
/// each step of the run, then either [`Self::answered`] or [`Self::failed`].
pub(crate) trait AnswerFormat: Send + 'static {
    /// The family whose error envelope the endpoint answers in.
    fn family(&self) -> Family;

    /// The answer of a whole run as one JSON body.
    fn whole(&self, steps: &[String], content: &str, usage: Usage) -> Value;

    /// The events that open a stream.
    fn opening(&mut self) -> Vec<Bytes>;

    /// The events of one step of the run.
    fn step(&mut self, step: &str) -> Vec<Bytes>;

    /// The events that close a stream on the final answer.
    fn answered(&mut self, content: &str, usage: Usage) -> Vec<Bytes>;

    /// The event that closes a stream on `error`, the error body of
    /// [`Self::family`].
    fn failed(&mut self, error: Value) -> Bytes;
}

/// One server-sent event carrying `data` alone.
pub(crate) fn data_event(data: &Value) -> Bytes {
    Bytes::from(format!("data: {data}\n\n"))
}

/// One server-sent event named `name` carrying `data`.
pub(crate) fn named_event(name: &str, data: &Value) -> Bytes {
    Bytes::from(format!("event: {name}\ndata: {data}\n\n"))
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
        RunOutcome::Unapproved { access, task_id } => {
            let mut extra = Map::new();
            extra.insert("access".to_owned(), Value::String(access.clone()));
            extra.insert("task_id".to_owned(), Value::String(task_id));
            (
                ErrorKind::ApprovalPending,
                format!(
                    "The model reached for '{access}', a tool of an MCP server this request \
                     brought. Nobody watches the call to approve it, and the connection ended \
                     with the call, so nothing is left to decide. Allow the tool with a rule \
                     in the developer mode of Apollia, then send the request again."
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

/// The error of a run whose follower stopped without reporting an end.
fn unreported() -> (ErrorKind, String, Map<String, Value>) {
    (
        ErrorKind::RunFailed,
        "the run ended without reporting its end".to_owned(),
        Map::new(),
    )
}

/// Answers `run` in `format`: as an event stream when `streamed`, as one JSON
/// body otherwise.
pub(crate) async fn answer<F: AnswerFormat>(
    run: StartedRun,
    format: F,
    streamed: bool,
) -> Response {
    if streamed {
        answer_streamed(run, format)
    } else {
        answer_whole(run, &format).await
    }
}

/// Waits for the end of `run` and answers it as one JSON body carrying its
/// steps and its final answer.
async fn answer_whole<F: AnswerFormat>(mut run: StartedRun, format: &F) -> Response {
    let mut steps = Vec::new();
    while let Some(progress) = run.progress.recv().await {
        match progress {
            Progress::Step(step) => steps.push(step),
            Progress::Ended(outcome) => {
                run.guard.release();
                return match outcome {
                    RunOutcome::Answered { content, usage } => {
                        Json(format.whole(&steps, &content, usage)).into_response()
                    }
                    other => {
                        let (kind, message, extra) = outcome_error(other);
                        error_with(format.family(), kind, message, extra)
                    }
                };
            }
        }
    }
    let (kind, message, extra) = unreported();
    error_with(format.family(), kind, message, extra)
}

/// Where a stream stands.
struct Streaming<F> {
    run: StartedRun,
    format: F,
    queued: VecDeque<Bytes>,
    ended: bool,
}

impl<F: AnswerFormat> Streaming<F> {
    fn fail(&mut self, (kind, message, extra): (ErrorKind, String, Map<String, Value>)) {
        let error = error_body(self.format.family(), kind, message, extra);
        let event = self.format.failed(error);
        self.queued.push_back(event);
    }

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
                    let events = self.format.step(&step);
                    self.queued.extend(events);
                }
                Ok(Some(Progress::Ended(outcome))) => {
                    self.run.guard.release();
                    self.ended = true;
                    match outcome {
                        RunOutcome::Answered { content, usage } => {
                            let events = self.format.answered(&content, usage);
                            self.queued.extend(events);
                        }
                        other => self.fail(outcome_error(other)),
                    }
                }
                Ok(None) => {
                    self.run.guard.release();
                    self.ended = true;
                    self.fail(unreported());
                }
            }
        }
    }
}

/// Answers `run` as an event stream: what opens it, the events of each step,
/// then the answer, or the error the run ended on. Dropping the stream before
/// its end, as a client that goes away does, stops the run.
fn answer_streamed<F: AnswerFormat>(run: StartedRun, mut format: F) -> Response {
    let queued = VecDeque::from(format.opening());
    let state = Streaming {
        run,
        format,
        queued,
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

/// The answer to `POST /v1/chat/completions`: a first chunk naming the
/// assistant, a `reasoning_content` chunk per step, then the answer in
/// `content`, the finish with the usage, and `[DONE]`.
pub(crate) struct ChatAnswer {
    header: AnswerHeader,
}

impl ChatAnswer {
    pub(crate) fn new(header: AnswerHeader) -> Self {
        Self { header }
    }

    fn id(&self) -> String {
        format!("chatcmpl-{}", self.header.run_id)
    }

    fn chunk(&self, delta: &Value, finish: Option<&str>, usage: Option<Usage>) -> Bytes {
        let mut body = json!({
            "id": self.id(),
            "object": "chat.completion.chunk",
            "created": self.header.created,
            "model": self.header.model,
            "choices": [{
                "index": 0,
                "delta": delta,
                "finish_reason": finish,
            }],
        });
        if let Some(usage) = usage {
            body["usage"] = chat_usage(usage);
        }
        data_event(&body)
    }
}

fn chat_usage(usage: Usage) -> Value {
    json!({
        "prompt_tokens": usage.prompt,
        "completion_tokens": usage.completion,
        "total_tokens": usage.prompt + usage.completion,
    })
}

impl AnswerFormat for ChatAnswer {
    fn family(&self) -> Family {
        Family::OpenAi
    }

    fn whole(&self, steps: &[String], content: &str, usage: Usage) -> Value {
        let mut message = Map::new();
        message.insert("role".to_owned(), json!("assistant"));
        message.insert("content".to_owned(), json!(content));
        if !steps.is_empty() {
            message.insert("reasoning_content".to_owned(), json!(steps.concat()));
        }
        json!({
            "id": self.id(),
            "object": "chat.completion",
            "created": self.header.created,
            "model": self.header.model,
            "choices": [{
                "index": 0,
                "message": message,
                "finish_reason": "stop",
            }],
            "usage": chat_usage(usage),
        })
    }

    fn opening(&mut self) -> Vec<Bytes> {
        vec![self.chunk(&json!({ "role": "assistant", "content": "" }), None, None)]
    }

    fn step(&mut self, step: &str) -> Vec<Bytes> {
        vec![self.chunk(&json!({ "reasoning_content": step }), None, None)]
    }

    fn answered(&mut self, content: &str, usage: Usage) -> Vec<Bytes> {
        vec![
            self.chunk(&json!({ "content": content }), None, None),
            self.chunk(&json!({}), Some("stop"), Some(usage)),
            Bytes::from_static(b"data: [DONE]\n\n"),
        ]
    }

    fn failed(&mut self, error: Value) -> Bytes {
        data_event(&error)
    }
}
