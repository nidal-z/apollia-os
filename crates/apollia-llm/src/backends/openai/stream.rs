//! The OpenAI-compatible streaming decoder.
//!
//! Split out of `openai.rs`: the client stays in the parent, the state machine
//! that turns delta chunks into completion chunks and reassembles the streamed
//! tool calls lives here.

use std::collections::HashMap;

use futures::StreamExt;

use crate::backends::openai::convert::{estimate_cost_usd, map_openai_error};
use crate::backends::openai::reasoning::{THINK_CLOSE, THINK_OPEN};
use crate::backends::openai::{TimedChatStream, TimedStreamResponse};
use crate::types::{LlmError, StreamChunk, TokenUsage, ToolCall};

/// State machine for the OpenAI streaming response.
///
/// Tool call fragments are accumulated during `Streaming` and flushed as
/// `StreamChunk::ToolCall` items during `Flushing`.
pub(super) enum OpenAIStreamState {
    /// Reading SSE chunks from the inner stream.
    Streaming {
        inner: TimedChatStream,
        /// First item peeked in `stream()` to surface a lazy non-2xx status;
        /// drained before polling `inner`. Boxed to keep the variant small.
        first: Option<Box<Result<TimedStreamResponse, async_openai::error::OpenAIError>>>,
        pending: HashMap<u32, PartialToolCall>,
        /// Resolved model id, kept to price the terminal usage chunk the same
        /// way the non-streaming `do_complete` path does.
        model: String,
        /// Engine timings seen on a chunk that also carried something else to
        /// emit. Held back so they surface on the following poll, since the
        /// state machine yields one item at a time.
        pending_timings: Option<serde_json::Value>,
        /// Whether a `<think>` block opened by a separate reasoning field is
        /// still open. See [`RawReasoning`].
        in_think: bool,
    },
    /// SSE ended inside a `<think>` block: the closing tag has been emitted and
    /// the terminal items still owe an appearance. A distinct state because the
    /// inner stream is exhausted and must not be polled again.
    Ending {
        timings: Option<serde_json::Value>,
        pending: HashMap<u32, PartialToolCall>,
    },
    /// SSE stream ended; emitting accumulated tool calls one by one.
    Flushing { remaining: Vec<ToolCall> },
    /// Fully consumed.
    Done,
}
/// Accumulated fragments for a single tool call during OpenAI streaming.
#[derive(Default)]
pub(super) struct PartialToolCall {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) arguments: String,
}
/// Single step of the OpenAI stream `unfold`: read the next SSE chunk, emit
/// text immediately, accumulate tool call fragments, then flush them when the
/// stream ends.
pub(super) async fn next_openai_stream_item(
    state: OpenAIStreamState,
) -> Option<(Result<StreamChunk, LlmError>, OpenAIStreamState)> {
    match state {
        OpenAIStreamState::Done => None,
        OpenAIStreamState::Flushing { mut remaining } => remaining.pop().map(|call| {
            (
                Ok(StreamChunk::ToolCall(call)),
                OpenAIStreamState::Flushing { remaining },
            )
        }),
        OpenAIStreamState::Ending { timings, pending } => match timings {
            Some(t) => Some((
                Ok(StreamChunk::Timings(t)),
                OpenAIStreamState::Flushing {
                    remaining: drain_pending_tool_calls(pending),
                },
            )),
            None => flush_pending_tool_calls(pending),
        },
        OpenAIStreamState::Streaming {
            mut inner,
            mut first,
            mut pending,
            model,
            mut pending_timings,
            mut in_think,
        } => {
            // Timings held back from an earlier chunk take priority, so they are
            // never lost behind a later text delta.
            if let Some(timings) = pending_timings.take() {
                return Some((
                    Ok(StreamChunk::Timings(timings)),
                    OpenAIStreamState::Streaming {
                        inner,
                        first,
                        pending,
                        model,
                        pending_timings: None,
                        in_think,
                    },
                ));
            }
            loop {
                let next = match first.take() {
                    Some(item) => Some(*item),
                    None => inner.next().await,
                };
                match next {
                    Some(Ok(envelope)) => {
                        // The engine attaches timings to its final chunk. Hold
                        // them until whatever else this chunk carries has been
                        // emitted.
                        if let Some(t) = envelope.timings {
                            pending_timings = Some(t);
                        }
                        let reasoning = envelope.reasoning;
                        let response = envelope.inner;
                        // The terminal chunk (empty `choices`, populated `usage`)
                        // carries the call's token accounting. Surface it as
                        // `StreamChunk::Usage`, priced like the non-streaming path.
                        if let Some(usage) = response.usage.as_ref() {
                            let mapped = TokenUsage {
                                prompt_tokens: usage.prompt_tokens,
                                completion_tokens: usage.completion_tokens,
                                cost_usd: estimate_cost_usd(
                                    &model,
                                    usage.prompt_tokens,
                                    usage.completion_tokens,
                                ),
                                ..Default::default()
                            };
                            return Some((
                                Ok(StreamChunk::Usage(mapped)),
                                OpenAIStreamState::Streaming {
                                    inner,
                                    first,
                                    pending,
                                    model,
                                    pending_timings,
                                    in_think,
                                },
                            ));
                        }
                        if let Some(text) =
                            next_emittable_text(&mut pending, response, reasoning, &mut in_think)
                        {
                            return Some((
                                Ok(StreamChunk::Text(text)),
                                OpenAIStreamState::Streaming {
                                    inner,
                                    first,
                                    pending,
                                    model,
                                    pending_timings,
                                    in_think,
                                },
                            ));
                        }
                        continue;
                    }
                    Some(Err(e)) => {
                        return Some((Err(map_openai_error(e)), OpenAIStreamState::Done));
                    }
                    None => {
                        // A stream that ends while a reasoning block is open
                        // (the model went straight from thinking to a tool call,
                        // or was cut off) would leave an unbalanced `<think>`
                        // that the downstream parser reads as reasoning
                        // swallowing the rest of the turn.
                        if in_think {
                            return Some((
                                Ok(StreamChunk::Text(THINK_CLOSE.to_owned())),
                                OpenAIStreamState::Ending {
                                    timings: pending_timings.take(),
                                    pending,
                                },
                            ));
                        }
                        // SSE ended. Timings from the final chunk outrank the
                        // tool-call flush: dropping them here would lose the
                        // measurement on every turn that ends in a tool call.
                        if let Some(timings) = pending_timings.take() {
                            return Some((
                                Ok(StreamChunk::Timings(timings)),
                                OpenAIStreamState::Flushing {
                                    remaining: drain_pending_tool_calls(pending),
                                },
                            ));
                        }
                        // SSE stream ended, flush accumulated tool calls
                        return flush_pending_tool_calls(pending);
                    }
                }
            }
        }
    }
}
/// Process an SSE chunk: accumulate tool call fragments into `pending` and
/// return the delta text if it is non-empty (to emit immediately), otherwise
/// `None` (chunk consumed, keep reading).
///
/// A `reasoning` delta is re-inlined as a `<think>` block so a backend that
/// splits reasoning out reaches the pipeline in the same shape as one that
/// keeps it in the content stream. `in_think` carries the open/closed state
/// across chunks, since the tags straddle them.
pub(super) fn next_emittable_text(
    pending: &mut HashMap<u32, PartialToolCall>,
    response: async_openai::types::CreateChatCompletionStreamResponse,
    reasoning: Option<String>,
    in_think: &mut bool,
) -> Option<String> {
    let choice = response.choices.into_iter().next()?;

    if let Some(tc_chunks) = choice.delta.tool_calls {
        accumulate_tool_calls(pending, tc_chunks);
    }

    let text = choice.delta.content.unwrap_or_default();
    let reasoning = reasoning.unwrap_or_default();
    if text.is_empty() && reasoning.is_empty() {
        return None;
    }

    let mut out = String::with_capacity(text.len() + reasoning.len());
    if !reasoning.is_empty() {
        if !*in_think {
            out.push_str(THINK_OPEN);
            *in_think = true;
        }
        out.push_str(&reasoning);
    }
    if !text.is_empty() {
        if *in_think {
            out.push_str(THINK_CLOSE);
            *in_think = false;
        }
        out.push_str(&text);
    }
    Some(out)
}
/// Accumulate the tool call fragments received in an SSE chunk, indexed by
/// `index`.
pub(super) fn accumulate_tool_calls(
    pending: &mut HashMap<u32, PartialToolCall>,
    tc_chunks: Vec<async_openai::types::ChatCompletionMessageToolCallChunk>,
) {
    for tc in tc_chunks {
        let entry = pending.entry(tc.index).or_default();
        if let Some(id) = tc.id {
            entry.id = id;
        }
        if let Some(func) = tc.function {
            if let Some(name) = func.name {
                entry.name = name;
            }
            if let Some(args) = func.arguments {
                entry.arguments.push_str(&args);
            }
        }
    }
}
/// At the end of the stream: sort the accumulated calls by index and switch to
/// `Flushing` to emit them one by one (or finish if there are none).
pub(super) fn flush_pending_tool_calls(
    pending: HashMap<u32, PartialToolCall>,
) -> Option<(Result<StreamChunk, LlmError>, OpenAIStreamState)> {
    let mut tool_calls = drain_pending_tool_calls(pending);
    tool_calls.pop().map(|call| {
        (
            Ok(StreamChunk::ToolCall(call)),
            OpenAIStreamState::Flushing {
                remaining: tool_calls,
            },
        )
    })
}
/// Sort the accumulated calls by index and reverse them, so `Flushing` can pop
/// from the end and still emit in index order.
pub(super) fn drain_pending_tool_calls(pending: HashMap<u32, PartialToolCall>) -> Vec<ToolCall> {
    let mut calls: Vec<(u32, PartialToolCall)> = pending.into_iter().collect();
    calls.sort_by_key(|(idx, _)| *idx);

    let mut tool_calls: Vec<ToolCall> = calls
        .into_iter()
        .map(|(_, partial)| {
            let arguments =
                serde_json::from_str(&partial.arguments).unwrap_or(serde_json::Value::Null);
            ToolCall {
                id: partial.id,
                name: partial.name,
                arguments,
            }
        })
        .collect();

    tool_calls.reverse();
    tool_calls
}
