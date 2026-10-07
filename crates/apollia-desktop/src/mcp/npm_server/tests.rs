use std::path::{Path, PathBuf};

use apollia_mcp::config::McpServerConfig;
use apollia_mcp::transport::node::NodeInstall;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;
use crate::mcp::python_server::remove_connection_dir;

const TEST_PACKAGE: &str = "apollia-echo-server";

fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

fn npx_config(name: &str, list: &[&str]) -> McpServerConfig {
    serde_json::from_value(serde_json::json!({
        "name": name,
        "command": "npx",
        "args": list,
        "transport": "stdio",
    }))
    .expect("config")
}

fn curated(package: &str, version: Option<&str>) -> ConnectorEnrichment {
    serde_json::from_value(serde_json::json!({
        "package_identifier": package,
        "operator_label": { "en": "Echo", "fr": "Echo" },
        "category": "utility",
        "icon_name": "clock",
        "trust_level": "community_verified",
        "auth_help_url": null,
        "auth_help_text": null,
        "package_registry_type": "npm",
        "package_runtime_hint": "npx",
        "package_version": version,
    }))
    .expect("enrichment")
}

/// The Node.js and npm of this host, the way the engine store lays them out
/// (`bin/node`, npm under `lib/node_modules`). `None` when the host has none,
/// in which case the calling test has nothing to run.
fn host_node() -> Option<NodeInstall> {
    let path = std::env::var_os("PATH")?;
    let node = std::env::split_paths(&path)
        .map(|d| d.join(if cfg!(windows) { "node.exe" } else { "node" }))
        .find(|p| p.is_file())?;
    let node = std::fs::canonicalize(&node).unwrap_or(node);
    let bin = node.parent()?;
    let npx_cli = [
        bin.parent()?.join("lib/node_modules/npm/bin/npx-cli.js"),
        bin.join("node_modules/npm/bin/npx-cli.js"),
    ]
    .into_iter()
    .find(|p| p.is_file())?;
    Some(NodeInstall {
        node,
        npx_cli: Some(npx_cli),
    })
}

/// A Node.js whose npm is missing: any attempt to install fails at once.
fn node_without_npm(dir: &Path) -> NodeInstall {
    NodeInstall {
        node: dir.join("no-node"),
        npx_cli: Some(dir.join("no-npm").join("npx-cli.js")),
    }
}

/// A package published at `version`, its tarball and its `integrity`.
struct Published {
    version: String,
    tarball: Vec<u8>,
    integrity: String,
}

/// Pack `apollia-echo-server` at `version` with the host's npm: one command,
/// printing `ok` and its arguments.
fn pack(node: &NodeInstall, work: &Path, version: &str) -> Published {
    let src = work.join(format!("src-{version}"));
    std::fs::create_dir_all(src.join("dist")).expect("src");
    std::fs::write(
        src.join("package.json"),
        serde_json::json!({
            "name": TEST_PACKAGE,
            "version": version,
            "bin": { TEST_PACKAGE: "dist/index.js" },
        })
        .to_string(),
    )
    .expect("manifest");
    std::fs::write(
        src.join("dist/index.js"),
        "console.log(['ok', ...process.argv.slice(2)].join(' '));\n",
    )
    .expect("script");
    let npm_cli = node
        .npx_cli
        .as_ref()
        .expect("npx")
        .with_file_name("npm-cli.js");
    let mut npm = std::process::Command::new(&node.node);
    apollia_core::subprocess_window::hide_console(&mut npm);
    let out = npm
        .arg(npm_cli)
        .args(["pack", "--json", "--pack-destination"])
        .arg(work)
        .current_dir(&src)
        .output()
        .expect("npm pack");
    assert!(
        out.status.success(),
        "npm pack: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).expect("pack json");
    let filename = report[0]["filename"].as_str().expect("filename");
    Published {
        version: version.to_string(),
        tarball: std::fs::read(work.join(filename)).expect("tarball"),
        integrity: report[0]["integrity"]
            .as_str()
            .expect("integrity")
            .to_string(),
    }
}

fn tarball_path(version: &str) -> String {
    format!("/{TEST_PACKAGE}/-/{TEST_PACKAGE}-{version}.tgz")
}

/// A registry answering for `apollia-echo-server` with `published`, the last
/// one as `latest`.
async fn registry(published: &[&Published]) -> MockServer {
    let server = MockServer::start().await;
    let mut versions = serde_json::Map::new();
    for p in published {
        versions.insert(
            p.version.clone(),
            serde_json::json!({
                "name": TEST_PACKAGE,
                "version": p.version,
                "bin": { TEST_PACKAGE: "dist/index.js" },
                "dist": {
                    "tarball": format!("{}{}", server.uri(), tarball_path(&p.version)),
                    "integrity": p.integrity,
                },
            }),
        );
        Mock::given(method("GET"))
            .and(path(tarball_path(&p.version)))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(p.tarball.clone()))
            .mount(&server)
            .await;
    }
    let latest = published
        .last()
        .map(|p| p.version.clone())
        .unwrap_or_default();
    Mock::given(method("GET"))
        .and(path(format!("/{TEST_PACKAGE}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "name": TEST_PACKAGE,
            "dist-tags": { "latest": latest },
            "versions": versions,
        })))
        .mount(&server)
        .await;
    server
}

/// The lock npm would write for `apollia-echo-server` at `p`, served by
/// `registry`, with `integrity` in place of the real digest.
fn lock_for(registry: &str, p: &Published, integrity: &str) -> String {
    serde_json::to_string_pretty(&serde_json::json!({
        "name": "apollia-connection",
        "version": "0.0.0",
        "lockfileVersion": 3,
        "requires": true,
        "packages": {
            "": {
                "name": "apollia-connection",
                "version": "0.0.0",
                "dependencies": { TEST_PACKAGE: p.version },
            },
            format!("node_modules/{TEST_PACKAGE}"): {
                "version": p.version,
                "resolved": format!("{registry}{}", tarball_path(&p.version)),
                "integrity": integrity,
                "bin": { TEST_PACKAGE: "dist/index.js" },
            },
        },
    }))
    .expect("lock")
}

fn locked_plan(lock: &str, version: &str) -> NpmServerPlan {
    NpmServerPlan {
        invocation: NpxInvocation::parse(&args(&["-y", TEST_PACKAGE])).expect("parse"),
        version: Some(version.to_string()),
        lock: Some(NpmLock::check(TEST_PACKAGE, version, lock).expect("lock")),
    }
}

fn run(node: &NodeInstall, config: &McpServerConfig) -> String {
    let mut command = std::process::Command::new(&node.node);
    apollia_core::subprocess_window::hide_console(&mut command);
    let out = command.args(&config.args).output().expect("run");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

// ─── Reading the command line ──────────────────────────────────────────────

#[test]
fn a_pinned_scoped_package_and_its_arguments_are_read() {
    // GIVEN the command lines the wizard writes for a scoped and a bare package
    // WHEN they are read
    let scoped = NpxInvocation::parse(&args(&[
        "-y",
        "@modelcontextprotocol/server-memory@2026.8.31",
        "--flag",
        "x",
    ]))
    .expect("scoped");
    let bare = NpxInvocation::parse(&args(&["--yes", "echo-server"])).expect("bare");
    // THEN name, version and forwarded arguments come apart
    assert_eq!(scoped.package, "@modelcontextprotocol/server-memory");
    assert_eq!(scoped.version.as_deref(), Some("2026.8.31"));
    assert_eq!(scoped.forwarded, args(&["--flag", "x"]));
    assert_eq!(bare.package, "echo-server");
    assert_eq!(bare.version, None);
}

#[test]
fn options_ranges_tags_and_paths_are_refused_rather_than_dropped() {
    // GIVEN command lines that would install something the entry does not say
    for refused in [
        &["-y"][..],
        &["--package=evil", "server"][..],
        &["-y", "server@latest"][..],
        &["-y", "server@^1.2.0"][..],
        &["-y", "../server"][..],
        &["-y", "Server"][..],
        &["-y", "@scope"][..],
    ] {
        // WHEN each is read
        let result = NpxInvocation::parse(&args(refused));
        // THEN it is refused
        assert!(
            matches!(result, Err(NpmServerError::Invocation(_))),
            "{refused:?} should be refused"
        );
    }
}

#[test]
fn only_one_exact_semantic_version_is_exact() {
    // GIVEN versions and near-versions
    // WHEN each is checked
    // THEN only full x.y.z forms, with optional pre-release and build, pass
    for ok in ["1.2.3", "2026.8.31", "0.6.2-rc.1", "1.0.0+build.5"] {
        assert!(is_exact_version(ok), "{ok}");
    }
    for no in [
        "1.2",
        "1.2.x",
        "^1.2.3",
        "~1.2.3",
        "latest",
        "1.2.3 - 2",
        "",
    ] {
        assert!(!is_exact_version(no), "{no}");
    }
}

// ─── Planning ──────────────────────────────────────────────────────────────

#[test]
fn only_a_stdio_npx_command_is_planned() {
    // GIVEN an installed connection and a uvx one
    let mut installed = npx_config("echo", &["/x/node_modules/echo/dist/index.js"]);
    installed.command = NODE.to_string();
    let mut uvx = npx_config("time", &["mcp-server-time"]);
    uvx.command = "uvx".to_string();
    // WHEN each is planned
    // THEN neither is an npm installation
    assert_eq!(plan(&installed, &[]).expect("plan"), None);
    assert_eq!(plan(&uvx, &[]).expect("plan"), None);
}

#[test]
fn a_pinned_curated_entry_brings_its_version_and_lock_and_refuses_another_version() {
    // GIVEN the shipped catalogue and the memory server it pins
    let enrichments = crate::mcp::load_builtin_enrichments();
    let package = "@modelcontextprotocol/server-memory";
    let pinned = crate::mcp::find_enrichment(&enrichments, package)
        .and_then(|e| e.package_version.clone())
        .expect("memory is pinned");
    // WHEN the bare command line and one asking another version are planned
    let planned = plan(&npx_config("memory", &["-y", package]), &enrichments)
        .expect("plan")
        .expect("an npx command is planned");
    let other = plan(
        &npx_config("memory", &["-y", &format!("{package}@0.0.1")]),
        &enrichments,
    );
    // THEN the first installs the pinned version from the embedded lock
    assert_eq!(planned.version.as_deref(), Some(pinned.as_str()));
    let lock = planned.lock.expect("a lock");
    assert_eq!(lock.raw, embedded_lock(package).expect("embedded"));
    // AND the second is refused
    assert!(matches!(other, Err(NpmServerError::Invocation(_))));
}

#[test]
fn a_pinned_entry_without_an_embedded_lock_is_refused() {
    // GIVEN a curated entry pinned at 0.1.0 that no embedded lock covers
    let entries = [curated(TEST_PACKAGE, Some("0.1.0"))];
    // WHEN its command line is planned
    let result = plan(&npx_config("echo", &["-y", TEST_PACKAGE]), &entries);
    // THEN it is refused rather than installed unverified
    assert!(matches!(result, Err(NpmServerError::InvalidLock { .. })));
}

#[test]
fn a_package_outside_the_selection_keeps_the_version_it_names() {
    // GIVEN a registry command line pinned at 1.4.0, and a custom one unpinned
    let pinned = npx_config("x", &["-y", "@someone/server@1.4.0"]);
    let custom = npx_config("y", &["-y", "@someone/server"]);
    // WHEN both are planned against the shipped catalogue
    let enrichments = crate::mcp::load_builtin_enrichments();
    let pinned = plan(&pinned, &enrichments).expect("plan").expect("planned");
    let custom = plan(&custom, &enrichments).expect("plan").expect("planned");
    // THEN neither has a lock; the first keeps its version, the second will
    //      have its own resolved at addition
    assert_eq!(pinned.lock, None);
    assert_eq!(pinned.version.as_deref(), Some("1.4.0"));
    assert_eq!(custom.lock, None);
    assert_eq!(custom.version, None);
}

#[test]
fn a_lock_with_a_package_npm_would_accept_unverified_is_refused() {
    // GIVEN a sound lock, and variants of it each missing one guarantee
    let p = Published {
        version: "0.1.0".into(),
        tarball: Vec::new(),
        integrity: "sha512-AAAA".into(),
    };
    let sound = lock_for("https://registry.npmjs.org", &p, &p.integrity);
    let sha1 = lock_for("https://registry.npmjs.org", &p, "sha1-AAAA");
    let mut unresolved: serde_json::Value = serde_json::from_str(&sound).expect("json");
    unresolved["packages"][format!("node_modules/{TEST_PACKAGE}")]
        .as_object_mut()
        .expect("entry")
        .remove("resolved");
    let mut two_roots: serde_json::Value = serde_json::from_str(&sound).expect("json");
    two_roots["packages"][""]["dependencies"]["other"] = "1.0.0".into();
    // WHEN each is checked as the lock of the package at 0.1.0
    // THEN only the sound one passes, and none of them at another version
    assert!(NpmLock::check(TEST_PACKAGE, "0.1.0", &sound).is_ok());
    assert!(NpmLock::check(TEST_PACKAGE, "0.2.0", &sound).is_err());
    for bad in [sha1, unresolved.to_string(), two_roots.to_string()] {
        assert!(matches!(
            NpmLock::check(TEST_PACKAGE, "0.1.0", &bad),
            Err(NpmServerError::InvalidLock { .. })
        ));
    }
}

#[test]
fn the_package_an_installed_connection_runs_is_read_from_its_path() {
    // GIVEN installed connections of a scoped and a bare package
    let mut scoped = npx_config(
        "memory",
        &["/d/connections/memory/node_modules/@modelcontextprotocol/server-memory/dist/index.js"],
    );
    scoped.command = NODE.to_string();
    let mut bare = npx_config("echo", &["C:\\d\\node_modules\\echo\\bin\\run.js"]);
    bare.command = NODE.to_string();
    // WHEN their package is read
    // THEN each names its package, and an npx line names none
    assert_eq!(
        installed_package(&scoped).as_deref(),
        Some("@modelcontextprotocol/server-memory")
    );
    assert_eq!(installed_package(&bare).as_deref(), Some("echo"));
    assert_eq!(installed_package(&npx_config("x", &["-y", "echo"])), None);
}

// ─── Installing ────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_locked_package_installs_with_npm_ci_and_restarts_without_npm_or_network() {
    // GIVEN a package published on a registry, and the lock that pins it
    let Some(node) = host_node() else {
        return;
    };
    let work = tempfile::tempdir().expect("work");
    let data = tempfile::tempdir().expect("data");
    let published = pack(&node, work.path(), "0.1.0");
    let server = registry(&[&published]).await;
    let lock = lock_for(&server.uri(), &published, &published.integrity);
    let installer =
        NpmServerInstaller::using(data.path().to_path_buf(), node.clone(), server.uri());

    // WHEN the connection is installed from the lock
    let installed = installer
        .provision("echo", &locked_plan(&lock, "0.1.0"))
        .await
        .expect("install");

    // THEN the entry point lies in the connection's own directory and answers
    assert!(installed
        .entry_path
        .starts_with(data.path().join("echo").join("node_modules")));
    assert!(installed.locked);
    let record =
        std::fs::read_to_string(data.path().join("echo").join("server.json")).expect("record");
    assert!(record.contains("\"version\": \"0.1.0\""), "{record}");
    assert!(!data.path().join("echo").join(".npm-scratch").exists());

    // AND WHEN the registry goes away and npm with it, and the connection is
    //      provisioned again
    drop(server);
    let offline = NpmServerInstaller::using(
        data.path().to_path_buf(),
        node_without_npm(work.path()),
        "http://127.0.0.1:9/".into(),
    );
    let again = offline
        .provision("echo", &locked_plan(&lock, "0.1.0"))
        .await
        .expect("reused without npm");

    // THEN the same entry point comes back, and it runs on Node.js alone
    assert_eq!(again.entry_path, installed.entry_path);
    let mut config = npx_config("echo", &[]);
    config.command = NODE.to_string();
    config.args = vec![again.entry_path.display().to_string(), "a".into()];
    assert_eq!(run(&node, &config), "ok a");
}

#[tokio::test]
async fn an_altered_integrity_refuses_the_installation_and_leaves_nothing_to_launch() {
    // GIVEN a published package and a lock whose digest is off by one character
    let Some(node) = host_node() else {
        return;
    };
    let work = tempfile::tempdir().expect("work");
    let data = tempfile::tempdir().expect("data");
    let published = pack(&node, work.path(), "0.1.0");
    let server = registry(&[&published]).await;
    let digest = published.integrity.strip_prefix("sha512-").expect("sha512");
    let flipped = if digest.starts_with('A') { "B" } else { "A" };
    let altered = format!("sha512-{flipped}{}", &digest[1..]);
    let lock = lock_for(&server.uri(), &published, &altered);
    let installer = NpmServerInstaller::using(data.path().to_path_buf(), node, server.uri());

    // WHEN the connection is installed from it
    let result = installer
        .provision("echo", &locked_plan(&lock, "0.1.0"))
        .await;

    // THEN npm refuses it on the digest
    match result {
        Err(NpmServerError::Install { stderr, .. }) => {
            assert!(
                stderr.contains("EINTEGRITY"),
                "npm should name the digest: {stderr}"
            );
        }
        other => panic!("expected a refused installation, got {other:?}"),
    }
    // AND the connection directory is gone, so nothing can be launched
    assert!(!data.path().join("echo").exists());
}

#[tokio::test]
async fn a_registry_entry_keeps_the_version_resolved_at_addition_and_never_reruns_npm() {
    // GIVEN an unpinned command line, only 0.1.0 published, and a first addition
    let Some(node) = host_node() else {
        return;
    };
    let work = tempfile::tempdir().expect("work");
    let data = tempfile::tempdir().expect("data");
    let first_release = pack(&node, work.path(), "0.1.0");
    let server = registry(&[&first_release]).await;
    let installer =
        NpmServerInstaller::using(data.path().to_path_buf(), node.clone(), server.uri());
    let config = npx_config("echo", &["-y", TEST_PACKAGE, "--flag"]);
    let first = prepare(config.clone(), &[], &installer)
        .await
        .expect("first");

    // THEN the persisted command runs Node.js on the installed file, the exact
    //      version is recorded, and no npx is left in it
    assert_eq!(first.command, NODE);
    assert!(first.args.iter().all(|a| !a.contains("npx")));
    assert_eq!(first.args.last().map(String::as_str), Some("--flag"));
    let record = data.path().join("echo").join("server.json");
    assert!(std::fs::read_to_string(&record)
        .expect("record")
        .contains("\"version\": \"0.1.0\""));
    let lock = std::fs::read_to_string(data.path().join("echo").join("package-lock.json"))
        .expect("npm wrote its lock");
    assert!(lock.contains(&first_release.integrity));

    // WHEN 0.2.0 gets published, npm disappears, and the same command line
    //      comes through the addition path again
    drop(server);
    let second_release = pack(&node, work.path(), "0.2.0");
    let server = registry(&[&first_release, &second_release]).await;
    let offline = NpmServerInstaller::using(
        data.path().to_path_buf(),
        node_without_npm(work.path()),
        server.uri(),
    );
    let second = prepare(config, &[], &offline).await.expect("second");

    // THEN the same file is returned, still 0.1.0: npm never ran, it could not
    //      have, and the registry was never asked
    assert_eq!(second.args, first.args);
    assert!(server
        .received_requests()
        .await
        .unwrap_or_default()
        .is_empty());
    assert_eq!(run(&node, &second), "ok --flag");
}

#[tokio::test]
async fn without_node_the_installation_is_refused_with_node_required() {
    // GIVEN an installer whose Node.js carries no npm
    let work = tempfile::tempdir().expect("work");
    let installer = NpmServerInstaller::using(
        work.path().join("connections"),
        node_without_npm(work.path()),
        "http://127.0.0.1:9/".into(),
    );
    // WHEN an npx connection is prepared
    let result = prepare(npx_config("echo", &["-y", TEST_PACKAGE]), &[], &installer).await;
    // THEN it is refused as needing Node.js, and nothing is left behind
    assert!(matches!(result, Err(NpmServerError::NodeRequired(_))));
    assert!(!work.path().join("connections").join("echo").exists());
}

#[test]
fn removing_a_connection_deletes_its_installed_packages() {
    // GIVEN an npm connection directory holding its packages
    let data = tempfile::tempdir().expect("data");
    let dir = data.path().join("memory").join("node_modules").join("x");
    std::fs::create_dir_all(&dir).expect("dir");
    std::fs::write(dir.join("index.js"), b"").expect("file");
    // WHEN the connection is removed
    remove_connection_dir(data.path(), "memory").expect("remove");
    // THEN its directory is gone
    assert!(!data.path().join("memory").exists());
}

// ─── The shipped catalogue, against the real registry ──────────────────────

#[tokio::test]
#[ignore = "requires network access to the npm registry and a Node.js with npm on PATH"]
async fn the_curated_npm_entries_install_from_their_locks() {
    // GIVEN the shipped catalogue and an installer on the host's Node.js
    let enrichments = crate::mcp::load_builtin_enrichments();
    let node = host_node().expect("a Node.js with npm on PATH");
    let data = tempfile::tempdir().expect("data");
    let installer = NpmServerInstaller::using(
        data.path().to_path_buf(),
        node,
        "https://registry.npmjs.org/".into(),
    );
    for (package, _) in EMBEDDED_LOCKS {
        // WHEN the command the wizard builds is prepared for addition
        let name = package.rsplit('/').next().unwrap_or(package);
        let prepared = prepare(npx_config(name, &["-y", package]), &enrichments, &installer)
            .await
            .unwrap_or_else(|e| panic!("{package}: {e}"));
        // THEN it runs a file of its own directory on Node.js
        let entry = PathBuf::from(&prepared.args[0]);
        assert!(entry.is_file(), "{package}: {}", entry.display());
    }
}
