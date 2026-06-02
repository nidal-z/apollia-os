//! Tests d'intégration E2E - distribution des agents bundled.
//!
//! Valide le flux complet de distribution via SupervisorConfig::bundled_agents_path :
//! auto-installation des agents bundled au premier boot,
//! et idempotence (agent déjà présent en DB non ré-installé).
//!
//! Pas de Python requis - utilise StubAgentLoader et InstantBackend.

use std::path::PathBuf;
use std::sync::Arc;

use apollia_core::AgentManifest;
use apollia_runtime::{
    api::{routes_agents::StubAgentLoader, APIServerConfig},
    coordinator::{DynBackend, ExecutionBackend},
    supervisor::{Supervisor, SupervisorConfig},
};
use apollia_tools::{AgentRepository, InstalledAgent};
use tempfile::TempDir;

// ─── Backend de test ──────────────────────────────────────────────────────────

/// Backend qui complète instantanément sans Python.
#[derive(Clone)]
struct InstantBackend;

impl From<DynBackend> for InstantBackend {
    fn from(_: DynBackend) -> Self {
        InstantBackend
    }
}

impl ExecutionBackend for InstantBackend {
    fn execute(
        &self,
        task: apollia_core::AIPTask,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<apollia_core::AIPResult, String>> + Send>,
    > {
        Box::pin(async move {
            Ok(apollia_core::AIPResult {
                task_id: task.task_id,
                status: apollia_core::TaskStatus::Completed,
                output: vec![],
                error: None,
                artifacts: vec![],
                input_required_data: None,
            })
        })
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Écrit `bundled/manifest.json` avec 4 agents dans le répertoire temporaire.
///
/// Retourne le chemin vers le répertoire `bundled/` créé.
fn write_bundled_manifest(tmp: &TempDir) -> PathBuf {
    let bundled_dir = tmp.path().join("bundled");
    std::fs::create_dir_all(&bundled_dir).expect("create bundled dir");

    let manifest = serde_json::json!({
        "version": "1.0.0",
        "bundled_agents": [
            { "name": "excel-worker",    "file": "excel-worker.py",    "auto_install": true, "description": "Excel" },
            { "name": "csv-data-worker", "file": "csv-data-worker.py", "auto_install": true, "description": "CSV"   },
            { "name": "pdf-worker",      "file": "pdf-worker.py",      "auto_install": true, "description": "PDF"   },
            { "name": "code-worker",     "file": "code-worker.py",     "auto_install": true, "description": "Code"  }
        ]
    });
    std::fs::write(
        bundled_dir.join("manifest.json"),
        serde_json::to_string_pretty(&manifest).expect("serialize manifest"),
    )
    .expect("write manifest.json");

    bundled_dir
}

/// Construit un `InstalledAgent` minimal avec l'horodatage fourni.
fn pre_installed_agent(name: &str, installed_at: &str) -> InstalledAgent {
    let manifest = AgentManifest {
        name: name.to_string(),
        version: "1.0.0".to_string(),
        description: format!("Pre-installed {name}"),
        tools_required: vec![],
        tools_optional: vec![],
        supports_streaming: false,
        supports_a2a: false,
        memory_namespace: None,
        shared_memory_namespaces: vec![],
        max_concurrent_tasks: 1,
        step_budget: None,
        network_allowlist: None,
        dangerous_tools_allowed: false,
        tags: vec![],
        skills: vec![],
        execution_mode: "auto".to_string(),
        system_prompt: None,
        tools_requiring_approval: vec![],
        llm_backend: None,
        packages: vec![],
        memory_config: None,
        agent_type: None,
        examples: vec![],
        limitations: vec![],
        setup_notes: None,
        agent_class: None,
        user_memory_write: false,
        datasources: vec![],
        templates: vec![],
        secrets: vec![],
    };
    InstalledAgent {
        name: name.to_string(),
        version: "1.0.0".to_string(),
        install_path: PathBuf::from(format!("/tmp/{name}.py")),
        source_path: PathBuf::from(format!("/tmp/{name}.py")),
        manifest,
        enabled: true,
        installed_at: installed_at.to_string(),
        updated_at: installed_at.to_string(),
    }
}

/// Construit un `SupervisorConfig` pointant vers le répertoire temporaire.
fn supervisor_config(
    tmp: &TempDir,
    tcp_port: u16,
    repo: Option<AgentRepository>,
    bundled_agents_path: Option<PathBuf>,
) -> SupervisorConfig {
    let socket_id = &uuid::Uuid::new_v4().to_string()[..8];
    SupervisorConfig {
        api_config: APIServerConfig {
            socket_path: PathBuf::from(format!("/tmp/ap-dist-{socket_id}.sock")),
            tcp_port,
            bind_addr: "127.0.0.1".to_string(),
            api_token: None,
        },
        startup_timeout_secs: 10,
        llm_config: None,
        config_path: None,
        runtime_config: apollia_core::RuntimeConfig::default(),
        hitl_config: apollia_core::HitlConfig::default(),
        data_dir: tmp.path().to_path_buf(),
        obs_config: apollia_core::ObservabilityConfig::default(),
        agent_repository: repo,
        bundled_agents_path,
        package_repository: None,
        tools_config: apollia_core::ToolsConfig::default(),
    }
}

/// Retourne un port TCP libre alloué par le système.
async fn free_port() -> u16 {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral port");
    listener.local_addr().expect("local addr").port()
}

// ─── Tests ───────────────────────────────────────────────────────────────────

/// Au premier boot, les 4 agents bundled sont auto-installés puis enregistrés.
#[tokio::test]
async fn test_bundled_agents_auto_installed() {
    // GIVEN manifest.json avec 4 agents bundled et DB vide
    let tmp = TempDir::new().expect("create temp dir");
    let db_path = tmp.path().join("agents.db");
    let bundled_dir = write_bundled_manifest(&tmp);

    let repo = AgentRepository::open(&db_path).expect("open repo");
    let port = free_port().await;
    let config = supervisor_config(&tmp, port, Some(repo), Some(bundled_dir));
    let supervisor = Supervisor::new(config);

    // WHEN le Supervisor démarre avec bundled_agents_path configuré
    let handles = supervisor
        .start::<InstantBackend>(InstantBackend, Arc::new(StubAgentLoader), None, None)
        .await
        .expect("supervisor start");

    // THEN 4 agents sont enregistrés dans l'AgentRegistry avec l'état Active
    let agents = handles
        .registry_handle
        .list_agents()
        .await
        .expect("list agents");

    assert_eq!(
        agents.len(),
        4,
        "les 4 agents bundled doivent être installés"
    );

    let names: Vec<String> = agents.iter().map(|a| a.manifest.name.clone()).collect();
    for expected in &[
        "excel-worker",
        "csv-data-worker",
        "pdf-worker",
        "code-worker",
    ] {
        assert!(
            names.contains(&expected.to_string()),
            "l'agent '{}' doit être dans le registry, trouvé : {:?}",
            expected,
            names
        );
    }

    handles.api_handle.shutdown();
}

/// Un agent déjà installé en DB n'est pas ré-installé lors d'un second boot.
#[tokio::test]
async fn test_bundled_agents_skip_existing() {
    // GIVEN excel-worker déjà présent en DB avec un horodatage sentinelle
    let tmp = TempDir::new().expect("create temp dir");
    let db_path = tmp.path().join("agents.db");
    let bundled_dir = write_bundled_manifest(&tmp);

    let sentinel = "2026-01-01T00:00:00Z";
    {
        let repo = AgentRepository::open(&db_path).expect("open repo for pre-install");
        repo.save(&pre_installed_agent("excel-worker", sentinel))
            .expect("pre-install excel-worker");
    }

    let repo2 = AgentRepository::open(&db_path).expect("reopen repo");
    let port = free_port().await;
    let config = supervisor_config(&tmp, port, Some(repo2), Some(bundled_dir));
    let supervisor = Supervisor::new(config);

    // WHEN le Supervisor démarre (excel-worker déjà en DB, 3 autres absents)
    let handles = supervisor
        .start::<InstantBackend>(InstantBackend, Arc::new(StubAgentLoader), None, None)
        .await
        .expect("supervisor start");

    // THEN le registry contient exactement 4 agents sans doublon
    let agents = handles
        .registry_handle
        .list_agents()
        .await
        .expect("list agents");

    assert_eq!(
        agents.len(),
        4,
        "4 agents au total - excel-worker non dupliqué, trouvé : {:?}",
        agents.iter().map(|a| &a.manifest.name).collect::<Vec<_>>()
    );

    let excel_count = agents
        .iter()
        .filter(|a| a.manifest.name == "excel-worker")
        .count();
    assert_eq!(
        excel_count, 1,
        "excel-worker doit apparaître exactement une fois"
    );

    // ET l'horodatage d'installation d'excel-worker est inchangé
    let repo3 = AgentRepository::open(&db_path).expect("recheck repo");
    let excel = repo3
        .get("excel-worker")
        .expect("get excel-worker")
        .expect("excel-worker must exist in DB");
    assert_eq!(
        excel.installed_at, sentinel,
        "installed_at ne doit pas changer si l'agent était déjà présent en DB"
    );

    handles.api_handle.shutdown();
}
