//! The local models the user pinned in memory, persisted in `system.db`.
//!
//! A pinned model stays loaded until the user unloads it by hand: it is never
//! unloaded after a delay without use, nor when the last conversation that
//! uses it closes. A model is keyed by the path it is loaded from, the key the
//! list of resident models and an unload take. The table is created by
//! `system_db::migrate_v9`.

use rusqlite::params;

use crate::llm_backend::{LlmBackendError, LlmBackendRepository};

impl LlmBackendRepository {
    /// The path of every pinned model, in path order.
    ///
    /// # Errors
    /// [`LlmBackendError::Db`].
    pub fn pinned_models(&self) -> Result<Vec<String>, LlmBackendError> {
        let conn = self.conn.borrow();
        let mut statement =
            conn.prepare("SELECT model_path FROM model_pins ORDER BY model_path")?;
        let paths = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(paths)
    }

    /// Pin `model_path` in memory, or unpin it. Pinning a pinned model, or
    /// unpinning one that is not, changes nothing.
    ///
    /// # Errors
    /// - [`LlmBackendError::InvalidPreset`] for a blank path.
    /// - [`LlmBackendError::Db`].
    pub fn set_model_pinned(&self, model_path: &str, pinned: bool) -> Result<(), LlmBackendError> {
        let model_path = model_path.trim();
        if model_path.is_empty() {
            return Err(LlmBackendError::InvalidPreset(
                "the model path is empty".to_owned(),
            ));
        }
        let conn = self.conn.borrow();
        if pinned {
            conn.execute(
                "INSERT INTO model_pins (model_path) VALUES (?1)
                 ON CONFLICT(model_path) DO NOTHING",
                params![model_path],
            )?;
        } else {
            conn.execute(
                "DELETE FROM model_pins WHERE model_path = ?1",
                params![model_path],
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_a_fresh_file_pins_no_model() {
        // GIVEN a system.db that has just been created
        let dir = tempfile::TempDir::new().unwrap();

        // WHEN the pinned models are read
        let repo = LlmBackendRepository::open(&dir.path().join("system.db")).unwrap();

        // THEN there is none
        assert!(repo.pinned_models().unwrap().is_empty());
    }

    #[test]
    fn test_a_pin_survives_a_reopen_and_an_unpin_removes_it() {
        // GIVEN two models pinned, one of them twice
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("system.db");
        {
            let repo = LlmBackendRepository::open(&path).unwrap();
            repo.set_model_pinned("/models/b.gguf", true).unwrap();
            repo.set_model_pinned("/models/a.gguf", true).unwrap();
            repo.set_model_pinned("/models/a.gguf", true).unwrap();
        }

        // WHEN the file is reopened and one model is unpinned
        let repo = LlmBackendRepository::open(&path).unwrap();
        let before = repo.pinned_models().unwrap();
        repo.set_model_pinned("/models/b.gguf", false).unwrap();

        // THEN both were kept across the reopen, once each, and the unpin
        //      leaves only the other
        assert_eq!(before, vec!["/models/a.gguf", "/models/b.gguf"]);
        assert_eq!(repo.pinned_models().unwrap(), vec!["/models/a.gguf"]);
    }

    #[test]
    fn test_a_blank_path_is_refused() {
        // GIVEN a fresh system.db
        let dir = tempfile::TempDir::new().unwrap();
        let repo = LlmBackendRepository::open(&dir.path().join("system.db")).unwrap();

        // WHEN a blank path is pinned
        let refused = repo.set_model_pinned("  ", true);

        // THEN it is refused and nothing is written
        assert!(matches!(refused, Err(LlmBackendError::InvalidPreset(_))));
        assert!(repo.pinned_models().unwrap().is_empty());
    }
}
