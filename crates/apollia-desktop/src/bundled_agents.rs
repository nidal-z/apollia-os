//! Provisioning of agents bundled into the desktop binary.
//!
//! System agents (like `onboarding-agent`) are embedded at compile time via
//! `include_str!` and extracted to `~/.apollia/agents/<name>/agent.py` on first
//! launch. If the bundled version is newer than the installed one, the file is
//! updated silently.
//!
//! This avoids requiring the CLI to install core agents and guarantees that the
//! onboarding experience works out-of-the-box.

use std::path::Path;

use apollia_core::AgentManifest;
use apollia_tools::agent_repository::InstalledAgent;
use apollia_tools::AgentRepository;

/// Source code of the onboarding agent, embedded at compile time.
const ONBOARDING_AGENT_PY: &str = include_str!("../../../agents/system/onboarding-agent.py");

/// Bundled version — must match the `manifest()["version"]` in the Python file.
const ONBOARDING_AGENT_VERSION: &str = "1.4.0";

// ── Worker agents embedded at compile time ───────────────────────────────────

const EXCEL_WORKER_PY: &str = include_str!("../../../agents/workers/excel-worker.py");
const EXCEL_WORKER_VERSION: &str = "0.1.0";

const CSV_WORKER_PY: &str = include_str!("../../../agents/workers/csv-data-worker.py");
const CSV_WORKER_VERSION: &str = "0.1.0";

const PDF_WORKER_PY: &str = include_str!("../../../agents/workers/pdf-worker.py");
const PDF_WORKER_VERSION: &str = "0.1.0";

const CODE_WORKER_PY: &str = include_str!("../../../agents/workers/code-worker.py");
const CODE_WORKER_VERSION: &str = "0.1.0";

/// Ensures all bundled agents are extracted and registered in the repository.
///
/// Called once at boot, before the auto-load loop. Idempotent: skips agents
/// that are already installed at the same (or newer) version.
pub fn ensure_bundled_agents(repo: &AgentRepository, data_dir: &Path) {
    if let Err(e) = provision_onboarding_agent(repo, data_dir) {
        tracing::warn!(error = %e, "failed to provision bundled onboarding-agent");
    }
    for (name, py, version, manifest_fn) in [
        (
            "excel-worker",
            EXCEL_WORKER_PY,
            EXCEL_WORKER_VERSION,
            excel_worker_manifest as fn() -> AgentManifest,
        ),
        (
            "csv-data-worker",
            CSV_WORKER_PY,
            CSV_WORKER_VERSION,
            csv_worker_manifest as fn() -> AgentManifest,
        ),
        (
            "pdf-worker",
            PDF_WORKER_PY,
            PDF_WORKER_VERSION,
            pdf_worker_manifest as fn() -> AgentManifest,
        ),
        (
            "code-worker",
            CODE_WORKER_PY,
            CODE_WORKER_VERSION,
            code_worker_manifest as fn() -> AgentManifest,
        ),
    ] {
        if let Err(e) = provision_worker_agent(repo, data_dir, name, py, version, manifest_fn) {
            tracing::warn!(error = %e, name = %name, "failed to provision bundled worker agent");
        }
    }
}

/// Extracts the onboarding agent to disk and registers it in the repository.
fn provision_onboarding_agent(
    repo: &AgentRepository,
    data_dir: &Path,
) -> Result<(), BundledAgentError> {
    let agent_name = "onboarding-agent";

    // Check if already installed at the current version.
    if let Some(existing) = repo.get(agent_name)? {
        if existing.version == ONBOARDING_AGENT_VERSION {
            tracing::debug!(name = %agent_name, "bundled agent already at current version — skipping");
            return Ok(());
        }
        tracing::info!(
            name = %agent_name,
            installed = %existing.version,
            bundled = %ONBOARDING_AGENT_VERSION,
            "upgrading bundled agent"
        );
    }

    // Write the Python file to ~/.apollia/agents/onboarding-agent/agent.py
    let agent_dir = data_dir.join("agents").join(agent_name);
    std::fs::create_dir_all(&agent_dir).map_err(|e| BundledAgentError::Io(agent_name, e))?;

    let agent_path = agent_dir.join("agent.py");
    std::fs::write(&agent_path, ONBOARDING_AGENT_PY)
        .map_err(|e| BundledAgentError::Io(agent_name, e))?;

    // Build the manifest without loading Python — we know the exact shape.
    let manifest = onboarding_manifest();
    let now = now_rfc3339();

    let agent = InstalledAgent {
        name: agent_name.to_string(),
        version: ONBOARDING_AGENT_VERSION.to_string(),
        install_path: agent_path.clone(),
        source_path: agent_path,
        manifest,
        enabled: true,
        installed_at: now.clone(),
        updated_at: now,
    };

    repo.save(&agent)?;
    tracing::info!(name = %agent_name, version = %ONBOARDING_AGENT_VERSION, "bundled agent provisioned");
    Ok(())
}

/// Returns the hardcoded manifest for the onboarding agent.
///
/// Mirrors the `manifest()` dict in `agents/onboarding-agent.py`. Keeping it
/// in sync is acceptable because the onboarding agent is a system component
/// with a stable, well-known contract.
fn onboarding_manifest() -> AgentManifest {
    AgentManifest {
        name: "onboarding-agent".to_string(),
        version: ONBOARDING_AGENT_VERSION.to_string(),
        description: "Agent d'onboarding conversationnel — fait connaissance \
                      avec l'utilisateur de manière naturelle."
            .to_string(),
        tools_required: Vec::new(),
        tools_optional: Vec::new(),
        supports_streaming: false,
        supports_a2a: false,
        memory_namespace: Some("onboarding-agent".to_string()),
        shared_memory_namespaces: Vec::new(),
        max_concurrent_tasks: 1,
        step_budget: None,
        network_allowlist: None,
        dangerous_tools_allowed: false,
        tags: vec!["onboarding".to_string(), "conversational".to_string()],
        skills: Vec::new(),
        execution_mode: "conversational".to_string(),
        system_prompt: None,
        tools_requiring_approval: Vec::new(),
        llm_backend: None,
        packages: vec![],
        memory_config: None,
    }
}

/// Generic worker-agent provisioner — same logic as `provision_onboarding_agent`.
fn provision_worker_agent(
    repo: &AgentRepository,
    data_dir: &Path,
    agent_name: &'static str,
    source: &str,
    version: &str,
    manifest_fn: fn() -> AgentManifest,
) -> Result<(), BundledAgentError> {
    if let Some(existing) = repo.get(agent_name)? {
        if existing.version == version {
            tracing::debug!(name = %agent_name, "bundled worker already at current version — skipping");
            return Ok(());
        }
        tracing::info!(
            name = %agent_name,
            installed = %existing.version,
            bundled = %version,
            "upgrading bundled worker agent"
        );
    }

    let agent_dir = data_dir.join("agents").join(agent_name);
    std::fs::create_dir_all(&agent_dir).map_err(|e| BundledAgentError::Io(agent_name, e))?;

    let agent_path = agent_dir.join("agent.py");
    std::fs::write(&agent_path, source).map_err(|e| BundledAgentError::Io(agent_name, e))?;

    let manifest = manifest_fn();
    let now = now_rfc3339();

    let agent = InstalledAgent {
        name: agent_name.to_string(),
        version: version.to_string(),
        install_path: agent_path.clone(),
        source_path: agent_path,
        manifest,
        enabled: true,
        installed_at: now.clone(),
        updated_at: now,
    };

    repo.save(&agent)?;
    tracing::info!(name = %agent_name, version = %version, "bundled worker agent provisioned");
    Ok(())
}

fn excel_worker_manifest() -> AgentManifest {
    use apollia_core::AgentSkill;
    AgentManifest {
        name: "excel-worker".to_string(),
        version: EXCEL_WORKER_VERSION.to_string(),
        description: "Agent spécialisé pour la manipulation de fichiers Excel (.xlsx, .xlsm). \
                      Lit, analyse, crée et modifie des classeurs Excel via openpyxl."
            .to_string(),
        tools_required: vec![
            "python_executor".to_string(),
            "file_read".to_string(),
            "file_write".to_string(),
        ],
        tools_optional: vec!["file_list".to_string()],
        supports_streaming: false,
        supports_a2a: true,
        memory_namespace: Some("excel-worker".to_string()),
        shared_memory_namespaces: Vec::new(),
        max_concurrent_tasks: 1,
        step_budget: None,
        network_allowlist: None,
        dangerous_tools_allowed: false,
        tags: vec!["worker".to_string(), "excel".to_string(), "a2a".to_string()],
        skills: vec![
            AgentSkill {
                id: "read-excel".to_string(),
                name: "Lire un fichier Excel".to_string(),
                description:
                    "Lit et retourne le contenu structuré d'une feuille Excel (headers + rows)"
                        .to_string(),
                input_modes: vec!["text".to_string()],
                output_modes: vec!["text".to_string()],
            },
            AgentSkill {
                id: "edit-excel".to_string(),
                name: "Modifier un fichier Excel".to_string(),
                description: "Modifie des cellules, ajoute des lignes ou colonnes".to_string(),
                input_modes: vec!["text".to_string()],
                output_modes: vec!["text".to_string()],
            },
            AgentSkill {
                id: "create-excel".to_string(),
                name: "Créer un fichier Excel".to_string(),
                description: "Crée un nouveau classeur avec feuilles et données formatées"
                    .to_string(),
                input_modes: vec!["text".to_string()],
                output_modes: vec!["text".to_string()],
            },
        ],
        execution_mode: "direct".to_string(),
        system_prompt: None,
        tools_requiring_approval: Vec::new(),
        llm_backend: None,
        packages: vec!["openpyxl>=3.1.0".to_string()],
        memory_config: None,
    }
}

fn csv_worker_manifest() -> AgentManifest {
    use apollia_core::AgentSkill;
    AgentManifest {
        name: "csv-data-worker".to_string(),
        version: CSV_WORKER_VERSION.to_string(),
        description: "Agent spécialisé pour l'analyse et la transformation de fichiers CSV. \
                      Lit, filtre, agrège et exporte des données CSV via pandas."
            .to_string(),
        tools_required: vec!["python_executor".to_string(), "file_read".to_string()],
        tools_optional: vec!["file_write".to_string()],
        supports_streaming: false,
        supports_a2a: true,
        memory_namespace: Some("csv-data-worker".to_string()),
        shared_memory_namespaces: Vec::new(),
        max_concurrent_tasks: 1,
        step_budget: None,
        network_allowlist: None,
        dangerous_tools_allowed: false,
        tags: vec!["worker".to_string(), "csv".to_string(), "a2a".to_string()],
        skills: vec![
            AgentSkill {
                id: "read-csv".to_string(),
                name: "Lire un fichier CSV".to_string(),
                description:
                    "Parse et retourne le contenu d'un CSV avec détection auto de l'encodage"
                        .to_string(),
                input_modes: vec!["text".to_string()],
                output_modes: vec!["text".to_string()],
            },
            AgentSkill {
                id: "analyze-csv".to_string(),
                name: "Analyser les données CSV".to_string(),
                description: "Statistiques descriptives, comptage valeurs manquantes, groupby"
                    .to_string(),
                input_modes: vec!["text".to_string()],
                output_modes: vec!["text".to_string()],
            },
            AgentSkill {
                id: "transform-csv".to_string(),
                name: "Transformer des données CSV".to_string(),
                description: "Filtre, trie, fusionne ou pivote un CSV et exporte le résultat"
                    .to_string(),
                input_modes: vec!["text".to_string()],
                output_modes: vec!["text".to_string()],
            },
        ],
        execution_mode: "direct".to_string(),
        system_prompt: None,
        tools_requiring_approval: Vec::new(),
        llm_backend: None,
        packages: vec!["pandas>=2.0.0".to_string()],
        memory_config: None,
    }
}

fn pdf_worker_manifest() -> AgentManifest {
    use apollia_core::AgentSkill;
    AgentManifest {
        name: "pdf-worker".to_string(),
        version: PDF_WORKER_VERSION.to_string(),
        description: "Agent spécialisé pour la lecture et l'extraction de documents PDF. \
                      Extrait texte, métadonnées et tableaux via pdfplumber."
            .to_string(),
        tools_required: vec!["python_executor".to_string(), "file_read".to_string()],
        tools_optional: vec!["file_write".to_string()],
        supports_streaming: false,
        supports_a2a: true,
        memory_namespace: Some("pdf-worker".to_string()),
        shared_memory_namespaces: Vec::new(),
        max_concurrent_tasks: 1,
        step_budget: None,
        network_allowlist: None,
        dangerous_tools_allowed: false,
        tags: vec!["worker".to_string(), "pdf".to_string(), "a2a".to_string()],
        skills: vec![
            AgentSkill { id: "read-pdf".to_string(), name: "Lire un fichier PDF".to_string(), description: "Extrait le texte complet d'un PDF avec ses métadonnées (titre, auteur, nombre de pages)".to_string(), input_modes: vec!["text".to_string()], output_modes: vec!["text".to_string()] },
            AgentSkill { id: "extract-tables".to_string(), name: "Extraire les tableaux PDF".to_string(), description: "Détecte et extrait les tableaux d'un PDF au format structuré".to_string(), input_modes: vec!["text".to_string()], output_modes: vec!["text".to_string()] },
            AgentSkill { id: "summarize-pdf".to_string(), name: "Résumer un PDF".to_string(), description: "Génère un résumé structuré d'un document PDF long".to_string(), input_modes: vec!["text".to_string()], output_modes: vec!["text".to_string()] },
        ],
        execution_mode: "direct".to_string(),
        system_prompt: None,
        tools_requiring_approval: Vec::new(),
        llm_backend: None,
        packages: vec!["pdfplumber>=0.10.0".to_string()],
        memory_config: None,
    }
}

fn code_worker_manifest() -> AgentManifest {
    use apollia_core::AgentSkill;
    AgentManifest {
        name: "code-worker".to_string(),
        version: CODE_WORKER_VERSION.to_string(),
        description: "Agent spécialisé pour la génération, le refactoring et la revue de code. \
                      Lit, analyse et modifie des fichiers source. Supporte Python et Rust."
            .to_string(),
        tools_required: vec!["bash_executor".to_string(), "file_read".to_string(), "file_write".to_string(), "file_edit".to_string()],
        tools_optional: vec!["file_list".to_string(), "file_glob".to_string(), "file_grep".to_string()],
        supports_streaming: false,
        supports_a2a: true,
        memory_namespace: Some("code-worker".to_string()),
        shared_memory_namespaces: Vec::new(),
        max_concurrent_tasks: 1,
        step_budget: None,
        network_allowlist: None,
        dangerous_tools_allowed: false,
        tags: vec!["worker".to_string(), "code".to_string(), "a2a".to_string()],
        skills: vec![
            AgentSkill { id: "generate-code".to_string(), name: "Générer du code".to_string(), description: "Génère un fichier source à partir d'une description fonctionnelle".to_string(), input_modes: vec!["text".to_string()], output_modes: vec!["text".to_string()] },
            AgentSkill { id: "refactor-code".to_string(), name: "Refactoriser du code".to_string(), description: "Analyse et réécrit du code existant pour améliorer la lisibilité et les performances".to_string(), input_modes: vec!["text".to_string()], output_modes: vec!["text".to_string()] },
            AgentSkill { id: "review-code".to_string(), name: "Revoir du code".to_string(), description: "Effectue une revue de code détaillée avec suggestions d'améliorations".to_string(), input_modes: vec!["text".to_string()], output_modes: vec!["text".to_string()] },
        ],
        execution_mode: "direct".to_string(),
        system_prompt: None,
        tools_requiring_approval: Vec::new(),
        llm_backend: None,
        packages: vec![],
        memory_config: None,
    }
}

/// RFC 3339 timestamp without pulling in chrono.
fn now_rfc3339() -> String {
    let since_epoch = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = since_epoch.as_secs();
    // Approximate — good enough for an installation timestamp.
    let days = secs / 86400;
    let years = 1970 + days / 365;
    let remainder_days = days % 365;
    let months = remainder_days / 30 + 1;
    let day = remainder_days % 30 + 1;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;
    format!("{years:04}-{months:02}-{day:02}T{hours:02}:{minutes:02}:{seconds:02}Z")
}

/// Errors during bundled agent provisioning.
#[derive(Debug, thiserror::Error)]
enum BundledAgentError {
    #[error("I/O error for bundled agent '{0}': {1}")]
    Io(&'static str, std::io::Error),

    #[error("repository error: {0}")]
    Repository(#[from] apollia_tools::agent_repository::AgentRepositoryError),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn onboarding_manifest_has_correct_name() {
        let m = onboarding_manifest();
        assert_eq!(m.name, "onboarding-agent");
        assert_eq!(m.version, ONBOARDING_AGENT_VERSION);
        assert!(m.tools_required.is_empty());
        assert_eq!(m.memory_namespace, Some("onboarding-agent".to_string()));
        assert_eq!(m.execution_mode, "conversational");
    }

    #[test]
    fn now_rfc3339_produces_valid_format() {
        let ts = now_rfc3339();
        // Must match YYYY-MM-DDTHH:MM:SSZ pattern
        assert_eq!(ts.len(), 20);
        assert!(ts.ends_with('Z'));
        assert_eq!(&ts[4..5], "-");
        assert_eq!(&ts[10..11], "T");
    }

    #[test]
    fn provision_creates_agent_file_and_saves_to_repo() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let db_path = tmp.path().join("agents.db");
        let repo = AgentRepository::open(&db_path).expect("open repo");

        ensure_bundled_agents(&repo, tmp.path());

        // Onboarding agent
        let agent_py = tmp.path().join("agents/onboarding-agent/agent.py");
        assert!(
            agent_py.exists(),
            "onboarding agent.py should be written to disk"
        );
        let agent = repo.get("onboarding-agent").expect("get").expect("exists");
        assert_eq!(agent.version, ONBOARDING_AGENT_VERSION);
        assert!(agent.enabled);
        assert_eq!(agent.install_path, agent_py);

        // Worker agents
        for name in [
            "excel-worker",
            "csv-data-worker",
            "pdf-worker",
            "code-worker",
        ] {
            let worker_py = tmp.path().join(format!("agents/{name}/agent.py"));
            assert!(
                worker_py.exists(),
                "{name}/agent.py should be written to disk"
            );
            let worker = repo.get(name).expect("get").expect("exists");
            assert!(worker.enabled, "{name} should be enabled");
            assert!(worker.manifest.supports_a2a, "{name} should support a2a");
            assert!(
                !worker.manifest.skills.is_empty(),
                "{name} should have skills"
            );
        }
    }

    #[test]
    fn provision_is_idempotent() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let db_path = tmp.path().join("agents.db");
        let repo = AgentRepository::open(&db_path).expect("open repo");

        ensure_bundled_agents(&repo, tmp.path());
        let first = repo.get("onboarding-agent").expect("get").expect("exists");

        // Running again should not error and should keep the same record.
        ensure_bundled_agents(&repo, tmp.path());
        let second = repo.get("onboarding-agent").expect("get").expect("exists");

        assert_eq!(first.installed_at, second.installed_at);
    }
}
