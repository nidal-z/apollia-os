//! MCP-facing tool schema definitions for the native Apollia tools.
//!
//! Each function returns a JSON object conforming to the MCP tool descriptor schema:
//! `{ name, description, inputSchema }`.
//!
//! [`native_tool_definitions`] is the authoritative source for the tools exposed via
//! the MCP stdio server. [`submit_task_definition`] is added only when a
//! `RuntimeHandle` is present.

/// Returns the MCP JSON schema definition for the `bash_executor` tool.
pub fn bash_executor_definition() -> serde_json::Value {
    serde_json::json!({
        "name": "bash_executor",
        "description": "Execute a shell command in a sandboxed environment. Returns stdout, stderr, and exit code.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "Shell command to run via /bin/sh -c."
                },
                "timeout_secs": {
                    "type": "integer",
                    "description": "Hard timeout in seconds before SIGKILL (default: 30).",
                    "default": 30
                },
                "working_dir": {
                    "type": "string",
                    "description": "Optional working directory. Defaults to the process cwd."
                }
            },
            "required": ["command"]
        }
    })
}

/// Returns the MCP JSON schema definition for the `file_read` tool.
pub fn file_read_definition() -> serde_json::Value {
    serde_json::json!({
        "name": "file_read",
        "description": "Read the text content of a file within the sandbox root.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Absolute or relative path to the file."
                },
                "offset": {
                    "type": "integer",
                    "description": "Line number to start reading from (1-based, optional)."
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of lines to return (optional)."
                }
            },
            "required": ["path"]
        }
    })
}

/// Returns the MCP JSON schema definition for the `file_write` tool.
pub fn file_write_definition() -> serde_json::Value {
    serde_json::json!({
        "name": "file_write",
        "description": "Write or overwrite a file within the sandbox root.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Absolute or relative path to the destination file."
                },
                "content": {
                    "type": "string",
                    "description": "Text content to write."
                }
            },
            "required": ["path", "content"]
        }
    })
}

/// Returns the MCP JSON schema definition for the `file_edit` tool.
pub fn file_edit_definition() -> serde_json::Value {
    serde_json::json!({
        "name": "file_edit",
        "description": "Perform an exact-string replacement in a file.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Absolute or relative path to the file."
                },
                "old_string": {
                    "type": "string",
                    "description": "Exact text to locate and replace."
                },
                "new_string": {
                    "type": "string",
                    "description": "Replacement text."
                }
            },
            "required": ["path", "old_string", "new_string"]
        }
    })
}

/// Returns the MCP JSON schema definition for the `file_list` tool.
pub fn file_list_definition() -> serde_json::Value {
    serde_json::json!({
        "name": "file_list",
        "description": "List the entries of a directory within the sandbox root.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Directory to list."
                },
                "recursive": {
                    "type": "boolean",
                    "description": "Recurse into subdirectories (default: false).",
                    "default": false
                }
            },
            "required": ["path"]
        }
    })
}

/// Returns the MCP JSON schema definition for the `file_glob` tool.
pub fn file_glob_definition() -> serde_json::Value {
    serde_json::json!({
        "name": "file_glob",
        "description": "Find files matching a glob pattern within the sandbox root.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Glob pattern relative to the sandbox root (e.g. src/**/*.rs)."
                }
            },
            "required": ["pattern"]
        }
    })
}

/// Returns the MCP JSON schema definition for the `file_grep` tool.
pub fn file_grep_definition() -> serde_json::Value {
    serde_json::json!({
        "name": "file_grep",
        "description": "Search file contents for a regex pattern, returning matching lines.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Regular expression to search for."
                },
                "path": {
                    "type": "string",
                    "description": "File or directory to search. Defaults to sandbox root."
                },
                "glob": {
                    "type": "string",
                    "description": "Glob filter applied to filenames (e.g. *.rs)."
                }
            },
            "required": ["pattern"]
        }
    })
}

/// Returns the MCP JSON schema definition for the `mcp_client` tool.
///
/// This tool proxies a call to an external MCP server already configured in
/// `mcp.toml`. It requires a running runtime with at least one MCP client
/// session.
pub fn mcp_client_definition() -> serde_json::Value {
    serde_json::json!({
        "name": "mcp_client",
        "description": "Invoke a tool on a configured external MCP server. Requires a running Apollia runtime with mcp.toml servers.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "server": {
                    "type": "string",
                    "description": "Name of the MCP server as declared in mcp.toml."
                },
                "tool": {
                    "type": "string",
                    "description": "Tool name exposed by the target MCP server."
                },
                "arguments": {
                    "type": "object",
                    "description": "Arguments forwarded verbatim to the remote tool."
                }
            },
            "required": ["server", "tool"]
        }
    })
}

/// Returns the MCP JSON schema definition for the `agent_install` tool.
pub fn agent_install_definition() -> serde_json::Value {
    serde_json::json!({
        "name": "agent_install",
        "description": "Install an Apollia OS agent from a local path or a Git URL.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "source": {
                    "type": "string",
                    "description": "Local filesystem path or Git URL (with optional #tag suffix)."
                }
            },
            "required": ["source"]
        }
    })
}

/// Returns the MCP JSON schema definition for the `submit_task` tool.
///
/// This tool is only exposed when the server was started with `--with-runtime`,
/// making the full `RuntimeHandle` available.
pub fn submit_task_definition() -> serde_json::Value {
    serde_json::json!({
        "name": "submit_task",
        "description": "Delegate a task to an Apollia OS agent. Returns the task_id for tracking.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "task": {
                    "type": "string",
                    "description": "Natural language description of the task."
                },
                "agent_id": {
                    "type": "string",
                    "description": "ID of the agent. Defaults to 'default'.",
                    "default": "default"
                }
            },
            "required": ["task"]
        }
    })
}

/// Returns the MCP schema definitions for the 9 native tools.
///
/// This list is the authoritative source for what the MCP stdio server exposes.
/// `submit_task` is not included here; it is added conditionally by the server
/// when a `RuntimeHandle` is available.
pub fn native_tool_definitions() -> Vec<serde_json::Value> {
    vec![
        bash_executor_definition(),
        file_read_definition(),
        file_write_definition(),
        file_edit_definition(),
        file_list_definition(),
        file_glob_definition(),
        file_grep_definition(),
        mcp_client_definition(),
        agent_install_definition(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_tool_definitions_returns_nine_tools() {
        // GIVEN / WHEN
        let tools = native_tool_definitions();
        // THEN
        assert_eq!(tools.len(), 9);
    }

    #[test]
    fn all_native_tools_have_required_fields() {
        // GIVEN
        let tools = native_tool_definitions();
        // WHEN / THEN: every tool must have name, description, inputSchema
        for tool in &tools {
            assert!(
                tool.get("name").and_then(|v| v.as_str()).is_some(),
                "missing name"
            );
            assert!(
                tool.get("description").and_then(|v| v.as_str()).is_some(),
                "missing description on {}",
                tool.get("name").and_then(|v| v.as_str()).unwrap_or("?")
            );
            assert!(tool.get("inputSchema").is_some(), "missing inputSchema");
        }
    }

    #[test]
    fn submit_task_definition_has_required_task_field() {
        // GIVEN / WHEN
        let def = submit_task_definition();
        // THEN required array contains "task"
        let required = def["inputSchema"]["required"]
            .as_array()
            .expect("required must be array");
        assert!(required.iter().any(|v| v.as_str() == Some("task")));
    }

    #[test]
    fn tool_names_are_unique() {
        // GIVEN
        let tools = native_tool_definitions();
        // WHEN
        let names: Vec<&str> = tools
            .iter()
            .filter_map(|t| t.get("name").and_then(|v| v.as_str()))
            .collect();
        let mut dedup = names.clone();
        dedup.dedup();
        // THEN
        assert_eq!(names.len(), dedup.len(), "duplicate tool names found");
    }
}
