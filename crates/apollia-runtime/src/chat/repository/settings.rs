//! The configuration of a conversation: its tools, its model, its profile,
//! and the settings and the notes it keeps for itself.
//!
//! The settings live in one JSON column read through [`SessionSettings`];
//! the notes are text beside them. The instructions an older client sends as
//! a prompt are a field of the settings.

use rusqlite::params;

use super::ChatSessionRepository;
use crate::chat::session_settings::SessionSettings;
use crate::chat::types::{ChatError, SessionConfigUpdate};

impl ChatSessionRepository {
    /// Update session configuration (instructions, available_tools,
    /// llm_backend, profile_id).
    ///
    /// Only updates fields that are `Some`. `system_prompt` is the name older
    /// clients give the instructions of the conversation: it is written to
    /// the settings and leaves their other fields as they are.
    pub fn update_session_config(
        &self,
        id: &str,
        update: &SessionConfigUpdate,
    ) -> Result<(), ChatError> {
        let mut parts = Vec::new();
        let mut values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

        if let Some(prompt) = &update.system_prompt {
            let mut settings = self.session_settings(id)?;
            settings.instructions = non_blank(prompt);
            parts.push("settings = ?");
            values.push(Box::new(settings.to_column()));
        }
        if let Some(tools) = &update.available_tools {
            let tools_json = serde_json::to_string(tools)
                .map_err(|e| ChatError::InternalError(format!("tools serialization: {e}")))?;
            parts.push("available_tools = ?");
            values.push(Box::new(tools_json));
        }
        if let Some(backend) = &update.llm_backend {
            parts.push("llm_backend = ?");
            values.push(Box::new(backend.clone()));
        }
        if let Some(profile) = &update.profile_id {
            parts.push("profile_id = ?");
            values.push(Box::new(profile.clone()));
        }
        if let Some(use_profile) = update.use_profile {
            parts.push("use_profile = ?");
            values.push(Box::new(i64::from(use_profile)));
        }

        if parts.is_empty() {
            return Ok(());
        }

        let set_clause = parts.join(", ");
        let sql = format!("UPDATE chat_sessions SET {set_clause} WHERE id = ?");
        values.push(Box::new(id.to_string()));

        let params: Vec<&dyn rusqlite::types::ToSql> = values.iter().map(|v| v.as_ref()).collect();
        let updated = self
            .conn
            .execute(&sql, params.as_slice())
            .map_err(|e| ChatError::InternalError(format!("update_session_config: {e}")))?;

        if updated == 0 {
            return Err(ChatError::SessionNotFound(id.to_string()));
        }
        Ok(())
    }

    /// The settings of session `id`.
    ///
    /// Returns `Err(ChatError::SessionNotFound)` if the session does not exist.
    pub fn session_settings(&self, id: &str) -> Result<SessionSettings, ChatError> {
        let row = self
            .get_session(id)?
            .ok_or_else(|| ChatError::SessionNotFound(id.to_string()))?;
        Ok(SessionSettings::from_column(id, &row.settings))
    }

    /// Replace the settings of session `id`.
    ///
    /// Returns `Err(ChatError::SessionNotFound)` if the session does not exist.
    pub fn set_session_settings(
        &self,
        id: &str,
        settings: &SessionSettings,
    ) -> Result<(), ChatError> {
        self.write_session_column(id, "settings", &settings.to_column())
    }

    /// Replace the notes of session `id`.
    ///
    /// Returns `Err(ChatError::SessionNotFound)` if the session does not exist.
    pub fn set_session_notes(&self, id: &str, notes: &str) -> Result<(), ChatError> {
        self.write_session_column(id, "notes", notes)
    }

    fn write_session_column(
        &self,
        id: &str,
        column: &'static str,
        value: &str,
    ) -> Result<(), ChatError> {
        let updated = self
            .conn
            .execute(
                &format!("UPDATE chat_sessions SET {column} = ?1 WHERE id = ?2"),
                params![value, id],
            )
            .map_err(|e| ChatError::InternalError(format!("write {column}: {e}")))?;
        if updated == 0 {
            return Err(ChatError::SessionNotFound(id.to_string()));
        }
        Ok(())
    }
}

/// `text` trimmed, or `None` when it is blank.
pub(super) fn non_blank(text: &str) -> Option<String> {
    let trimmed = text.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}
