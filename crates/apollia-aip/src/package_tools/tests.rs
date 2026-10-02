//! Package tools reach the registry from the manifest, without their module.

use std::path::Path;

use apollia_tools::{
    ActionFamily, FamilySource, InstalledPackage, PackageItemKind, PackageRepository, ToolKind,
    ToolRegistryHandle,
};
use pyo3::prelude::*;

use super::{dotted_module, executor::module_prefix, PackageTools};
use crate::apollia_manifest::{self, ApolliaManifest};

const COMPLETE: &str = include_str!("../apollia_manifest/fixtures/complete.toml");

/// The module the fixture's tool points at: importing it fails loudly.
const RAISING_MODULE: &str = "raise RuntimeError('the module was imported')\n";

fn manifest() -> ApolliaManifest {
    let mut manifest = apollia_manifest::parse(COMPLETE).unwrap();
    let mut unlinked = manifest.tools[0].clone();
    unlinked.name = "unlinked".to_string();
    manifest.tools.push(unlinked);
    manifest
}

fn package(root: &Path, name: &str, manifest_json: String) -> InstalledPackage {
    InstalledPackage {
        name: name.to_string(),
        version: "1.2.0".to_string(),
        root_path: root.to_path_buf(),
        manifest_json,
        installed_at: "2026-10-02T00:00:00Z".to_string(),
        updated_at: "2026-10-02T00:00:00Z".to_string(),
    }
}

/// A store holding the package `invoices`, whose `extract_totals` tool is
/// linked to it and whose module raises on import, and a package stored with
/// a manifest of the older agent format.
fn store(dir: &Path) -> PackageRepository {
    let root = dir.join("invoices");
    std::fs::create_dir_all(root.join("tools")).unwrap();
    std::fs::write(root.join("tools/extract.py"), RAISING_MODULE).unwrap();
    let repo = PackageRepository::open(&dir.join("agents.db")).unwrap();
    let json = serde_json::to_string(&manifest()).unwrap();
    repo.save(&package(&root, "invoices", json)).unwrap();
    repo.link_item("invoices", PackageItemKind::Tool, "extract_totals")
        .unwrap();
    repo.save(&package(
        &dir.join("legacy"),
        "legacy",
        r#"{"package":{"name":"legacy","version":"0.1.0"},"agents":[]}"#.to_string(),
    ))
    .unwrap();
    repo.link_item("legacy", PackageItemKind::Tool, "anything")
        .unwrap();
    repo
}

// GIVEN a stored package whose manifest describes a linked tool, an unlinked
//       one, and whose module raises when imported; and a package whose
//       manifest is not an apollia.toml
// WHEN its tools are read and entered in the registry
// THEN the registry holds `pkg:invoices/extract_totals` alone, with the input
//      schema and the family the manifest declares, and no module was imported
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_tools_are_registered_from_the_manifest_without_their_module() {
    // GIVEN
    let dir = tempfile::tempdir().unwrap();
    let repo = store(dir.path());
    let registry = ToolRegistryHandle::start();

    // WHEN
    let tools = PackageTools::read(&repo, dir.path().join("venvs")).unwrap();
    let registered = tools.register(&registry).await;

    // THEN
    assert_eq!(registered, 1);
    assert_eq!(tools.names(), vec!["pkg:invoices/extract_totals"]);
    let descriptor = registry
        .get("pkg:invoices/extract_totals")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        descriptor.input_schema,
        manifest().tools[0].inputs.clone().unwrap()
    );
    assert_eq!(descriptor.input_schema["required"][0], "path");
    assert_eq!(descriptor.family, ActionFamily::ReadPersonal);
    assert_eq!(descriptor.family_source, FamilySource::Package);
    assert!(!descriptor.is_read_only);
    assert!(matches!(
        descriptor.kind,
        ToolKind::Package { ref package, ref module }
            if package == "invoices" && module == "tools/extract.py"
    ));
    assert!(registry
        .get("pkg:invoices/unlinked")
        .await
        .unwrap()
        .is_none());
    assert!(registry.get("pkg:legacy/anything").await.unwrap().is_none());
    let imported = Python::with_gil(|py| -> PyResult<bool> {
        py.import("sys")?
            .getattr("modules")?
            .call_method1("__contains__", ("tools.extract",))?
            .extract()
    })
    .unwrap();
    assert!(!imported, "registering the tool imported its module");
    registry.shutdown().await;
}

// GIVEN the module paths a manifest may carry
// WHEN each is turned into a dotted module name
// THEN a relative `.py` path inside the package gives its dotted name, and an
//      absolute path, a path that climbs out, or a non-Python file gives none
#[test]
fn test_module_path_stays_inside_the_package() {
    // GIVEN
    let cases = [
        ("tools/extract.py", Some("tools.extract")),
        ("extract.py", Some("extract")),
        ("tools/__init__.py", Some("tools")),
        ("/etc/extract.py", None),
        ("../extract.py", None),
        ("tools/../../x.py", None),
        ("tools/extract.txt", None),
    ];

    // WHEN
    let dotted: Vec<Option<String>> = cases.iter().map(|(m, _)| dotted_module(m)).collect();

    // THEN
    for ((module, expected), got) in cases.iter().zip(dotted) {
        assert_eq!(got.as_deref(), *expected, "{module}");
    }
}

/// A working module for `extract_totals`, which counts its own imports in
/// the file `__COUNT__`.
const COUNTING_MODULE: &str = r#"
import pathlib
import types

COUNT = pathlib.Path(__COUNT__)
COUNT.write_text(str(int(COUNT.read_text()) + 1) if COUNT.exists() else "1")


async def extract_totals(path):
    return {"path": path}


extract_totals.__apollia_tool__ = types.SimpleNamespace(name="extract_totals")
"#;

fn modules_under(prefix: &str) -> usize {
    Python::with_gil(|py| -> PyResult<usize> {
        let keys: Vec<String> = py
            .import("sys")?
            .getattr("modules")?
            .call_method0("keys")?
            .try_iter()?
            .filter_map(|k| k.ok()?.extract().ok())
            .collect();
        Ok(keys.iter().filter(|k| k.starts_with(prefix)).count())
    })
    .unwrap()
}

// GIVEN a stored package whose tool was called once, its module imported
// WHEN the idle check runs nine minutes, then ten minutes and one second,
//      after the call, with a ten-minute delay
// THEN the package stays loaded the first time; the second time it is
//      unloaded, its function released and its module out of `sys.modules`;
//      and a later call imports the module again
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_a_package_is_unloaded_after_ten_minutes_without_a_call() {
    // GIVEN
    pyo3::prepare_freethreaded_python();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("invoices-idle");
    std::fs::create_dir_all(root.join("tools")).unwrap();
    let count = dir.path().join("imports");
    let module = COUNTING_MODULE.replace("__COUNT__", &format!("{:?}", count.to_string_lossy()));
    std::fs::write(root.join("tools/extract.py"), module).unwrap();
    let repo = PackageRepository::open(&dir.path().join("agents.db")).unwrap();
    let json = serde_json::to_string(&manifest()).unwrap();
    repo.save(&package(&root, "invoices", json)).unwrap();
    repo.link_item("invoices", PackageItemKind::Tool, "extract_totals")
        .unwrap();
    let tools = PackageTools::read(&repo, dir.path().join("venvs")).unwrap();
    let cancel = apollia_runtime::coordinator::CancellationToken::new();
    let call = |tools: &PackageTools| {
        use apollia_runtime::package_tools::PackageToolHost;
        let executor = tools.executors("clerk", &cancel).remove(0);
        async move { executor.execute(serde_json::json!({"path": "a.pdf"})).await }
    };
    let answer = call(&tools).await.unwrap();
    assert_eq!(answer["path"], "a.pdf");
    let called_at = std::time::Instant::now();
    let prefix = module_prefix(&root);
    let ten_minutes = std::time::Duration::from_secs(600);
    assert!(tools.is_loaded("invoices"));
    assert_eq!(modules_under(&prefix), 1);

    // WHEN
    let after_nine = {
        let tools = tools.clone();
        tokio::task::spawn_blocking(move || {
            tools.unload_idle(called_at + std::time::Duration::from_secs(540), ten_minutes)
        })
        .await
        .unwrap()
    };

    // THEN
    assert!(after_nine.is_empty());
    assert!(tools.is_loaded("invoices"));

    // WHEN
    let after_ten = {
        let tools = tools.clone();
        tokio::task::spawn_blocking(move || {
            tools.unload_idle(called_at + std::time::Duration::from_secs(601), ten_minutes)
        })
        .await
        .unwrap()
    };

    // THEN
    assert_eq!(after_ten, vec!["invoices".to_string()]);
    assert!(!tools.is_loaded("invoices"));
    assert_eq!(modules_under(&prefix), 0);
    let again = call(&tools).await.unwrap();
    assert_eq!(again["path"], "a.pdf");
    assert_eq!(std::fs::read_to_string(&count).unwrap(), "2");
}
