//! The example packages the binary proposes: embedded by the build, listed
//! without being installed, installed by choice.

use apollia_tools::apo_archive::{ApoArchive, SignatureState};

use super::examples::{shipped_examples, FIXTURE};
use super::tests::{connection, Env};
use super::tests_install::install_with;
use super::*;
use crate::connections::ConnectionKind;

/// The package of `tests/fixtures/example-packages/`.
const SAMPLE: &str = "sample-watch";

#[test]
fn test_the_build_embeds_an_example_that_reads_back_as_an_archive() {
    // GIVEN the package folder of the fixtures, embedded by the build script
    let env = Env::new();
    let (example, manifest) = find_example(FIXTURE, SAMPLE).unwrap();
    assert_eq!(manifest.package.version, "1.2.0");

    // WHEN it is assembled into an archive
    let apo = example.assemble(env.data_dir()).unwrap();

    // THEN the archive reads back: its manifest, its data file, no signature,
    // and nothing of the assembly is left beside it
    let mut archive = ApoArchive::open(&apo).unwrap();
    assert_eq!(archive.manifest().package.name, SAMPLE);
    assert_eq!(archive.manifest().agents[0].name, "Sample watch");
    assert!(archive
        .entries()
        .iter()
        .any(|entry| entry.path == "notes/readme.txt"));
    assert_eq!(archive.signature_state().unwrap(), SignatureState::Unsigned);
    let beside: Vec<String> = std::fs::read_dir(apo.parent().unwrap())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(beside, ["sample-watch.apo"]);
}

#[test]
fn test_every_shipped_example_is_a_declarative_package_that_assembles() {
    // GIVEN the examples the binary ships
    let env = Env::new();

    for example in shipped_examples() {
        // WHEN each is read and assembled
        let manifest = example.manifest().unwrap();
        let apo = example.assemble(env.data_dir()).unwrap();

        // THEN it is named after its folder, has an agent and no code, and
        // its archive reads back
        let folder = example.folder;
        assert_eq!(manifest.package.name, folder);
        assert!(!manifest.agents.is_empty(), "{folder}: no agent");
        assert!(manifest.tools.is_empty(), "{folder}: a tool is code");
        assert!(
            manifest.workflows.is_empty(),
            "{folder}: a workflow is code"
        );
        assert!(manifest.pip.is_none(), "{folder}: a Python requirement");
        assert_eq!(
            ApoArchive::open(&apo).unwrap().manifest().package.name,
            folder
        );
    }
}

#[test]
fn test_an_example_is_proposed_and_not_installed() {
    // GIVEN a fresh data directory
    let env = Env::new();

    // WHEN the examples are listed
    let proposed = list_examples(FIXTURE, &env.packages, &env.connections).unwrap();

    // THEN the example is there with its mission and the connection it
    // requires, missing, and nothing is installed
    assert_eq!(proposed.len(), 1);
    let example = &proposed[0];
    assert_eq!(example.name, SAMPLE);
    assert!(example.agents[0].mission.starts_with("Watch the topic"));
    assert_eq!(example.requires[0].kind, "imap");
    assert!(!example.requires[0].present);
    assert_eq!(example.installed_version, None);
    assert!(env.packages.list().unwrap().is_empty());
    let stored = env.agents.list_stored().unwrap();
    let names: Vec<&str> = stored.iter().map(|agent| agent.name.as_str()).collect();
    assert_eq!(names, [apollia_tools::APOLLIA_AGENT_ID]);
}

#[test]
fn test_a_required_connection_this_machine_has_is_reported_present() {
    // GIVEN a mailbox on this machine
    let mut env = Env::new();
    env.connections
        .push(connection("imap:work", ConnectionKind::Imap));

    // WHEN the examples are listed
    let proposed = list_examples(FIXTURE, &env.packages, &env.connections).unwrap();

    // THEN the mailbox the example requires is present
    assert!(proposed[0].requires[0].present);
}

#[tokio::test]
async fn test_an_example_installs_unsigned_where_a_signature_is_required_and_is_not_built_in() {
    // GIVEN governance that requires a signature
    let mut env = Env::new();
    env.governance.require_signature = true;
    let (example, manifest) = find_example(FIXTURE, SAMPLE).unwrap();

    // WHEN the example is previewed, then installed by choice
    let preview = preview_example(&manifest, &env.sources()).unwrap();
    let apo = example.assemble(env.data_dir()).unwrap();
    let options = InstallOptions {
        actor: "test".into(),
        origin: PackageOrigin::Example,
        ..InstallOptions::default()
    };
    let report = install_with(&env, &apo, &options, None).await.unwrap();

    // THEN it is neither flagged nor refused for its signature, it is
    // installed, listed as such, and stays a package the user can uninstall
    assert_eq!(preview.refusal, None);
    assert!(!preview.notices.contains(&PackageNotice::Unsigned));
    assert!(!report.notices.contains(&PackageNotice::Unsigned));
    assert!(!env.packages.is_embedded(SAMPLE).unwrap());
    let proposed = list_examples(FIXTURE, &env.packages, &env.connections).unwrap();
    assert_eq!(proposed[0].installed_version.as_deref(), Some("1.2.0"));
}

#[test]
fn test_the_same_archive_handed_over_as_a_file_is_still_refused() {
    // GIVEN governance that requires a signature, and the assembled example
    let mut env = Env::new();
    env.governance.require_signature = true;
    let (example, _) = find_example(FIXTURE, SAMPLE).unwrap();
    let apo = example.assemble(env.data_dir()).unwrap();

    // WHEN the archive is previewed as a file a user hands over
    let preview = preview(&apo, &env.sources()).unwrap();

    // THEN nothing in the file makes it an example: it is refused and flagged
    assert_eq!(preview.refusal, Some(InstallRefusal::SignatureRequired));
    assert!(preview.notices.contains(&PackageNotice::Unsigned));
}

#[test]
fn test_an_unknown_name_is_no_example() {
    // GIVEN the examples of the fixtures
    // WHEN a name none of them carries is looked up
    let found = find_example(FIXTURE, "ghost");

    // THEN there is none
    assert!(found.is_none());
}
