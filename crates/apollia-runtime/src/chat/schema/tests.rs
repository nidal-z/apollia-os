use rusqlite::{params, Connection};

use super::*;
use crate::chat::session_settings::SessionSettings;

/// The version the chat database carried before the settings of a
/// conversation.
const BEFORE_SETTINGS: u32 = 20;

/// A chat database as the previous binary left it, holding three sessions:
/// one with a prompt of its own, one without, one with a prompt longer than
/// the instructions of a conversation may be.
fn database_before_settings() -> Connection {
    let conn = Connection::open_in_memory().expect("open");
    open_versioned(
        &conn,
        "chat.db",
        BEFORE_SETTINGS,
        &MIGRATIONS[..BEFORE_SETTINGS as usize],
    )
    .expect("the schema before the settings");
    for (id, prompt) in [
        ("with-prompt", "  Answer in French.  ".to_string()),
        ("without-prompt", "   ".to_string()),
        ("long-prompt", "€".repeat(5000)),
    ] {
        conn.execute(
            "INSERT INTO chat_sessions (id, mode, system_prompt, created_at)
             VALUES (?1, 'libre', ?2, '2026-10-01T00:00:00Z')",
            params![id, prompt],
        )
        .expect("insert session");
    }
    conn
}

fn stored(conn: &Connection, id: &str) -> (String, SessionSettings, String) {
    conn.query_row(
        "SELECT system_prompt, settings, notes FROM chat_sessions WHERE id = ?1",
        params![id],
        |row| {
            let settings: String = row.get(1)?;
            Ok((
                row.get(0)?,
                SessionSettings::from_column(id, &settings),
                row.get(2)?,
            ))
        },
    )
    .expect("session row")
}

fn version(conn: &Connection) -> u32 {
    conn.query_row("PRAGMA user_version", [], |row| row.get(0))
        .expect("user_version")
}

#[test]
fn test_the_migration_adds_settings_and_notes_and_turns_a_session_prompt_into_instructions() {
    // GIVEN a chat database written before the settings of a conversation
    let conn = database_before_settings();
    assert_eq!(version(&conn), BEFORE_SETTINGS);
    assert!(!column_exists(&conn, "chat_sessions", "settings").unwrap());
    assert!(!column_exists(&conn, "chat_sessions", "notes").unwrap());

    // WHEN this binary opens it
    migrate(&conn).expect("migrate");

    // THEN the file is at the current version, each session has settings and
    // empty notes, a prompt became the instructions of its conversation and
    // left the column it replaced the mission from, and a prompt too long
    // for instructions was cut at their cap
    assert_eq!(version(&conn), SCHEMA_VERSION);
    let (prompt, settings, notes) = stored(&conn, "with-prompt");
    assert_eq!(prompt, "");
    assert_eq!(settings.instructions.as_deref(), Some("Answer in French."));
    assert_eq!(notes, "");
    let (prompt, settings, _) = stored(&conn, "without-prompt");
    assert_eq!(prompt.trim(), "");
    assert_eq!(settings, SessionSettings::default());
    let (prompt, settings, _) = stored(&conn, "long-prompt");
    assert_eq!(prompt, "");
    assert_eq!(
        settings.instructions.map(|text| text.chars().count()),
        Some(apollia_prompts::SESSION_INSTRUCTIONS_MAX_CHARS)
    );
}

#[test]
fn test_replaying_the_migration_keeps_the_settings_written_since() {
    // GIVEN a migrated database whose user then wrote settings and notes,
    // and whose version stamp is lost
    let conn = database_before_settings();
    migrate(&conn).expect("migrate");
    let written = SessionSettings {
        instructions: Some("Answer in Spanish.".into()),
        max_tokens: Some(512),
        ..SessionSettings::default()
    };
    conn.execute(
        "UPDATE chat_sessions SET settings = ?1, notes = 'kept' WHERE id = 'with-prompt'",
        params![written.to_column()],
    )
    .expect("write settings");
    conn.pragma_update(None, "user_version", 0)
        .expect("unstamp");

    // WHEN every step is replayed
    migrate(&conn).expect("replay");

    // THEN what the user wrote is still there
    let (_, settings, notes) = stored(&conn, "with-prompt");
    assert_eq!(settings, written);
    assert_eq!(notes, "kept");
    assert_eq!(version(&conn), SCHEMA_VERSION);
}

#[test]
fn test_a_fresh_database_is_born_with_empty_settings_and_notes() {
    // GIVEN no database
    let conn = Connection::open_in_memory().expect("open");

    // WHEN this binary creates one and a session is written into it
    migrate(&conn).expect("migrate");
    conn.execute(
        "INSERT INTO chat_sessions (id, mode, created_at)
         VALUES ('new', 'libre', '2026-10-06T00:00:00Z')",
        [],
    )
    .expect("insert session");

    // THEN the session inherits everything and holds no note
    let (_, settings, notes) = stored(&conn, "new");
    assert_eq!(settings, SessionSettings::default());
    assert_eq!(notes, "");
}
