//! The reasoning of a model, kept apart from its answer or put back into it.
//!
//! The local engine separates the reasoning of a thinking model from its
//! answer: a chat completion carries it in `reasoning_content`, in the message
//! of a JSON answer and in the delta of each event of a stream. That is what
//! the compatible server relays while `[server] separate_reasoning_content` is
//! on, the default. Off, the relay puts the reasoning back at the head of
//! `content`, between `<think>` and `</think>`, for a client that reads only
//! `content`, and drops `reasoning_content`.
//!
//! Only `/v1/chat/completions` carries the field. The responses and messages
//! formats give the reasoning blocks of their own, which are relayed as the
//! engine wrote them.

use std::collections::HashSet;

use axum::body::Bytes;
use futures::{Stream, StreamExt};
use serde_json::{Map, Value};

/// The tag that opens the reasoning in `content`.
const OPEN: &str = "<think>";
/// The tag that closes it.
const CLOSE: &str = "</think>";
/// The field the engine puts the reasoning in.
const FIELD: &str = "reasoning_content";
/// What ends one event of a stream.
const EVENT_END: &[u8] = b"\n\n";

/// Puts the reasoning of each choice of a JSON chat completion back into its
/// `content`. A body that is not a chat completion is left as it is.
pub(crate) fn merge_answer(answer: &mut Value) {
    let Some(choices) = answer.get_mut("choices").and_then(Value::as_array_mut) else {
        return;
    };
    for choice in choices {
        let Some(message) = choice.get_mut("message").and_then(Value::as_object_mut) else {
            continue;
        };
        let reasoning = take_reasoning(message);
        if reasoning.is_empty() {
            continue;
        }
        let content = message.get("content").and_then(Value::as_str).unwrap_or("");
        let merged = format!("{OPEN}{reasoning}{CLOSE}{content}");
        message.insert("content".to_owned(), Value::String(merged));
    }
}

/// Removes `reasoning_content` from `object` and returns it, empty when it was
/// absent, null or not a string.
fn take_reasoning(object: &mut Map<String, Value>) -> String {
    match object.remove(FIELD) {
        Some(Value::String(reasoning)) => reasoning,
        _ => String::new(),
    }
}

/// The events of a chat completion stream, with the reasoning of each delta
/// put back into its `content`.
///
/// An event can be split across two chunks of the engine, so the bytes after
/// the last complete event wait for the next chunk. A tag opened in one event
/// is closed in the event that brings the first answer text of the same
/// choice, or the one that finishes it.
#[derive(Debug, Default)]
pub(crate) struct StreamMerger {
    pending: Vec<u8>,
    /// The choices whose reasoning was opened and not yet closed.
    open: HashSet<u64>,
}

impl StreamMerger {
    /// Takes one chunk of the engine and returns the complete events it ends,
    /// rewritten. Returns nothing while no event is complete.
    pub(crate) fn push(&mut self, chunk: &[u8]) -> Bytes {
        self.pending.extend_from_slice(chunk);
        let Some(last) = find_last(&self.pending, EVENT_END) else {
            return Bytes::new();
        };
        let rest = self.pending.split_off(last + EVENT_END.len());
        let complete = std::mem::replace(&mut self.pending, rest);
        let mut out = Vec::with_capacity(complete.len());
        for event in complete.split_inclusive(|byte| *byte == b'\n') {
            out.extend_from_slice(&self.line(event));
        }
        Bytes::from(out)
    }

    /// The bytes left when the engine closes the stream, unchanged: an event
    /// it never finished is not a chat completion event to rewrite.
    pub(crate) fn finish(&mut self) -> Bytes {
        Bytes::from(std::mem::take(&mut self.pending))
    }

    /// One line of an event, rewritten when it is a `data:` line carrying a
    /// chat completion chunk, else as it came.
    fn line(&mut self, line: &[u8]) -> Vec<u8> {
        let Ok(text) = std::str::from_utf8(line) else {
            return line.to_vec();
        };
        let Some(payload) = text.strip_prefix("data:") else {
            return line.to_vec();
        };
        let body = payload.trim_end_matches(['\r', '\n']);
        let Ok(mut chunk) = serde_json::from_str::<Value>(body.trim_start()) else {
            return line.to_vec();
        };
        if !self.merge_chunk(&mut chunk) {
            return line.to_vec();
        }
        let newline = &payload[body.len()..];
        format!("data: {chunk}{newline}").into_bytes()
    }

    /// Rewrites the deltas of one stream chunk; false when nothing changed.
    fn merge_chunk(&mut self, chunk: &mut Value) -> bool {
        let Some(choices) = chunk.get_mut("choices").and_then(Value::as_array_mut) else {
            return false;
        };
        let mut changed = false;
        for choice in choices {
            changed |= self.merge_choice(choice);
        }
        changed
    }

    fn merge_choice(&mut self, choice: &mut Value) -> bool {
        let index = choice.get("index").and_then(Value::as_u64).unwrap_or(0);
        let finished = choice.get("finish_reason").is_some_and(|r| !r.is_null());
        let Some(delta) = choice.get_mut("delta").and_then(Value::as_object_mut) else {
            return false;
        };
        let had_field = delta.contains_key(FIELD);
        let reasoning = take_reasoning(delta);
        let content = delta
            .get("content")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();
        let mut merged = String::new();
        if !reasoning.is_empty() {
            if self.open.insert(index) {
                merged.push_str(OPEN);
            }
            merged.push_str(&reasoning);
        }
        if !content.is_empty() && self.open.remove(&index) {
            merged.push_str(CLOSE);
        }
        merged.push_str(&content);
        if finished && self.open.remove(&index) {
            merged.push_str(CLOSE);
        }
        if merged == content {
            return had_field;
        }
        delta.insert("content".to_owned(), Value::String(merged));
        true
    }
}

/// The position of the last occurrence of `needle` in `haystack`.
fn find_last(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .rposition(|window| window == needle)
}

/// `stream`, a chat completion event stream, with its reasoning put back into
/// `content`. An error of the engine ends the stream after it is passed on.
pub(crate) fn merge_stream<S, E>(stream: S) -> impl Stream<Item = Result<Bytes, E>>
where
    S: Stream<Item = Result<Bytes, E>> + Send + 'static,
    E: Send + 'static,
{
    let state = (Box::pin(stream), StreamMerger::default(), false);
    futures::stream::unfold(state, |(mut inner, mut merger, done)| async move {
        if done {
            return None;
        }
        let item = match inner.next().await {
            Some(Ok(chunk)) => (Ok(merger.push(&chunk)), false),
            Some(Err(error)) => (Err(error), true),
            None => (Ok(merger.finish()), true),
        };
        Some((item.0, (inner, merger, item.1)))
    })
    .filter(|item| futures::future::ready(!matches!(item, Ok(bytes) if bytes.is_empty())))
}
