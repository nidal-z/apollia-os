#![allow(clippy::unwrap_used, clippy::expect_used)]
#![cfg_attr(not(unix), allow(unused_imports, dead_code))]

use std::path::{Path, PathBuf};

use super::*;
use crate::transport::node::{resolve_or_machine, NODE_ENGINE_DIR, NODE_VARIANT_DIR, RECORD_FILE};

/// Lay out a Node.js release under `prefix` the way its upstream archive
/// does (`bin/node`, npm under `lib/node_modules`), with a `node` that prints
/// `version` and writes the arguments it was given to `<prefix>/calls`.
/// `with_npm` false leaves npm out. Returns the path of `node`.
#[cfg(unix)]
fn fake_release(prefix: &Path, version: &str, with_npm: bool) -> PathBuf {
    fake_release_doing(prefix, &format!("echo {version}"), with_npm)
}

/// [`fake_release`] with a `node` that runs `answer` after logging its
/// arguments, rather than printing a release.
#[cfg(unix)]
fn fake_release_doing(prefix: &Path, answer: &str, with_npm: bool) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let bin = prefix.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let node = bin.join("node");
    let calls = prefix.join("calls");
    std::fs::write(
        &node,
        format!(
            "#!/bin/sh\necho \"$@\" >> '{}'\n{answer}\n",
            calls.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&node, std::fs::Permissions::from_mode(0o755)).unwrap();
    if with_npm {
        let npm = prefix.join("lib/node_modules/npm/bin");
        std::fs::create_dir_all(&npm).unwrap();
        std::fs::write(npm.join("npx-cli.js"), b"").unwrap();
        std::fs::write(npm.join("npm-cli.js"), b"").unwrap();
    }
    node
}

fn search_path(dirs: &[&Path]) -> MachineNodeSearch {
    let path = std::env::join_paths(dirs).unwrap();
    MachineNodeSearch::new(Some(path), None, Vec::new())
}

#[cfg(unix)]
#[test]
fn test_a_node_on_the_path_with_its_npm_is_found_by_running_only_its_version() {
    // GIVEN a Node.js 22 with its npm, on the PATH of the search
    let dir = tempfile::tempdir().unwrap();
    let prefix = dir.path().join("node22");
    let node = fake_release(&prefix, "v22.11.0", true);
    let search = search_path(&[&prefix.join("bin")]);

    // WHEN the machine is searched
    let found = search.find().unwrap();

    // THEN that `node` is kept with the npm beside it, and the only thing
    //      it was asked is its version
    assert_eq!(found.install.node, node);
    assert_eq!(found.version, "v22.11.0");
    let cli = found.install.npx_cli.unwrap();
    assert!(
        cli.ends_with("lib/node_modules/npm/bin/npx-cli.js"),
        "{cli:?}"
    );
    let calls = std::fs::read_to_string(prefix.join("calls")).unwrap();
    assert_eq!(calls, "--version\n");
}

#[cfg(unix)]
#[test]
fn test_a_node_older_than_the_supported_major_is_refused_and_said_so() {
    // GIVEN only a Node.js 18 on the PATH
    let dir = tempfile::tempdir().unwrap();
    let prefix = dir.path().join("node18");
    let node = fake_release(&prefix, "v18.19.0", true);
    let search = search_path(&[&prefix.join("bin")]);

    // WHEN the machine is searched
    let missing = search.find().unwrap_err();

    // THEN nothing is accepted, and the refusal names that node and its age
    assert_eq!(missing.rejected.len(), 1);
    assert_eq!(missing.rejected[0].node, node);
    assert!(missing.rejected[0].reason.contains("v18.19.0"), "{missing}");
    assert!(
        missing.to_string().contains("older than the 22"),
        "{missing}"
    );
}

#[cfg(unix)]
#[test]
fn test_an_old_node_on_the_path_gives_way_to_a_newer_one_of_a_version_manager() {
    // GIVEN a Node.js 18 on the PATH, and nvm holding 20 and 22 under home
    let dir = tempfile::tempdir().unwrap();
    let old = dir.path().join("old");
    fake_release(&old, "v18.19.0", true);
    let home = dir.path().join("home");
    let nvm = home.join(".nvm/versions/node");
    fake_release(&nvm.join("v20.10.0"), "v20.10.0", true);
    let newest = fake_release(&nvm.join("v22.11.0"), "v22.11.0", true);
    let path = std::env::join_paths([old.join("bin")]).unwrap();
    let search = MachineNodeSearch::new(Some(path), Some(home), Vec::new());

    // WHEN the machine is searched
    let found = search.find().unwrap();

    // THEN the newest release of nvm is kept, the one Apollia accepts
    assert_eq!(found.install.node, newest);
    assert_eq!(found.version, "v22.11.0");
}

#[cfg(unix)]
#[test]
fn test_an_install_directory_is_searched_after_the_path() {
    // GIVEN an empty PATH, and a Node.js 24 in an install directory
    let dir = tempfile::tempdir().unwrap();
    let prefix = dir.path().join("homebrew");
    let node = fake_release(&prefix, "v24.1.0", true);
    let search = MachineNodeSearch::new(None, None, vec![prefix.join("bin")]);

    // WHEN the machine is searched
    let found = search.find().unwrap();

    // THEN the install directory's node is kept
    assert_eq!(found.install.node, node);
}

#[cfg(unix)]
#[test]
fn test_a_node_without_npm_is_refused() {
    // GIVEN a Node.js 22 whose npm is missing
    let dir = tempfile::tempdir().unwrap();
    let prefix = dir.path().join("bare");
    fake_release(&prefix, "v22.11.0", false);
    let search = search_path(&[&prefix.join("bin")]);

    // WHEN the machine is searched
    let missing = search.find().unwrap_err();

    // THEN it is left out for its missing npm, and never run
    assert_eq!(missing.rejected[0].reason, "no npm beside it");
    assert!(!prefix.join("calls").exists());
}

#[test]
fn test_no_node_at_all_says_none_was_found() {
    // GIVEN a search over empty directories
    let dir = tempfile::tempdir().unwrap();
    let search = MachineNodeSearch::new(
        Some(dir.path().as_os_str().to_owned()),
        Some(dir.path().to_path_buf()),
        vec![dir.path().to_path_buf()],
    );

    // WHEN the machine is searched
    let missing = search.find().unwrap_err();

    // THEN nothing was seen, and the message says so
    assert!(missing.rejected.is_empty());
    assert_eq!(missing.to_string(), "no Node.js was found on this machine");
}

#[cfg(unix)]
#[test]
fn test_npx_falls_back_on_the_machine_node_when_the_store_is_empty() {
    // GIVEN an empty engine store, and a Node.js 22 on the machine
    let store = tempfile::tempdir().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let prefix = dir.path().join("node22");
    let node = fake_release(&prefix, "v22.11.0", true);
    let search = search_path(&[&prefix.join("bin")]);

    // WHEN `npx` is resolved with the machine as fallback
    let launch = resolve_or_machine("npx", &[], Some(store.path()), Some(&search))
        .unwrap()
        .unwrap();

    // THEN the machine's node runs the npx of the same release
    assert_eq!(launch.program, node);
    assert!(launch.args[0].starts_with(prefix.to_str().unwrap()));
    assert!(launch.args[0].ends_with("npx-cli.js"));
    assert_eq!(launch.bin_dir, prefix.join("bin"));
}

#[cfg(unix)]
#[test]
fn test_the_store_node_is_preferred_to_the_machine_one() {
    // GIVEN a store holding a verified Node.js, and another on the machine
    let store = tempfile::tempdir().unwrap();
    let release = store
        .path()
        .join(NODE_ENGINE_DIR)
        .join("v22.11.0")
        .join(NODE_VARIANT_DIR);
    let stored = fake_release(&release.join("node-v22.11.0-linux-x64"), "v22.11.0", true);
    std::fs::write(release.join(RECORD_FILE), b"{}").unwrap();
    let dir = tempfile::tempdir().unwrap();
    let machine = dir.path().join("machine");
    fake_release(&machine, "v24.1.0", true);
    let search = search_path(&[&machine.join("bin")]);

    // WHEN `node` is resolved
    let launch = resolve_or_machine("node", &[], Some(store.path()), Some(&search))
        .unwrap()
        .unwrap();

    // THEN the store's node runs, and the machine's is not even asked
    assert_eq!(launch.program, stored);
    assert!(!machine.join("calls").exists());
}

#[test]
fn test_a_relative_or_empty_path_entry_is_never_searched() {
    // GIVEN a PATH whose entries are relative, empty (the current directory)
    //       and absolute
    let dir = tempfile::tempdir().unwrap();
    let absolute = dir.path().join("bin");
    let path = std::env::join_paths([
        PathBuf::from("node_modules/.bin"),
        PathBuf::new(),
        PathBuf::from("."),
        absolute.clone(),
    ])
    .unwrap();
    let search = MachineNodeSearch::new(Some(path), None, Vec::new());

    // WHEN the directories to search are listed
    let dirs = search.dirs();

    // THEN only the absolute entry is kept: a `node` planted in the current
    //      directory or below it is never run
    assert_eq!(dirs, vec![absolute]);
}

#[cfg(unix)]
#[test]
fn test_a_keg_only_homebrew_node_is_found_newest_major_first() {
    // GIVEN an empty PATH, and a Homebrew prefix holding node@20, node@22
    //       and node@24 as keg-only formulas, unlinked from its `bin`
    let dir = tempfile::tempdir().unwrap();
    let opt = dir.path().join("opt");
    fake_release(&opt.join("node@20"), "v20.10.0", true);
    fake_release(&opt.join("node@22"), "v22.11.0", true);
    let newest = fake_release(&opt.join("node@24"), "v24.1.0", true);
    std::fs::create_dir_all(opt.join("python@3.13/bin")).unwrap();
    let search = MachineNodeSearch::new(None, None, Vec::new()).with_keg_roots(vec![opt.clone()]);

    // WHEN the machine is searched
    let found = search.find().unwrap();

    // THEN the newest supported formula is kept, and the older ones not run
    assert_eq!(found.install.node, newest);
    assert!(!opt.join("node@22/calls").exists());
    assert!(!opt.join("node@20/calls").exists());
}

#[cfg(unix)]
#[test]
fn test_the_search_of_the_process_looks_in_both_homebrew_prefixes() {
    // GIVEN the search of this process
    let search = MachineNodeSearch::from_env();

    // WHEN its keg roots are read
    // THEN both the Apple silicon and the Intel Homebrew prefixes are there
    assert_eq!(
        search.keg_roots,
        vec![
            PathBuf::from("/opt/homebrew/opt"),
            PathBuf::from("/usr/local/opt")
        ]
    );
}

#[cfg(unix)]
#[test]
fn test_a_node_that_does_not_answer_is_stopped_at_its_deadline() {
    // GIVEN a Node.js with its npm whose `node` never answers `--version`,
    //       and a search that waits for it a fifth of a second
    let dir = tempfile::tempdir().unwrap();
    let prefix = dir.path().join("hung");
    fake_release_doing(&prefix, "exec sleep 30", true);
    let search = search_path(&[&prefix.join("bin")])
        .with_version_deadline(std::time::Duration::from_millis(200));

    // WHEN the machine is searched
    let started = std::time::Instant::now();
    let missing = search.find().unwrap_err();

    // THEN it is left out for not answering, long before it would have
    //      finished on its own
    assert_eq!(missing.rejected[0].reason, "it did not answer `--version`");
    assert!(
        started.elapsed() < std::time::Duration::from_secs(10),
        "{:?}",
        started.elapsed()
    );
}

#[cfg(unix)]
#[test]
fn test_a_node_with_npx_but_without_npm_is_refused() {
    // GIVEN a Node.js 22 whose npm folder holds `npx-cli.js` but not
    //       `npm-cli.js`
    let dir = tempfile::tempdir().unwrap();
    let prefix = dir.path().join("half");
    fake_release(&prefix, "v22.11.0", true);
    std::fs::remove_file(prefix.join("lib/node_modules/npm/bin/npm-cli.js")).unwrap();
    let search = search_path(&[&prefix.join("bin")]);

    // WHEN the machine is searched
    let missing = search.find().unwrap_err();

    // THEN it is left out for its missing npm, and never run
    assert_eq!(missing.rejected[0].reason, "no npm beside it");
    assert!(!prefix.join("calls").exists());
}
