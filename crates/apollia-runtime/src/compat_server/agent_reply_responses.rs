//! The answer of an agent or a workflow in the format of
//! `POST /v1/responses`.
//!
//! The output is a list of items: one `reasoning` item per step of the run,
//! its text in a summary part, then one `message` item holding the final
//! answer as `output_text`. A stream sends the events of the Responses API in
//! order, each numbered: the response created, each item added, filled and
//! done, then the response completed with its usage. A run that did not
//! answer closes the stream on an `error` event.

use axum::body::Bytes;
use serde_json::{json, Value};

use super::agent_reply::{named_event, AnswerFormat, AnswerHeader};
use super::agent_run::Usage;
use super::errors::Family;

/// The answer to `POST /v1/responses`.
pub(crate) struct ResponsesAnswer {
    header: AnswerHeader,
    /// The number the next event of a stream carries.
    sequence: u64,
    /// The items a stream has closed so far.
    items: Vec<Value>,
}

fn usage_body(usage: Usage) -> Value {
    json!({
        "input_tokens": usage.prompt,
        "input_tokens_details": { "cached_tokens": 0 },
        "output_tokens": usage.completion,
        "output_tokens_details": { "reasoning_tokens": 0 },
        "total_tokens": usage.prompt + usage.completion,
    })
}

fn summary_part(text: &str) -> Value {
    json!({ "type": "summary_text", "text": text })
}

fn text_part(text: &str) -> Value {
    json!({ "type": "output_text", "text": text, "annotations": [] })
}

impl ResponsesAnswer {
    pub(crate) fn new(header: AnswerHeader) -> Self {
        Self {
            header,
            sequence: 0,
            items: Vec::new(),
        }
    }

    fn reasoning_id(&self, index: usize) -> String {
        format!("rs_{}_{index}", self.header.run_id)
    }

    fn message_id(&self) -> String {
        format!("msg_{}", self.header.run_id)
    }

    fn reasoning_item(&self, index: usize, step: &str) -> Value {
        json!({
            "id": self.reasoning_id(index),
            "type": "reasoning",
            "summary": [summary_part(step)],
        })
    }

    fn message_item(&self, content: &str) -> Value {
        json!({
            "id": self.message_id(),
            "type": "message",
            "status": "completed",
            "role": "assistant",
            "content": [text_part(content)],
        })
    }

    fn response(&self, status: &str, output: &[Value], usage: Option<Usage>) -> Value {
        json!({
            "id": format!("resp_{}", self.header.run_id),
            "object": "response",
            "created_at": self.header.created,
            "status": status,
            "model": self.header.model,
            "output": output,
            "usage": usage.map(usage_body),
        })
    }

    /// The event `kind` carrying `fields`, numbered after the one before it.
    fn event(&mut self, kind: &str, mut fields: Value) -> Bytes {
        fields["type"] = json!(kind);
        fields["sequence_number"] = json!(self.sequence);
        self.sequence += 1;
        named_event(kind, &fields)
    }
}

impl AnswerFormat for ResponsesAnswer {
    fn family(&self) -> Family {
        Family::OpenAi
    }

    fn whole(&self, steps: &[String], content: &str, usage: Usage) -> Value {
        let mut output: Vec<Value> = steps
            .iter()
            .enumerate()
            .map(|(index, step)| self.reasoning_item(index, step.trim_end()))
            .collect();
        output.push(self.message_item(content));
        self.response("completed", &output, Some(usage))
    }

    fn opening(&mut self) -> Vec<Bytes> {
        let response = self.response("in_progress", &[], None);
        vec![
            self.event("response.created", json!({ "response": response })),
            self.event("response.in_progress", json!({ "response": response })),
        ]
    }

    fn step(&mut self, step: &str) -> Vec<Bytes> {
        let step = step.trim_end();
        let index = self.items.len();
        let id = self.reasoning_id(index);
        let item = self.reasoning_item(index, step);
        let at = |extra: Value| {
            let mut fields = json!({ "item_id": id, "output_index": index, "summary_index": 0 });
            if let (Value::Object(fields), Value::Object(extra)) = (&mut fields, extra) {
                fields.extend(extra);
            }
            fields
        };
        let events = vec![
            self.event(
                "response.output_item.added",
                json!({
                    "output_index": index,
                    "item": { "id": id, "type": "reasoning", "summary": [] },
                }),
            ),
            self.event(
                "response.reasoning_summary_part.added",
                at(json!({ "part": summary_part("") })),
            ),
            self.event(
                "response.reasoning_summary_text.delta",
                at(json!({ "delta": step })),
            ),
            self.event(
                "response.reasoning_summary_text.done",
                at(json!({ "text": step })),
            ),
            self.event(
                "response.reasoning_summary_part.done",
                at(json!({ "part": summary_part(step) })),
            ),
            self.event(
                "response.output_item.done",
                json!({ "output_index": index, "item": item }),
            ),
        ];
        self.items.push(item);
        events
    }

    fn answered(&mut self, content: &str, usage: Usage) -> Vec<Bytes> {
        let index = self.items.len();
        let id = self.message_id();
        let item = self.message_item(content);
        let at = |extra: Value| {
            let mut fields = json!({ "item_id": id, "output_index": index, "content_index": 0 });
            if let (Value::Object(fields), Value::Object(extra)) = (&mut fields, extra) {
                fields.extend(extra);
            }
            fields
        };
        let mut events = vec![
            self.event(
                "response.output_item.added",
                json!({
                    "output_index": index,
                    "item": {
                        "id": id,
                        "type": "message",
                        "status": "in_progress",
                        "role": "assistant",
                        "content": [],
                    },
                }),
            ),
            self.event(
                "response.content_part.added",
                at(json!({ "part": text_part("") })),
            ),
            self.event(
                "response.output_text.delta",
                at(json!({ "delta": content })),
            ),
            self.event("response.output_text.done", at(json!({ "text": content }))),
            self.event(
                "response.content_part.done",
                at(json!({ "part": text_part(content) })),
            ),
            self.event(
                "response.output_item.done",
                json!({ "output_index": index, "item": item }),
            ),
        ];
        self.items.push(item);
        let response = self.response("completed", &self.items, Some(usage));
        events.push(self.event("response.completed", json!({ "response": response })));
        events
    }

    /// The `error` event of the Responses API is flat: the fields of the
    /// error object sit beside `type` and `sequence_number`.
    fn failed(&mut self, error: Value) -> Bytes {
        let mut fields = match error {
            Value::Object(mut body) => body.remove("error").unwrap_or(Value::Null),
            other => other,
        };
        if !fields.is_object() {
            fields = json!({});
        }
        fields["param"] = Value::Null;
        self.event("error", fields)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn events(bytes: &[Bytes]) -> Vec<(String, Value)> {
        bytes
            .iter()
            .map(|event| {
                let text = std::str::from_utf8(event).unwrap();
                let (name, data) = text.trim_end().split_once("\ndata: ").unwrap();
                (
                    name.trim_start_matches("event: ").to_owned(),
                    serde_json::from_str(data).unwrap(),
                )
            })
            .collect()
    }

    #[test]
    fn test_a_stream_numbers_its_events_and_names_each_by_its_type() {
        // GIVEN the answer of a run with one step
        let mut answer = ResponsesAnswer::new(AnswerHeader::new("t-1", "apollia/clerk"));

        // WHEN the stream is written from its opening to its answer
        let mut written = answer.opening();
        written.extend(answer.step("Calling file_read: {}\n"));
        written.extend(answer.answered(
            "Done.",
            Usage {
                prompt: 3,
                completion: 2,
            },
        ));

        // THEN each event is named by its type and numbered in order, the
        // step is the first item and the message the second
        let written = events(&written);
        for (number, (name, data)) in written.iter().enumerate() {
            assert_eq!(data["type"], name.as_str());
            assert_eq!(data["sequence_number"], number);
        }
        let completed = &written.last().unwrap().1["response"];
        assert_eq!(completed["output"][0]["id"], "rs_t-1_0");
        assert_eq!(
            completed["output"][0]["summary"][0]["text"],
            "Calling file_read: {}"
        );
        assert_eq!(completed["output"][1]["id"], "msg_t-1");
        assert_eq!(completed["usage"]["total_tokens"], 5);
    }

    #[test]
    fn test_an_error_closes_a_stream_as_a_flat_error_event() {
        // GIVEN a stream, and the error body of a run that waits
        let mut answer = ResponsesAnswer::new(AnswerHeader::new("t-1", "apollia/clerk"));
        let error = json!({"error": {
            "message": "wait", "type": "approval_pending", "code": "approval_pending",
            "pending_id": "batch:b:0"
        }});

        // WHEN the stream fails on it
        let written = events(&[answer.failed(error)]);

        // THEN the event is `error`, its fields beside its type
        let (name, data) = &written[0];
        assert_eq!(name, "error");
        assert_eq!(data["type"], "error");
        assert_eq!(data["code"], "approval_pending");
        assert_eq!(data["message"], "wait");
        assert_eq!(data["pending_id"], "batch:b:0");
    }
}
