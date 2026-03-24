//! Commandes IPC Tauri pour l'introspection des outils.
//!
//! Délègue à l'API REST interne (`GET /api/v1/tools` et `GET /api/v1/tools/:name`)
//! pour récupérer la liste des outils et le descripteur complet d'un outil
//! enregistré dans le ToolRegistry.

use apollia_runtime::embedded::RuntimeHandle;
use serde::{Deserialize, Serialize};
use tauri::State;

use super::http_get_json;

// ─────────────────────────────────────────────────────────────────────────────
// Types
// ─────────────────────────────────────────────────────────────────────────────

/// Résumé d'un outil pour l'affichage en liste.
#[derive(Debug, Serialize, Deserialize)]
pub struct ToolSummary {
    /// Nom unique de l'outil.
    pub name: String,
    /// Version semver.
    pub version: String,
    /// Description humaine.
    pub description: String,
    /// Type d'outil (`"native"`, `"mcp"`, `"python"`).
    pub kind: String,
}

/// Vue détaillée d'un outil pour le frontend.
#[derive(Debug, Serialize, Deserialize)]
pub struct ToolDescriptorView {
    /// Nom unique de l'outil.
    pub name: String,
    /// Version semver.
    pub version: String,
    /// Description humaine.
    pub description: String,
    /// Type d'outil (`"native"`, `"mcp"`, etc.).
    pub kind: String,
    /// Schéma JSON des entrées, ou `null`.
    pub input_schema: Option<serde_json::Value>,
    /// Schéma JSON des sorties, ou `null`.
    pub output_schema: Option<serde_json::Value>,
    /// Permissions requises par l'outil.
    pub permissions: Vec<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Inner logic (testable without Tauri State)
// ─────────────────────────────────────────────────────────────────────────────

/// Récupère la liste des outils enregistrés via l'API REST interne.
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

/// Récupère le descripteur d'un outil via l'API REST interne.
///
/// Retourne `Ok(None)` si le nom est vide ou si l'outil n'existe pas (404).
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

/// Retourne la liste des outils enregistrés dans le runtime.
#[tauri::command]
pub async fn list_tools(state: State<'_, RuntimeHandle>) -> Result<Vec<ToolSummary>, String> {
    list_tools_inner(state.api_port).await
}

/// Retourne le descripteur complet d'un outil par son nom.
///
/// Retourne `null` si l'outil n'existe pas ou si le nom est vide.
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
