#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::*;

/// Lay out a Node.js release under `root` as the engine store installs it,
/// with the upstream folder of a macOS or Linux release, and return the path
/// of its `node`.
fn install(root: &Path, tag: &str, recorded: bool) -> PathBuf {
    let dir = root.join(NODE_ENGINE_DIR).join(tag).join(NODE_VARIANT_DIR);
    let prefix = dir.join(format!("node-{tag}-linux-x64"));
    let bin = prefix.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let node = bin.join(if cfg!(windows) { "node.exe" } else { "node" });
    std::fs::write(&node, b"").unwrap();
    if cfg!(windows) {
        let cli = bin.join(NPX_CLI);
        std::fs::create_dir_all(cli.parent().unwrap()).unwrap();
        std::fs::write(cli, b"").unwrap();
    } else {
        let cli = prefix.join("lib").join(NPX_CLI);
        std::fs::create_dir_all(cli.parent().unwrap()).unwrap();
        std::fs::write(cli, b"").unwrap();
    }
    if recorded {
        std::fs::write(dir.join(RECORD_FILE), b"{}").unwrap();
    }
    node
}

#[test]
fn test_only_bare_node_and_npx_are_resolved() {
    // GIVEN commands of every spelling, and no store
    // WHEN each is resolved
    // THEN only the bare names ask for Node.js, whatever their case or
    //      Windows suffix; the others are left as configured
    for yes in ["node", "npx", "NPX", "npx.cmd", "node.exe"] {
        assert!(
            matches!(
                resolve(yes, &[], None),
                Err(TransportError::NodeRequired { .. })
            ),
            "{yes}"
        );
    }
    for no in [
        "uvx",
        "python3",
        "/usr/local/bin/node",
        "C:\\node\\npx.cmd",
        "nodemon",
    ] {
        assert!(matches!(resolve(no, &[], None), Ok(None)), "{no}");
    }
}

#[test]
fn test_npx_resolves_to_the_store_node_by_absolute_path() {
    // GIVEN a store holding a verified Node.js release
    let dir = tempfile::tempdir().unwrap();
    let node = install(dir.path(), "v22.11.0", true);
    let args = vec!["-y".to_owned(), "@scope/server".to_owned()];

    // WHEN `npx` is resolved
    let launch = resolve("npx", &args, Some(dir.path())).unwrap().unwrap();

    // THEN the store's `node` runs `npx-cli.js` with the configured arguments
    assert_eq!(launch.program, node);
    assert!(launch.program.is_absolute());
    assert!(launch.args[0].ends_with("npx-cli.js"), "{:?}", launch.args);
    assert!(Path::new(&launch.args[0]).is_file());
    assert_eq!(&launch.args[1..], &args[..]);
    assert_eq!(launch.bin_dir, node.parent().unwrap());
}

#[test]
fn test_node_resolves_with_its_arguments_unchanged() {
    // GIVEN a store holding a verified Node.js release
    let dir = tempfile::tempdir().unwrap();
    let node = install(dir.path(), "v22.11.0", true);
    let args = vec!["server.js".to_owned()];

    // WHEN `node` is resolved
    let launch = resolve("node", &args, Some(dir.path())).unwrap().unwrap();

    // THEN the store's binary is the program and the arguments are untouched
    assert_eq!(launch.program, node);
    assert_eq!(launch.args, args);
}

#[test]
fn test_the_newest_release_is_taken_by_its_numbers() {
    // GIVEN two releases whose byte order differs from their version order
    let dir = tempfile::tempdir().unwrap();
    install(dir.path(), "v22.9.0", true);
    let newest = install(dir.path(), "v22.11.0", true);

    // WHEN the installed Node.js is read
    let found = installed_node(dir.path()).unwrap();

    // THEN v22.11.0 wins over v22.9.0
    assert_eq!(found.node, newest);
}

#[test]
fn test_a_release_without_its_record_is_not_an_install() {
    // GIVEN a release extracted but never recorded as verified
    let dir = tempfile::tempdir().unwrap();
    install(dir.path(), "v22.11.0", false);

    // WHEN `npx` is resolved
    let err = resolve("npx", &[], Some(dir.path())).unwrap_err();

    // THEN Node.js is required, the unverified files are not run
    assert!(matches!(err, TransportError::NodeRequired { ref command } if command == "npx"));
}

#[test]
fn test_no_store_means_node_required_never_the_path() {
    // GIVEN an empty store, and a `node` that may well sit on this machine's PATH
    let dir = tempfile::tempdir().unwrap();

    // WHEN `node` is resolved, with and without a store root
    let empty = resolve("node", &[], Some(dir.path())).unwrap_err();
    let none = resolve("node", &[], None).unwrap_err();

    // THEN both are the typed refusal, readable as such
    for err in [empty, none] {
        assert!(matches!(err, TransportError::NodeRequired { .. }));
        assert!(err.to_string().contains("requires Node.js"), "{err}");
    }
}

#[test]
fn test_other_commands_are_left_alone() {
    // GIVEN an empty store
    let dir = tempfile::tempdir().unwrap();

    // WHEN a command that is not Node.js is resolved
    let launch = resolve("uvx", &["server".to_owned()], Some(dir.path())).unwrap();

    // THEN nothing is rewritten
    assert!(launch.is_none());
}

#[test]
fn test_node_dir_goes_first_on_the_child_path() {
    // GIVEN a configuration that sets its own PATH
    let mut envs = HashMap::new();
    envs.insert("PATH".to_owned(), "/opt/tools".to_owned());

    // WHEN the Node.js directory is added
    let out = with_node_on_path(envs, Path::new("/store/node/bin"));

    // THEN it comes first, the configured entries after it
    let dirs: Vec<PathBuf> = std::env::split_paths(&out["PATH"]).collect();
    assert_eq!(
        dirs,
        vec![
            PathBuf::from("/store/node/bin"),
            PathBuf::from("/opt/tools")
        ]
    );
}
