//! The last local model the user loaded by hand, persisted in `system.db`.
//!
//! One row: the identifier the model index gives the model, and the load
//! settings it was loaded with, so "reload the last model" brings back the
//! same model under the same settings after a restart. Only a load that
//! succeeded is written. The table is created by `system_db::migrate_v7`.

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::llm_backend::{LlmBackendError, LlmBackendRepository};
use crate::system_db::SYSTEM_PROMPT_KEYS;

/// The last local model loaded by hand.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LastLoadedModel {
    /// The identifier the model index gives the model.
    pub model_id: String,
    /// The load settings it was loaded with, in the keys a preset's load
    /// settings use. Empty when it was loaded under its default preset alone.
    pub load: serde_json::Map<String, serde_json::Value>,
    /// When it was loaded, RFC 3339.
    pub loaded_at: String,
}

impl LlmBackendRepository {
    /// The last local model loaded by hand, `None` before the first load.
    ///
    /// # Errors
    /// [`LlmBackendError::Db`] or [`LlmBackendError::Serialization`].
    pub fn last_loaded_model(&self) -> Result<Option<LastLoadedModel>, LlmBackendError> {
        let raw: Option<(String, String, String)> = self
            .conn
            .borrow()
            .query_row(
                "SELECT model_id, load_json, loaded_at FROM last_loaded_model WHERE id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        raw.map(|(model_id, load, loaded_at)| {
            Ok(LastLoadedModel {
                model_id,
                load: serde_json::from_str(&load)
                    .map_err(|e| LlmBackendError::Serialization(e.to_string()))?,
                loaded_at,
            })
        })
        .transpose()
    }

    /// Record `model_id`, loaded under `load`, as the last model loaded by
    /// hand, replacing the previous one.
    ///
    /// # Errors
    /// - [`LlmBackendError::InvalidPreset`] for a blank identifier, or a system
    ///   prompt among the settings.
    /// - [`LlmBackendError::Db`] or [`LlmBackendError::Serialization`].
    pub fn set_last_loaded_model(
        &self,
        model_id: &str,
        load: &serde_json::Map<String, serde_json::Value>,
    ) -> Result<(), LlmBackendError> {
        let model_id = model_id.trim();
        if model_id.is_empty() {
            return Err(LlmBackendError::InvalidPreset(
                "the model identifier is empty".to_owned(),
            ));
        }
        if let Some(key) = SYSTEM_PROMPT_KEYS.iter().find(|k| load.contains_key(**k)) {
            return Err(LlmBackendError::InvalidPreset(format!(
                "load settings carry no system prompt: '{key}' is refused"
            )));
        }
        let load = serde_json::to_string(load)
            .map_err(|e| LlmBackendError::Serialization(e.to_string()))?;
        self.conn.borrow().execute(
            "INSERT INTO last_loaded_model (id, model_id, load_json, loaded_at)
             VALUES (1, ?1, ?2, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
             ON CONFLICT(id) DO UPDATE SET
                model_id  = excluded.model_id,
                load_json = excluded.load_json,
                loaded_at = excluded.loaded_at",
            params![model_id, load],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn load(value: serde_json::Value) -> serde_json::Map<String, serde_json::Value> {
        value.as_object().cloned().unwrap()
    }

    #[test]
    fn test_a_fresh_file_has_no_last_loaded_model() {
        // GIVEN a system.db that has just been created
        let dir = tempfile::TempDir::new().unwrap();

        // WHEN the last loaded model is read
        let repo = LlmBackendRepository::open(&dir.path().join("system.db")).unwrap();

        // THEN there is none
        assert_eq!(repo.last_loaded_model().unwrap(), None);
    }

    #[test]
    fn test_the_last_loaded_model_survives_a_reopen_and_the_last_load_wins() {
        // GIVEN two loads recorded one after the other
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("system.db");
        {
            let repo = LlmBackendRepository::open(&path).unwrap();
            repo.set_last_loaded_model("gemma-3-4b", &load(json!({ "n_ctx": 8192 })))
                .unwrap();
            repo.set_last_loaded_model(
                " Qwen3-8B-GGUF ",
                &load(json!({ "n_ctx": 16384, "n_gpu_layers": 20 })),
            )
            .unwrap();
        }

        // WHEN the file is opened again
        let repo = LlmBackendRepository::open(&path).unwrap();
        let last = repo.last_loaded_model().unwrap().unwrap();

        // THEN the second load is the one read, with its settings
        assert_eq!(last.model_id, "Qwen3-8B-GGUF");
        assert_eq!(
            last.load,
            load(json!({ "n_ctx": 16384, "n_gpu_layers": 20 }))
        );
        assert!(!last.loaded_at.is_empty());
    }

    #[test]
    fn test_a_blank_model_or_a_system_prompt_is_refused_and_keeps_the_last() {
        // GIVEN a last loaded model
        let dir = tempfile::TempDir::new().unwrap();
        let repo = LlmBackendRepository::open(&dir.path().join("system.db")).unwrap();
        repo.set_last_loaded_model("gemma-3-4b", &serde_json::Map::new())
            .unwrap();

        // WHEN a blank identifier, then a system prompt, are written
        let blank = repo.set_last_loaded_model("  ", &serde_json::Map::new());
        let prompt =
            repo.set_last_loaded_model("other", &load(json!({ "system_prompt": "be nice" })));

        // THEN both are refused and the last model is unchanged
        assert!(matches!(blank, Err(LlmBackendError::InvalidPreset(_))));
        assert!(matches!(prompt, Err(LlmBackendError::InvalidPreset(_))));
        assert_eq!(
            repo.last_loaded_model().unwrap().map(|m| m.model_id),
            Some("gemma-3-4b".to_owned())
        );
    }

    #[test]
    fn test_a_file_at_version_six_gains_the_table_and_keeps_its_rows() {
        // GIVEN a system.db written by a binary at schema version six, with a
        // default embeddings model
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("system.db");
        {
            let conn = rusqlite::Connection::open(&path).unwrap();
            for step in &crate::system_db::SYSTEM_DB_MIGRATIONS[..6] {
                step(&conn).unwrap();
            }
            conn.pragma_update(None, "user_version", 6).unwrap();
            conn.execute(
                "INSERT INTO embedding_default (id, model_id) VALUES (1, 'e5')",
                [],
            )
            .unwrap();
        }

        // WHEN the repository opens it
        let repo = LlmBackendRepository::open(&path).unwrap();

        // THEN the file is at the current version, the table answers, and the row is kept
        assert_eq!(repo.last_loaded_model().unwrap(), None);
        let conn = repo.conn.borrow();
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(
            version,
            i64::from(crate::system_db::SYSTEM_DB_SCHEMA_VERSION)
        );
        let kept: String = conn
            .query_row("SELECT model_id FROM embedding_default", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(kept, "e5");
    }
}
