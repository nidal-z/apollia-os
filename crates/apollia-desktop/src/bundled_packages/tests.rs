//! The guide shipped as a package: its manifest, its archive, its
//! installation over the guide earlier versions ran as code.

use std::sync::Mutex;

use apollia_runtime::governance::{
    AccessCall, AccessDecision, DeclaredAccess, EffectiveAccess, GovernanceContract,
    GovernanceError, GovernanceEvent, InstallSubject,
};
use apollia_tools::apo_archive::{ApoArchive, SignatureState};
use apollia_tools::{AgentOrigin, OverrideField, StoredAgent};

use super::*;

/// Governance that requires a signature, as a strict organisation would,
/// and keeps what it records.
#[derive(Default)]
struct StrictGovernance {
    recorded: Mutex<Vec<GovernanceEvent>>,
}

impl GovernanceContract for StrictGovernance {
    fn effective_accesses(
        &self,
        _agent_id: &str,
        declared: &[DeclaredAccess],
    ) -> Vec<EffectiveAccess> {
        declared
            .iter()
            .map(|a| EffectiveAccess {
                access_id: a.access_id.clone(),
                permits: a.permits.clone(),
                sensitive: a.declared_sensitive,
            })
            .collect()
    }

    fn decide(&self, _call: &AccessCall<'_>) -> AccessDecision {
        AccessDecision::Allowed {
            reason: apollia_runtime::governance::DecisionReason::NoConsentNeeded,
        }
    }

    fn signature_required(&self, _subject: &InstallSubject<'_>) -> bool {
        true
    }

    fn record(&self, event: &GovernanceEvent) -> Result<(), GovernanceError> {
        self.recorded.lock().unwrap().push(event.clone());
        Ok(())
    }
}

/// Assembles the shipped package `name` the way the desktop does at start.
fn assemble_shipped(name: &str, data_dir: &Path) -> Result<PathBuf, BundledPackageError> {
    let shipped = SHIPPED
        .iter()
        .find(|s| (s.manifest)().is_ok_and(|m| m.package.name == name))
        .unwrap();
    assemble(data_dir, &(shipped.manifest)()?, shipped.files)
}

fn assemble_guide(data_dir: &Path) -> Result<PathBuf, BundledPackageError> {
    assemble_shipped(GUIDE, data_dir)
}

fn assemble_onboarding(data_dir: &Path) -> Result<PathBuf, BundledPackageError> {
    assemble_shipped(ONBOARDING, data_dir)
}

fn assemble_agent_builder(data_dir: &Path) -> Result<PathBuf, BundledPackageError> {
    assemble_shipped(AGENT_BUILDER, data_dir)
}

fn agents(dir: &Path) -> AgentRepository {
    AgentRepository::open(&apollia_core::paths::DataFile::Agents.path(dir)).unwrap()
}

/// The row an earlier desktop wrote for the guide: Python code at `version`.
fn code_guide(dir: &Path, version: &str) -> (StoredAgent, std::path::PathBuf) {
    let path = dir.join("agents/apollia-guide/agent.py");
    let agent = StoredAgent {
        name: GUIDE.into(),
        origin: AgentOrigin::Package,
        package: None,
        version: version.into(),
        definition: serde_json::json!({
            "id": GUIDE, "name": GUIDE, "mission": "Coach.", "defaults": {},
        }),
        enabled: true,
        installed_at: "2026-09-01T00:00:00Z".into(),
        updated_at: "2026-09-01T00:00:00Z".into(),
    };
    (agent, path)
}

#[test]
fn the_guide_is_a_declarative_agent_with_its_knowledge_in_the_mission() {
    // GIVEN the manifest and the knowledge base embedded in the binary
    // WHEN the manifest of the guide is assembled
    let manifest = guide_manifest().unwrap();

    // THEN it is one agent, no code, its mission carrying both knowledge files
    assert_eq!(manifest.package.name, GUIDE);
    assert_eq!(manifest.package.version, env!("CARGO_PKG_VERSION"));
    assert!(manifest.tools.is_empty() && manifest.workflows.is_empty());
    assert!(manifest.pip.is_none() && manifest.mcp_servers.is_empty());
    assert_eq!(manifest.agents.len(), 1);
    let agent = &manifest.agents[0];
    assert_eq!(agent.id, GUIDE);
    assert!(!agent.examples.is_empty());
    assert!(
        agent.defaults.access.is_empty(),
        "the guide is granted nothing"
    );
    assert!(agent.mission.contains(KNOWLEDGE_HEADING));
    for knowledge in GUIDE_KNOWLEDGE {
        let first_line = knowledge.lines().next().unwrap();
        assert!(agent.mission.contains(first_line), "{first_line}");
    }
}

#[test]
fn the_knowledge_names_no_retired_contract_and_no_missing_screen() {
    // GIVEN the knowledge base the guide answers from
    let knowledge = GUIDE_KNOWLEDGE.join("\n");

    // WHEN it is searched for the old agent contract and for screens the
    // application does not have
    let retired = [
        "manifest()",
        "run()",
        "@on_message",
        "@skill",
        "ctx.a2a",
        "agent start",
        "/pipelines",
        "/triggers",
        "/onboarding",
    ];

    // THEN none is there
    for needle in retired {
        assert!(!knowledge.contains(needle), "{needle}");
    }
}

#[test]
fn the_guide_assembles_into_an_unsigned_archive() {
    // GIVEN an empty data directory
    let dir = tempfile::tempdir().unwrap();

    // WHEN the guide is assembled
    let path = assemble_guide(dir.path()).unwrap();

    // THEN the archive reads back with the assembled manifest, unsigned, and
    // no staging folder is left behind
    let mut archive = ApoArchive::open(&path).unwrap();
    assert_eq!(archive.manifest(), &guide_manifest().unwrap());
    assert_eq!(archive.signature_state().unwrap(), SignatureState::Unsigned);
    let left: Vec<_> = std::fs::read_dir(path.parent().unwrap())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(left, vec![format!("{GUIDE}.apo")]);
}

#[tokio::test]
async fn a_guide_installed_as_code_becomes_the_package_agent_and_stays_disabled() {
    // GIVEN the guide an earlier version installed as Python code, at an
    // older version, with its mission overridden and disabled by the user
    let dir = tempfile::tempdir().unwrap();
    let repo = agents(dir.path());
    let (guide, code) = code_guide(dir.path(), "0.1.0");
    repo.save_as_code_row(&guide, &code).unwrap();
    repo.set_override(GUIDE, OverrideField::Mission, &serde_json::json!("Mine."))
        .unwrap();
    repo.set_enabled(GUIDE, false).unwrap();
    let governance = StrictGovernance::default();

    // WHEN the desktop starts: the code guide is retired before the agents
    // that run code are loaded, then the shipped guide is installed under a
    // governance that requires signatures
    retire_code_agents(&repo);
    let archive = assemble_guide(dir.path()).unwrap();
    let outcome = install_embedded(&archive, dir.path(), &governance)
        .await
        .unwrap();

    // THEN the guide runs no code, carries the shipped definition, keeps the
    // user's override, and is still disabled
    assert!(
        matches!(outcome, EmbeddedOutcome::Installed(_)),
        "{outcome:?}"
    );
    assert_eq!(
        repo.legacy_install_path(GUIDE).unwrap(),
        None,
        "no code is left"
    );
    let stored = repo.stored(GUIDE).unwrap().unwrap();
    assert_eq!(stored.origin, AgentOrigin::Package);
    assert_eq!(stored.package.as_deref(), Some(GUIDE));
    assert_eq!(stored.version, env!("CARGO_PKG_VERSION"));
    assert!(!stored.enabled, "a disabled guide is never enabled again");
    let resolved = repo
        .resolved(GUIDE, None, apollia_core::AutonomyLevel::Assisted)
        .unwrap()
        .unwrap();
    assert_eq!(resolved.mission, "Mine.");
    assert_eq!(resolved.autonomy, apollia_core::AutonomyLevel::Assisted);
    assert!(governance
        .recorded
        .lock()
        .unwrap()
        .iter()
        .any(|e| matches!(e, GovernanceEvent::PackageInstalled { signed: false, .. })));
}

#[tokio::test]
async fn a_newer_guide_is_not_downgraded_and_the_current_one_is_left_alone() {
    // GIVEN the guide package installed at a version newer than the binary's
    let dir = tempfile::tempdir().unwrap();
    let governance = StrictGovernance::default();
    let mut newer = guide_manifest().unwrap();
    newer.package.version = "99.0.0".into();
    let src = dir.path().join("newer");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(src.join("apollia.toml"), newer.to_toml().unwrap()).unwrap();
    let newer_apo = dir.path().join("newer.apo");
    build_from_dir(&src, &newer_apo, &BuildOptions::default()).unwrap();
    install_embedded(&newer_apo, dir.path(), &governance)
        .await
        .unwrap();

    // WHEN the binary's guide is installed, then installed over itself
    let archive = assemble_guide(dir.path()).unwrap();
    let kept = install_embedded(&archive, dir.path(), &governance)
        .await
        .unwrap();
    let other = tempfile::tempdir().unwrap();
    let other_archive = assemble_guide(other.path()).unwrap();
    install_embedded(&other_archive, other.path(), &governance)
        .await
        .unwrap();
    let again = install_embedded(&other_archive, other.path(), &governance)
        .await
        .unwrap();

    // THEN the newer version stays, and the same version installs nothing
    assert!(matches!(kept, EmbeddedOutcome::Newer { ref installed, .. } if installed == "99.0.0"));
    assert_eq!(
        agents(dir.path()).stored(GUIDE).unwrap().unwrap().version,
        "99.0.0"
    );
    assert!(matches!(again, EmbeddedOutcome::Current { .. }));
}

/// The row an earlier desktop wrote for the onboarding: Python code at
/// `version`.
fn code_onboarding(dir: &Path, version: &str) -> (StoredAgent, std::path::PathBuf) {
    let path = dir.join("agents/onboarding-agent/agent.py");
    let agent = StoredAgent {
        name: ONBOARDING.into(),
        origin: AgentOrigin::Package,
        package: None,
        version: version.into(),
        definition: serde_json::json!({
            "id": ONBOARDING, "name": ONBOARDING, "mission": "Onboards.", "defaults": {},
        }),
        enabled: true,
        installed_at: "2026-09-01T00:00:00Z".into(),
        updated_at: "2026-09-01T00:00:00Z".into(),
    };
    (agent, path)
}

#[test]
fn the_onboarding_is_a_declarative_agent_granted_its_one_tool() {
    // GIVEN the manifest of the onboarding embedded in the binary
    // WHEN it is read
    let manifest = onboarding_manifest().unwrap();

    // THEN it is one agent and one tool, the tool in the module the binary
    // carries, granted to the agent, at the product version
    assert_eq!(manifest.package.name, ONBOARDING);
    assert_eq!(manifest.package.version, env!("CARGO_PKG_VERSION"));
    assert!(manifest.workflows.is_empty());
    assert!(manifest.pip.is_none() && manifest.mcp_servers.is_empty());
    assert_eq!(manifest.agents.len(), 1);
    let agent = &manifest.agents[0];
    assert_eq!(agent.id, ONBOARDING);
    assert!(agent.mission.contains("propose_profile"));
    assert_eq!(manifest.tools.len(), 1);
    let tool = &manifest.tools[0];
    assert_eq!(tool.name, "propose_profile");
    assert_eq!(tool.module, ONBOARDING_MODULE);
    assert_eq!(
        agent.defaults.access,
        vec![format!("pkg:{ONBOARDING}/propose_profile")]
    );
}

#[test]
fn the_onboarding_assembles_with_its_tool_unsigned() {
    // GIVEN an empty data directory
    let dir = tempfile::tempdir().unwrap();

    // WHEN the onboarding is assembled
    let path = assemble_onboarding(dir.path()).unwrap();

    // THEN the archive reads back with its manifest, unsigned, and carries
    // the module of its tool as the binary ships it
    let mut archive = ApoArchive::open(&path).unwrap();
    assert_eq!(archive.manifest(), &onboarding_manifest().unwrap());
    assert_eq!(archive.signature_state().unwrap(), SignatureState::Unsigned);
    let out = dir.path().join("unpacked");
    archive.extract_to(&out).unwrap();
    assert_eq!(
        std::fs::read_to_string(out.join(ONBOARDING_MODULE)).unwrap(),
        ONBOARDING_SOURCE
    );
}

#[tokio::test]
async fn an_onboarding_installed_as_code_becomes_the_package_agent_and_its_tool_is_granted() {
    // GIVEN the onboarding an earlier version installed as Python code,
    // disabled by the user
    let dir = tempfile::tempdir().unwrap();
    let repo = agents(dir.path());
    let (onboarding, code) = code_onboarding(dir.path(), "0.1.0");
    repo.save_as_code_row(&onboarding, &code).unwrap();
    repo.set_enabled(ONBOARDING, false).unwrap();
    let governance = StrictGovernance::default();

    // WHEN the desktop starts: the code onboarding is retired, then the
    // shipped onboarding is installed under a governance that requires
    // signatures
    retire_code_agents(&repo);
    let archive = assemble_onboarding(dir.path()).unwrap();
    let outcome = install_embedded(&archive, dir.path(), &governance)
        .await
        .unwrap();

    // THEN no code is loaded, the agent is the package's and stays disabled,
    // and the installed tool is the code the host grants the profile to
    assert!(
        matches!(outcome, EmbeddedOutcome::Installed(_)),
        "{outcome:?}"
    );
    assert_eq!(
        repo.legacy_install_path(ONBOARDING).unwrap(),
        None,
        "no code is left"
    );
    let stored = repo.stored(ONBOARDING).unwrap().unwrap();
    assert_eq!(stored.origin, AgentOrigin::Package);
    assert_eq!(stored.package.as_deref(), Some(ONBOARDING));
    assert!(
        !stored.enabled,
        "a disabled onboarding is never enabled again"
    );
    let packages =
        PackageRepository::open(&apollia_core::paths::DataFile::Agents.path(dir.path())).unwrap();
    let installed = packages.get(ONBOARDING).unwrap().unwrap();
    assert!(
        apollia_aip::package_tools::ProfileGrant::onboarding().covers(
            ONBOARDING,
            &installed.root_path,
            ONBOARDING_MODULE
        )
    );
}

/// Identifier of the agent workshop package.
const AGENT_BUILDER: &str = "agent-builder";

/// The workflow the Agents screen launches, as `<package>/<workflow>`.
const AGENT_BUILDER_WORKFLOW: &str = "agent-builder/draft";

#[test]
fn the_agent_workshop_is_one_workflow_and_no_agent() {
    // GIVEN the manifest of the agent workshop embedded in the binary
    // WHEN it is read
    let manifest = agent_builder_manifest().unwrap();

    // THEN it holds no agent and no tool, and one workflow that reads only,
    // in the module the binary carries, at the product version
    assert_eq!(manifest.package.name, AGENT_BUILDER);
    assert_eq!(manifest.package.version, env!("CARGO_PKG_VERSION"));
    assert!(manifest.agents.is_empty() && manifest.tools.is_empty());
    assert!(manifest.pip.is_none() && manifest.mcp_servers.is_empty());
    assert_eq!(manifest.workflows.len(), 1);
    let workflow = &manifest.workflows[0];
    assert_eq!(
        format!("{AGENT_BUILDER}/{}", workflow.name),
        AGENT_BUILDER_WORKFLOW
    );
    assert_eq!(workflow.module, AGENT_BUILDER_FILES[0].0);
    assert_eq!(workflow.steps, ["understand", "ask", "write"]);
}

#[tokio::test]
async fn the_agent_workshop_installs_with_its_workflow_under_a_strict_governance() {
    // GIVEN an empty data directory and a governance that requires signatures
    let dir = tempfile::tempdir().unwrap();
    let governance = StrictGovernance::default();

    // WHEN the agent workshop is assembled and installed
    let path = assemble_agent_builder(dir.path()).unwrap();
    let outcome = install_embedded(&path, dir.path(), &governance)
        .await
        .unwrap();

    // THEN it installs unsigned, as an embedded package, and its archive
    // carries the workflow as the binary ships it
    assert!(
        matches!(outcome, EmbeddedOutcome::Installed(_)),
        "{outcome:?}"
    );
    let mut archive = ApoArchive::open(&path).unwrap();
    assert_eq!(archive.signature_state().unwrap(), SignatureState::Unsigned);
    let out = dir.path().join("unpacked");
    archive.extract_to(&out).unwrap();
    assert_eq!(
        std::fs::read_to_string(out.join(AGENT_BUILDER_FILES[0].0)).unwrap(),
        AGENT_BUILDER_FILES[0].1
    );
}

#[test]
fn the_digest_known_before_assembly_is_the_digest_of_the_assembled_archive() {
    // GIVEN an empty data directory
    let dir = tempfile::tempdir().unwrap();

    for shipped in &SHIPPED {
        // WHEN each shipped package is assembled
        let manifest = (shipped.manifest)().unwrap();
        let path = assemble(dir.path(), &manifest, shipped.files).unwrap();

        // THEN the digest the start compares without assembling is the one
        // the installation records from the archive
        assert_eq!(
            shipped.content_digest(&manifest).unwrap(),
            ApoArchive::open(&path).unwrap().content_digest().unwrap(),
            "{}",
            manifest.package.name
        );
    }
}

/// The shipped entry of the agent workshop.
fn shipped_agent_builder() -> &'static Shipped {
    SHIPPED
        .iter()
        .find(|s| (s.manifest)().unwrap().package.name == AGENT_BUILDER)
        .unwrap()
}

#[tokio::test]
async fn the_same_version_with_the_same_content_is_current_and_nothing_is_assembled() {
    // GIVEN the agent workshop installed by a first start, its archive then
    // removed from the folder the start assembles into
    let dir = tempfile::tempdir().unwrap();
    let governance = StrictGovernance::default();
    let shipped = shipped_agent_builder();
    let first = install_shipped(shipped, dir.path(), &governance)
        .await
        .unwrap();
    let archive = dir.path().join("agents/bundled/agent-builder.apo");
    std::fs::remove_file(&archive).unwrap();
    let recorded = governance.recorded.lock().unwrap().len();

    // WHEN the desktop starts again with the same binary
    let again = install_shipped(shipped, dir.path(), &governance)
        .await
        .unwrap();

    // THEN the first start installed it, the second is current, assembles
    // nothing and records nothing
    assert!(matches!(first, EmbeddedOutcome::Installed(_)), "{first:?}");
    assert_eq!(
        again,
        EmbeddedOutcome::Current {
            version: env!("CARGO_PKG_VERSION").into()
        }
    );
    assert!(
        !archive.exists(),
        "nothing is assembled for a current package"
    );
    assert_eq!(governance.recorded.lock().unwrap().len(), recorded);
}

#[tokio::test]
async fn the_same_version_with_another_content_is_installed_again() {
    // GIVEN the agent workshop installed at the version the binary carries,
    // with a workflow that differs from the one the binary carries, as an
    // earlier build of the same version shipped it
    let dir = tempfile::tempdir().unwrap();
    let governance = StrictGovernance::default();
    let src = dir.path().join("earlier");
    std::fs::create_dir_all(src.join("workflows")).unwrap();
    std::fs::write(
        src.join(apollia_manifest::MANIFEST_FILE_NAME),
        agent_builder_manifest().unwrap().to_toml().unwrap(),
    )
    .unwrap();
    let earlier_code = format!("{}\n# an earlier build\n", AGENT_BUILDER_FILES[0].1);
    std::fs::write(src.join(AGENT_BUILDER_FILES[0].0), &earlier_code).unwrap();
    let earlier = dir.path().join("earlier.apo");
    build_from_dir(&src, &earlier, &BuildOptions::default()).unwrap();
    install_embedded(&earlier, dir.path(), &governance)
        .await
        .unwrap();
    let packages =
        PackageRepository::open(&apollia_core::paths::DataFile::Agents.path(dir.path())).unwrap();
    let root = packages.get(AGENT_BUILDER).unwrap().unwrap().root_path;
    assert_eq!(
        std::fs::read_to_string(root.join(AGENT_BUILDER_FILES[0].0)).unwrap(),
        earlier_code
    );

    // WHEN the desktop starts with the binary's agent workshop
    let outcome = install_shipped(shipped_agent_builder(), dir.path(), &governance)
        .await
        .unwrap();

    // THEN the workflow the binary carries replaces the installed one, and
    // the reinstallation is recorded as an update at the same version
    let version = env!("CARGO_PKG_VERSION");
    assert!(
        matches!(outcome, EmbeddedOutcome::Installed(ref r) if r.previous_version.as_deref() == Some(version)),
        "{outcome:?}"
    );
    let root = packages.get(AGENT_BUILDER).unwrap().unwrap().root_path;
    assert_eq!(
        std::fs::read_to_string(root.join(AGENT_BUILDER_FILES[0].0)).unwrap(),
        AGENT_BUILDER_FILES[0].1
    );
    assert!(governance.recorded.lock().unwrap().iter().any(|e| matches!(
        e,
        GovernanceEvent::PackageUpdated { from_version, to_version, .. }
            if from_version == version && to_version == version
    )));
}
