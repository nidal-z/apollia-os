//! User profile types for the REST API.
//!
//! These types represent the aggregated user profile built from
//! [`UserMemoryRepository`] entries. They are shared across crates
//! (runtime routes, Tauri commands, agents).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Aggregated user profile built from [`UserMemoryRepository`] entries.
///
/// Each field maps to a [`UserMemoryCategory`]: `preferences`, `habits`,
/// and `context` are `HashMap<String, String>` representing key→value pairs
/// stored in the user memory database.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UserProfile {
    /// Display name of the user (stored as `preferences.name`).
    pub name: Option<String>,
    /// User preferences (language, output format, tone, etc.).
    pub preferences: HashMap<String, String>,
    /// Observed user habits (working hours, review frequency, etc.).
    pub habits: HashMap<String, String>,
    /// Contextual information (current project, role, team, etc.).
    pub context: HashMap<String, String>,
}

/// Request body for `PUT /api/v1/user/profile`.
///
/// All fields are optional — only provided fields are upserted.
#[derive(Debug, Deserialize)]
pub struct UpdateProfileRequest {
    /// Display name of the user.
    pub name: Option<String>,
    /// Preferences to upsert (merge, not replace).
    pub preferences: Option<HashMap<String, String>>,
    /// Habits to upsert (merge, not replace).
    pub habits: Option<HashMap<String, String>>,
    /// Context entries to upsert (merge, not replace).
    pub context: Option<HashMap<String, String>>,
}

/// Response body for `GET /api/v1/user/memory`.
#[derive(Debug, Serialize, Deserialize)]
pub struct UserMemoryResponse {
    /// List of memory entries matching the query.
    pub entries: Vec<UserMemoryEntryResponse>,
}

/// A single user memory entry in API responses.
#[derive(Debug, Serialize, Deserialize)]
pub struct UserMemoryEntryResponse {
    /// Short key identifying the entry within its category.
    pub key: String,
    /// Human-readable value.
    pub value: String,
    /// How this entry was created (e.g. `"user_explicit"`, `"chat_inference"`).
    pub source: String,
    /// ISO 8601 last-update timestamp.
    pub updated_at: String,
}
