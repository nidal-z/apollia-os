//! Installing a package Apollia ships inside its binary.

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
