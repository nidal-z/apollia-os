//! Uninstalling a package or an agent: what always goes, what the user
//! chooses to keep, what is refused, and the journal.

use std::sync::Mutex;

use apollia_core::model_preset::{AgentModel, ModelSource};
use apollia_core::paths::DataFile;
use apollia_core::LlmBackendRepository;
use apollia_tools::{AgentOrigin, OverrideField, PackageItemKind, StoredAgent, TaskRepository};
use apollia_triggers::{OnBusy, TriggerDefinitionRepository, TriggerDefinitionRow};

use super::tests::{archive, manifest, Env};
use super::tests_install::{install_with, options, FakeMcp};
use super::*;
use crate::governance::GovernanceEvent;

/// The stores of `env` that an uninstallation reaches beyond `agents.db`.
struct Stores {
    triggers: Mutex<TriggerDefinitionRepository>,
    models: Mutex<LlmBackendRepository>,
}

impl Stores {
    fn new(env: &Env) -> Self {
        Self {
            triggers: Mutex::new(
                TriggerDefinitionRepository::open(&DataFile::Triggers.path(env.data_dir()))
                    .unwrap(),
            ),
            models: Mutex::new(
                LlmBackendRepository::open(&DataFile::System.path(env.data_dir())).unwrap(),
            ),
        }
    }

    fn trigger(&self, id: &str, agent: &str) {
        self.triggers
            .lock()
            .unwrap()
            .insert(&TriggerDefinitionRow {
                id: id.into(),
                agent: Some(agent.into()),
                enabled: true,
                on_busy: OnBusy::Queue,
                source_type: "interval".into(),
                source_config: serde_json::json!({ "every": "1h" }),
                input_template: None,
                created_at: String::new(),
                updated_at: String::new(),
            })
            .unwrap();
    }

    fn trigger_ids(&self) -> Vec<String> {
        let triggers = self.triggers.lock().unwrap();
        triggers.list().unwrap().into_iter().map(|t| t.id).collect()
    }

    fn assign_model(&self, agent: &str) {
        self.models
            .lock()
            .unwrap()
            .set_agent_model(&AgentModel {
                agent: agent.into(),
                model_source: ModelSource::Local,
                model_id: "qwen3-8b".into(),
                preset_id: None,
            })
            .unwrap();
    }

    fn has_model(&self, agent: &str) -> bool {
        self.models
            .lock()
            .unwrap()
            .agent_model(agent)
            .unwrap()
            .is_some()
    }
}

fn targets<'a>(
    env: &'a Env,
    stores: &'a Stores,
    mcp: Option<&'a FakeMcp>,
) -> UninstallTargets<'a, FakeMcp> {
    UninstallTargets {
        packages: &env.packages,
        agents: &env.agents,
        governance: &env.governance,
        mcp,
        triggers: Some(&stores.triggers),
        models: Some(&stores.models),
        data_dir: env.data_dir(),
    }
}

/// Installs the `crm` package (agent `crm-agent`, tool `lookup`), bringing
/// the MCP server `ledger` and requiring a `gmail` connection.
async fn install_crm(env: &Env, mcp: &FakeMcp) {
    let apo = archive(
        env.data_dir(),
        "crm.apo",
        &manifest(
            "1.0.0",
            &[],
            "[[requires]]\ntype = \"gmail\"\n\n[[mcp_servers]]\nname = \"ledger\"\ncommand = \"uvx\"\n",
        ),
        None,
    );
    install_with(env, &apo, &options(), Some(mcp))
        .await
        .unwrap();
    env.governance.recorded.lock().unwrap().clear();
}

fn write_memory(env: &Env, agent: &str) -> std::path::PathBuf {
    let dir = env.data_dir().join("memory");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{agent}.db"));
    apollia_memory::store::MemoryStore::open(&path).unwrap();
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute(
            "INSERT INTO semantic_memories (id, namespace, key, value, created_at, updated_at) \
             VALUES ('m-1', ?1, 'drink', 'tea', '2026-10-01', '2026-10-01')",
            [agent],
        )
        .unwrap();
    path
}

fn memory_rows(path: &std::path::Path, agent: &str) -> i64 {
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.query_row(
        "SELECT COUNT(*) FROM semantic_memories WHERE namespace = ?1",
        [agent],
        |row| row.get(0),
    )
    .unwrap()
}

async fn write_history(env: &Env, agent: &str) -> std::path::PathBuf {
    let path = DataFile::Hitl.path(env.data_dir());
    TaskRepository::open(&path).await.unwrap();
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute_batch(&format!(
        "INSERT INTO tasks (task_id, agent_name, status) VALUES ('t-1', '{agent}', 'completed');
         INSERT INTO batches (batch_id, agent_name, source_json, max_failures, state)
             VALUES ('b-1', '{agent}', '{{}}', 5, 'done');
         INSERT INTO workflow_runs (run_id, workflow_id, inputs_json)
             VALUES ('r-1', 'crm/follow-up', '{{}}');"
    ))
    .unwrap();
    path
}

fn count(path: &std::path::Path, table: &str) -> i64 {
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
        row.get(0)
    })
    .unwrap()
}

// ── Criterion 1: what always goes ─────────────────────────────────────────

#[tokio::test]
async fn test_uninstall_removes_definitions_overrides_model_triggers_files_and_brought_servers() {
    // GIVEN the crm package installed with its MCP server, an override and a
    // model on its agent, a trigger on it and one on another agent, and its
    // environments on disk
    let env = Env::new();
    let mcp = FakeMcp::default();
    install_crm(&env, &mcp).await;
    let stores = Stores::new(&env);
    env.agents
        .set_override(
            "crm-agent",
            OverrideField::Mission,
            &serde_json::json!("Mine."),
        )
        .unwrap();
    stores.assign_model("crm-agent");
    stores.trigger("crm-daily", "crm-agent");
    stores.trigger("other-daily", "other-agent");
    let venvs = env.data_dir().join("venvs");
    std::fs::create_dir_all(venvs.join("crm")).unwrap();
    std::fs::create_dir_all(venvs.join("crm-agent")).unwrap();
    let root = env.data_dir().join("agents/packages/crm");
    assert!(root.is_dir());

    // WHEN the package is uninstalled, keeping memory and history
    let report = uninstall_package(
        "crm",
        UninstallOptions::default(),
        &targets(&env, &stores, Some(&mcp)),
        "test",
    )
    .await
    .unwrap();

    // THEN the package, its items, its agent and the agent's overrides are gone
    assert!(env.packages.get("crm").unwrap().is_none());
    assert!(env.packages.list_items("crm").unwrap().is_empty());
    assert!(env.agents.stored("crm-agent").unwrap().is_none());
    assert!(env.agents.overrides("crm-agent").unwrap().is_empty());
    // AND its model assignment and its trigger, not the other agent's
    assert!(!stores.has_model("crm-agent"));
    assert_eq!(stores.trigger_ids(), vec!["other-daily".to_owned()]);
    assert_eq!(report.triggers_removed, vec!["crm-daily".to_owned()]);
    // AND its files and environments
    assert!(!root.exists());
    assert!(!venvs.join("crm").exists());
    assert!(!venvs.join("crm-agent").exists());
    // AND the server it brought, while the connection it required stays
    assert_eq!(*mcp.removed.lock().unwrap(), vec!["ledger".to_owned()]);
    assert_eq!(report.mcp_servers_removed, vec!["ledger".to_owned()]);
    assert_eq!(report.connections_kept, vec!["gmail".to_owned()]);
    assert_eq!(report.agents, vec!["crm-agent".to_owned()]);
    assert_eq!(report.tools, vec!["lookup".to_owned()]);
    assert!(report.leftovers.is_empty(), "{:?}", report.leftovers);
}

#[tokio::test]
async fn test_uninstall_of_an_unknown_package_is_refused() {
    // GIVEN no package installed
    let env = Env::new();
    let stores = Stores::new(&env);

    // WHEN a package is uninstalled
    let result = uninstall_package(
        "ghost",
        UninstallOptions::default(),
        &targets(&env, &stores, None),
        "test",
    )
    .await;

    // THEN it is not found and nothing is recorded
    assert!(matches!(result, Err(UninstallError::NotFound(name)) if name == "ghost"));
    assert!(env.governance.recorded.lock().unwrap().is_empty());
}

// ── Criterion 2: memory and history, at the user's choice ─────────────────

#[tokio::test]
async fn test_uninstall_keeps_memory_and_history_by_default() {
    // GIVEN the crm package installed, with memory and history for its agent
    let env = Env::new();
    let mcp = FakeMcp::default();
    install_crm(&env, &mcp).await;
    let stores = Stores::new(&env);
    let memory = write_memory(&env, "crm-agent");
    let history = write_history(&env, "crm-agent").await;

    // WHEN it is uninstalled without asking to erase them
    let report = uninstall_package(
        "crm",
        UninstallOptions::default(),
        &targets(&env, &stores, Some(&mcp)),
        "test",
    )
    .await
    .unwrap();

    // THEN the memory and the history are still there
    assert!(!report.memory_cleared && !report.history_cleared);
    assert_eq!(memory_rows(&memory, "crm-agent"), 1);
    assert_eq!(count(&history, "tasks"), 1);
    assert_eq!(count(&history, "batches"), 1);
    assert_eq!(count(&history, "workflow_runs"), 1);
}

#[tokio::test]
async fn test_uninstall_erases_memory_and_history_when_asked() {
    // GIVEN the crm package installed with a workflow, memory and history
    let env = Env::new();
    let mcp = FakeMcp::default();
    install_crm(&env, &mcp).await;
    env.packages
        .link_item("crm", PackageItemKind::Workflow, "follow-up")
        .unwrap();
    let stores = Stores::new(&env);
    let memory = write_memory(&env, "crm-agent");
    let history = write_history(&env, "crm-agent").await;

    // WHEN it is uninstalled, erasing both
    let report = uninstall_package(
        "crm",
        UninstallOptions {
            clear_memory: true,
            clear_history: true,
        },
        &targets(&env, &stores, Some(&mcp)),
        "test",
    )
    .await
    .unwrap();

    // THEN the agent's memory, its tasks and batches, and the workflow's runs are gone
    assert!(report.memory_cleared && report.history_cleared);
    assert_eq!(memory_rows(&memory, "crm-agent"), 0);
    assert_eq!(count(&history, "tasks"), 0);
    assert_eq!(count(&history, "batches"), 0);
    assert_eq!(count(&history, "workflow_runs"), 0);
    assert_eq!(
        (
            report.tasks_removed,
            report.batches_removed,
            report.workflow_runs_removed
        ),
        (1, 1, 1)
    );
}

// ── Criterion 4: Apollia stays, and every uninstallation is journaled ─────

#[tokio::test]
async fn test_apollia_cannot_be_uninstalled() {
    // GIVEN the stores of a machine
    let env = Env::new();
    let stores = Stores::new(&env);

    // WHEN Apollia is uninstalled
    let result = uninstall_agent(
        apollia_tools::agent_definition::APOLLIA_AGENT_ID,
        UninstallOptions::default(),
        &targets(&env, &stores, None),
        "test",
    )
    .await;

    // THEN it is refused, and nothing is recorded
    assert!(matches!(result, Err(UninstallError::BuiltIn(_))));
    assert!(env.governance.recorded.lock().unwrap().is_empty());
}

#[tokio::test]
async fn test_uninstall_of_a_package_is_journaled_with_the_choice() {
    // GIVEN the crm package installed
    let env = Env::new();
    let mcp = FakeMcp::default();
    install_crm(&env, &mcp).await;
    let stores = Stores::new(&env);

    // WHEN it is uninstalled, erasing the memory and keeping the history
    uninstall_package(
        "crm",
        UninstallOptions {
            clear_memory: true,
            clear_history: false,
        },
        &targets(&env, &stores, Some(&mcp)),
        "user",
    )
    .await
    .unwrap();

    // THEN the journal holds the package, then its agent, with the choice
    let recorded = env.governance.recorded.lock().unwrap().clone();
    assert_eq!(recorded.len(), 2);
    assert!(matches!(
        &recorded[0],
        GovernanceEvent::PackageUninstalled {
            package_id, version, memory_cleared: true, history_cleared: false, actor
        } if package_id == "crm" && version == "1.0.0" && actor == "user"
    ));
    assert!(matches!(
        &recorded[1],
        GovernanceEvent::Uninstalled { agent_id, memory_cleared: true, history_cleared: false, .. }
            if agent_id == "crm-agent"
    ));
}

#[tokio::test]
async fn test_uninstall_of_an_agent_alone_is_journaled_and_removes_its_trigger() {
    // GIVEN an agent created without a package, with a trigger
    let env = Env::new();
    let stores = Stores::new(&env);
    env.agents
        .save_definition(&StoredAgent {
            name: "notes".into(),
            origin: AgentOrigin::Interface,
            package: None,
            version: "0.1.0".into(),
            definition: serde_json::json!({ "mission": "Take notes." }),
            enabled: true,
            installed_at: "2026-10-01T00:00:00Z".into(),
            updated_at: "2026-10-01T00:00:00Z".into(),
        })
        .unwrap();
    stores.trigger("notes-daily", "notes");

    // WHEN it is uninstalled
    let report = uninstall_agent(
        "notes",
        UninstallOptions::default(),
        &targets(&env, &stores, None),
        "user",
    )
    .await
    .unwrap();

    // THEN it and its trigger are gone, and the journal says so
    assert!(env.agents.stored("notes").unwrap().is_none());
    assert_eq!(report.triggers_removed, vec!["notes-daily".to_owned()]);
    let recorded = env.governance.recorded.lock().unwrap().clone();
    assert!(matches!(
        recorded.as_slice(),
        [GovernanceEvent::Uninstalled { agent_id, version, .. }]
            if agent_id == "notes" && version == "0.1.0"
    ));
}

#[tokio::test]
async fn test_an_agent_of_an_installed_package_goes_with_its_package() {
    // GIVEN the crm package installed
    let env = Env::new();
    let mcp = FakeMcp::default();
    install_crm(&env, &mcp).await;
    let stores = Stores::new(&env);

    // WHEN its agent is uninstalled on its own
    let result = uninstall_agent(
        "crm-agent",
        UninstallOptions::default(),
        &targets(&env, &stores, Some(&mcp)),
        "user",
    )
    .await;

    // THEN it is refused, naming the package, and the agent stays
    assert!(matches!(
        result,
        Err(UninstallError::OwnedByPackage { package, .. }) if package == "crm"
    ));
    assert!(env.agents.stored("crm-agent").unwrap().is_some());
}
