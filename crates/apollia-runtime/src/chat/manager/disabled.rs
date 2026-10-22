//! What a turn may propose: a granted tool that is not disabled.
//!
//! The dispatcher leaves a disabled tool out, so a call to it is refused; this
//! module keeps it out of what the turn proposes as well: the agent's access
//! list and the deferred MCP index the model searches. An MCP tool the agent
//! is not granted is kept out of that index, so `tool_search` cannot find it.
//! Natives, MCP tools and connector tools answer to the same lists.

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
    let is_disabled = |name: &str| apollia_tools::is_disabled(name, disabled);
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

/// `index` without the MCP tools `granted` does not list. An index entry is
/// named `mcp:<server>/<tool>`, as the dispatcher registers it.
pub(in crate::chat::manager) fn only_granted(
    index: Vec<ToolIndexSnapshot>,
    granted: &[String],
) -> Vec<ToolIndexSnapshot> {
    index
        .into_iter()
        .filter(|e| {
            let name = format!("mcp:{}/{}", e.server_name, e.tool_name);
            granted.contains(&name)
        })
        .collect()
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
            ..Default::default()
        }
    }

    // A disabled MCP tool, connector tool or native is not proposed, whether
    // the session lists it or the deferred index holds it.
    #[test]
    fn test_disabled_tools_are_not_proposed() {
        // GIVEN a session listing a native, the tool of a connected account
        // and an MCP tool, an index holding two MCP tools, and three of them
        // disabled, the connector one by its operation
        let available = vec![
            "file_read".to_string(),
            "conn:google:ada@example.com/gmail.send".to_string(),
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

    // An MCP tool the agent is not granted is not in the index it searches.
    #[test]
    fn test_index_keeps_only_granted_mcp_tools() {
        // GIVEN an index of three MCP tools, one of them granted with a native
        let index = vec![
            entry("notion", "search"),
            entry("notion", "create_page"),
            entry("drive", "list"),
        ];
        let granted = vec!["file_read".to_string(), "mcp:notion/search".to_string()];

        // WHEN the index is restricted to the access list
        let index = only_granted(index, &granted);

        // THEN only the granted MCP tool is left
        assert_eq!(index.len(), 1);
        assert_eq!(index[0].server_name, "notion");
        assert_eq!(index[0].tool_name, "search");
    }
}
