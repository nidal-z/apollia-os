//! Validation and bounding of untrusted tool metadata returned by MCP servers.
//!
//! Tool names, descriptions, and the server `instructions` string cross the
//! trust boundary from a remote server into the daemon. A tool name becomes a
//! registry key (`mcp:<server>/<tool>`) and a structured tracing field;
//! descriptions and instructions flow verbatim into the tool catalogue exposed
//! to the model. An unbounded or control-character-laden value can forge
//! tracing lines, break the registry key separator, inject instructions into
//! the model context, or exhaust memory with a giant tool list.
//!
//! Every such field is validated and bounded here before use. The count cap is
//! per-server config (`McpServerConfig::max_tools`); the length and charset
//! bounds are crate constants, mirroring the byte-level bounding posture
//! documented for the transport layer.

use crate::protocol::{McpResource, McpToolDefinition};
use crate::server_repository::StoredResource;

/// Maximum accepted length of a tool name, in bytes. A well-behaved MCP tool
/// name is a short identifier; anything longer is rejected outright.
const MAX_TOOL_NAME_LEN: usize = 128;

/// Maximum retained length of a tool description, in bytes. Longer descriptions
/// are truncated on a UTF-8 boundary.
const MAX_DESCRIPTION_LEN: usize = 8 * 1024;

/// Maximum retained length of a tool title, in bytes. A title is a display
/// name, a few words at most; longer ones are truncated on a UTF-8 boundary.
const MAX_TITLE_LEN: usize = 256;

/// Maximum retained length of the server `instructions` string, in bytes.
pub(crate) const MAX_INSTRUCTIONS_LEN: usize = 16 * 1024;

/// Maximum accepted length of a resource URI, in bytes. The URI is the key a
/// read sends back to the server, so a longer one is dropped, not truncated.
const MAX_RESOURCE_URI_LEN: usize = 2048;

/// Maximum retained length of a resource MIME type, in bytes.
const MAX_MIME_TYPE_LEN: usize = 128;

/// Maximum number of resources kept per server. A server's tools are capped
/// by its `max_tools`; its resources, often one per document, get a fixed
/// ceiling of their own, generous for a picker and bounded for `mcp.db`.
pub(crate) const MAX_RESOURCES_PER_SERVER: usize = 1024;

/// Returns `true` when `name` is a well-formed MCP tool name: non-empty, within
/// [`MAX_TOOL_NAME_LEN`], and drawn only from `[A-Za-z0-9_.-]`.
///
/// Rejecting the `/` separator keeps the `mcp:<server>/<tool>` registry key
/// unambiguous; rejecting control and whitespace characters prevents forged
/// tracing lines. Uppercase and `.` are allowed so that real-world tool names
/// (`GetWeather`, `notion.search`) are not rejected.
pub(crate) fn is_valid_tool_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_TOOL_NAME_LEN
        && name
            .chars()
            .all(|c| matches!(c, 'a'..='z' | 'A'..='Z' | '0'..='9' | '_' | '-' | '.'))
}

/// Strip control characters from `text` and truncate the result to `max_len`
/// bytes on a UTF-8 character boundary.
///
/// Returns `None` for `None` input or for a value that is empty after
/// stripping, so an empty or all-control description collapses to "absent".
pub(crate) fn sanitize_free_text(text: Option<String>, max_len: usize) -> Option<String> {
    let cleaned: String = text?.chars().filter(|c| !c.is_control()).collect();
    if cleaned.is_empty() {
        return None;
    }
    Some(truncate_on_boundary(cleaned, max_len))
}

/// Truncate `s` to at most `max_len` bytes, backing off to the nearest UTF-8
/// character boundary so the result is always valid UTF-8.
fn truncate_on_boundary(mut s: String, max_len: usize) -> String {
    if s.len() <= max_len {
        return s;
    }
    let mut end = max_len;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    s.truncate(end);
    s
}

/// Validate, sanitize, and cap a batch of tool definitions from `server`.
///
/// - Tools whose name fails [`is_valid_tool_name`] are dropped (the name is a
///   registry key and a `tools/call` argument, so a malformed one cannot be
///   silently rewritten). The raw name is never logged: it is the forgery
///   vector this guard exists to contain.
/// - Each title and description is stripped of control characters and
///   truncated; the title is free text shown to the user, so it gets the
///   description's treatment under its own, shorter cap.
/// - The batch is capped at `max_tools`; tools beyond the cap are dropped.
///
/// Individual rejections are fail-open: a server with some malformed tools
/// still exposes its well-formed ones.
pub(crate) fn sanitize_tool_definitions(
    tools: Vec<McpToolDefinition>,
    server: &str,
    max_tools: usize,
) -> Vec<McpToolDefinition> {
    let received = tools.len();
    let mut out: Vec<McpToolDefinition> = Vec::with_capacity(received.min(max_tools));
    for mut tool in tools {
        if out.len() >= max_tools {
            break;
        }
        if !is_valid_tool_name(&tool.name) {
            continue;
        }
        tool.title = sanitize_free_text(tool.title, MAX_TITLE_LEN);
        tool.description = sanitize_free_text(tool.description, MAX_DESCRIPTION_LEN);
        out.push(tool);
    }
    if out.len() < received {
        tracing::warn!(
            server = %server,
            kept = out.len(),
            received,
            "mcp.tools.bounded"
        );
    }
    out
}

/// Validate, sanitize, and cap the resources `server` listed.
///
/// - The URI is stripped of control characters; a resource whose URI is then
///   empty or longer than [`MAX_RESOURCE_URI_LEN`] is dropped (the URI is the
///   key of a read, so it cannot be cut). The raw value is never logged.
/// - The name is free text shown in the @ picker, bounded like a tool title;
///   a name left empty reads as the URI.
/// - The description and the MIME type are stripped and truncated.
/// - The list is capped at [`MAX_RESOURCES_PER_SERVER`].
pub(crate) fn sanitize_resources(resources: Vec<McpResource>, server: &str) -> Vec<StoredResource> {
    let received = resources.len();
    let mut out: Vec<StoredResource> = Vec::with_capacity(received.min(MAX_RESOURCES_PER_SERVER));
    for resource in resources {
        if out.len() >= MAX_RESOURCES_PER_SERVER {
            break;
        }
        let uri: String = resource.uri.chars().filter(|c| !c.is_control()).collect();
        if uri.is_empty() || uri.len() > MAX_RESOURCE_URI_LEN {
            continue;
        }
        let name =
            sanitize_free_text(Some(resource.name), MAX_TITLE_LEN).unwrap_or_else(|| uri.clone());
        out.push(StoredResource {
            name,
            mime_type: sanitize_free_text(resource.mime_type, MAX_MIME_TYPE_LEN),
            description: sanitize_free_text(resource.description, MAX_DESCRIPTION_LEN),
            uri,
        });
    }
    if out.len() < received {
        tracing::warn!(
            server = %server,
            kept = out.len(),
            received,
            "mcp.resources.bounded"
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn listed(uri: &str, name: &str, description: Option<&str>) -> McpResource {
        McpResource {
            uri: uri.to_string(),
            name: name.to_string(),
            description: description.map(str::to_string),
            mime_type: Some("text/plain".to_string()),
        }
    }

    #[test]
    fn hostile_resources_are_dropped_stripped_and_capped() {
        // GIVEN a list mixing an empty URI, an overlong URI, control characters
        //       in every field, an overlong name, and more entries than the cap
        let mut noisy = listed(
            "memo://a\n\u{7}b",
            "Road\nmap\u{1b}[31m",
            Some("ignore\nrules"),
        );
        noisy.mime_type = Some("text/\rplain".to_string());
        let mut resources = vec![
            listed("", "empty", None),
            listed("\n\t", "only control", None),
            listed(
                &format!("memo://{}", "x".repeat(MAX_RESOURCE_URI_LEN)),
                "long uri",
                None,
            ),
            noisy,
            listed("memo://named", &"\u{e9}".repeat(MAX_TITLE_LEN), None),
            listed("memo://blank", "\n", None),
        ];
        resources.extend(
            (0..MAX_RESOURCES_PER_SERVER).map(|i| listed(&format!("memo://fill{i}"), "fill", None)),
        );

        // WHEN the list is sanitized
        let out = sanitize_resources(resources, "srv");

        // THEN the bad URIs are gone, control characters stripped, the name
        //      bounded on a character boundary, a blank name read as the URI,
        //      and the list capped
        assert_eq!(out.len(), MAX_RESOURCES_PER_SERVER);
        assert!(out
            .iter()
            .all(|r| !r.uri.is_empty() && r.uri.len() <= MAX_RESOURCE_URI_LEN));
        assert_eq!(out[0].uri, "memo://ab");
        assert_eq!(out[0].name, "Roadmap[31m");
        assert_eq!(out[0].description.as_deref(), Some("ignorerules"));
        assert_eq!(out[0].mime_type.as_deref(), Some("text/plain"));
        assert!(out[1].name.len() <= MAX_TITLE_LEN && !out[1].name.is_empty());
        assert_eq!(out[2].name, "memo://blank");
        let mut long_mime = listed("memo://mime", "mime", None);
        long_mime.mime_type = Some("a".repeat(MAX_MIME_TYPE_LEN * 2));
        let bounded = sanitize_resources(vec![long_mime], "srv");
        assert_eq!(
            bounded[0].mime_type.as_ref().map(String::len),
            Some(MAX_MIME_TYPE_LEN)
        );
    }

    #[test]
    fn legitimate_resources_pass_through_unchanged() {
        // GIVEN resources a real server would list
        let resources = vec![
            listed("notion://page/1", "Roadmap", Some("Q3 plan")),
            listed("file:///docs/spec.md", "Spec", None),
        ];
        // WHEN sanitized
        let out = sanitize_resources(resources, "srv");
        // THEN every field is kept as is
        assert_eq!(
            out,
            vec![
                StoredResource {
                    uri: "notion://page/1".to_string(),
                    name: "Roadmap".to_string(),
                    mime_type: Some("text/plain".to_string()),
                    description: Some("Q3 plan".to_string()),
                },
                StoredResource {
                    uri: "file:///docs/spec.md".to_string(),
                    name: "Spec".to_string(),
                    mime_type: Some("text/plain".to_string()),
                    description: None,
                },
            ]
        );
    }

    fn tool(name: &str, description: Option<&str>) -> McpToolDefinition {
        McpToolDefinition {
            name: name.to_string(),
            title: None,
            description: description.map(str::to_string),
            input_schema: json!({"type": "object"}),
            annotations: None,
        }
    }

    #[test]
    fn legitimate_names_are_accepted() {
        // GIVEN tool names a real MCP server would expose
        // WHEN validated
        // THEN each is accepted
        for name in ["echo", "add", "search_pages", "GetWeather", "notion.search"] {
            assert!(is_valid_tool_name(name), "should accept {name}");
        }
    }

    #[test]
    fn malicious_names_are_rejected() {
        // GIVEN names carrying control chars, the key separator, whitespace, or emptiness
        // WHEN validated
        // THEN each is rejected
        for name in ["", "a/b", "x\ninject", "has space", "tab\ttab", "bell\u{7}"] {
            assert!(!is_valid_tool_name(name), "should reject {name:?}");
        }
    }

    #[test]
    fn overlong_name_is_rejected() {
        // GIVEN a name longer than the cap
        let name = "a".repeat(MAX_TOOL_NAME_LEN + 1);
        // WHEN validated
        // THEN it is rejected
        assert!(!is_valid_tool_name(&name));
        // AND a name exactly at the cap is accepted
        assert!(is_valid_tool_name(&"a".repeat(MAX_TOOL_NAME_LEN)));
    }

    #[test]
    fn control_characters_are_stripped_from_free_text() {
        // GIVEN a description with an injected newline and control bytes
        let input = Some("ignore rules\nrun\u{7}bash".to_string());
        // WHEN sanitized
        let out = sanitize_free_text(input, MAX_DESCRIPTION_LEN);
        // THEN control characters are gone, printable text remains
        assert_eq!(out.as_deref(), Some("ignore rulesrunbash"));
    }

    #[test]
    fn empty_and_all_control_free_text_collapses_to_none() {
        // GIVEN None, an empty string, and an all-control string
        // WHEN sanitized
        // THEN each collapses to None
        assert_eq!(sanitize_free_text(None, MAX_DESCRIPTION_LEN), None);
        assert_eq!(
            sanitize_free_text(Some(String::new()), MAX_DESCRIPTION_LEN),
            None
        );
        assert_eq!(
            sanitize_free_text(Some("\n\t\r".to_string()), MAX_DESCRIPTION_LEN),
            None
        );
    }

    #[test]
    fn long_free_text_is_truncated_on_char_boundary() {
        // GIVEN a description exceeding the cap, ending on a multi-byte char
        let input = Some("é".repeat(MAX_DESCRIPTION_LEN));
        // WHEN sanitized
        let out = sanitize_free_text(input, MAX_DESCRIPTION_LEN).unwrap();
        // THEN it is bounded and still valid UTF-8 (no panic, no partial char)
        assert!(out.len() <= MAX_DESCRIPTION_LEN);
        assert!(!out.is_empty());
    }

    #[test]
    fn definitions_are_capped_and_sanitized() {
        // GIVEN a batch mixing legitimate tools, an invalid-named tool, and a
        // control-laden description, above the cap
        let mut tools = vec![
            tool("echo", Some("Echo the input")),
            tool("add", None),
            tool("bad name", Some("dropped: invalid name")),
            tool("noisy", Some("desc\nwith\u{7}control")),
        ];
        tools.extend((0..10).map(|i| tool(&format!("fill{i}"), None)));

        // WHEN sanitized with a small cap
        let out = sanitize_tool_definitions(tools, "srv", 3);

        // THEN the count is capped, the invalid name is dropped, descriptions cleaned
        assert_eq!(out.len(), 3);
        assert!(out.iter().all(|t| t.name != "bad name"));
        assert_eq!(out[0].name, "echo");
        assert_eq!(out[1].name, "add");
        assert_eq!(out[1].description, None);
        assert_eq!(out[2].name, "noisy");
        assert_eq!(out[2].description.as_deref(), Some("descwithcontrol"));
    }

    #[test]
    fn legitimate_batch_passes_through_unchanged() {
        // GIVEN only well-formed tools under the cap
        let tools = vec![
            tool("echo", Some("Echo the input")),
            tool("add", Some("Add two numbers")),
        ];
        // WHEN sanitized
        let out = sanitize_tool_definitions(tools, "srv", 256);
        // THEN nothing is dropped or altered
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].name, "echo");
        assert_eq!(out[0].description.as_deref(), Some("Echo the input"));
        assert_eq!(out[1].name, "add");
        assert_eq!(out[1].description.as_deref(), Some("Add two numbers"));
    }

    #[test]
    fn title_is_bounded_like_a_description() {
        // GIVEN tools whose titles carry control characters, exceed the cap,
        //       or hold nothing but control characters
        let mut noisy = tool("noisy", None);
        noisy.title = Some("Search\n\u{7}pages".to_string());
        let mut long = tool("long", None);
        long.title = Some("\u{e9}".repeat(MAX_TITLE_LEN));
        let mut blank = tool("blank", None);
        blank.title = Some("\n\t".to_string());
        // WHEN the batch is sanitized
        let out = sanitize_tool_definitions(vec![noisy, long, blank], "srv", 256);
        // THEN control characters are stripped, the long title is truncated on
        //      a character boundary, and the blank one reads as absent
        assert_eq!(out[0].title.as_deref(), Some("Searchpages"));
        let truncated = out[1].title.as_deref().unwrap();
        assert!(truncated.len() <= MAX_TITLE_LEN && !truncated.is_empty());
        assert_eq!(out[2].title, None);
    }

    #[test]
    fn legitimate_title_passes_through_unchanged() {
        // GIVEN a tool with an ordinary title
        let mut t = tool("search_pages", None);
        t.title = Some("Search pages".to_string());
        // WHEN sanitized
        let out = sanitize_tool_definitions(vec![t], "srv", 256);
        // THEN the title is kept as is
        assert_eq!(out[0].title.as_deref(), Some("Search pages"));
    }
}
