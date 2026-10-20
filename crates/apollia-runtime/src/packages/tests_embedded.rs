//! Installing a package Apollia ships inside its binary.

use apollia_tools::apo_archive::{build_from_dir, BuildOptions};
use apollia_tools::OverrideField;

use super::tests::{archive, manifest, Env};
use super::*;
use crate::governance::GovernanceEvent;

#[tokio::test]
async fn test_an_embedded_package_installs_unsigned_where_a_signature_is_required() {
    // GIVEN governance that requires a signature, and an unsigned package the
    // binary carries
    let mut env = Env::new();
    env.governance.require_signature = true;
    let apo = archive(env.data_dir(), "crm.apo", &manifest("1.0.0", &[], ""), None);

    // WHEN it is installed as embedded
    let outcome = install_embedded(&apo, env.data_dir(), &env.governance)
        .await
        .unwrap();

    // THEN it is installed with no notice about its signature, and recorded
    let EmbeddedOutcome::Installed(report) = outcome else {
        panic!("expected an installation, got {outcome:?}");
    };
    assert_eq!(report.version, "1.0.0");
    assert!(!report.notices.contains(&PackageNotice::Unsigned));
    assert_eq!(env.packages.get("crm").unwrap().unwrap().version, "1.0.0");
    assert!(env
        .governance
        .recorded
        .lock()
        .unwrap()
        .iter()
        .any(|e| matches!(e, GovernanceEvent::PackageInstalled { actor, signed: false, .. } if actor == EMBEDDED_ACTOR)));
}

#[tokio::test]
async fn test_the_same_file_from_elsewhere_is_still_refused() {
    // GIVEN governance that requires a signature, and the same unsigned package
    let mut env = Env::new();
    env.governance.require_signature = true;
    let apo = archive(env.data_dir(), "crm.apo", &manifest("1.0.0", &[], ""), None);

    // WHEN it is previewed as a file, the origin of anything a user hands over
    let preview = preview(&apo, &env.sources()).unwrap();

    // THEN nothing in the archive makes it embedded: it is refused and flagged
    assert_eq!(preview.refusal, Some(InstallRefusal::SignatureRequired));
    assert!(preview.notices.contains(&PackageNotice::Unsigned));
}

#[tokio::test]
async fn test_the_installed_version_is_left_alone() {
    // GIVEN the embedded package installed at the version the binary carries
    let env = Env::new();
    let apo = archive(env.data_dir(), "crm.apo", &manifest("1.0.0", &[], ""), None);
    install_embedded(&apo, env.data_dir(), &env.governance)
        .await
        .unwrap();
    let recorded = env.governance.recorded.lock().unwrap().len();

    // WHEN the next start offers the same version
    let outcome = install_embedded(&apo, env.data_dir(), &env.governance)
        .await
        .unwrap();

    // THEN nothing is installed and nothing more is recorded
    assert_eq!(
        outcome,
        EmbeddedOutcome::Current {
            version: "1.0.0".into()
        }
    );
    assert_eq!(env.governance.recorded.lock().unwrap().len(), recorded);
}

#[tokio::test]
async fn test_the_same_version_with_another_content_is_installed_again_keeping_the_user_state() {
    // GIVEN 1.0.0 installed, its agent's mission overridden and the agent
    // disabled by the user, and a binary that carries 1.0.0 again with the
    // code of its tool fixed and its manifest unchanged
    let env = Env::new();
    let first = archive(env.data_dir(), "crm.apo", &manifest("1.0.0", &[], ""), None);
    install_embedded(&first, env.data_dir(), &env.governance)
        .await
        .unwrap();
    env.agents
        .set_override(
            "crm-agent",
            OverrideField::Mission,
            &serde_json::json!("My own mission."),
        )
        .unwrap();
    env.agents.set_enabled("crm-agent", false).unwrap();
    let fixed = archive(
        env.data_dir(),
        "fixed.apo",
        &manifest("1.0.0", &[], ""),
        None,
    );
    let fixed_src = env.data_dir().join("src-fixed.apo");
    let fixed_code = "@tool(name='lookup')\nasync def lookup(ctx, inputs):\n    return {}\n";
    std::fs::write(fixed_src.join("tools/lookup.py"), fixed_code).unwrap();
    build_from_dir(&fixed_src, &fixed, &BuildOptions::default()).unwrap();

    // WHEN the binary starts, then starts again
    let outcome = install_embedded(&fixed, env.data_dir(), &env.governance)
        .await
        .unwrap();
    let again = install_embedded(&fixed, env.data_dir(), &env.governance)
        .await
        .unwrap();

    // THEN the fixed code is installed and recorded as an update, the
    // override stays and the agent is still disabled, and the next start
    // installs nothing
    assert!(
        matches!(outcome, EmbeddedOutcome::Installed(ref r) if r.previous_version.as_deref() == Some("1.0.0")),
        "{outcome:?}"
    );
    let root = env.packages.get("crm").unwrap().unwrap().root_path;
    assert_eq!(
        std::fs::read_to_string(root.join("tools/lookup.py")).unwrap(),
        fixed_code
    );
    assert!(env
        .governance
        .recorded
        .lock()
        .unwrap()
        .iter()
        .any(|e| matches!(
            e,
            GovernanceEvent::PackageUpdated { from_version, to_version, .. }
                if from_version == "1.0.0" && to_version == "1.0.0"
        )));
    let stored = env.agents.stored("crm-agent").unwrap().unwrap();
    assert!(
        !stored.enabled,
        "a reinstallation never enables a disabled agent"
    );
    let overrides = env.agents.overrides("crm-agent").unwrap();
    assert_eq!(overrides.len(), 1);
    assert_eq!(overrides[0].value, serde_json::json!("My own mission."));
    assert!(env.packages.is_embedded("crm").unwrap());
    assert_eq!(
        again,
        EmbeddedOutcome::Current {
            version: "1.0.0".into()
        }
    );
}

#[tokio::test]
async fn test_an_installation_without_a_digest_is_installed_again_once() {
    // GIVEN the embedded package installed by a binary that recorded no
    // content digest
    let env = Env::new();
    let apo = archive(env.data_dir(), "crm.apo", &manifest("1.0.0", &[], ""), None);
    install_embedded(&apo, env.data_dir(), &env.governance)
        .await
        .unwrap();
    let installed = env.packages.get("crm").unwrap().unwrap();
    env.packages.save(&installed).unwrap();
    assert_eq!(env.packages.content_digest("crm").unwrap(), None);

    // WHEN the binary starts with the same version, then starts again
    let first = install_embedded(&apo, env.data_dir(), &env.governance)
        .await
        .unwrap();
    let second = install_embedded(&apo, env.data_dir(), &env.governance)
        .await
        .unwrap();

    // THEN the first start installs it and records its digest, and the
    // second leaves it alone
    assert!(matches!(first, EmbeddedOutcome::Installed(_)), "{first:?}");
    assert!(env.packages.content_digest("crm").unwrap().is_some());
    assert!(matches!(second, EmbeddedOutcome::Current { .. }));
}

#[tokio::test]
async fn test_a_newer_installed_version_is_never_downgraded() {
    // GIVEN version 2.0.0 installed, and a binary that carries 1.0.0
    let env = Env::new();
    let v2 = archive(env.data_dir(), "v2.apo", &manifest("2.0.0", &[], ""), None);
    install_embedded(&v2, env.data_dir(), &env.governance)
        .await
        .unwrap();
    let v1 = archive(env.data_dir(), "v1.apo", &manifest("1.0.0", &[], ""), None);

    // WHEN the embedded 1.0.0 is installed
    let outcome = install_embedded(&v1, env.data_dir(), &env.governance)
        .await
        .unwrap();

    // THEN 2.0.0 stays
    assert_eq!(
        outcome,
        EmbeddedOutcome::Newer {
            installed: "2.0.0".into(),
            bundled: "1.0.0".into()
        }
    );
    assert_eq!(env.packages.get("crm").unwrap().unwrap().version, "2.0.0");
    assert_eq!(
        env.agents.stored("crm-agent").unwrap().unwrap().version,
        "2.0.0"
    );
}

#[tokio::test]
async fn test_a_new_embedded_version_keeps_the_overrides_and_the_disabled_agent() {
    // GIVEN 1.0.0 installed, its agent's mission overridden and the agent
    // disabled by the user
    let env = Env::new();
    let v1 = archive(env.data_dir(), "v1.apo", &manifest("1.0.0", &[], ""), None);
    install_embedded(&v1, env.data_dir(), &env.governance)
        .await
        .unwrap();
    env.agents
        .set_override(
            "crm-agent",
            OverrideField::Mission,
            &serde_json::json!("My own mission."),
        )
        .unwrap();
    env.agents.set_enabled("crm-agent", false).unwrap();
    let v2 = archive(
        env.data_dir(),
        "v2.apo",
        &manifest("1.1.0", &[], "").replace(
            "mission = \"Track leads.\"",
            "mission = \"Track leads, v2.\"",
        ),
        None,
    );

    // WHEN a later binary carries 1.1.0
    let outcome = install_embedded(&v2, env.data_dir(), &env.governance)
        .await
        .unwrap();

    // THEN the definition is replaced, the override stays, and the agent is
    // still disabled
    assert!(
        matches!(outcome, EmbeddedOutcome::Installed(ref r) if r.previous_version.as_deref() == Some("1.0.0"))
    );
    let stored = env.agents.stored("crm-agent").unwrap().unwrap();
    assert_eq!(stored.version, "1.1.0");
    assert_eq!(stored.definition["mission"], "Track leads, v2.");
    assert!(!stored.enabled, "an update never enables a disabled agent");
    let overrides = env.agents.overrides("crm-agent").unwrap();
    assert_eq!(overrides.len(), 1);
    assert_eq!(overrides[0].value, serde_json::json!("My own mission."));
}

#[tokio::test]
async fn test_an_embedded_package_is_marked_and_a_file_is_not() {
    // GIVEN one package the binary carries and one a user hands over
    let env = Env::new();
    let carried = archive(env.data_dir(), "crm.apo", &manifest("1.0.0", &[], ""), None);
    let handed = archive(
        env.data_dir(),
        "desk.apo",
        &manifest("1.0.0", &[], "").replace("crm", "desk"),
        None,
    );

    // WHEN the first is installed as embedded and the second as a file
    install_embedded(&carried, env.data_dir(), &env.governance)
        .await
        .unwrap();
    super::tests_install::install_with(&env, &handed, &super::tests_install::options(), None)
        .await
        .unwrap();

    // THEN only the first is recorded as shipped inside the binary
    assert!(env.packages.is_embedded("crm").unwrap());
    assert!(!env.packages.is_embedded("desk").unwrap());
}

#[tokio::test]
async fn test_an_installation_older_than_the_mark_is_marked_at_the_next_start() {
    // GIVEN the embedded package installed, its mark lost as before it existed
    let env = Env::new();
    let apo = archive(env.data_dir(), "crm.apo", &manifest("1.0.0", &[], ""), None);
    install_embedded(&apo, env.data_dir(), &env.governance)
        .await
        .unwrap();
    env.packages.set_embedded("crm", false).unwrap();

    // WHEN the binary starts again with the same version
    let outcome = install_embedded(&apo, env.data_dir(), &env.governance)
        .await
        .unwrap();

    // THEN nothing is installed and the package is marked
    assert!(matches!(outcome, EmbeddedOutcome::Current { .. }));
    assert!(env.packages.is_embedded("crm").unwrap());
}

#[tokio::test]
async fn test_an_embedded_package_is_never_uninstalled() {
    // GIVEN a package the binary carries, installed
    let env = Env::new();
    let apo = archive(env.data_dir(), "crm.apo", &manifest("1.0.0", &[], ""), None);
    install_embedded(&apo, env.data_dir(), &env.governance)
        .await
        .unwrap();

    // WHEN its uninstallation is asked
    let targets = UninstallTargets::<super::tests_install::FakeMcp> {
        packages: &env.packages,
        agents: &env.agents,
        governance: &env.governance,
        mcp: None,
        triggers: None,
        models: None,
        data_dir: env.data_dir(),
    };
    let result = uninstall_package("crm", UninstallOptions::default(), &targets, "test").await;

    // THEN it is refused as part of Apollia, and the package stays
    assert!(matches!(result, Err(UninstallError::BuiltIn(name)) if name == "crm"));
    assert!(env.packages.get("crm").unwrap().is_some());
}
