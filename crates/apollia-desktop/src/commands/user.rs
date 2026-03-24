//! Tauri IPC commands for user profile and memory management.
//!
//! Each command delegates to the REST API endpoints defined in
//! `apollia-runtime/src/api/routes_user.rs` via the HTTP helpers.

use apollia_runtime::embedded::RuntimeHandle;
use serde::{Deserialize, Serialize};
use tauri::State;

use super::{http_delete_json, http_get_json, http_put_json};

/// Aggregated user profile returned by `get_user_profile`.
#[derive(Debug, Serialize, Deserialize)]
pub struct UserProfileView {
    /// Display name of the user.
    pub name: Option<String>,
    /// User preferences (language, tone, output format, etc.).
    pub preferences: std::collections::HashMap<String, String>,
    /// Observed user habits (working hours, review frequency, etc.).
    pub habits: std::collections::HashMap<String, String>,
    /// Contextual information (current project, role, team, etc.).
    pub context: std::collections::HashMap<String, String>,
}

/// A single user memory entry for display in the UI.
#[derive(Debug, Serialize, Deserialize)]
pub struct UserMemoryEntryView {
    /// Entry key (e.g. `"language"`, `"working_hours"`).
    pub key: String,
    /// Entry value.
    pub value: String,
    /// Source of the entry (`"user_explicit"`, `"chat_inference"`, etc.).
    pub source: String,
    /// Last update timestamp (ISO 8601).
    pub updated_at: String,
}

/// Request body for `update_user_profile`.
#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateProfileInput {
    /// Display name to set.
    pub name: Option<String>,
    /// Preferences to upsert.
    pub preferences: Option<std::collections::HashMap<String, String>>,
    /// Habits to upsert.
    pub habits: Option<std::collections::HashMap<String, String>>,
    /// Context entries to upsert.
    pub context: Option<std::collections::HashMap<String, String>>,
}

/// Fetches the aggregated user profile from all memory categories.
#[tauri::command]
pub async fn get_user_profile(state: State<'_, RuntimeHandle>) -> Result<UserProfileView, String> {
    let json = http_get_json(state.api_port, "/api/v1/user/profile").await?;
    serde_json::from_value(json).map_err(|e| format!("failed to parse user profile: {e}"))
}

/// Updates the user profile by upserting the provided fields.
#[tauri::command]
pub async fn update_user_profile(
    state: State<'_, RuntimeHandle>,
    data: UpdateProfileInput,
) -> Result<(), String> {
    let body = serde_json::to_value(&data)
        .map_err(|e| format!("failed to serialize update request: {e}"))?;
    http_put_json(state.api_port, "/api/v1/user/profile", &body).await?;
    Ok(())
}

/// Lists user memory entries, optionally filtered by category.
#[tauri::command]
pub async fn get_user_memory(
    state: State<'_, RuntimeHandle>,
    category: Option<String>,
) -> Result<Vec<UserMemoryEntryView>, String> {
    let path = match &category {
        Some(cat) => format!("/api/v1/user/memory?category={cat}"),
        None => "/api/v1/user/memory".to_string(),
    };
    let json = http_get_json(state.api_port, &path).await?;

    let entries = json
        .get("entries")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    entries
        .into_iter()
        .map(|e| {
            serde_json::from_value::<UserMemoryEntryView>(e)
                .map_err(|err| format!("failed to parse memory entry: {err}"))
        })
        .collect()
}

/// Deletes a single user memory entry by key.
#[tauri::command]
pub async fn forget_user_memory(
    state: State<'_, RuntimeHandle>,
    key: String,
) -> Result<(), String> {
    let path = format!("/api/v1/user/memory/{key}");
    http_delete_json(state.api_port, &path).await?;
    Ok(())
}
