//! Integration test: the example packages of `agents/examples/` build as they
//! are written, and their Python tools run in the embedded interpreter.
//!
//! `hello` declares an agent and one tool; `word-count` declares a tool and no
//! agent. Each folder is copied out of the tree, built into a `.apo` archive
//! with no Python run, stored the way the package store records a package,
//! and its tool is called through the dispatcher that runs the native tools.
//!
//! The SDK of this repository is put first on `sys.path`, so the test reads the
//! `@tool` decorator of the tree it builds. Gated behind `python-tests`:
//!
//!   PYO3_PYTHON="$(command -v python3)" \
//!   cargo test -p apollia-e2e-tests --features python-tests --test test_hello_agent

use std::path::{Path, PathBuf};

use apollia_aip::package_tools::PackageTools;
use apollia_runtime::coordinator::CancellationToken;
use apollia_runtime::package_tools::PackageToolHost;
use apollia_tools::apo_archive::{build_from_dir, ApoArchive, BuildOptions, BuildReport};
use apollia_tools::executor::ToolDispatcher;
use apollia_tools::{InstalledPackage, PackageItemKind, PackageRepository};
use pyo3::prelude::*;
use serde_json::json;

/// The agent that calls the tools: one created without code.
const CALLER: &str = "notes";

/// Puts the SDK of this tree first on `sys.path`.
fn use_tree_sdk() {
    let sdk = Path::new(env!("CARGO_MANIFEST_DIR")).join("../sdk");
    Python::with_gil(|py| -> PyResult<()> {
        py.import("sys")?
            .getattr("path")?
            .call_method1("insert", (0, sdk.to_string_lossy().as_ref()))?;
        Ok(())
    })
    .expect("sys.path accepts the SDK");
}

/// An example package, copied out of the tree and built.
struct Example {
    dir: tempfile::TempDir,
    root: PathBuf,
    report: BuildReport,
}

/// Copies `agents/examples/<name>` under a temporary folder, so that nothing
/// is written in the tree, and builds its archive beside the copy.
fn built_example(name: &str) -> Example {
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../agents/examples")
        .join(name);
    let dir = tempfile::tempdir().expect("temp dir");
    let root = dir.path().join("packages").join(name);
    copy_folder(&source, &root);
    let archive = dir.path().join(format!("{name}.apo"));
    let report =
        build_from_dir(&root, &archive, &BuildOptions::default()).expect("the example builds");
    Example { dir, root, report }
}

fn copy_folder(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("create the folder");
    for entry in std::fs::read_dir(from).expect("read the folder") {
        let entry = entry.expect("folder entry");
        let name = entry.file_name();
        if name == "__pycache__" {
            continue;
        }
        let target = to.join(&name);
        if entry.file_type().expect("entry type").is_dir() {
            copy_folder(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).expect("copy the file");
        }
    }
}

/// Stores the example the way an install records it, and reads its tools.
fn stored_tools(example: &Example) -> PackageTools {
    use_tree_sdk();
    let manifest = &example.report.manifest;
    let repo =
        PackageRepository::open(&example.dir.path().join("agents.db")).expect("open the store");
    repo.save(&InstalledPackage {
        name: manifest.package.name.clone(),
        version: manifest.package.version.clone(),
        root_path: example.root.clone(),
        manifest_json: serde_json::to_string(manifest).expect("manifest as JSON"),
        installed_at: "2026-10-07T00:00:00Z".into(),
        updated_at: "2026-10-07T00:00:00Z".into(),
    })
    .expect("save the package");
    for tool in &manifest.tools {
        repo.link_item(&manifest.package.name, PackageItemKind::Tool, &tool.name)
            .expect("link the tool");
    }
    PackageTools::read(&repo, example.dir.path().join("venvs")).expect("read the tools")
}

// GIVEN the folder of the example `hello`, as the tree holds it
// WHEN its archive is built and read back
// THEN the archive carries the agent `hello`, granted the tool of its own
//      package, and that tool with the schema the code declares
#[test]
fn hello_example_builds_into_an_archive_that_declares_its_agent_and_its_tool() {
    // GIVEN
    let example = built_example("hello");

    // WHEN
    let archive =
        ApoArchive::open(&example.dir.path().join("hello.apo")).expect("the archive opens");
    let manifest = archive.manifest();

    // THEN
    assert_eq!(manifest.package.name, "hello");
    let agents: Vec<&str> = manifest.agents.iter().map(|a| a.id.as_str()).collect();
    assert_eq!(agents, ["hello"]);
    assert_eq!(manifest.agents[0].defaults.access, ["pkg:hello/greet"]);
    let tools: Vec<&str> = manifest.tools.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(tools, ["greet"]);
    assert_eq!(manifest.tools[0].module, "tools/greet.py");
    assert_eq!(
        manifest.tools[0]
            .inputs
            .as_ref()
            .and_then(|schema| schema.get("required")),
        Some(&json!(["name"]))
    );
    let entries: Vec<&str> = archive.entries().iter().map(|e| e.path.as_str()).collect();
    assert_eq!(entries, ["apollia.toml", "tools/greet.py"]);
}

// GIVEN the example `hello` stored as an installed package
// WHEN its tool is called through the dispatcher of the native tools
// THEN the tool answers with the greeting for the name it was given
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn hello_example_tool_runs_in_the_embedded_interpreter() {
    // GIVEN
    let example = built_example("hello");
    let tools = stored_tools(&example);
    let dispatcher = ToolDispatcher::new(tools.executors("hello", &CancellationToken::new()));

    // WHEN
    let answer = dispatcher
        .dispatch("pkg:hello/greet", json!({"name": " Ada "}))
        .await;

    // THEN
    assert_eq!(
        answer.expect("the tool answers"),
        json!({"greeting": "Hello, Ada!"})
    );
}

// GIVEN the example `word-count`, a package that declares no agent
// WHEN it is built, stored, and its tool is called for an agent that belongs
//      to no package
// THEN the build accepts it and the tool answers that agent
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn package_without_an_agent_builds_and_its_tool_serves_another_agent() {
    // GIVEN
    let example = built_example("word-count");
    assert!(example.report.manifest.agents.is_empty());
    let tools = stored_tools(&example);
    let dispatcher = ToolDispatcher::new(tools.executors(CALLER, &CancellationToken::new()));

    // WHEN
    let answer = dispatcher
        .dispatch(
            "pkg:word-count/count_words",
            json!({"text": "A short note.\nOn two lines."}),
        )
        .await;

    // THEN
    assert_eq!(
        answer.expect("the tool answers"),
        json!({"words": 6, "lines": 2, "characters": 27})
    );
}
