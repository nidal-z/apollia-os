//! Provisioning of the built-in system agent that still runs code
//! (onboarding-agent).
//!
//! Only **system-level** agents are shipped inside the desktop binary. The
//! guide of Apollia is not one of them any more: it ships as a package, see
//! [`crate::bundled_packages`].
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
const ONBOARDING_AGENT_VERSION: &str = "0.2.0-preview";

/// Ensures the built-in agents are extracted and registered in the repository.
///
/// Called once at boot, before the auto-load loop. Idempotent: skips agents
/// that are already installed at the same (or newer) version.
pub fn ensure_bundled_agents(repo: &AgentRepository, data_dir: &Path) {
    if let Err(e) = provision_onboarding_agent(repo, data_dir) {
        tracing::warn!(error = %e, "bundled.onboarding.provision.failed");
    }
}

/// Whether a registered agent still has the file the registry points at.
///
/// The provisioning used to trust the version recorded in `agents.db` alone,
/// and a row survives what wrote it: a profile carried over, a data directory
/// cleaned by hand, a first write that failed. The application then booted with
/// an agent it believed installed, the loader answered `file not found`, and
/// the registry stayed empty, which is what left a first run unable to start
/// its onboarding. Measured on 2026-09-08 on the packaged Windows build.
fn agent_file_present(existing: &InstalledAgent) -> bool {
    existing.install_path.is_file()
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

    // Installed at the current version AND still on disk: nothing to do. The
    // second half of that condition is what a stale row costs when it is
    // missing, see `agent_file_present`.
    if let Some(existing) = repo.get(agent_name)? {
        if existing.version == ONBOARDING_AGENT_VERSION && agent_file_present(&existing) {
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
        autonomy_level: None,
        format_version: 1,
        name: "onboarding-agent".to_string(),
        version: ONBOARDING_AGENT_VERSION.to_string(),
        description: "Conversational onboarding agent - gets to know the \
                      user in a natural way."
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
        model: None,
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
        // GIVEN the manifest the desktop ships for the onboarding agent
        // WHEN it is built
        let m = onboarding_manifest();
        // THEN it carries the name, the version and the conversational mode the runtime looks for
        assert_eq!(m.name, "onboarding-agent");
        assert_eq!(m.version, ONBOARDING_AGENT_VERSION);
        assert!(m.tools_required.is_empty());
        assert_eq!(m.memory_namespace, Some("onboarding-agent".to_string()));
        assert_eq!(m.execution_mode, "conversational");
    }

    #[test]
    fn embedded_toml_has_matching_version() {
        // Sanity check: the TOML we ship is parseable and its version matches.
        // GIVEN the manifest.toml embedded in the binary
        // WHEN it is parsed
        let parsed: toml::Value = toml::from_str(ONBOARDING_AGENT_TOML)
            .expect("embedded manifest.toml must be valid TOML");
        let version = parsed
            .get("agent")
            .and_then(|a| a.get("version"))
            .and_then(|v| v.as_str())
            .expect("manifest.toml must contain [agent].version");
        // THEN its version is the one the Rust constant declares, so the two cannot drift apart
        assert_eq!(version, ONBOARDING_AGENT_VERSION);
    }

    #[test]
    fn now_rfc3339_produces_valid_format() {
        // GIVEN the timestamp helper the provisioning writes with
        // WHEN it is called
        let ts = now_rfc3339();
        // Must match YYYY-MM-DDTHH:MM:SSZ pattern
        // THEN the shape is the RFC 3339 second-precision form SQLite sorts correctly
        assert_eq!(ts.len(), 20);
        assert!(ts.ends_with('Z'));
        assert_eq!(&ts[4..5], "-");
        assert_eq!(&ts[10..11], "T");
    }

    #[test]
    fn provision_rewrites_an_agent_whose_file_was_removed() {
        // GIVEN a profile where the agents were provisioned once and their
        // files have since gone, which is what a carried-over profile or a
        // hand-cleaned data directory leaves behind
        let tmp = tempfile::tempdir().expect("tempdir");
        let db_path = tmp.path().join("agents.db");
        let repo = AgentRepository::open(&db_path).expect("open repo");
        ensure_bundled_agents(&repo, tmp.path());
        let agent_py = tmp.path().join("agents/onboarding-agent/agent.py");
        assert!(agent_py.is_file(), "the first provisioning writes the file");
        std::fs::remove_dir_all(tmp.path().join("agents/onboarding-agent")).expect("remove");
        assert!(
            repo.get("onboarding-agent").expect("get").is_some(),
            "the row survives the file, which is the whole trap"
        );

        // WHEN the application boots again and provisions
        ensure_bundled_agents(&repo, tmp.path());

        // THEN the file is written back, so the loader finds what the registry
        // promises instead of answering `file not found`
        assert!(agent_py.is_file());
    }

    #[test]
    fn provision_creates_bundle_files_and_saves_to_repo() {
        // GIVEN an empty data directory and a fresh agent repository
        let tmp = tempfile::tempdir().expect("tempdir");
        let db_path = tmp.path().join("agents.db");
        let repo = AgentRepository::open(&db_path).expect("open repo");

        // WHEN the bundled agents are provisioned
        ensure_bundled_agents(&repo, tmp.path());

        // THEN the onboarding bundle is on disk and the repository points at it
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
    fn provision_is_idempotent() {
        // GIVEN a data directory already provisioned once
        let tmp = tempfile::tempdir().expect("tempdir");
        let db_path = tmp.path().join("agents.db");
        let repo = AgentRepository::open(&db_path).expect("open repo");

        ensure_bundled_agents(&repo, tmp.path());
        let first = repo.get("onboarding-agent").expect("get").expect("exists");

        // Running again should not error and should keep the same record.
        // WHEN the provisioning runs a second time
        ensure_bundled_agents(&repo, tmp.path());
        let second = repo.get("onboarding-agent").expect("get").expect("exists");

        // THEN the record is the same one, install date included, so a restart does not reinstall
        assert_eq!(first.installed_at, second.installed_at);
    }

    #[test]
    fn bundled_versions_track_the_product_version() {
        // GIVEN the built-in agent, which ships inside the binary and carries
        // the product version rather than a lifecycle of its own.
        let product = env!("CARGO_PKG_VERSION");

        // WHEN the bundled constants are compared to it.
        // THEN it has not drifted.
        assert_eq!(ONBOARDING_AGENT_VERSION, product);
    }

    #[test]
    fn embedded_python_decorator_declares_the_bundled_version() {
        // GIVEN the onboarding source embedded by `include_str!`. Its
        // `@agent(version=...)` literal is what the runtime registers, and no
        // other test reads it: the Rust-side manifest restates the constant.
        let onboarding = format!("version=\"{ONBOARDING_AGENT_VERSION}\",");

        // WHEN the source is searched for its decorator literal.
        // THEN it declares the bundled version.
        assert!(
            ONBOARDING_AGENT_PY.contains(&onboarding),
            "onboarding-agent/agent.py must declare {onboarding}"
        );
    }
}
