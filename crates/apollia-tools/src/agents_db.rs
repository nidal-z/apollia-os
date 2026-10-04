//! Versioned schema of `agents.db`, shared by every store in the file.
//!
//! `agents.db` holds the agents (`agents` and `agent_overrides`, owned by
//! [`crate::agent_repository`]) and the package tables
//! (`installed_packages`, `package_items`, owned by
//! [`crate::package_repository`]), and the keys of the publishers the user
//! trusts (`trusted_keys`, owned by [`crate::trusted_keys`]), and the items
//! the user exposes to an MCP client (`exposed_items`, owned by
//! [`crate::exposed_items`]). `PRAGMA user_version` belongs to the
//! database file, not to a table, so the two repositories share one
//! migration list: whichever opens first brings the whole file to the
//! current version, and both refuse a file written by a newer binary.
//!
//! The file always holds one agent nobody installed: Apollia, the built-in
//! chat, written by the v3 step with origin `interface`. It cannot be
//! uninstalled ([`crate::AgentRepository::delete`] refuses it).

use rusqlite::Connection;

use apollia_core::schema::Migration;

/// Current schema version of `agents.db`.
pub(crate) const AGENTS_DB_SCHEMA_VERSION: u32 = 5;

/// Numbered migration steps of `agents.db`.
///
/// Step `k` migrates the file from version `k` to `k + 1`; the list length
/// always equals [`AGENTS_DB_SCHEMA_VERSION`].
pub(crate) const AGENTS_DB_MIGRATIONS: &[Migration] =
    &[migrate_v1, migrate_v2, migrate_v3, migrate_v4, migrate_v5];

/// v1: the pre-versioning lineage of the file, replayed idempotently.
///
/// Every `agents.db` written before the versioned layer is at
/// `user_version = 0`; this step brings any of them (and a fresh file) to
/// the full current schema, `007_agent_tables` then `008_package_tables`.
fn migrate_v1(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(include_str!("../migrations/007_agent_tables.sql"))?;
    conn.execute_batch(include_str!("../migrations/008_package_tables.sql"))
}

/// v2: `installed_agents` becomes `agents`, `package_agents` becomes
/// `package_items`, and `agent_overrides` is created.
///
/// Every agent row is copied with its code columns, the package it was
/// linked to, and a definition read from its manifest; every package link
/// becomes an item of kind `agent`. The two old tables are then dropped. The
/// whole step runs in one transaction, so a failure leaves the file at v1.
fn migrate_v2(conn: &Connection) -> Result<(), rusqlite::Error> {
    let tx = conn.unchecked_transaction()?;
    tx.execute_batch(include_str!("../migrations/011_agents_and_overrides.sql"))?;
    if table_exists(&tx, "installed_agents")? {
        copy_installed_agents(&tx)?;
    }
    if table_exists(&tx, "package_agents")? {
        tx.execute_batch(
            "INSERT OR IGNORE INTO package_items (package_name, kind, item_id)
                 SELECT package_name, 'agent', agent_name FROM package_agents;",
        )?;
    }
    tx.execute_batch(
        "DROP TABLE IF EXISTS installed_agents;
         DROP TABLE IF EXISTS package_agents;",
    )?;
    tx.commit()
}

/// v3: the Apollia agent, the built-in chat, becomes a row of `agents`.
///
/// Its definition names it and leaves everything else empty: no mission (the
/// built-in prompt of its tier applies), no access, no source. What the user
/// had set for the chat is carried over as overrides by the runtime, which
/// also holds the files the old settings lived in.
fn migrate_v3(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute(
        "INSERT OR IGNORE INTO agents (name, origin, version, definition_json, enabled) \
         VALUES (?1, 'interface', ?2, ?3, 1)",
        rusqlite::params![
            crate::agent_definition::APOLLIA_AGENT_ID,
            crate::agent_definition::APOLLIA_DEFINITION_VERSION,
            crate::agent_definition::apollia_definition().to_string(),
        ],
    )?;
    Ok(())
}

/// v4: `installed_packages` records who signed a package, and
/// `trusted_keys` holds the publisher keys the user trusts.
///
/// Each column is added only when the probe does not find it, so the step
/// can run over a file a previous attempt left half-way.
fn migrate_v4(conn: &Connection) -> Result<(), rusqlite::Error> {
    let tx = conn.unchecked_transaction()?;
    tx.execute_batch(include_str!("../migrations/012_package_trust.sql"))?;
    for (column, ddl) in [
        (
            "publisher",
            "ALTER TABLE installed_packages ADD COLUMN publisher TEXT NOT NULL DEFAULT ''",
        ),
        (
            "key_fingerprint",
            "ALTER TABLE installed_packages ADD COLUMN key_fingerprint TEXT",
        ),
        (
            "signed",
            "ALTER TABLE installed_packages ADD COLUMN signed BOOLEAN NOT NULL DEFAULT 0",
        ),
    ] {
        if !has_column(&tx, "installed_packages", column)? {
            tx.execute_batch(ddl)?;
        }
    }
    tx.commit()
}

/// v5: `exposed_items` records the agents and workflows the user exposes to
/// an MCP client. The table starts empty: nothing is exposed until the user
/// says so.
fn migrate_v5(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(include_str!("../migrations/013_exposed_items.sql"))
}

/// `true` when `table` has a column named `column`.
fn has_column(conn: &Connection, table: &str, column: &str) -> Result<bool, rusqlite::Error> {
    conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM pragma_table_info(?1) WHERE name = ?2)",
        [table, column],
        |row| row.get(0),
    )
}

fn table_exists(conn: &Connection, name: &str) -> Result<bool, rusqlite::Error> {
    conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
        [name],
        |row| row.get(0),
    )
}

/// One row of `installed_agents`, with the package it was linked to.
type LegacyAgentRow = (
    String,
    String,
    String,
    String,
    String,
    bool,
    String,
    String,
    Option<String>,
);

fn copy_installed_agents(conn: &Connection) -> Result<(), rusqlite::Error> {
    let package_link = if table_exists(conn, "package_agents")? {
        "(SELECT package_name FROM package_agents p WHERE p.agent_name = a.name \
          ORDER BY package_name LIMIT 1)"
    } else {
        "NULL"
    };
    let rows: Vec<LegacyAgentRow> = {
        let mut stmt = conn.prepare(&format!(
            "SELECT name, version, install_path, source_path, manifest_json, enabled, \
                    installed_at, updated_at, {package_link} \
             FROM installed_agents a"
        ))?;
        let mapped = stmt.query_map([], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
                row.get(7)?,
                row.get(8)?,
            ))
        })?;
        mapped.collect::<Result<_, _>>()?
    };
    for (
        name,
        version,
        install_path,
        source_path,
        manifest_json,
        enabled,
        installed,
        updated,
        package,
    ) in rows
    {
        let definition =
            crate::agent_definition::definition_from_manifest_json(&name, &manifest_json);
        conn.execute(
            "INSERT OR IGNORE INTO agents \
                 (name, origin, package, version, definition_json, install_path, source_path, \
                  manifest_json, enabled, installed_at, updated_at) \
             VALUES (?1, 'package', ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            rusqlite::params![
                name,
                package,
                version,
                definition.to_string(),
                install_path,
                source_path,
                manifest_json,
                enabled,
                installed,
                updated
            ],
        )?;
    }
    Ok(())
}

/// Opens the file's schema at the current version, or refuses it.
///
/// Shared entry point for the two `agents.db` repositories.
///
/// # Errors
///
/// Returns [`apollia_core::schema::SchemaError`] when the migration fails or
/// the database was written by a newer binary.
pub(crate) fn open_agents_schema(
    conn: &Connection,
) -> Result<(), apollia_core::schema::SchemaError> {
    apollia_core::schema::open_versioned(
        conn,
        apollia_core::paths::DataFile::Agents.file_name(),
        AGENTS_DB_SCHEMA_VERSION,
        AGENTS_DB_MIGRATIONS,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_core::schema::SchemaError;

    /// Pre-versioning `agents.db` schema (`user_version = 0`): the same
    /// tables, written by a binary that did not stamp a version.
    const AGENTS_V0_SQL: &str = include_str!("../tests/fixtures/schemas/agents_v0.sql");

    // GIVEN a database written by a pre-versioning binary, with rows
    // WHEN opening it through the versioned layer
    // THEN the rows survive in the current tables and the version is stamped
    #[test]
    fn test_agents_db_old_format_migrates_and_keeps_rows() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(AGENTS_V0_SQL).unwrap();
        conn.execute(
            "INSERT INTO installed_agents (name, version, install_path, source_path, manifest_json)
             VALUES ('mailer', '1.0.0', '/a/agent.py', '/src/agent.py', '{}')",
            [],
        )
        .unwrap();

        open_agents_schema(&conn).unwrap();

        let name: String = conn
            .query_row(
                "SELECT name FROM agents WHERE origin = 'package'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(name, "mailer");
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, i64::from(AGENTS_DB_SCHEMA_VERSION));
    }

    // GIVEN a v0 file with an agent linked to a package, a second agent and a
    //   damaged manifest
    // WHEN it is migrated to the current version
    // THEN `agents` holds every agent with its code columns, its package and
    //   a definition read from its manifest; `package_items` holds the link;
    //   `agent_overrides` exists and is empty; the old tables are gone
    #[test]
    fn test_agents_db_v0_rows_become_agents_and_package_items() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(AGENTS_V0_SQL).unwrap();
        conn.execute_batch(
            r#"INSERT INTO installed_agents
                   (name, version, install_path, source_path, manifest_json, enabled,
                    installed_at, updated_at)
               VALUES ('triage', '2.1.0', '/a/triage/agent.py', '/src/triage.py',
                       '{"name":"triage","version":"2.1.0","description":"Sorts the inbox.",
                         "tools_required":["file_read"],"tools_optional":["web_fetch"],
                         "examples":["Sort today"],"datasources":["mail"]}',
                       0, '2026-07-01T00:00:00Z', '2026-07-02T00:00:00Z');
               INSERT INTO installed_agents (name, version, install_path, source_path, manifest_json)
               VALUES ('broken', '0.1.0', '/a/broken/agent.py', '/src/broken.py', 'not json');
               INSERT INTO installed_packages (name, version, root_path, manifest_json)
               VALUES ('office', '1.0.0', '/p/office', '{}');
               INSERT INTO package_agents (package_name, agent_name) VALUES ('office', 'triage');"#,
        )
        .unwrap();

        open_agents_schema(&conn).unwrap();

        let (origin, package, enabled, install_path, definition): (
            String,
            Option<String>,
            bool,
            String,
            String,
        ) = conn
            .query_row(
                "SELECT origin, package, enabled, install_path, definition_json
                 FROM agents WHERE name = 'triage'",
                [],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(origin, "package");
        assert_eq!(package.as_deref(), Some("office"));
        assert!(!enabled);
        assert_eq!(install_path, "/a/triage/agent.py");
        let definition: serde_json::Value = serde_json::from_str(&definition).unwrap();
        assert_eq!(
            definition,
            serde_json::json!({
                "id": "triage",
                "name": "triage",
                "mission": "Sorts the inbox.",
                "examples": ["Sort today"],
                "defaults": {"access": ["file_read", "web_fetch"]}
            })
        );
        let broken: String = conn
            .query_row(
                "SELECT definition_json FROM agents WHERE name = 'broken'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(broken.contains(r#""id":"broken""#));
        let items: Vec<(String, String, String)> = conn
            .prepare("SELECT package_name, kind, item_id FROM package_items")
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(
            items,
            vec![("office".into(), "agent".into(), "triage".into())]
        );
        let overrides: i64 = conn
            .query_row("SELECT COUNT(*) FROM agent_overrides", [], |row| row.get(0))
            .unwrap();
        assert_eq!(overrides, 0);
        let old_tables: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master
                 WHERE name IN ('installed_agents', 'package_agents')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(old_tables, 0);
    }

    // GIVEN a file already at the current version
    // WHEN it is opened again
    // THEN nothing is replayed and the tables are intact
    #[test]
    fn test_agents_db_second_open_is_a_no_op() {
        let conn = Connection::open_in_memory().unwrap();
        open_agents_schema(&conn).unwrap();
        conn.execute(
            "INSERT INTO agents (name, version, definition_json) VALUES ('a', '1', '{}')",
            [],
        )
        .unwrap();

        open_agents_schema(&conn).unwrap();

        let names: Vec<String> = conn
            .prepare("SELECT name FROM agents ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(names, vec!["a".to_string(), "apollia:chat".to_string()]);
    }

    // GIVEN a fresh file, and a file at v2 holding a package agent
    // WHEN each is opened through the versioned layer
    // THEN both hold the Apollia agent, of origin interface, named Apollia,
    //   with an empty mission, and the package agent is kept
    #[test]
    fn test_agents_db_v3_writes_the_apollia_agent() {
        let fresh = Connection::open_in_memory().unwrap();
        let at_v2 = Connection::open_in_memory().unwrap();
        for step in &AGENTS_DB_MIGRATIONS[..2] {
            step(&at_v2).unwrap();
        }
        at_v2.pragma_update(None, "user_version", 2).unwrap();
        at_v2
            .execute(
                "INSERT INTO agents (name, version, definition_json) VALUES ('mailer', '1', '{}')",
                [],
            )
            .unwrap();

        open_agents_schema(&fresh).unwrap();
        open_agents_schema(&at_v2).unwrap();

        for conn in [&fresh, &at_v2] {
            let (origin, definition): (String, String) = conn
                .query_row(
                    "SELECT origin, definition_json FROM agents WHERE name = 'apollia:chat'",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .unwrap();
            assert_eq!(origin, "interface");
            let definition: serde_json::Value = serde_json::from_str(&definition).unwrap();
            assert_eq!(definition["name"], "Apollia");
            assert_eq!(definition["mission"], "");
        }
        let kept: i64 = at_v2
            .query_row(
                "SELECT COUNT(*) FROM agents WHERE name = 'mailer'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(kept, 1);
    }

    // GIVEN a file at v3 holding an installed package
    // WHEN it is opened through the versioned layer
    // THEN the package is kept, unsigned and without publisher, and the
    //   trusted keys table exists, empty
    #[test]
    fn test_agents_db_v4_adds_the_signature_columns_and_the_trusted_keys() {
        let at_v3 = Connection::open_in_memory().unwrap();
        for step in &AGENTS_DB_MIGRATIONS[..3] {
            step(&at_v3).unwrap();
        }
        at_v3.pragma_update(None, "user_version", 3).unwrap();
        at_v3
            .execute(
                "INSERT INTO installed_packages (name, version, root_path, manifest_json) \
                 VALUES ('crm', '1.0.0', '/p', '{}')",
                [],
            )
            .unwrap();

        open_agents_schema(&at_v3).unwrap();

        let (publisher, fingerprint, signed): (String, Option<String>, bool) = at_v3
            .query_row(
                "SELECT publisher, key_fingerprint, signed FROM installed_packages \
                 WHERE name = 'crm'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(publisher, "");
        assert_eq!(fingerprint, None);
        assert!(!signed);
        let keys: i64 = at_v3
            .query_row("SELECT COUNT(*) FROM trusted_keys", [], |row| row.get(0))
            .unwrap();
        assert_eq!(keys, 0);
    }

    // GIVEN a file whose v4 step already added its columns
    // WHEN the step runs again
    // THEN it succeeds and adds nothing twice
    #[test]
    fn test_agents_db_v4_step_is_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        for step in AGENTS_DB_MIGRATIONS {
            step(&conn).unwrap();
        }

        migrate_v4(&conn).unwrap();

        let columns: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('installed_packages') \
                 WHERE name IN ('publisher', 'key_fingerprint', 'signed')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(columns, 3);
    }

    // GIVEN a database stamped by a newer binary
    // WHEN opening it through the versioned layer
    // THEN the open is refused
    #[test]
    fn test_agents_db_newer_version_is_refused() {
        let conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "user_version", AGENTS_DB_SCHEMA_VERSION + 1)
            .unwrap();

        let err = open_agents_schema(&conn).unwrap_err();

        assert!(matches!(err, SchemaError::NewerThanBinary { .. }));
    }
}
