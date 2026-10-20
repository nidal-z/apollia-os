//! The name a tool travels under on the wire.
//!
//! The OpenAI API, and the servers that validate a request against it (the
//! MLX engine among them), refuse a function name outside
//! `^[A-Za-z0-9_-]{1,64}$`. Apollia names the tools of a connection
//! `mcp:<server>/<tool>`, so such a request came back as a 422 the client
//! could not even parse. A name that already fits is sent as it is; any other
//! is written with `_` in place of each refused character, shortened with a
//! digest when it is too long, and read back to its own name in the calls the
//! model answers with, from the tools of the same request.

use std::collections::HashMap;

use crate::types::{StreamChunk, ToolCall, ToolSpec};

/// Longest function name the API accepts.
const MAX_LEN: usize = 64;

fn allowed(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}

/// FNV-1a, so the digest of a name is the same from one build to the next and
/// a conversation stored before a restart keeps naming its calls the same way.
fn digest(name: &str) -> u32 {
    name.bytes().fold(0x811c_9dc5_u32, |hash, byte| {
        (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193)
    })
}

/// The name `name` is sent under.
pub(super) fn wire_name(name: &str) -> String {
    if !name.is_empty() && name.len() <= MAX_LEN && name.chars().all(allowed) {
        return name.to_owned();
    }
    let written: String = name
        .chars()
        .map(|c| if allowed(c) { c } else { '_' })
        .collect();
    let written = if written.is_empty() {
        "tool".to_owned()
    } else {
        written
    };
    if written.len() <= MAX_LEN {
        return written;
    }
    let suffix = format!("_{:08x}", digest(name));
    let mut head: String = written.chars().take(MAX_LEN - suffix.len()).collect();
    head.push_str(&suffix);
    head
}

/// The own name of each tool of a request, by the name it was sent under.
#[derive(Debug, Clone, Default)]
pub(super) struct WireNames(HashMap<String, String>);

impl WireNames {
    /// The names of `tools` that change on the wire.
    pub(super) fn of(tools: &[ToolSpec]) -> Self {
        Self(
            tools
                .iter()
                .filter_map(|spec| {
                    let wire = wire_name(&spec.name);
                    (wire != spec.name).then(|| (wire, spec.name.clone()))
                })
                .collect(),
        )
    }

    /// `call` under the own name of its tool.
    pub(super) fn restore(&self, mut call: ToolCall) -> ToolCall {
        if let Some(own) = self.0.get(&call.name) {
            call.name = own.clone();
        }
        call
    }

    /// `chunk`, with a tool call under the own name of its tool.
    pub(super) fn restore_chunk<E>(&self, chunk: Result<StreamChunk, E>) -> Result<StreamChunk, E> {
        match chunk {
            Ok(StreamChunk::ToolCall(call)) => Ok(StreamChunk::ToolCall(self.restore(call))),
            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(name: &str) -> ToolSpec {
        ToolSpec {
            name: name.to_owned(),
            description: String::new(),
            parameters: serde_json::json!({ "type": "object" }),
        }
    }

    fn call(name: &str) -> ToolCall {
        ToolCall {
            id: "call-1".to_owned(),
            name: name.to_owned(),
            arguments: serde_json::Value::Null,
        }
    }

    #[test]
    fn a_name_the_api_accepts_is_sent_as_it_is() {
        // GIVEN native tool names
        // WHEN they are written for the wire
        // THEN none changes
        assert_eq!(wire_name("file_read"), "file_read");
        assert_eq!(wire_name("web-search"), "web-search");
    }

    #[test]
    fn a_connection_tool_is_sent_under_a_name_the_api_accepts() {
        // GIVEN the tool of a connection, named with a colon and a slash
        // WHEN it is written for the wire
        let wire = wire_name("mcp:navigateur/browser_click");
        // THEN only allowed characters remain
        assert_eq!(wire, "mcp_navigateur_browser_click");
    }

    #[test]
    fn a_name_too_long_is_shortened_to_sixty_four_characters_and_kept_apart() {
        // GIVEN two long names that differ only at their end
        let a = format!("mcp:{}/tool_a", "s".repeat(80));
        let b = format!("mcp:{}/tool_b", "s".repeat(80));
        // WHEN they are written for the wire
        let (wa, wb) = (wire_name(&a), wire_name(&b));
        // THEN each fits the limit and they stay distinct
        assert_eq!(wa.len(), MAX_LEN);
        assert_eq!(wb.len(), MAX_LEN);
        assert_ne!(wa, wb);
        assert!(wa.chars().all(allowed));
    }

    #[test]
    fn a_call_the_model_answers_with_is_read_back_to_the_own_name_of_its_tool() {
        // GIVEN a request with a connection tool and a native one
        let names = WireNames::of(&[spec("mcp:navigateur/browser_navigate"), spec("file_read")]);
        // WHEN the model calls both under their wire names
        let connection = names.restore(call("mcp_navigateur_browser_navigate"));
        let native = names.restore_chunk::<()>(Ok(StreamChunk::ToolCall(call("file_read"))));
        // THEN each call names its own tool
        assert_eq!(connection.name, "mcp:navigateur/browser_navigate");
        assert!(matches!(native, Ok(StreamChunk::ToolCall(c)) if c.name == "file_read"));
    }
}
