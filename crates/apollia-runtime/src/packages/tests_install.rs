//! Installing and updating a package after its preview.

use std::sync::Mutex;

use apollia_mcp::config::McpServerConfig;
use apollia_mcp::Provenance;
use apollia_tools::apo_archive::SigningKeyPair;
use apollia_tools::{OverrideField, PackageItemKind};

use super::tests::{archive, manifest, tamper, Env};
use super::*;
use crate::governance::GovernanceEvent;

/// An MCP manager that records the servers it is given, refusing `refuse`.
#[derive(Default)]
struct FakeMcp {
    refuse: Option<String>,
    added: Mutex<Vec<McpServerConfig>>,
    removed: Mutex<Vec<String>>,
}

impl McpServerAdder for FakeMcp {
    async fn add(&self, config: McpServerConfig) -> Result<(), String> {
        if self.refuse.as_deref() == Some(config.name.as_str()) {
            return Err("refused by the admission".into());
        }
        self.added.lock().unwrap().push(config);
        Ok(())
    }

    async fn remove(&self, name: &str) -> Result<(), String> {
        self.removed.lock().unwrap().push(name.to_owned());
        Ok(())
    }
}

fn options() -> InstallOptions {
    InstallOptions {
        actor: "test".into(),
        ..InstallOptions::default()
    }
}

async fn install_with(
    env: &Env,
    apo: &std::path::Path,
    options: &InstallOptions,
    mcp: Option<&FakeMcp>,
) -> Result<InstallReport, PackageError> {
    let staged = prepare(apo, options, &env.sources(), env.data_dir())?;
    let venvs = env.data_dir().join("venvs");
    let targets = CommitTargets {
        packages: &env.packages,
        agents: &env.agents,
        governance: &env.governance,
        mcp,
        venvs: &venvs,
    };
    commit(staged, &targets, &options.actor).await
}

fn leftovers(env: &Env) -> Vec<String> {
    let dir = env.data_dir().join("agents").join("packages");
    match std::fs::read_dir(dir) {
        Ok(entries) => entries
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with('.'))
            .collect(),
        Err(_) => Vec::new(),
    }
}

// ── Criterion 2: installation ─────────────────────────────────────────────

#[tokio::test]
async fn test_install_writes_files_definitions_items_and_journal() {
    // GIVEN an unsigned package with one agent and one tool
    let env = Env::new();
    let apo = archive(env.data_dir(), "crm.apo", &manifest("1.0.0", &[], ""), None);

    // WHEN it is installed
    let report = install_with(&env, &apo, &options(), None).await.unwrap();

    // THEN the files sit under agents/packages/crm, nothing is left in staging
    let root = env.data_dir().join("agents/packages/crm");
    assert!(root.join("apollia.toml").is_file());
    assert!(root.join("tools/lookup.py").is_file());
    assert!(leftovers(&env).is_empty());

    // AND the package, its signature state, its items and its agent are stored
    let package = env.packages.get("crm").unwrap().unwrap();
    assert_eq!(package.version, "1.0.0");
    assert_eq!(package.root_path, root);
    let signature = env.packages.signature("crm").unwrap().unwrap();
    assert_eq!(signature.publisher, "Acme");
    assert!(!signature.signed);
    assert_eq!(signature.key_fingerprint, None);
    let items = env.packages.list_items("crm").unwrap();
    assert!(items
        .iter()
        .any(|i| i.kind == PackageItemKind::Agent && i.item_id == "crm-agent"));
    assert!(items
        .iter()
        .any(|i| i.kind == PackageItemKind::Tool && i.item_id == "lookup"));
    let stored = env.agents.stored("crm-agent").unwrap().unwrap();
    assert_eq!(stored.package.as_deref(), Some("crm"));
    assert_eq!(stored.definition["mission"], "Track leads.");

    // AND the report carries the unsigned notice, the journal the package
    // then its agent
    assert!(report.notices.contains(&PackageNotice::Unsigned));
    assert_eq!(report.previous_version, None);
    let recorded = env.governance.recorded.lock().unwrap().clone();
    assert!(matches!(
        &recorded[0],
        GovernanceEvent::PackageInstalled { package_id, signed: false, actor, .. }
            if package_id == "crm" && actor == "test"
    ));
    assert!(matches!(
        &recorded[1],
        GovernanceEvent::Installed { agent_id, version, .. }
            if agent_id == "crm-agent" && version == "1.0.0"
    ));
}

#[tokio::test]
async fn test_install_of_a_signed_package_records_publisher_and_key() {
    // GIVEN a package signed by a key
    let env = Env::new();
    let key = SigningKeyPair::from_seed(&[4u8; 32]);
    let apo = archive(
        env.data_dir(),
        "crm.apo",
        &manifest("1.0.0", &[], ""),
        Some(&key),
    );

    // WHEN it is installed
    install_with(&env, &apo, &options(), None).await.unwrap();

    // THEN the package row records who signed it, and the journal says signed
    let signature = env.packages.signature("crm").unwrap().unwrap();
    assert!(signature.signed);
    assert_eq!(
        signature.key_fingerprint,
        Some(key.public_key().fingerprint())
    );
    let recorded = env.governance.recorded.lock().unwrap().clone();
    assert!(matches!(
        &recorded[0],
        GovernanceEvent::PackageInstalled {
            signed: true,
            key_fingerprint: Some(_),
            ..
        }
    ));
}

#[tokio::test]
async fn test_install_refuses_an_invalid_signature_and_writes_nothing() {
    // GIVEN a signed package one entry of which was renamed after signing
    let env = Env::new();
    let key = SigningKeyPair::from_seed(&[5u8; 32]);
    let signed = archive(
        env.data_dir(),
        "crm.apo",
        &manifest("1.0.0", &[], ""),
        Some(&key),
    );
    let altered = env.data_dir().join("altered.apo");
    tamper(&signed, &altered);

    // WHEN it is installed
    let result = install_with(&env, &altered, &options(), None).await;

    // THEN it is refused and nothing is written
    assert!(matches!(
        result,
        Err(PackageError::Refused(
            InstallRefusal::InvalidSignature { .. }
        ))
    ));
    assert!(env.packages.get("crm").unwrap().is_none());
    assert!(!env.data_dir().join("agents/packages/crm").exists());
    assert!(env.governance.recorded.lock().unwrap().is_empty());
}

#[tokio::test]
async fn test_install_refuses_unsigned_when_governance_requires_a_signature() {
    // GIVEN governance that requires a signature, and an unsigned package
    let mut env = Env::new();
    env.governance.require_signature = true;
    let apo = archive(env.data_dir(), "crm.apo", &manifest("1.0.0", &[], ""), None);

    // WHEN it is installed
    let result = install_with(&env, &apo, &options(), None).await;

    // THEN it is refused and nothing is written
    assert!(matches!(
        result,
        Err(PackageError::Refused(InstallRefusal::SignatureRequired))
    ));
    assert!(env.packages.get("crm").unwrap().is_none());
}

#[tokio::test]
async fn test_install_asks_to_confirm_python_dependencies() {
    // GIVEN a package that installs a Python dependency
    let env = Env::new();
    let apo = archive(
        env.data_dir(),
        "crm.apo",
        &manifest("1.0.0", &[], "[pip]\npackages = [\"requests==2.32.3\"]\n"),
        None,
    );

    // WHEN it is installed without confirming them
    let result = install_with(&env, &apo, &options(), None).await;

    // THEN the confirmation is asked for, naming the dependency
    match result {
        Err(PackageError::DepsConfirmationRequired { packages }) => {
            assert_eq!(packages, vec!["requests==2.32.3".to_owned()]);
        }
        other => panic!("expected a confirmation request, got {other:?}"),
    }
    assert!(leftovers(&env).is_empty());
}

#[tokio::test]
async fn test_install_adds_the_servers_it_brings_and_links_them() {
    // GIVEN a package that brings the MCP server `ledger`
    let env = Env::new();
    let apo = archive(
        env.data_dir(),
        "crm.apo",
        &manifest(
            "1.0.0",
            &[],
            "[[mcp_servers]]\nname = \"ledger\"\ncommand = \"uvx\"\n",
        ),
        None,
    );
    let mcp = FakeMcp::default();

    // WHEN it is installed
    let report = install_with(&env, &apo, &options(), Some(&mcp))
        .await
        .unwrap();

    // THEN the server was added as brought by a package, and linked to it
    let added = mcp.added.lock().unwrap().clone();
    assert_eq!(added.len(), 1);
    assert_eq!(added[0].name, "ledger");
    assert_eq!(
        added[0].origin.map(|o| o.provenance),
        Some(Provenance::Package)
    );
    assert_eq!(report.mcp_servers_added, vec!["ledger".to_owned()]);
    assert!(env
        .packages
        .list_items("crm")
        .unwrap()
        .iter()
        .any(|i| i.kind == PackageItemKind::Connection && i.item_id == "ledger"));
}

#[tokio::test]
async fn test_install_without_mcp_refuses_a_package_that_brings_a_server() {
    // GIVEN a package that brings a server, and no MCP manager
    let env = Env::new();
    let apo = archive(
        env.data_dir(),
        "crm.apo",
        &manifest(
            "1.0.0",
            &[],
            "[[mcp_servers]]\nname = \"ledger\"\ncommand = \"uvx\"\n",
        ),
        None,
    );

    // WHEN it is installed
    let result = install_with(&env, &apo, &options(), None).await;

    // THEN it is refused before any file is put in place
    assert!(matches!(result, Err(PackageError::McpUnavailable)));
    assert!(!env.data_dir().join("agents/packages/crm").exists());
    assert!(leftovers(&env).is_empty());
}

#[tokio::test]
async fn test_a_refused_server_puts_the_previous_version_back() {
    // GIVEN version 1.0.0 installed, then 1.1.0 bringing two servers, the
    // second of which the admission refuses
    let env = Env::new();
    let v1 = archive(env.data_dir(), "v1.apo", &manifest("1.0.0", &[], ""), None);
    install_with(&env, &v1, &options(), None).await.unwrap();
    let v2 = archive(
        env.data_dir(),
        "v2.apo",
        &manifest(
            "1.1.0",
            &[],
            "[[mcp_servers]]\nname = \"ledger\"\ncommand = \"uvx\"\n\n\
             [[mcp_servers]]\nname = \"bad\"\ncommand = \"uvx\"\n",
        ),
        None,
    );
    let mcp = FakeMcp {
        refuse: Some("bad".into()),
        ..FakeMcp::default()
    };
    let confirmed = InstallOptions {
        changes_confirmed: true,
        ..options()
    };

    // WHEN the update is installed
    let result = install_with(&env, &v2, &confirmed, Some(&mcp)).await;

    // THEN it fails naming the server, the first server is removed again, and
    // version 1.0.0 is still what is installed, files included
    assert!(matches!(result, Err(PackageError::McpServer { name, .. }) if name == "bad"));
    assert_eq!(*mcp.removed.lock().unwrap(), vec!["ledger".to_owned()]);
    assert_eq!(env.packages.get("crm").unwrap().unwrap().version, "1.0.0");
    let on_disk =
        std::fs::read_to_string(env.data_dir().join("agents/packages/crm/apollia.toml")).unwrap();
    assert!(on_disk.contains("version = \"1.0.0\""));
    assert!(leftovers(&env).is_empty());
}

// ── Criterion 3: update ───────────────────────────────────────────────────

#[tokio::test]
async fn test_update_previews_what_it_adds_and_needs_it_confirmed() {
    // GIVEN version 1.0.0 installed, and 1.1.0 that grants `web_search`,
    // requires `gmail` and brings `ledger`
    let env = Env::new();
    let v1 = archive(env.data_dir(), "v1.apo", &manifest("1.0.0", &[], ""), None);
    install_with(&env, &v1, &options(), None).await.unwrap();
    let v2 = archive(
        env.data_dir(),
        "v2.apo",
        &manifest(
            "1.1.0",
            &["web_search"],
            "[[requires]]\ntype = \"gmail\"\n\n[[mcp_servers]]\nname = \"ledger\"\ncommand = \"uvx\"\n",
        ),
        None,
    );

    // WHEN it is previewed, then installed without and with confirmation
    let preview = preview(&v2, &env.sources()).unwrap();
    let mcp = FakeMcp::default();
    let unconfirmed = install_with(&env, &v2, &options(), Some(&mcp)).await;
    let confirmed = InstallOptions {
        changes_confirmed: true,
        ..options()
    };
    let report = install_with(&env, &v2, &confirmed, Some(&mcp))
        .await
        .unwrap();

    // THEN the preview carries the diff with the installed version
    assert_eq!(preview.installed_version.as_deref(), Some("1.0.0"));
    let diff = preview.update.unwrap();
    assert_eq!(diff.from_version, "1.0.0");
    assert_eq!(diff.to_version, "1.1.0");
    assert!(!diff.downgrade);
    assert_eq!(diff.added_access.len(), 1);
    assert_eq!(diff.added_access[0].agent_id, "crm-agent");
    assert_eq!(diff.added_access[0].access_id, "web_search");
    assert_eq!(diff.added_requires, vec!["gmail".to_owned()]);
    assert_eq!(diff.added_mcp_servers, vec!["ledger".to_owned()]);

    // AND the unconfirmed installation is refused, the confirmed one updates
    assert!(matches!(
        unconfirmed,
        Err(PackageError::ChangesConfirmationRequired)
    ));
    assert_eq!(report.previous_version.as_deref(), Some("1.0.0"));
    assert_eq!(env.packages.get("crm").unwrap().unwrap().version, "1.1.0");
    let recorded = env.governance.recorded.lock().unwrap().clone();
    assert!(recorded.iter().any(|e| matches!(
        e,
        GovernanceEvent::PackageUpdated { from_version, to_version, .. }
            if from_version == "1.0.0" && to_version == "1.1.0"
    )));
    assert!(recorded.iter().any(|e| matches!(
        e,
        GovernanceEvent::Updated { agent_id, .. } if agent_id == "crm-agent"
    )));
}

#[tokio::test]
async fn test_update_keeps_the_user_overrides() {
    // GIVEN version 1.0.0 installed and its agent's mission overridden
    let env = Env::new();
    let v1 = archive(env.data_dir(), "v1.apo", &manifest("1.0.0", &[], ""), None);
    install_with(&env, &v1, &options(), None).await.unwrap();
    env.agents
        .set_override(
            "crm-agent",
            OverrideField::Mission,
            &serde_json::json!("My own mission."),
        )
        .unwrap();
    let v2 = archive(
        env.data_dir(),
        "v2.apo",
        &manifest("1.2.0", &[], "").replace(
            "mission = \"Track leads.\"",
            "mission = \"Track leads, v2.\"",
        ),
        None,
    );

    // WHEN 1.2.0 is installed over it
    install_with(&env, &v2, &options(), None).await.unwrap();

    // THEN the definition is the new one and the override is still the user's
    let stored = env.agents.stored("crm-agent").unwrap().unwrap();
    assert_eq!(stored.version, "1.2.0");
    assert_eq!(stored.definition["mission"], "Track leads, v2.");
    let overrides = env.agents.overrides("crm-agent").unwrap();
    assert_eq!(overrides.len(), 1);
    assert_eq!(overrides[0].field, OverrideField::Mission);
    assert_eq!(overrides[0].value, serde_json::json!("My own mission."));
}

#[tokio::test]
async fn test_an_older_version_is_refused_unless_allowed() {
    // GIVEN version 2.0.0 installed, and 1.0.0 offered
    let env = Env::new();
    let v2 = archive(env.data_dir(), "v2.apo", &manifest("2.0.0", &[], ""), None);
    install_with(&env, &v2, &options(), None).await.unwrap();
    let v1 = archive(env.data_dir(), "v1.apo", &manifest("1.0.0", &[], ""), None);

    // WHEN 1.0.0 is installed, then installed again with the downgrade allowed
    let refused = install_with(&env, &v1, &options(), None).await;
    let allowed = InstallOptions {
        allow_downgrade: true,
        ..options()
    };
    let report = install_with(&env, &v1, &allowed, None).await.unwrap();

    // THEN the first is refused naming both versions, the second installs
    assert!(matches!(
        refused,
        Err(PackageError::Refused(InstallRefusal::Downgrade { installed, incoming }))
            if installed == "2.0.0" && incoming == "1.0.0"
    ));
    assert_eq!(report.previous_version.as_deref(), Some("2.0.0"));
    assert_eq!(env.packages.get("crm").unwrap().unwrap().version, "1.0.0");
}
