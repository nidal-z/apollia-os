//! The answer of an agent or a workflow in the format of `POST /v1/messages`.
//!
//! The content is a list of blocks: one `thinking` block per step of the run,
//! then one `text` block holding the final answer. A stream sends the events
//! of the Messages API in order: the message started, each block started,
//! filled and stopped, then the stop reason with the usage, and the message
//! stopped. A run that did not answer closes the stream on an `error` event.
//!
//! A `thinking` block written here carries an empty `signature`: the steps
//! are the runtime's account of the work, not a model's signed reasoning, and
//! the call holds no state a signature would protect.

use axum::body::Bytes;
use serde_json::{json, Value};

use super::agent_reply::{named_event, AnswerFormat, AnswerHeader};
use super::agent_run::Usage;
use super::errors::Family;

/// The answer to `POST /v1/messages`.
pub(crate) struct MessagesAnswer {
    header: AnswerHeader,
    /// The index the next block of a stream takes.
    index: usize,
}

fn usage_body(usage: Usage) -> Value {
    json!({
        "input_tokens": usage.prompt,
        "output_tokens": usage.completion,
    })
}

fn thinking_block(text: &str) -> Value {
    json!({ "type": "thinking", "thinking": text, "signature": "" })
}

fn text_block(text: &str) -> Value {
    json!({ "type": "text", "text": text })
}

fn event(kind: &str, mut fields: Value) -> Bytes {
    fields["type"] = json!(kind);
    named_event(kind, &fields)
}

impl MessagesAnswer {
    pub(crate) fn new(header: AnswerHeader) -> Self {
        Self { header, index: 0 }
    }

    fn message(&self, content: Vec<Value>, stop_reason: Option<&str>, usage: Usage) -> Value {
        json!({
            "id": format!("msg_{}", self.header.run_id),
            "type": "message",
            "role": "assistant",
            "model": self.header.model,
            "content": content,
            "stop_reason": stop_reason,
            "stop_sequence": null,
            "usage": usage_body(usage),
        })
    }

    /// The three events of one block: started empty, filled by `delta`,
    /// stopped.
    fn block(&mut self, empty: Value, delta: Value) -> Vec<Bytes> {
        let index = self.index;
        self.index += 1;
        vec![
            event(
                "content_block_start",
                json!({ "index": index, "content_block": empty }),
            ),
            event(
                "content_block_delta",
                json!({ "index": index, "delta": delta }),
            ),
            event("content_block_stop", json!({ "index": index })),
        ]
    }
}

impl AnswerFormat for MessagesAnswer {
    fn family(&self) -> Family {
        Family::Anthropic
    }

    fn whole(&self, steps: &[String], content: &str, usage: Usage) -> Value {
        let mut blocks: Vec<Value> = steps
            .iter()
            .map(|step| thinking_block(step.trim_end()))
            .collect();
        blocks.push(text_block(content));
        self.message(blocks, Some("end_turn"), usage)
    }

    fn opening(&mut self) -> Vec<Bytes> {
        let message = self.message(Vec::new(), None, Usage::default());
        vec![event("message_start", json!({ "message": message }))]
    }

    fn step(&mut self, step: &str) -> Vec<Bytes> {
        self.block(
            thinking_block(""),
            json!({ "type": "thinking_delta", "thinking": step.trim_end() }),
        )
    }

    fn answered(&mut self, content: &str, usage: Usage) -> Vec<Bytes> {
        let mut events = self.block(
            text_block(""),
            json!({ "type": "text_delta", "text": content }),
        );
        events.push(event(
            "message_delta",
            json!({
                "delta": { "stop_reason": "end_turn", "stop_sequence": null },
                "usage": usage_body(usage),
            }),
        ));
        events.push(event("message_stop", json!({})));
        events
    }

    fn failed(&mut self, error: Value) -> Bytes {
        named_event("error", &error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_each_block_of_a_stream_takes_the_next_index() {
        // GIVEN the answer of a run with two steps
        let mut answer = MessagesAnswer::new(AnswerHeader::new("t-1", "apollia/clerk"));

        // WHEN both steps and the answer are written
        let mut written = answer.step("Calling file_read: {}\n");
        written.extend(answer.step("file_read answered: ok\n"));
        written.extend(answer.answered("Done.", Usage::default()));

        // THEN the blocks are numbered 0, 1 and 2, each started, filled and
        // stopped at its own index
        let indexes: Vec<u64> = written
            .iter()
            .filter_map(|event| {
                let text = std::str::from_utf8(event).unwrap();
                let (_, data) = text.trim_end().split_once("\ndata: ").unwrap();
                serde_json::from_str::<Value>(data).unwrap()["index"].as_u64()
            })
            .collect();
        assert_eq!(indexes, [0, 0, 0, 1, 1, 1, 2, 2, 2]);
    }
}
