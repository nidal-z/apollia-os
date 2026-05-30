//! Tauri IPC commands for tool introspection.
//!
//! Delegates to the internal REST API (`GET /api/v1/tools` and
//! `GET /api/v1/tools/:name`) to fetch the tool list and the full descriptor of
//! a tool registered in the ToolRegistry.

use apollia_runtime::embedded::RuntimeHandle;
use serde::{Deserialize, Serialize};
use tauri::State;

use super::http_get_json;

// ─────────────────────────────────────────────────────────────────────────────
// Types
// ─────────────────────────────────────────────────────────────────────────────

/// Summary of a tool for list display.
#[derive(Debug, Serialize, Deserialize)]
pub struct ToolSummary {
    /// Unique tool name.
    pub name: String,
    /// Semver version.
    pub version: String,
    /// Human-readable description.
    pub description: String,
    /// Tool kind (`"native"`, `"mcp"`, `"python"`).
    pub kind: String,
}

/// Detailed view of a tool for the frontend.
#[derive(Debug, Serialize, Deserialize)]
pub struct ToolDescriptorView {
    /// Unique tool name.
    pub name: String,
    /// Semver version.
    pub version: String,
    /// Human-readable description.
    pub description: String,
    /// Tool kind (`"native"`, `"mcp"`, etc.).
    pub kind: String,
    /// JSON schema of the inputs, or `null`.
    pub input_schema: Option<serde_json::Value>,
    /// JSON schema of the outputs, or `null`.
    pub output_schema: Option<serde_json::Value>,
    /// Permissions required by the tool.
    pub permissions: Vec<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Inner logic (testable without Tauri State)
// ─────────────────────────────────────────────────────────────────────────────

/// Fetches the list of registered tools via the internal REST API.
async fn list_tools_inner(port: u16) -> Result<Vec<ToolSummary>, String> {
    let json = http_get_json(port, "/api/v1/tools").await?;

    let tools_array = json
        .get("tools")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let summaries = tools_array
        .iter()
        .map(|t| ToolSummary {
            name: t
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            version: t
                .get("version")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            description: t
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            kind: t
                .get("kind")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
        })
        .collect();

    Ok(summaries)
}

/// Fetches a tool's descriptor via the internal REST API.
///
/// Returns `Ok(None)` if the name is empty or the tool does not exist (404).
async fn describe_tool_inner(port: u16, name: &str) -> Result<Option<ToolDescriptorView>, String> {
    if name.is_empty() {
        return Ok(None);
    }

    let path = format!("/api/v1/tools/{name}");
    match http_get_json(port, &path).await {
        Ok(json) => {
            let view = ToolDescriptorView {
                name: json
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                version: json
                    .get("version")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                description: json
                    .get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                kind: json
                    .get("kind")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                input_schema: json.get("input_schema").cloned(),
                output_schema: json.get("output_schema").cloned(),
                permissions: json
                    .get("permissions")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default(),
            };
            Ok(Some(view))
        }
        Err(e) if e.contains("404") => Ok(None),
        Err(e) => Err(format!("describe_tool: {e}")),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tauri commands
// ─────────────────────────────────────────────────────────────────────────────

/// Returns the list of tools registered in the runtime.
#[tauri::command]
pub async fn list_tools(state: State<'_, RuntimeHandle>) -> Result<Vec<ToolSummary>, String> {
    list_tools_inner(state.api_port).await
}

/// Returns the full descriptor of a tool by its name.
///
/// Returns `null` if the tool does not exist or the name is empty.
#[tauri::command]
pub async fn describe_tool(
    state: State<'_, RuntimeHandle>,
    name: String,
) -> Result<Option<ToolDescriptorView>, String> {
    describe_tool_inner(state.api_port, &name).await
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_descriptor_view_serializes() {
        // GIVEN a ToolDescriptorView with all fields
        let view = ToolDescriptorView {
            name: "bash_executor".to_string(),
            version: "1.0.0".to_string(),
            description: "Execute bash commands".to_string(),
            kind: "native".to_string(),
            input_schema: Some(serde_json::json!({"type": "object"})),
            output_schema: None,
            permissions: vec!["execute".to_string()],
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&view).expect("serialize");

        // THEN all fields are present
        assert_eq!(json["name"], "bash_executor");
        assert_eq!(json["version"], "1.0.0");
        assert_eq!(json["kind"], "native");
        assert!(json["input_schema"].is_object());
        assert!(json["output_schema"].is_null());
        assert_eq!(json["permissions"], serde_json::json!(["execute"]));
    }

    #[test]
    fn test_tool_descriptor_view_with_no_schema() {
        // GIVEN a view without schemas
        let view = ToolDescriptorView {
            name: "file_io".to_string(),
            version: "1.0.0".to_string(),
            description: "File I/O".to_string(),
            kind: "native".to_string(),
            input_schema: None,
            output_schema: None,
            permissions: vec![],
        };

        // WHEN serialized
        let json = serde_json::to_value(&view).expect("serialize");

        // THEN optional fields are null
        assert!(json["input_schema"].is_null());
        assert!(json["output_schema"].is_null());
        assert_eq!(json["permissions"], serde_json::json!([]));
    }

    #[test]
    fn test_tool_summary_serializes() {
        // GIVEN a ToolSummary
        let summary = ToolSummary {
            name: "bash_executor".to_string(),
            version: "1.0.0".to_string(),
            description: "Execute bash commands".to_string(),
            kind: "native".to_string(),
        };

        // WHEN serialized to JSON
        let json = serde_json::to_value(&summary).expect("serialize");

        // THEN all fields are present
        assert_eq!(json["name"], "bash_executor");
        assert_eq!(json["version"], "1.0.0");
        assert_eq!(json["kind"], "native");
        assert_eq!(json["description"], "Execute bash commands");
    }
}
