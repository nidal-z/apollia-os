//! Building an archive from a package folder, then reading it back.

use std::path::Path;

use apollia_signing::SigningKeyPair;

use super::*;

const MANIFEST: &str = r#"
[package]
name = "lone"
version = "0.1.0"
publisher = "Acme Agents"
sdk = ">=0.3"

[[tools]]
name = "ping"
module = "tools/ping.py"
description = "Answer pong."
action = "read"
inputs = { type = "object" }
"#;

const TOOL: &str = "@tool(name='ping')\nasync def ping(ctx, inputs):\n    return 'pong'\n";

/// A package folder holding the manifest and its one tool module.
fn package(dir: &Path, manifest: &str) {
    std::fs::create_dir_all(dir.join("tools")).unwrap();
    std::fs::write(dir.join("apollia.toml"), manifest).unwrap();
    std::fs::write(dir.join("tools/ping.py"), TOOL).unwrap();
}

#[test]
fn test_build_then_open_round_trip_unsigned() {
    // GIVEN a package folder with a Python cache that must stay out
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("pkg");
    package(&src, MANIFEST);
    std::fs::create_dir_all(src.join("tools/__pycache__")).unwrap();
    std::fs::write(src.join("tools/__pycache__/ping.cpython-312.pyc"), b"\0").unwrap();
    let out = dir.path().join("lone.apo");

    // WHEN it is built without a key and opened again
    let report = build_from_dir(&src, &out, &BuildOptions::default()).unwrap();
    let mut opened = ApoArchive::open(&out).unwrap();

    // THEN the archive holds the manifest first, the module, nothing else,
    // and reads back unsigned
    assert_eq!(report.signature, SignatureState::Unsigned);
    let paths: Vec<&str> = opened.entries().iter().map(|e| e.path.as_str()).collect();
    assert_eq!(paths, vec!["apollia.toml", "tools/ping.py"]);
    assert_eq!(opened.manifest().package.name, "lone");
    assert_eq!(opened.signature_state().unwrap(), SignatureState::Unsigned);
}

#[test]
fn test_build_signed_reads_back_with_publisher_and_fingerprint() {
    // GIVEN a package folder and a signing key
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("pkg");
    package(&src, MANIFEST);
    let out = dir.path().join("lone.apo");
    let key = SigningKeyPair::from_seed(&[3u8; 32]);
    let options = BuildOptions {
        signing_key: Some(&key),
        ..BuildOptions::default()
    };

    // WHEN it is built signed and opened again
    build_from_dir(&src, &out, &options).unwrap();
    let mut opened = ApoArchive::open(&out).unwrap();

    // THEN the signature holds and names the manifest publisher and the key
    assert_eq!(
        opened.signature_state().unwrap(),
        SignatureState::Signed {
            publisher: "Acme Agents".into(),
            key_fingerprint: key.public_key().fingerprint(),
        }
    );
}

#[test]
fn test_build_is_reproducible() {
    // GIVEN a package folder
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("pkg");
    package(&src, MANIFEST);
    let first = dir.path().join("a.apo");
    let second = dir.path().join("b.apo");

    // WHEN it is built twice
    build_from_dir(&src, &first, &BuildOptions::default()).unwrap();
    build_from_dir(&src, &second, &BuildOptions::default()).unwrap();

    // THEN both archives are byte for byte the same
    assert_eq!(
        std::fs::read(first).unwrap(),
        std::fs::read(second).unwrap()
    );
}

#[test]
fn test_build_refuses_invalid_manifest() {
    // GIVEN a package folder whose manifest has an unknown key
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("pkg");
    package(&src, &format!("{MANIFEST}\nsurprise = true\n"));

    // WHEN it is built
    let result = build_from_dir(&src, &dir.path().join("x.apo"), &BuildOptions::default());

    // THEN the manifest error comes back and no archive is written
    assert!(matches!(result, Err(ApoError::Manifest(_))));
    assert!(!dir.path().join("x.apo").exists());
}

#[test]
fn test_build_refuses_folder_without_manifest() {
    // GIVEN a folder without apollia.toml
    let dir = tempfile::tempdir().unwrap();

    // WHEN it is built
    let result = build_from_dir(
        dir.path(),
        &dir.path().join("x.apo"),
        &BuildOptions::default(),
    );

    // THEN it is refused for its missing manifest
    assert!(matches!(result, Err(ApoError::MissingManifest)));
}

#[test]
fn test_build_refuses_undescribed_tool_module() {
    // GIVEN a package whose second module defines a workflow the manifest
    // does not describe
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("pkg");
    package(&src, MANIFEST);
    std::fs::write(
        src.join("tools/report.py"),
        "from apollia import workflow\n\n@workflow(name='report', steps=['a'])\nasync def report(ctx, inputs):\n    pass\n",
    )
    .unwrap();

    // WHEN it is built
    let result = build_from_dir(&src, &dir.path().join("x.apo"), &BuildOptions::default());

    // THEN it is refused, naming the module that needs descriptors
    assert!(matches!(
        result,
        Err(ApoError::MissingDescriptors { modules }) if modules == vec!["tools/report.py".to_string()]
    ));
}

#[test]
fn test_build_refuses_descriptor_without_input_schema() {
    // GIVEN a manifest whose tool descriptor lacks the input schema the SDK
    // always writes
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("pkg");
    package(
        &src,
        &MANIFEST.replace("inputs = { type = \"object\" }\n", ""),
    );

    // WHEN it is built
    let result = build_from_dir(&src, &dir.path().join("x.apo"), &BuildOptions::default());

    // THEN it is refused as missing descriptors
    assert!(matches!(result, Err(ApoError::MissingDescriptors { .. })));
}

#[test]
fn test_build_refuses_declared_module_absent_from_folder() {
    // GIVEN a manifest naming a module the folder does not hold
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("pkg");
    package(&src, MANIFEST);
    std::fs::remove_file(src.join("tools/ping.py")).unwrap();

    // WHEN it is built
    let result = build_from_dir(&src, &dir.path().join("x.apo"), &BuildOptions::default());

    // THEN it is refused, naming the module
    assert!(matches!(
        result,
        Err(ApoError::MissingModule { module }) if module == "tools/ping.py"
    ));
}

#[cfg(unix)]
#[test]
fn test_build_refuses_symlink_in_folder() {
    // GIVEN a package folder holding a symbolic link
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("pkg");
    package(&src, MANIFEST);
    std::os::unix::fs::symlink("/etc/hosts", src.join("hosts")).unwrap();

    // WHEN it is built
    let result = build_from_dir(&src, &dir.path().join("x.apo"), &BuildOptions::default());

    // THEN it is refused
    assert!(matches!(result, Err(ApoError::Symlink { path }) if path == "hosts"));
}

#[test]
fn test_build_refuses_reserved_signature_name() {
    // GIVEN a package folder that already holds a SIGNATURE file
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("pkg");
    package(&src, MANIFEST);
    std::fs::write(src.join("SIGNATURE"), "forged\n").unwrap();

    // WHEN it is built
    let result = build_from_dir(&src, &dir.path().join("x.apo"), &BuildOptions::default());

    // THEN it is refused
    assert!(matches!(result, Err(ApoError::ReservedName { .. })));
}

#[test]
fn test_build_skips_its_own_output_inside_the_folder() {
    // GIVEN a package folder that already holds a previous build of itself
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("pkg");
    package(&src, MANIFEST);
    let out = src.join("lone.apo");
    build_from_dir(&src, &out, &BuildOptions::default()).unwrap();

    // WHEN it is built again to the same path
    let report = build_from_dir(&src, &out, &BuildOptions::default()).unwrap();

    // THEN the previous archive is not packed into the new one
    assert!(report.entries.iter().all(|e| e.path != "lone.apo"));
}
