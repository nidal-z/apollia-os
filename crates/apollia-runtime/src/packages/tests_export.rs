//! Exporting an agent created in the interface, and installing the export.

use apollia_core::model_preset::{AgentModel, ModelSource};
use apollia_core::AutonomyLevel;
use apollia_tools::apo_archive::{ApoArchive, SignatureState, SigningKeyPair};
use apollia_tools::{AgentOrigin, AgentRepository, OverrideField, ResolvedAgent, StoredAgent};
use serde_json::json;

use super::tests::Env;
use super::*;

/// Stores `definition` as an agent created in the interface, named `name`.
fn store_interface_agent(agents: &AgentRepository, name: &str, definition: serde_json::Value) {
    agents
        .save_definition(&StoredAgent {
            name: name.to_owned(),
            origin: AgentOrigin::Interface,
            package: None,
            version: "1".to_owned(),
            definition,
            enabled: true,
            installed_at: String::new(),
            updated_at: String::new(),
        })
        .unwrap();
}

/// The agent `name` in force in `agents`, with `model` assigned.
fn resolved(agents: &AgentRepository, name: &str, model: Option<AgentModel>) -> ResolvedAgent {
    agents
        .resolved(name, model, crate::agents::global_default_autonomy())
        .unwrap()
        .unwrap()
}

/// An agent created in the interface, adjusted on its card: a mission and
/// examples of its own, tools of Apollia, an MCP tool, the tool of a
/// connected account, a tool of another package and another agent, a
/// source, an autonomy tier, a profile, and a model.
fn adjusted_agent(env: &Env) -> ResolvedAgent {
    store_interface_agent(
        &env.agents,
        "veille",
        json!({
            "id": "veille",
            "name": "Veille",
            "mission": "Watch the market.",
            "defaults": {},
        }),
    );
    let set = |field, value| {
        env.agents.set_override("veille", field, &value).unwrap();
    };
    set(
        OverrideField::Mission,
        json!("Watch the market every morning."),
    );
    set(OverrideField::Examples, json!(["What changed today?"]));
    set(
        OverrideField::Access,
        json!([
            "web_search",
            "file_read",
            "mcp:notion/search",
            "conn:google:ada@example.com/gmail.send",
            "pkg:crm/lookup",
            "agent:writer",
        ]),
    );
    set(OverrideField::Sources, json!(["profile"]));
    set(OverrideField::Autonomy, json!("supervised"));
    set(OverrideField::Profile, json!("profile-of-ada"));
    resolved(
        &env.agents,
        "veille",
        Some(AgentModel {
            agent: "veille".into(),
            model_source: ModelSource::Local,
            model_id: "qwen3-4b".into(),
            preset_id: Some(7),
        }),
    )
}

/// The text of every entry of the archive `bytes`, by name.
fn entries_of(bytes: &[u8]) -> Vec<(String, String)> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    (0..zip.len())
        .map(|i| {
            let mut entry = zip.by_index(i).unwrap();
            let mut text = String::new();
            std::io::Read::read_to_string(&mut entry, &mut text).unwrap();
            (entry.name().to_owned(), text)
        })
        .collect()
}

#[test]
fn test_plan_keeps_tools_requires_connections_and_lists_what_it_removes() {
    // GIVEN an agent created in the interface and adjusted on its card
    let env = Env::new();
    let agent = adjusted_agent(&env);

    // WHEN its export is planned
    let plan = export_plan(&agent, "Ada").unwrap();

    // THEN the current values are the defaults of the package
    let decl = &plan.manifest.agents[0];
    assert_eq!(plan.manifest.package.name, "veille");
    assert_eq!(plan.manifest.package.version, "1.0.0");
    assert_eq!(plan.manifest.package.publisher, "Ada");
    assert_eq!(decl.mission, "Watch the market every morning.");
    assert_eq!(decl.examples, vec!["What changed today?"]);
    assert_eq!(decl.defaults.sources, vec!["profile"]);
    assert_eq!(decl.defaults.autonomy, Some(AutonomyLevel::Supervised));
    let model = decl.defaults.model.as_ref().unwrap();
    assert_eq!(model.model.as_deref(), Some("qwen3-4b"));
    assert_eq!(model.preset, None);
    // AND the tools of Apollia and the MCP tool are kept, the account is not
    assert_eq!(
        decl.defaults.access,
        vec!["web_search", "file_read", "mcp:notion/search"]
    );
    // AND the MCP server and the provider are required connections
    assert_eq!(plan.required_connections, vec!["google", "notion"]);
    let kinds: Vec<&str> = plan
        .manifest
        .requires
        .iter()
        .map(|r| r.kind.as_str())
        .collect();
    assert_eq!(kinds, vec!["google", "notion"]);
    // AND the accesses into another package or agent are removed and listed
    assert_eq!(plan.removed_access, vec!["pkg:crm/lookup", "agent:writer"]);
}

#[test]
fn test_only_an_agent_created_in_the_interface_is_exported() {
    // GIVEN an agent installed from a package, the built-in agent, and an
    // agent created in the interface
    let env = Env::new();
    let mut packaged = adjusted_agent(&env);
    packaged.origin = AgentOrigin::Package;
    let mut built_in = adjusted_agent(&env);
    built_in.id = apollia_tools::APOLLIA_AGENT_ID.to_owned();

    // WHEN each is planned
    let refused_package = export_plan(&packaged, "Ada");
    let refused_built_in = export_plan(&built_in, "Ada");

    // THEN the first two are refused, each with its reason
    assert!(matches!(
        refused_package,
        Err(ExportError::NotInterface { .. })
    ));
    assert!(matches!(refused_built_in, Err(ExportError::BuiltIn { .. })));
}

#[test]
fn test_publisher_is_the_given_name_else_the_setting() {
    // GIVEN a setting, an override and nothing at all
    // WHEN the publisher is resolved for each case
    let from_setting = publisher_of(None, "Ada Lovelace");
    let overridden = publisher_of(Some("  Grace "), "Ada Lovelace");
    let blank_override = publisher_of(Some("  "), "Ada Lovelace");
    let nobody = publisher_of(None, "");
    let invalid = publisher_of(Some("line\nbreak"), "");

    // THEN the override wins, a blank one falls back, and none is refused
    assert_eq!(from_setting.unwrap(), "Ada Lovelace");
    assert_eq!(overridden.unwrap(), "Grace");
    assert_eq!(blank_override.unwrap(), "Ada Lovelace");
    assert!(matches!(nobody, Err(ExportError::PublisherRequired)));
    assert!(matches!(invalid, Err(ExportError::InvalidPublisher(_))));
}

#[test]
fn test_archive_carries_no_secret_no_memory_and_no_history() {
    // GIVEN an agent whose stored definition carries a secret, a memory
    // entry and a run of its history beside its fields, with a personal
    // profile and a connected account on its card
    let env = Env::new();
    store_interface_agent(
        &env.agents,
        "veille",
        json!({
            "id": "veille",
            "name": "Veille",
            "mission": "Watch the market.",
            "defaults": { "access": ["conn:google:ada@example.com/gmail.send"] },
            "secret": "sk-SECRET-MARKER",
            "memory": [{ "content": "MEMORY-MARKER" }],
            "history": [{ "task": "HISTORY-MARKER" }],
        }),
    );
    env.agents
        .set_override("veille", OverrideField::Profile, &json!("PROFILE-MARKER"))
        .unwrap();
    let agent = resolved(&env.agents, "veille", None);
    let key = SigningKeyPair::from_seed(&[9u8; 32]);

    // WHEN it is exported signed
    let export = export_agent(&agent, "Ada", Some(&key), env.data_dir()).unwrap();

    // THEN the archive holds the manifest and the signature, nothing else
    let entries = entries_of(&export.bytes);
    let names: Vec<&str> = entries.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, vec!["apollia.toml", "SIGNATURE", "SIGNATURE.key"]);
    // AND no entry carries the secret, the memory, the history, the profile,
    // the account or the private key
    let seed = hex::encode([9u8; 32]);
    for (name, text) in &entries {
        for marker in [
            "sk-SECRET-MARKER",
            "MEMORY-MARKER",
            "HISTORY-MARKER",
            "PROFILE-MARKER",
            "ada@example.com",
            seed.as_str(),
        ] {
            assert!(!text.contains(marker), "{name} carries {marker}");
        }
    }
    // AND the working folder is gone
    let leftovers: Vec<_> = std::fs::read_dir(env.data_dir())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with(".export-"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
}

#[tokio::test]
async fn test_reinstalling_the_export_on_a_blank_profile_gives_the_same_card() {
    // GIVEN an agent created in the interface and adjusted on its card,
    // exported signed, and a blank profile on another machine
    let author = Env::new();
    let agent = adjusted_agent(&author);
    let key = SigningKeyPair::from_seed(&[5u8; 32]);
    let export = export_agent(&agent, "Ada", Some(&key), author.data_dir()).unwrap();
    let blank = Env::new();
    let apo = blank.data_dir().join(&export.file_name);
    std::fs::write(&apo, &export.bytes).unwrap();

    // WHEN the colleague installs it
    let options = InstallOptions {
        actor: "test".into(),
        ..InstallOptions::default()
    };
    let staged = prepare(&apo, &options, &blank.sources(), blank.data_dir()).unwrap();
    let venvs = blank.data_dir().join("venvs");
    let targets = CommitTargets::<NoMcp> {
        packages: &blank.packages,
        agents: &blank.agents,
        governance: &blank.governance,
        mcp: None,
        venvs: &venvs,
    };
    commit(staged, &targets, "test").await.unwrap();

    // THEN the installed agent reads the same card, less what stays with the
    // author: the accesses into what the package does not carry, the account
    // of a connection, and the profile
    let installed = resolved(&blank.agents, "veille", None);
    assert_eq!(installed.name, agent.name);
    assert_eq!(installed.mission, agent.mission);
    assert_eq!(installed.examples, agent.examples);
    assert_eq!(installed.sources, agent.sources);
    assert_eq!(installed.autonomy, agent.autonomy);
    assert_eq!(
        installed.access,
        vec!["web_search", "file_read", "mcp:notion/search"]
    );
    assert_eq!(installed.profile, None);
    assert_eq!(
        installed.definition["defaults"]["model"]["model"],
        json!("qwen3-4b")
    );
    // AND it comes from the package, signed by the author's key
    assert_eq!(installed.origin, AgentOrigin::Package);
    let signature = blank.packages.signature("veille").unwrap().unwrap();
    assert!(signature.signed);
    assert_eq!(signature.publisher, "Ada");
    assert_eq!(
        signature.key_fingerprint.as_deref(),
        Some(key.public_key().fingerprint().as_str())
    );
    let mut archive = ApoArchive::open(&apo).unwrap();
    assert!(matches!(
        archive.signature_state().unwrap(),
        SignatureState::Signed { publisher, .. } if publisher == "Ada"
    ));
}

/// An MCP manager for a package that brings no server.
struct NoMcp;

impl McpServerAdder for NoMcp {
    async fn add(&self, _config: apollia_mcp::config::McpServerConfig) -> Result<(), String> {
        Err("no server expected".into())
    }

    async fn remove(&self, _name: &str) -> Result<(), String> {
        Ok(())
    }
}
