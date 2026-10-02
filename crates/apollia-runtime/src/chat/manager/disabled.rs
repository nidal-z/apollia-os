//! A disabled tool is not offered to the model.
//!
//! The dispatcher leaves a disabled tool out, so a call to it is refused; this
//! module keeps it out of what the turn proposes as well: the session's tool
//! list and the deferred MCP index the model searches. Natives, MCP tools and
//! connector tools answer to the same list.

use apollia_mcp::tool_search::ToolIndexSnapshot;

/// `available` and `index` without the tools `disabled` names. An index entry
/// is named `mcp:<server>/<tool>`, as the dispatcher registers it.
pub(in crate::chat::manager) fn without_disabled(
    available: Vec<String>,
    index: Vec<ToolIndexSnapshot>,
    disabled: &[String],
) -> (Vec<String>, Vec<ToolIndexSnapshot>) {
    if disabled.is_empty() {
        return (available, index);
    }
    let is_disabled = |name: &str| disabled.iter().any(|d| d == name);
    let available = available
        .into_iter()
        .filter(|name| !is_disabled(name))
        .collect();
    let index = index
        .into_iter()
        .filter(|e| !is_disabled(&format!("mcp:{}/{}", e.server_name, e.tool_name)))
        .collect();
    (available, index)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(server: &str, tool: &str) -> ToolIndexSnapshot {
        ToolIndexSnapshot {
            server_name: server.into(),
            tool_name: tool.into(),
            description: None,
            tags: Vec::new(),
            input_schema: None,
        }
    }

    // A disabled MCP tool, connector tool or native is not proposed, whether
    // the session lists it or the deferred index holds it.
    #[test]
    fn test_disabled_tools_are_not_proposed() {
        // GIVEN a session listing a native, a connector tool and an MCP tool,
        // an index holding two MCP tools, and three of them disabled
        let available = vec![
            "file_read".to_string(),
            "gmail.send".to_string(),
            "mcp:notion/create_page".to_string(),
        ];
        let index = vec![entry("notion", "create_page"), entry("notion", "search")];
        let disabled = vec![
            "file_read".to_string(),
            "gmail.send".to_string(),
            "mcp:notion/create_page".to_string(),
        ];

        // WHEN the proposal is filtered
        let (available, index) = without_disabled(available, index, &disabled);

        // THEN none of the disabled tools is left, and the enabled one is
        assert!(available.is_empty(), "{available:?}");
        assert_eq!(index.len(), 1);
        assert_eq!(index[0].tool_name, "search");
    }
}
