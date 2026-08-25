//! STT configuration persisted in `system.db`.
//!
//! [`SttConfigRepository`] manages a singleton row in the `stt_config` table.
//! On first access, if the table is empty, the default values are inserted automatically.

use std::path::Path;

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use thiserror::Error;

// ── Migration ────────────────────────────────────────────────────────
//
// The `stt_config` DDL lives in `crate::system_db`: `system.db` is shared
// with the LLM backend registry, and `PRAGMA user_version` belongs to the
// file, so the two stores migrate through one numbered list.

// ── Types ────────────────────────────────────────────────────────────

/// STT configuration persisted as a singleton row in `system.db`.
///
/// All fields have sane defaults so that partial JSON payloads (e.g. from a
/// PUT request) are accepted without requiring every field to be present.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SttConfigRow {
    /// Whether the STT engine is enabled.
    #[serde(default)]
    pub enabled: bool,
    /// Absolute path to the GGML model file (empty string = not configured).
    #[serde(default)]
    pub model_path: String,
    /// Global hotkey string to trigger recording.
    #[serde(default = "default_hotkey")]
    pub hotkey: String,
    /// Text injection mode: `"paste"` or `"clipboard"`.
    #[serde(default = "default_clipboard_mode")]
    pub clipboard_mode: String,
    /// Whether to restore the clipboard content after text injection.
    #[serde(default = "default_clipboard_restore")]
    pub clipboard_restore: bool,
    /// RMS silence threshold in dB for end-of-speech detection.
    #[serde(default = "default_silence_threshold_db")]
    pub silence_threshold_db: f32,
    /// Maximum recording duration in seconds.
    #[serde(default = "default_max_recording_sec")]
    pub max_recording_sec: u32,
    /// Target transcription language (ISO 639-1). `None` = auto-detect.
    #[serde(default)]
    pub language: Option<String>,
    /// Recording trigger mode: `"toggle"` or `"push-to-talk"`.
    #[serde(default = "default_trigger_mode")]
    pub trigger_mode: String,
    /// Audio input device name to capture from. `None` = system default.
    #[serde(default)]
    pub input_device: Option<String>,
}

impl Default for SttConfigRow {
    fn default() -> Self {
        Self {
            enabled: false,
            model_path: String::new(),
            hotkey: default_hotkey(),
            clipboard_mode: default_clipboard_mode(),
            clipboard_restore: default_clipboard_restore(),
            silence_threshold_db: default_silence_threshold_db(),
            max_recording_sec: default_max_recording_sec(),
            language: None,
            trigger_mode: default_trigger_mode(),
            input_device: None,
        }
    }
}

fn default_hotkey() -> String {
    "ctrl+shift+space".to_owned()
}

fn default_clipboard_mode() -> String {
    "paste".to_owned()
}

fn default_clipboard_restore() -> bool {
    true
}

fn default_silence_threshold_db() -> f32 {
    -40.0
}

fn default_max_recording_sec() -> u32 {
    60
}

fn default_trigger_mode() -> String {
    "toggle".to_owned()
}

// ── Errors ───────────────────────────────────────────────────────────

/// Errors from [`SttConfigRepository`].
#[derive(Debug, Error)]
pub enum SttConfigError {
    /// Underlying SQLite error.
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),

    /// The database schema could not be brought to the supported version.
    #[error(transparent)]
    Schema(#[from] crate::schema::SchemaError),
}

// ── Repository ───────────────────────────────────────────────────────

/// Singleton repository for STT configuration stored in `system.db`.
///
/// The `stt_config` table holds at most one row (`id = 1`). The `CHECK (id = 1)`
/// constraint ensures no duplicate rows can be inserted.
///
/// **Thread safety:** `rusqlite::Connection` is `Send` but not `Sync`.
/// Wrap in `Arc<Mutex<SttConfigRepository>>` when sharing across tasks.
pub struct SttConfigRepository {
    conn: Connection,
}

impl SttConfigRepository {
    /// Open (or create) `system.db` at the given path and migrate it.
    ///
    /// The file is brought to the current `system.db` schema version through
    /// [`crate::schema::open_versioned`]; a database written by a newer binary
    /// is refused instead of misread.
    ///
    /// # Errors
    /// Returns [`SttConfigError::Db`] if the file cannot be opened, and
    /// [`SttConfigError::Schema`] if the migration fails or the database is
    /// newer than this binary.
    pub fn open(path: &Path) -> Result<Self, SttConfigError> {
        let conn = Connection::open(path)?;
        crate::schema::open_versioned(
            &conn,
            crate::paths::DataFile::System.file_name(),
            crate::system_db::SYSTEM_DB_SCHEMA_VERSION,
            crate::system_db::SYSTEM_DB_MIGRATIONS,
        )?;
        Ok(Self { conn })
    }

    /// Return the current configuration, inserting defaults if the table is empty.
    ///
    /// On first call against an empty table, the default row is written and returned.
    /// Subsequent calls return the persisted row without writing.
    ///
    /// # Errors
    /// Returns [`SttConfigError::Db`] on any SQLite failure.
    pub fn get_or_default(&self) -> Result<SttConfigRow, SttConfigError> {
        let mut stmt = self.conn.prepare(
            "SELECT enabled, model_path, hotkey, clipboard_mode, clipboard_restore,
                    silence_threshold_db, max_recording_sec, language, trigger_mode,
                    input_device
             FROM stt_config WHERE id = 1",
        )?;

        let mut rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, f64>(5)?,
                row.get::<_, i64>(6)?,
                row.get::<_, Option<String>>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, Option<String>>(9)?,
            ))
        })?;

        match rows.next() {
            Some(row) => {
                let (
                    enabled,
                    model_path,
                    hotkey,
                    clipboard_mode,
                    clipboard_restore,
                    silence_threshold_db,
                    max_recording_sec,
                    language,
                    trigger_mode,
                    input_device,
                ) = row?;
                Ok(SttConfigRow {
                    enabled: enabled != 0,
                    model_path,
                    hotkey,
                    clipboard_mode,
                    clipboard_restore: clipboard_restore != 0,
                    silence_threshold_db: silence_threshold_db as f32,
                    max_recording_sec: max_recording_sec as u32,
                    language,
                    trigger_mode,
                    input_device,
                })
            }
            None => {
                let defaults = SttConfigRow::default();
                self.upsert(&defaults)?;
                Ok(defaults)
            }
        }
    }

    /// Insert or replace the singleton configuration row (`id = 1`).
    ///
    /// Uses `INSERT … ON CONFLICT DO UPDATE` so it is safe to call regardless
    /// of whether a row already exists.
    ///
    /// # Errors
    /// Returns [`SttConfigError::Db`] on any SQLite failure.
    pub fn upsert(&self, config: &SttConfigRow) -> Result<(), SttConfigError> {
        self.conn.execute(
            "INSERT INTO stt_config (
                id, enabled, model_path, hotkey, clipboard_mode, clipboard_restore,
                silence_threshold_db, max_recording_sec, language, trigger_mode,
                input_device, updated_at
             ) VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
                       strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
             ON CONFLICT(id) DO UPDATE SET
                enabled              = excluded.enabled,
                model_path           = excluded.model_path,
                hotkey               = excluded.hotkey,
                clipboard_mode       = excluded.clipboard_mode,
                clipboard_restore    = excluded.clipboard_restore,
                silence_threshold_db = excluded.silence_threshold_db,
                max_recording_sec    = excluded.max_recording_sec,
                language             = excluded.language,
                trigger_mode         = excluded.trigger_mode,
                input_device         = excluded.input_device,
                updated_at           = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')",
            params![
                config.enabled as i32,
                config.model_path,
                config.hotkey,
                config.clipboard_mode,
                config.clipboard_restore as i32,
                config.silence_threshold_db as f64,
                config.max_recording_sec as i64,
                config.language,
                config.trigger_mode,
                config.input_device,
            ],
        )?;
        Ok(())
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_repo() -> (SttConfigRepository, TempDir) {
        let dir = TempDir::new().unwrap();
        let repo = SttConfigRepository::open(&dir.path().join("system.db")).unwrap();
        (repo, dir)
    }

    // GIVEN an empty system.db
    // WHEN get_or_default()
    // THEN default values are returned and inserted into the table
    #[test]
    fn test_empty_db_returns_defaults() {
        let (repo, _dir) = make_repo();
        let config = repo.get_or_default().unwrap();
        assert!(!config.enabled);
        assert_eq!(config.hotkey, "ctrl+shift+space");
        assert_eq!(config.clipboard_mode, "paste");
        assert!(config.clipboard_restore);
        assert!((config.silence_threshold_db - (-40.0_f32)).abs() < f32::EPSILON);
        assert_eq!(config.max_recording_sec, 60);
        assert!(config.language.is_none());
        assert_eq!(config.trigger_mode, "toggle");
    }

    // GIVEN a default row was inserted
    // WHEN get_or_default() is called again
    // THEN the same values are returned without a second insert
    #[test]
    fn test_get_or_default_idempotent() {
        let (repo, _dir) = make_repo();
        let first = repo.get_or_default().unwrap();
        let second = repo.get_or_default().unwrap();
        assert_eq!(first.hotkey, second.hotkey);
        assert_eq!(first.enabled, second.enabled);
    }

    // GIVEN a config with enabled=true and a specific model_path
    // WHEN upsert() then get_or_default()
    // THEN the saved values are read back exactly
    #[test]
    fn test_upsert_and_read_back() {
        let (repo, _dir) = make_repo();
        let config = SttConfigRow {
            enabled: true,
            model_path: "/path/model.gguf".to_string(),
            ..Default::default()
        };
        repo.upsert(&config).unwrap();
        let read = repo.get_or_default().unwrap();
        assert!(read.enabled);
        assert_eq!(read.model_path, "/path/model.gguf");
    }

    // GIVEN an existing row
    // WHEN upsert() with different values
    // THEN the new values replace the previous ones
    #[test]
    fn test_upsert_replaces_existing() {
        let (repo, _dir) = make_repo();
        let mut config = SttConfigRow {
            hotkey: "ctrl+alt+s".to_owned(),
            ..Default::default()
        };
        repo.upsert(&config).unwrap();

        config.hotkey = "ctrl+shift+r".to_owned();
        repo.upsert(&config).unwrap();

        let read = repo.get_or_default().unwrap();
        assert_eq!(read.hotkey, "ctrl+shift+r");
    }

    // GIVEN a config with a non-null language
    // WHEN upsert() then get_or_default()
    // THEN the language value is preserved
    #[test]
    fn test_language_roundtrip() {
        let (repo, _dir) = make_repo();
        let config = SttConfigRow {
            language: Some("fr".to_owned()),
            ..Default::default()
        };
        repo.upsert(&config).unwrap();
        let read = repo.get_or_default().unwrap();
        assert_eq!(read.language, Some("fr".to_owned()));
    }

    // GIVEN a repo opened twice on the same file
    // WHEN the second open runs
    // THEN migration is idempotent (no error)
    #[test]
    fn test_migration_is_idempotent() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("system.db");
        SttConfigRepository::open(&path).unwrap();
        SttConfigRepository::open(&path).unwrap();
    }

    // GIVEN a legacy database created before the input_device column existed,
    //       holding a populated row
    // WHEN the repository opens it with the additive migration
    // THEN the input_device column is added (defaulting to NULL) and every
    //      pre-existing value is preserved
    #[test]
    fn test_additive_input_device_migration_preserves_legacy_rows() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("system.db");

        // GIVEN: build the pre-column schema by hand and insert a row.
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(
                "CREATE TABLE stt_config (
                    id                   INTEGER PRIMARY KEY CHECK (id = 1),
                    enabled              INTEGER NOT NULL DEFAULT 0,
                    model_path           TEXT    NOT NULL DEFAULT '',
                    hotkey               TEXT    NOT NULL DEFAULT 'ctrl+shift+space',
                    clipboard_mode       TEXT    NOT NULL DEFAULT 'paste',
                    clipboard_restore    INTEGER NOT NULL DEFAULT 1,
                    silence_threshold_db REAL    NOT NULL DEFAULT -40.0,
                    max_recording_sec    INTEGER NOT NULL DEFAULT 60,
                    language             TEXT,
                    trigger_mode         TEXT    NOT NULL DEFAULT 'toggle',
                    updated_at           TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                );",
            )
            .unwrap();
            conn.execute(
                "INSERT INTO stt_config (id, enabled, model_path, hotkey) VALUES (1, 1, '/legacy.bin', 'ctrl+alt+d')",
                [],
            )
            .unwrap();
        }

        // WHEN
        let repo = SttConfigRepository::open(&path).unwrap();
        let row = repo.get_or_default().unwrap();

        // THEN legacy values survive and the new column reads as None.
        assert!(row.enabled);
        assert_eq!(row.model_path, "/legacy.bin");
        assert_eq!(row.hotkey, "ctrl+alt+d");
        assert!(row.input_device.is_none());
    }

    // GIVEN a config with an explicit input_device
    // WHEN upsert() then get_or_default()
    // THEN the device name round-trips
    #[test]
    fn test_input_device_roundtrip() {
        let (repo, _dir) = make_repo();
        let config = SttConfigRow {
            input_device: Some("MacBook Pro Microphone".to_owned()),
            ..Default::default()
        };
        repo.upsert(&config).unwrap();
        let read = repo.get_or_default().unwrap();
        assert_eq!(read.input_device.as_deref(), Some("MacBook Pro Microphone"));
    }
}
