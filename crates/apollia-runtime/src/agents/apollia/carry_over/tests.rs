use super::*;
use crate::audit_journal::{AuditJournalHandle, JournalEntryKind};
use crate::governance::governance_run_id;
use crate::governance::NoPolicyGovernance;
use apollia_core::model_preset::ModelSource;
use apollia_core::LlmBackendConfig;
use rusqlite::Connection;

const KEY: &[u8] = b"apollia-carry-over-journal-key";

/// A data directory whose `governance.db` holds the legacy row `set`, with
/// the access upgrade already run.
fn data_dir_with(set: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let conn = Connection::open(DataFile::Governance.path(dir.path())).expect("open");
    apollia_permissions::open_governance_schema(&conn).expect("schema");
    conn.execute_batch(set).expect("legacy row");
    conn.execute(
        "UPDATE governance_upgrades SET done_at = 1 WHERE name = 'access_upgrade'",
        [],
    )
    .expect("upgrade done");
    dir
}

async fn journal(dir: &Path) -> AuditJournalHandle {
    AuditJournalHandle::open_with_key_bytes(&dir.join("audit_journal.db"), KEY.to_vec())
        .await
        .expect("journal")
}

fn legacy_table_exists(dir: &Path) -> bool {
    Connection::open(DataFile::Governance.path(dir))
        .expect("open")
        .query_row(
            "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE name = 'chat_libre_config')",
            [],
            |row| row.get(0),
        )
        .expect("query")
}

fn apollia(dir: &Path) -> apollia_tools::ResolvedAgent {
    AgentRepository::open(&DataFile::Agents.path(dir))
        .expect("agents.db")
        .resolved(
            APOLLIA_AGENT_ID,
            None,
            crate::agents::global_default_autonomy(),
        )
        .expect("resolve")
        .expect("Apollia is stored")
}

#[tokio::test]
async fn a_filled_row_becomes_the_settings_of_apollia() {
    // GIVEN a legacy row with a prompt, two tools and a local engine backend
    let dir = data_dir_with(
        "UPDATE chat_libre_config SET system_prompt = 'Answer in French.', \
         allowed_tools = '[\"web_search\",\"file_read\"]', llm_backend = 'local' WHERE id = 1;",
    );
    LlmBackendRepository::open(&DataFile::System.path(dir.path()))
        .expect("system.db")
        .save(&LlmBackendConfig {
            name: "local".into(),
            provider: LlmProvider::LlamaCpp,
            model: "/models/qwen.gguf".into(),
            config_json: serde_json::json!({}),
            enabled: true,
            is_default: true,
        })
        .expect("backend");
    let journal = journal(dir.path()).await;
    let governance = NoPolicyGovernance::new(journal.clone());

    // AND web_search and web_read active, the reading family of this start
    let reading = ["web_read".to_string(), "web_search".to_string()];

    // WHEN the settings are carried over
    let outcome = carry_over_chat_config(dir.path(), &governance, &reading).expect("carry over");

    // THEN the prompt, followed by the built-in prompt, is the mission; the
    // access is the reading family then the legacy tools it lacks; the
    // backend is the local model of Apollia
    assert_eq!(
        outcome,
        CarryOver::Done {
            overrides: vec![OverrideField::Mission, OverrideField::Access],
            model_assigned: true,
        }
    );
    let agent = apollia(dir.path());
    let built_in = crate::chat::builtin_agent::built_in_prompt(agent.autonomy);
    assert_eq!(agent.mission, format!("Answer in French.\n\n{built_in}"));
    assert_eq!(agent.access, vec!["web_read", "web_search", "file_read"]);
    // AND the carried-over access is kept: Apollia no longer reads by default
    assert_eq!(
        apollia_tools::AgentAccess::of(&agent),
        apollia_tools::AgentAccess::Listed(agent.access.clone())
    );
    let model = LlmBackendRepository::open(&DataFile::System.path(dir.path()))
        .expect("system.db")
        .agent_model(APOLLIA_AGENT_ID)
        .expect("read")
        .expect("assigned");
    assert_eq!(model.model_id, "local");
    assert_eq!(model.model_source, ModelSource::Local);
    // AND the legacy table is gone
    assert!(!legacy_table_exists(dir.path()));
    // AND each override, and each access granted, is in the journal
    let entries = journal
        .query_run(&governance_run_id(APOLLIA_AGENT_ID))
        .await;
    let overrides = entries
        .iter()
        .filter(|e| e.kind == JournalEntryKind::AgentOverrideChanged)
        .count();
    let granted = entries
        .iter()
        .filter(|e| e.kind == JournalEntryKind::AccessChanged)
        .count();
    assert_eq!((overrides, granted), (2, 3));
    journal.shutdown().await;
}

#[tokio::test]
async fn the_default_row_writes_nothing_and_is_dropped() {
    // GIVEN the row a fresh install writes
    let dir = data_dir_with("");
    let journal = journal(dir.path()).await;
    let governance = NoPolicyGovernance::new(journal.clone());

    // WHEN the settings are carried over, then again at the next start
    let reading = ["web_search".to_string()];
    let first = carry_over_chat_config(dir.path(), &governance, &reading).expect("carry over");
    let second = carry_over_chat_config(dir.path(), &governance, &reading).expect("second");

    // THEN no override and no model are written, the table is dropped, the
    // second start finds nothing to do, and Apollia reads by default
    assert_eq!(
        first,
        CarryOver::Done {
            overrides: vec![],
            model_assigned: false,
        }
    );
    assert_eq!(second, CarryOver::AlreadyDone);
    let agent = apollia(dir.path());
    assert!(agent.overridden.is_empty());
    assert!(apollia_tools::reads_by_default(&agent));
    assert_eq!(agent.name, "Apollia");
    assert!(!legacy_table_exists(dir.path()));
    assert!(journal
        .query_run(&governance_run_id(APOLLIA_AGENT_ID))
        .await
        .is_empty());
    journal.shutdown().await;
}

#[tokio::test]
async fn the_row_waits_for_the_access_upgrade() {
    // GIVEN a legacy row with a tool, and an access upgrade not yet run
    let dir = data_dir_with("UPDATE chat_libre_config SET allowed_tools = '[\"file_write\"]';");
    Connection::open(DataFile::Governance.path(dir.path()))
        .expect("open")
        .execute(
            "UPDATE governance_upgrades SET done_at = NULL WHERE name = 'access_upgrade'",
            [],
        )
        .expect("pending");
    let journal = journal(dir.path()).await;
    let governance = NoPolicyGovernance::new(journal.clone());

    // WHEN the carry-over runs
    let outcome = carry_over_chat_config(dir.path(), &governance, &[]).expect("carry over");

    // THEN it waits, and the row stays for the access upgrade to read
    assert_eq!(outcome, CarryOver::Waiting);
    assert!(legacy_table_exists(dir.path()));
    journal.shutdown().await;
}
