//! The versioned schema of the trigger-definition store.
//!
//! v1 is the table as it stood before versioning, replayed idempotently. v2
//! types the target: a trigger starts an agent, given a message, or a
//! workflow, given its inputs. SQLite cannot relax the `NOT NULL` of the
//! `agent` column in place, so v2 rebuilds the table, and every trigger the
//! file already holds comes out as an agent target.

use rusqlite::Connection;

/// Current schema version of the trigger-definition store.
pub(super) const TRIGGERS_SCHEMA_VERSION: u32 = 2;

/// Numbered migration steps of the trigger-definition store.
///
/// Step `k` migrates the file from version `k` to `k + 1`; the list length
/// always equals [`TRIGGERS_SCHEMA_VERSION`].
pub(super) const TRIGGERS_MIGRATIONS: &[apollia_core::schema::Migration] =
    &[migrate_v1, migrate_v2];

/// v1: the pre-versioning schema of the file, replayed idempotently.
fn migrate_v1(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(MIGRATION_008)
}

/// v2: the typed target, existing triggers carried over as agent targets, in
/// one transaction so that an interrupted rebuild leaves the v1 table whole.
fn migrate_v2(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(&format!("BEGIN;\n{MIGRATION_TYPED_TARGET}\nCOMMIT;"))
}

/// Idempotent creation of the v1 `trigger_definitions` table.
const MIGRATION_008: &str = "\
CREATE TABLE IF NOT EXISTS trigger_definitions (
    id              TEXT PRIMARY KEY,
    agent           TEXT NOT NULL,
    enabled         BOOLEAN NOT NULL DEFAULT 1,
    on_busy         TEXT NOT NULL DEFAULT 'queue',
    source_type     TEXT NOT NULL,
    source_config   TEXT NOT NULL,
    input_template  TEXT,
    created_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    CHECK (on_busy IN ('queue', 'drop')),
    CHECK (source_type IN ('cron', 'interval', 'oneshot', 'file_watch', 'webhook'))
);";

/// The v2 table, filled from the v1 one with every row an agent target.
///
/// `target_kind` says what the trigger starts; `agent` is set for an agent
/// target and `workflow` for a workflow target, never both. `input_template`
/// is the agent's message, `workflow_inputs` the JSON object of the
/// workflow's input templates.
const MIGRATION_TYPED_TARGET: &str = "\
CREATE TABLE IF NOT EXISTS trigger_definitions_typed (
    id              TEXT PRIMARY KEY,
    target_kind     TEXT NOT NULL DEFAULT 'agent',
    agent           TEXT,
    workflow        TEXT,
    workflow_inputs TEXT,
    enabled         BOOLEAN NOT NULL DEFAULT 1,
    on_busy         TEXT NOT NULL DEFAULT 'queue',
    source_type     TEXT NOT NULL,
    source_config   TEXT NOT NULL,
    input_template  TEXT,
    created_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    CHECK (on_busy IN ('queue', 'drop')),
    CHECK (source_type IN ('cron', 'interval', 'oneshot', 'file_watch', 'webhook')),
    CHECK (
        (target_kind = 'agent' AND agent IS NOT NULL AND workflow IS NULL)
        OR (target_kind = 'workflow' AND workflow IS NOT NULL AND agent IS NULL)
    )
);
INSERT INTO trigger_definitions_typed
    (id, target_kind, agent, enabled, on_busy, source_type, source_config,
     input_template, created_at, updated_at)
SELECT id, 'agent', agent, enabled, on_busy, source_type, source_config,
       input_template, created_at, updated_at
FROM trigger_definitions;
DROP TABLE trigger_definitions;
ALTER TABLE trigger_definitions_typed RENAME TO trigger_definitions;";

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;

    #[test]
    fn test_v2_carries_every_v1_trigger_over_as_an_agent_target() {
        // GIVEN a file at v1 holding one trigger
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("triggers_def.db");
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(MIGRATION_008).unwrap();
            conn.execute_batch("PRAGMA user_version = 1;").unwrap();
            conn.execute(
                "INSERT INTO trigger_definitions \
                 (id, agent, enabled, on_busy, source_type, source_config, input_template) \
                 VALUES (?1, 'veille', 1, 'drop', 'interval', '{\"every\":\"1h\"}', 'go')",
                params!["hourly"],
            )
            .unwrap();
        }

        // WHEN the repository opens it
        let repo = super::super::TriggerDefinitionRepository::open(&path).unwrap();

        // THEN the trigger is an agent target with its message, and the file is at v2
        let row = repo.get("hourly").unwrap().unwrap();
        assert_eq!(row.agent.as_deref(), Some("veille"));
        assert_eq!(row.workflow, None);
        assert_eq!(row.input_template.as_deref(), Some("go"));
        let def = crate::types::TriggerDefinition::try_from(row).unwrap();
        assert_eq!(
            def.target,
            crate::types::TriggerTarget::agent("veille", "go")
        );
        let conn = Connection::open(&path).unwrap();
        let version: u32 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, TRIGGERS_SCHEMA_VERSION);
        let kind: String = conn
            .query_row(
                "SELECT target_kind FROM trigger_definitions WHERE id = 'hourly'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(kind, "agent");
    }

    #[test]
    fn test_the_table_refuses_a_row_with_two_targets() {
        // GIVEN a file at v2
        let conn = Connection::open_in_memory().unwrap();
        migrate_v1(&conn).unwrap();
        migrate_v2(&conn).unwrap();

        // WHEN a row names both an agent and a workflow
        let result = conn.execute(
            "INSERT INTO trigger_definitions \
             (id, target_kind, agent, workflow, source_type, source_config) \
             VALUES ('both', 'agent', 'a', 'p/w', 'interval', '{}')",
            [],
        );

        // THEN SQLite refuses it
        assert!(result.is_err());
    }

    #[test]
    fn test_a_workflow_trigger_is_stored_and_read_back_typed() {
        // GIVEN a workflow trigger with one input template
        let dir = tempfile::TempDir::new().unwrap();
        let repo =
            super::super::TriggerDefinitionRepository::open(&dir.path().join("t.db")).unwrap();
        let row = super::super::TriggerDefinitionRow {
            id: "weekly".into(),
            agent: None,
            workflow: Some("reports/weekly".into()),
            workflow_inputs: Some(serde_json::json!({ "when": "{{fired_at}}" })),
            enabled: true,
            on_busy: super::super::OnBusy::Queue,
            source_type: "interval".into(),
            source_config: serde_json::json!({ "every": "1h" }),
            input_template: None,
            created_at: String::new(),
            updated_at: String::new(),
        };

        // WHEN it is inserted and read back
        repo.insert(&row).unwrap();
        let got = repo.get("weekly").unwrap().unwrap();

        // THEN its target is the workflow with its input
        assert_eq!(got.target_kind(), "workflow");
        let def = crate::types::TriggerDefinition::try_from(got).unwrap();
        let mut inputs = std::collections::BTreeMap::new();
        inputs.insert(
            "when".to_string(),
            crate::types::InputTemplate("{{fired_at}}".into()),
        );
        assert_eq!(
            def.target,
            crate::types::TriggerTarget::Workflow {
                workflow: "reports/weekly".into(),
                inputs,
            }
        );
    }
}
