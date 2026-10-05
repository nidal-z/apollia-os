//! The journal of the compatible server: one signed entry per request it
//! answered, saying who called and from where.
//!
//! Every request, answered, refused or failed, leaves a `server_call` entry in
//! the audit journal, in the run [`SERVER_CALLS_RUN_ID`], chained and signed
//! as every other entry. The entry names the endpoint, the model or the agent
//! the request named, the name of the token it presented (never its value),
//! the address of the caller and whether that is the machine or the local
//! network, the status, the tokens spent and the duration. It never carries a
//! message, an answer, a header or a query string.
//!
//! The address is the one read when the connection was accepted, which the
//! accept loop leaves on each request as `ConnectInfo`, beside its
//! [`CallOrigin`].
//!
//! The tokens are read where the engine or the run wrote them: in the `usage`
//! of a JSON answer, or in passing in the events of a stream, which is relayed
//! untouched. An answer that states none leaves the two counts absent. The
//! entry of a stream is written when the stream closes, so its duration is the
//! whole answer; a client that goes away before the end closes it too.
//!
//! A call that started an agent or a workflow names the task and the run it
//! started, so the entries of that run are found from the call.
//!
//! This is the durable record. The ring of recent lines in `logs` is a
//! developer's view, cleared at will; nothing here is ever cleared.

use std::net::{IpAddr, SocketAddr};
use std::time::Instant;

use axum::body::{Body, Bytes, HttpBody};
use axum::extract::connect_info::ConnectInfo;
use axum::extract::{Request, State};
use axum::http::header::CONTENT_TYPE;
use axum::middleware::Next;
use axum::response::Response;
use axum::Router;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::agents::MODEL_PREFIX;
use super::auth::presented_token;
use super::errors::{error_response, ErrorKind, Family};
use super::logs::RequestedModel;
use super::origin::CallOrigin;
use super::tokens::ServerTokensHandle;
use crate::audit_journal::{AuditJournalHandle, JournalEntryDraft, JournalEntryKind};

/// The run of the journal that holds the calls to the compatible server.
pub const SERVER_CALLS_RUN_ID: &str = "server:calls";

/// The longest path or model name an entry keeps: both come from the caller.
const MAX_NAME_CHARS: usize = 256;

/// The longest line of a stream kept while its end has not arrived. The line
/// that carries the usage is a few hundred bytes; a longer one is skipped.
const MAX_EVENT_LINE: usize = 256 * 1024;

/// What a call named in its `model` field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServerCallTarget {
    /// A local model, relayed to its engine.
    Model,
    /// An exposed agent or workflow, `apollia/<id>`.
    Agent,
}

/// Payload of a `server_call` journal entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerCallRecord {
    /// Always `server_call`, as the other payloads name their event.
    pub event: String,
    /// The HTTP method of the request.
    pub method: String,
    /// The path the request asked for, without its query string.
    pub endpoint: String,
    /// The model, the agent or the workflow the request named, when it named
    /// one the server could read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    /// Whether `target` is a model, or an agent or a workflow.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_kind: Option<ServerCallTarget>,
    /// The name of the token in use the request presented. Absent when it
    /// presented none, or one that is not in use. Never the value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_name: Option<String>,
    /// The address of the caller, read when its connection was accepted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caller_address: Option<String>,
    /// Whether the caller is the machine itself or a device of the local
    /// network.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<CallOrigin>,
    /// The status the request was answered with.
    pub status: u16,
    /// The tokens of the prompt, when the answer stated them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_tokens: Option<u64>,
    /// The tokens generated, when the answer stated them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completion_tokens: Option<u64>,
    /// How long the answer took, in milliseconds: for a stream, until it
    /// closed.
    pub duration_ms: u64,
    /// The task the call started, when it ran an agent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    /// The run the call started: for an agent, the run its journal entries
    /// are kept under; for a workflow, its workflow run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
}

/// The run a call to an agent or a workflow started, left on its response for
/// the journal.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct LaunchedRun {
    /// The task of an agent run.
    pub(crate) task_id: Option<String>,
    /// The run of the agent in the journal, or the workflow run.
    pub(crate) run_id: Option<String>,
}

/// What the journal layer holds: the journal it writes, and the tokens it
/// asks for the name of the one a request presents.
#[derive(Clone)]
pub(crate) struct CallJournal {
    journal: AuditJournalHandle,
    tokens: ServerTokensHandle,
}

impl CallJournal {
    pub(crate) fn new(journal: AuditJournalHandle, tokens: ServerTokensHandle) -> Self {
        Self { journal, tokens }
    }
}

/// `router` with one journal entry per request, above every other layer, so a
/// request refused by the token check or answered by the cross-origin layer
/// leaves its entry too. Without a journal, `router` unchanged.
pub(crate) fn with_call_journal(router: Router, calls: Option<CallJournal>) -> Router {
    match calls {
        Some(calls) => router.layer(axum::middleware::from_fn_with_state(calls, record_call)),
        None => router,
    }
}

/// `name`, cut at [`MAX_NAME_CHARS`] characters.
fn bounded(name: &str) -> String {
    name.chars().take(MAX_NAME_CHARS).collect()
}

/// The address of a peer as a person reads it: an IPv4 address carried in an
/// IPv6 one is written as the IPv4 address, and the port, which changes with
/// every connection, is left out.
fn address_of(peer: SocketAddr) -> String {
    match peer.ip() {
        IpAddr::V6(v6) => v6
            .to_ipv4_mapped()
            .map_or_else(|| v6.to_string(), |v4| v4.to_string()),
        IpAddr::V4(v4) => v4.to_string(),
    }
}

/// The tokens an answer stated.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Spent {
    prompt: Option<u64>,
    completion: Option<u64>,
}

impl Spent {
    /// Takes what `usage` states, in the names of either family: OpenAI's
    /// `prompt_tokens` and `completion_tokens`, Anthropic's `input_tokens` and
    /// `output_tokens`. A count it does not state is left as it was.
    fn read(&mut self, usage: &Value) {
        let count = |names: [&str; 2]| names.iter().find_map(|name| usage.get(*name)?.as_u64());
        if let Some(prompt) = count(["prompt_tokens", "input_tokens"]) {
            self.prompt = Some(prompt);
        }
        if let Some(completion) = count(["completion_tokens", "output_tokens"]) {
            self.completion = Some(completion);
        }
    }

    /// Takes the usage of one event of a stream: at its root, as a chat
    /// completion chunk and an Anthropic `message_delta` write it, or under
    /// `message` or `response`, as `message_start` and `response.completed`
    /// do.
    fn read_event(&mut self, event: &Value) {
        for usage in [
            event.get("usage"),
            event.get("message").and_then(|m| m.get("usage")),
            event.get("response").and_then(|r| r.get("usage")),
        ]
        .into_iter()
        .flatten()
        {
            self.read(usage);
        }
    }
}

/// The `usage` of a JSON answer, read without keeping anything else of it.
#[derive(Deserialize)]
struct WholeAnswer {
    #[serde(default)]
    usage: Option<Value>,
}

/// Reads the usage out of the events of a stream as its chunks pass. It keeps
/// at most one unfinished line, and reads as JSON only the `data:` lines that
/// name a usage.
#[derive(Debug, Default)]
struct StreamUsage {
    pending: Vec<u8>,
    skipping: bool,
    spent: Spent,
}

impl StreamUsage {
    fn feed(&mut self, chunk: &[u8]) {
        let mut rest = chunk;
        while let Some(end) = rest.iter().position(|byte| *byte == b'\n') {
            let (line, tail) = rest.split_at(end);
            rest = &tail[1..];
            if self.skipping {
                self.skipping = false;
            } else if self.pending.is_empty() {
                self.spent_in(line);
            } else {
                let mut whole = std::mem::take(&mut self.pending);
                whole.extend_from_slice(line);
                self.spent_in(&whole);
            }
        }
        if self.skipping {
            return;
        }
        if self.pending.len() + rest.len() > MAX_EVENT_LINE {
            self.pending.clear();
            self.skipping = true;
        } else {
            self.pending.extend_from_slice(rest);
        }
    }

    fn spent_in(&mut self, line: &[u8]) {
        let Ok(line) = std::str::from_utf8(line) else {
            return;
        };
        let Some(data) = line.strip_prefix("data:") else {
            return;
        };
        if !data.contains("\"usage\"") {
            return;
        }
        if let Ok(event) = serde_json::from_str::<Value>(data.trim()) {
            self.spent.read_event(&event);
        }
    }
}

/// A call whose entry is still to be written. Dropping it writes the entry:
/// at the end of the answer, or when the client goes away before it.
struct Pending {
    journal: AuditJournalHandle,
    record: ServerCallRecord,
    started: Instant,
    usage: StreamUsage,
}

impl Drop for Pending {
    fn drop(&mut self) {
        self.record.prompt_tokens = self.usage.spent.prompt;
        self.record.completion_tokens = self.usage.spent.completion;
        self.record.duration_ms =
            u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX);
        append(&self.journal, &self.record);
    }
}

/// Appends `record` to `journal`, stamped now.
fn append(journal: &AuditJournalHandle, record: &ServerCallRecord) {
    match serde_json::to_value(record) {
        Ok(payload) => {
            journal.append(JournalEntryDraft {
                run_id: SERVER_CALLS_RUN_ID.to_owned(),
                ts: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
                kind: JournalEntryKind::ServerCall,
                payload,
            });
            tracing::event!(
                tracing::Level::DEBUG,
                endpoint = %record.endpoint,
                status = record.status,
                "compat_server.call.journaled"
            );
        }
        Err(error) => tracing::event!(
            tracing::Level::WARN,
            error = %error,
            "compat_server.call.unjournaled"
        ),
    }
}

fn is_event_stream(response: &Response) -> bool {
    response
        .headers()
        .get(CONTENT_TYPE)
        .is_some_and(|value| value.as_bytes().starts_with(b"text/event-stream"))
}

/// Middleware of the compatible server: one journal entry per request.
pub(crate) async fn record_call(
    State(calls): State<CallJournal>,
    request: Request,
    next: Next,
) -> Response {
    let started = Instant::now();
    let method = request.method().to_string();
    let path = request.uri().path().to_owned();
    let peer = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ConnectInfo(peer)| *peer);
    let origin = request.extensions().get::<CallOrigin>().copied();
    let token_name = match presented_token(request.headers()) {
        Some(presented) => calls.tokens.name_of(presented).await,
        None => None,
    };

    let response = next.run(request).await;

    let streamed = is_event_stream(&response);
    let target = response
        .extensions()
        .get::<RequestedModel>()
        .map(|model| bounded(&model.0));
    let launched = response
        .extensions()
        .get::<LaunchedRun>()
        .cloned()
        .unwrap_or_default();
    let record = ServerCallRecord {
        event: JournalEntryKind::ServerCall.tag().to_owned(),
        method,
        endpoint: bounded(&path),
        target_kind: target.as_deref().map(|target| {
            if target.starts_with(MODEL_PREFIX) {
                ServerCallTarget::Agent
            } else {
                ServerCallTarget::Model
            }
        }),
        target,
        token_name,
        caller_address: peer.map(address_of),
        origin: origin.or_else(|| peer.map(CallOrigin::of_peer)),
        status: response.status().as_u16(),
        prompt_tokens: None,
        completion_tokens: None,
        duration_ms: 0,
        task_id: launched.task_id,
        run_id: launched.run_id,
    };
    let mut pending = Pending {
        journal: calls.journal,
        record,
        started,
        usage: StreamUsage::default(),
    };

    let (parts, body) = response.into_parts();
    if streamed || body.size_hint().exact().is_none() {
        // A stream is passed on chunk by chunk as it arrives, never buffered
        // here; its entry is written when it closes.
        let chunks = body.into_data_stream().map(move |chunk| {
            if let (true, Ok(bytes)) = (streamed, &chunk) {
                pending.usage.feed(bytes);
            }
            chunk
        });
        return Response::from_parts(parts, Body::from_stream(chunks));
    }
    // The body is already whole in memory, under the cap its writer read it
    // with: taking it reads nothing new.
    let bytes: Bytes = match axum::body::to_bytes(body, usize::MAX).await {
        Ok(bytes) => bytes,
        Err(error) => {
            tracing::event!(
                tracing::Level::WARN,
                error = %error,
                "compat_server.call.answer_unreadable"
            );
            let refusal = error_response(
                Family::of_path(&path),
                ErrorKind::EngineUnavailable,
                "The answer could not be read.".to_owned(),
            );
            pending.record.status = refusal.status().as_u16();
            return refusal;
        }
    };
    if parts.status.is_success() {
        if let Ok(WholeAnswer { usage: Some(usage) }) = serde_json::from_slice(&bytes) {
            pending.usage.spent.read(&usage);
        }
    }
    drop(pending);
    Response::from_parts(parts, Body::from(bytes))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn test_the_usage_of_a_stream_is_read_across_chunks_in_either_family() {
        // GIVEN an OpenAI stream whose last chunk states the usage, cut in the
        // middle of that line, and an Anthropic stream that states the input
        // at its start and the output at its end
        let openai = [
            "data: {\"choices\":[{\"delta\":{\"content\":\"usage\"}}]}\n\n",
            "data: {\"choices\":[],\"usage\":{\"prompt_",
            "tokens\":12,\"completion_tokens\":5}}\n\ndata: [DONE]\n\n",
        ];
        let anthropic = [
            "event: message_start\ndata: {\"type\":\"message_start\",\"message\":\
             {\"usage\":{\"input_tokens\":30,\"output_tokens\":1}}}\n\n",
            "event: message_delta\ndata: {\"type\":\"message_delta\",\
             \"usage\":{\"output_tokens\":44}}\n\n",
        ];

        // WHEN each passes chunk by chunk
        let read = |chunks: &[&str]| {
            let mut usage = StreamUsage::default();
            for chunk in chunks {
                usage.feed(chunk.as_bytes());
            }
            usage.spent
        };

        // THEN both counts are the ones stated, the last statement winning
        assert_eq!(
            read(&openai),
            Spent {
                prompt: Some(12),
                completion: Some(5)
            }
        );
        assert_eq!(
            read(&anthropic),
            Spent {
                prompt: Some(30),
                completion: Some(44)
            }
        );
    }

    #[test]
    fn test_a_stream_without_usage_or_with_an_endless_line_states_nothing() {
        // GIVEN a stream that states no usage, and one whose first line never
        // ends within the bound before a usage line follows
        let mut plain = StreamUsage::default();
        let mut endless = StreamUsage::default();

        // WHEN each passes
        plain.feed(b"data: {\"n\":1}\n\ndata: [DONE]\n\n");
        endless.feed(&vec![b'x'; MAX_EVENT_LINE + 1]);
        let kept_while_skipping = endless.pending.len();
        endless.feed(b"tail\ndata: {\"usage\":{\"prompt_tokens\":3}}\n");

        // THEN the first states nothing, and the second keeps nothing of the
        // endless line and reads the usage that follows it
        assert_eq!(plain.spent, Spent::default());
        assert_eq!(kept_while_skipping, 0);
        assert_eq!(endless.spent.prompt, Some(3));
    }

    #[test]
    fn test_the_address_of_a_caller_has_no_port_and_reads_as_ipv4_when_carried() {
        // GIVEN a peer of the local network, and a loopback peer carried in
        // an IPv6 address
        let lan: SocketAddr = "192.168.1.20:50123".parse().unwrap();
        let mapped: SocketAddr = "[::ffff:127.0.0.1]:50124".parse().unwrap();

        // WHEN each is written for the journal
        // THEN the address alone is kept
        assert_eq!(address_of(lan), "192.168.1.20");
        assert_eq!(address_of(mapped), "127.0.0.1");
    }

    #[test]
    fn test_a_record_round_trips_and_leaves_out_what_it_does_not_know() {
        // GIVEN the record of a refused call that named nothing
        let record = ServerCallRecord {
            event: "server_call".into(),
            method: "POST".into(),
            endpoint: "/v1/chat/completions".into(),
            target: None,
            target_kind: None,
            token_name: None,
            caller_address: Some("192.168.1.20".into()),
            origin: Some(CallOrigin::LocalNetwork),
            status: 401,
            prompt_tokens: None,
            completion_tokens: None,
            duration_ms: 2,
            task_id: None,
            run_id: None,
        };

        // WHEN it is written as a payload and read back
        let payload = serde_json::to_value(&record).unwrap();
        let back: ServerCallRecord = serde_json::from_value(payload.clone()).unwrap();

        // THEN the payload holds only what is known, and reads back the same
        assert_eq!(
            payload,
            json!({
                "event": "server_call", "method": "POST",
                "endpoint": "/v1/chat/completions",
                "caller_address": "192.168.1.20", "origin": "local_network",
                "status": 401, "duration_ms": 2,
            })
        );
        assert_eq!(back, record);
    }
}
