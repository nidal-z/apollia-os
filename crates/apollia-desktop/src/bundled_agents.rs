//! Provisioning of the built-in system agent (onboarding-agent).
//!
//! Only **system-level** agents are shipped inside the desktop binary.
//! Assistants and workers are distributed separately via
//! `agents-distributable/` and installed by the user through the UI or the
//! CLI (`apollia-os agent install <bundle>`).
//!
//! The onboarding agent is embedded at compile time via `include_str!` and
//! extracted to `~/.apollia/agents/onboarding-agent/` on first launch so the
//! first-run experience works out-of-the-box, with no network round-trip.

use std::path::Path;

use apollia_core::AgentManifest;
use apollia_tools::agent_repository::InstalledAgent;
use apollia_tools::AgentRepository;

/// Source code of the onboarding agent, embedded at compile time.
const ONBOARDING_AGENT_PY: &str = include_str!("../../../agents/system/onboarding-agent/agent.py");

/// Static metadata (`manifest.toml`), embedded at compile time.
const ONBOARDING_AGENT_TOML: &str =
    include_str!("../../../agents/system/onboarding-agent/manifest.toml");

/// Bundled version, must match the `manifest()["version"]` in the Python file
/// and the `[agent].version` in `manifest.toml`.
///
/// The built-in agents ship inside the binary and carry the product version
/// rather than a lifecycle of their own, so this tracks `CARGO_PKG_VERSION`;
/// `bundled_versions_track_the_product_version` fails the build when it drifts.
///
/// `provision_onboarding_agent` skips re-extraction when the installed version
/// equals this constant. Between two releases the version does not move, so a
/// local edit to `agent.py` will not be picked up until the extracted copy
/// under `~/.apollia/agents/…/` is removed.
const ONBOARDING_AGENT_VERSION: &str = "0.1.0-preview";

/// Source code of the Apollia Guide agent.
const APOLLIA_GUIDE_PY: &str = include_str!("../../../agents/system/apollia-guide/agent.py");

/// Manifest metadata for the Apollia Guide agent.
const APOLLIA_GUIDE_TOML: &str = include_str!("../../../agents/system/apollia-guide/manifest.toml");

/// Knowledge base: capabilities sheet. Bundled so the agent works offline
/// on the very first launch without any post-install download step.
const APOLLIA_GUIDE_CAPABILITIES_MD: &str =
    include_str!("../../../agents/system/apollia-guide/knowledge/capabilities.md");

/// Knowledge base: tutorials with suggested action buttons per intent.
const APOLLIA_GUIDE_TUTORIALS_MD: &str =
    include_str!("../../../agents/system/apollia-guide/knowledge/tutorials.md");

/// Bundled version, must match `manifest()["version"]` in `agent.py` and
/// `[agent].version` in `manifest.toml`. Tracks the product version, same
/// contract and same caveat as [`ONBOARDING_AGENT_VERSION`].
const APOLLIA_GUIDE_VERSION: &str = "0.1.0-preview";

/// Ensures the built-in agents are extracted and registered in the repository.
///
/// Called once at boot, before the auto-load loop. Idempotent: skips agents
/// that are already installed at the same (or newer) version.
pub fn ensure_bundled_agents(repo: &AgentRepository, data_dir: &Path) {
    if let Err(e) = provision_onboarding_agent(repo, data_dir) {
        tracing::warn!(error = %e, "bundled.onboarding.provision.failed");
    }
    if let Err(e) = provision_apollia_guide_agent(repo, data_dir) {
        tracing::warn!(error = %e, "bundled.guide.provision.failed");
    }
}

/// Extracts the onboarding agent bundle to disk and registers it in the repository.
///
/// Layout produced:
/// ```text
/// <data_dir>/agents/onboarding-agent/
///   ├── agent.py
///   └── manifest.toml
/// ```
fn provision_onboarding_agent(
    repo: &AgentRepository,
    data_dir: &Path,
) -> Result<(), BundledAgentError> {
    let agent_name = "onboarding-agent";

    // Check if already installed at the current version.
    if let Some(existing) = repo.get(agent_name)? {
        if existing.version == ONBOARDING_AGENT_VERSION {
            tracing::debug!(
                name = %agent_name,
                detail = "the upgrade is skipped",
                "bundled.onboarding.current"
            );
            return Ok(());
        }
        tracing::info!(
            name = %agent_name,
            installed = %existing.version,
            bundled = %ONBOARDING_AGENT_VERSION,
            "bundled.onboarding.upgrading"
        );
    }

    let agent_dir = data_dir.join("agents").join(agent_name);
    std::fs::create_dir_all(&agent_dir).map_err(|e| BundledAgentError::Io(agent_name, e))?;

    let agent_path = agent_dir.join("agent.py");
    std::fs::write(&agent_path, ONBOARDING_AGENT_PY)
        .map_err(|e| BundledAgentError::Io(agent_name, e))?;

    let manifest_path = agent_dir.join("manifest.toml");
    std::fs::write(&manifest_path, ONBOARDING_AGENT_TOML)
        .map_err(|e| BundledAgentError::Io(agent_name, e))?;

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
    tracing::info!(
        name = %agent_name,
        version = %ONBOARDING_AGENT_VERSION,
        "bundled.onboarding.provisioned"
    );
    Ok(())
}

/// Returns the hardcoded manifest for the onboarding agent.
///
/// Mirrors the `manifest()` dict in `agents/system/onboarding-agent/agent.py`.
/// Keeping this in sync is acceptable because the onboarding agent is a system
/// component with a stable, well-known contract.
fn onboarding_manifest() -> AgentManifest {
    AgentManifest {
        format_version: 1,
        name: "onboarding-agent".to_string(),
        version: ONBOARDING_AGENT_VERSION.to_string(),
        description: "Agent d'onboarding conversationnel - fait connaissance \
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
        supports_mailbox: false,
        mailbox_allowlist: None,
        system_prompt: None,
        tools_requiring_approval: Vec::new(),
        llm_backend: None,
        packages: vec![],
        memory_config: None,
        agent_type: Some("system".to_string()),
        examples: vec![],
        limitations: vec![],
        setup_notes: None,
        agent_class: None,
        // Onboarding agent owns the user profile, only it may write
        // into the global `__user__` namespace.
        user_memory_write: true,
        datasources: vec![],
        templates: vec![],
        secrets: vec![],
        check_commands: vec![],
    }
}

/// Extracts the Apollia Guide agent bundle (code + manifest + knowledge
/// base) and registers it as a non-uninstallable system agent.
fn provision_apollia_guide_agent(
    repo: &AgentRepository,
    data_dir: &Path,
) -> Result<(), BundledAgentError> {
    let agent_name = "apollia-guide";

    if let Some(existing) = repo.get(agent_name)? {
        if existing.version == APOLLIA_GUIDE_VERSION {
            tracing::debug!(
                name = %agent_name,
                detail = "the upgrade is skipped",
                "bundled.guide.current"
            );
            return Ok(());
        }
        tracing::info!(
            name = %agent_name,
            installed = %existing.version,
            bundled = %APOLLIA_GUIDE_VERSION,
            "bundled.guide.upgrading"
        );
    }

    let agent_dir = data_dir.join("agents").join(agent_name);
    let knowledge_dir = agent_dir.join("knowledge");
    std::fs::create_dir_all(&knowledge_dir).map_err(|e| BundledAgentError::Io(agent_name, e))?;

    let agent_path = agent_dir.join("agent.py");
    std::fs::write(&agent_path, APOLLIA_GUIDE_PY)
        .map_err(|e| BundledAgentError::Io(agent_name, e))?;

    std::fs::write(agent_dir.join("manifest.toml"), APOLLIA_GUIDE_TOML)
        .map_err(|e| BundledAgentError::Io(agent_name, e))?;

    std::fs::write(
        knowledge_dir.join("capabilities.md"),
        APOLLIA_GUIDE_CAPABILITIES_MD,
    )
    .map_err(|e| BundledAgentError::Io(agent_name, e))?;
    std::fs::write(
        knowledge_dir.join("tutorials.md"),
        APOLLIA_GUIDE_TUTORIALS_MD,
    )
    .map_err(|e| BundledAgentError::Io(agent_name, e))?;

    let now = now_rfc3339();
    let agent = InstalledAgent {
        name: agent_name.to_string(),
        version: APOLLIA_GUIDE_VERSION.to_string(),
        install_path: agent_path.clone(),
        source_path: agent_path,
        manifest: apollia_guide_manifest(),
        enabled: true,
        installed_at: now.clone(),
        updated_at: now,
    };
    repo.save(&agent)?;
    tracing::info!(
        name = %agent_name,
        version = %APOLLIA_GUIDE_VERSION,
        "bundled.guide.provisioned"
    );
    Ok(())
}

/// Mirrors the `manifest()` dict from `agents/system/apollia-guide/agent.py`.
fn apollia_guide_manifest() -> AgentManifest {
    AgentManifest {
        format_version: 1,
        name: "apollia-guide".to_string(),
        version: APOLLIA_GUIDE_VERSION.to_string(),
        description: "Conversational coach for Apollia OS - knows product \
                      capabilities and suggests actionable deep-links."
            .to_string(),
        tools_required: Vec::new(),
        tools_optional: vec![
            "navigate".to_string(),
            "read_memory_namespace".to_string(),
            "get_user_integrations".to_string(),
            "get_installed_agents".to_string(),
        ],
        supports_streaming: false,
        supports_a2a: false,
        memory_namespace: Some("apollia-guide".to_string()),
        shared_memory_namespaces: Vec::new(),
        max_concurrent_tasks: 1,
        step_budget: None,
        network_allowlist: None,
        dangerous_tools_allowed: false,
        tags: vec![
            "coach".to_string(),
            "system".to_string(),
            "guide".to_string(),
        ],
        skills: Vec::new(),
        execution_mode: "conversational".to_string(),
        supports_mailbox: false,
        mailbox_allowlist: None,
        system_prompt: None,
        tools_requiring_approval: Vec::new(),
        llm_backend: None,
        packages: vec![],
        memory_config: None,
        agent_type: Some("system".to_string()),
        examples: vec![],
        limitations: vec![],
        setup_notes: None,
        agent_class: None,
        user_memory_write: false,
        datasources: vec![],
        templates: vec![],
        secrets: vec![],
        check_commands: vec![],
    }
}

/// RFC 3339 timestamp without pulling in chrono.
fn now_rfc3339() -> String {
    let since_epoch = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = since_epoch.as_secs();
    // Approximate, good enough for an installation timestamp.
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
    fn embedded_toml_has_matching_version() {
        // Sanity check: the TOML we ship is parseable and its version matches.
        let parsed: toml::Value = toml::from_str(ONBOARDING_AGENT_TOML)
            .expect("embedded manifest.toml must be valid TOML");
        let version = parsed
            .get("agent")
            .and_then(|a| a.get("version"))
            .and_then(|v| v.as_str())
            .expect("manifest.toml must contain [agent].version");
        assert_eq!(version, ONBOARDING_AGENT_VERSION);
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
    fn provision_creates_bundle_files_and_saves_to_repo() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let db_path = tmp.path().join("agents.db");
        let repo = AgentRepository::open(&db_path).expect("open repo");

        ensure_bundled_agents(&repo, tmp.path());

        let agent_py = tmp.path().join("agents/onboarding-agent/agent.py");
        let manifest_toml = tmp.path().join("agents/onboarding-agent/manifest.toml");
        assert!(
            agent_py.exists(),
            "onboarding agent.py should be written to disk"
        );
        assert!(
            manifest_toml.exists(),
            "onboarding manifest.toml should be written to disk"
        );

        let agent = repo.get("onboarding-agent").expect("get").expect("exists");
        assert_eq!(agent.version, ONBOARDING_AGENT_VERSION);
        assert!(agent.enabled);
        assert_eq!(agent.install_path, agent_py);
    }

    #[test]
    fn apollia_guide_manifest_is_system_tier() {
        let m = apollia_guide_manifest();
        assert_eq!(m.name, "apollia-guide");
        assert_eq!(m.agent_type.as_deref(), Some("system"));
        assert!(!m.dangerous_tools_allowed);
        assert!(m.tools_required.is_empty());
    }

    #[test]
    fn apollia_guide_embedded_toml_matches_version() {
        let parsed: toml::Value = toml::from_str(APOLLIA_GUIDE_TOML)
            .expect("apollia-guide manifest.toml must be valid TOML");
        let version = parsed
            .get("agent")
            .and_then(|a| a.get("version"))
            .and_then(|v| v.as_str())
            .expect("manifest.toml must contain [agent].version");
        assert_eq!(version, APOLLIA_GUIDE_VERSION);
    }

    #[test]
    fn apollia_guide_bundle_is_provisioned_with_knowledge() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let db_path = tmp.path().join("agents.db");
        let repo = AgentRepository::open(&db_path).expect("open repo");

        ensure_bundled_agents(&repo, tmp.path());

        let agent_py = tmp.path().join("agents/apollia-guide/agent.py");
        let caps = tmp
            .path()
            .join("agents/apollia-guide/knowledge/capabilities.md");
        let tuts = tmp
            .path()
            .join("agents/apollia-guide/knowledge/tutorials.md");
        assert!(agent_py.exists());
        assert!(caps.exists(), "capabilities.md must be extracted");
        assert!(tuts.exists(), "tutorials.md must be extracted");

        let stored = repo.get("apollia-guide").expect("get").expect("exists");
        assert_eq!(stored.version, APOLLIA_GUIDE_VERSION);
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

    #[test]
    fn bundled_versions_track_the_product_version() {
        // GIVEN the built-in agents, which ship inside the binary and carry the
        // product version rather than a lifecycle of their own.
        let product = env!("CARGO_PKG_VERSION");

        // WHEN the bundled constants are compared to it.
        // THEN neither has drifted.
        assert_eq!(ONBOARDING_AGENT_VERSION, product);
        assert_eq!(APOLLIA_GUIDE_VERSION, product);
    }

    #[test]
    fn embedded_python_decorators_declare_the_bundled_version() {
        // GIVEN the two agent sources embedded by `include_str!`. Their
        // `@agent(version=...)` literal is what the runtime registers, and no
        // other test reads it: the Rust-side manifests restate the constant.
        let onboarding = format!("version=\"{ONBOARDING_AGENT_VERSION}\",");
        let guide = format!("version=\"{APOLLIA_GUIDE_VERSION}\",");

        // WHEN each source is searched for its decorator literal.
        // THEN both declare the bundled version.
        assert!(
            ONBOARDING_AGENT_PY.contains(&onboarding),
            "onboarding-agent/agent.py must declare {onboarding}"
        );
        assert!(
            APOLLIA_GUIDE_PY.contains(&guide),
            "apollia-guide/agent.py must declare {guide}"
        );
    }
}
