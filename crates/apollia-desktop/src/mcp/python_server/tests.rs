use std::path::Path;

use apollia_mcp::config::McpServerConfig;
use apollia_tools::tools::python_discovery::resolve_interpreter;
use apollia_tools::tools::python_executor::agent_venv_site_packages;

use super::*;
use crate::mcp::enrichments::ConnectorEnrichment;

const TEST_PACKAGE: &str = "apollia-echo-server";

fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

fn uvx_config(name: &str, list: &[&str]) -> McpServerConfig {
    serde_json::from_value(serde_json::json!({
        "name": name,
        "command": "uvx",
        "args": list,
        "transport": "stdio",
    }))
    .expect("config")
}

fn curated(
    package: &str,
    version: Option<&str>,
    lock: Vec<LockedRequirement>,
) -> ConnectorEnrichment {
    serde_json::from_value(serde_json::json!({
        "package_identifier": package,
        "operator_label": { "en": "Echo", "fr": "Echo" },
        "category": "utility",
        "icon_name": "clock",
        "trust_level": "community_verified",
        "auth_help_url": null,
        "auth_help_text": null,
        "package_registry_type": "pypi",
        "package_runtime_hint": "uvx",
        "package_version": version,
    }))
    .map(|mut e: ConnectorEnrichment| {
        e.package_lock = lock;
        e
    })
    .expect("enrichment")
}

fn locked(name: &str, version: &str, hash: &str) -> LockedRequirement {
    LockedRequirement {
        name: name.to_string(),
        version: version.to_string(),
        marker: None,
        hashes: vec![format!("sha256:{hash}")],
    }
}

/// Run a program the way the product would, environment scrubbed and no
/// console window.
fn spawn<A: AsRef<std::ffi::OsStr>>(
    program: &Path,
    argv: &[A],
) -> std::io::Result<std::process::Output> {
    let mut command = std::process::Command::new(program);
    command.args(argv);
    apollia_core::subprocess_env::scrub_bundled_python(&mut command);
    apollia_core::subprocess_window::hide_console(&mut command);
    command.output()
}

/// Write a minimal pure-Python wheel with one console script printing `ok`
/// and its arguments, and return its sha256. `None` when no interpreter is
/// available on the host, in which case the calling test has nothing to run.
fn build_wheel(dir: &Path, name: &str, version: &str) -> Option<String> {
    const SCRIPT: &str = r#"
import base64, hashlib, sys, zipfile
out, name, version = sys.argv[1:4]
mod = name.replace("-", "_")
path = f"{out}/{mod}-{version}-py3-none-any.whl"
info = f"{mod}-{version}.dist-info"
files = {
    f"{mod}/__init__.py": "import sys\ndef main():\n    print('ok ' + ' '.join(sys.argv[1:]))\n",
    f"{info}/METADATA": f"Metadata-Version: 2.1\nName: {name}\nVersion: {version}\n",
    f"{info}/WHEEL": "Wheel-Version: 1.0\nGenerator: test\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
    f"{info}/entry_points.txt": f"[console_scripts]\n{name} = {mod}:main\n",
}
record = []
with zipfile.ZipFile(path, "w") as z:
    for p, c in files.items():
        b = c.encode()
        z.writestr(p, b)
        h = base64.urlsafe_b64encode(hashlib.sha256(b).digest()).rstrip(b"=").decode()
        record.append(f"{p},sha256={h},{len(b)}")
    record.append(f"{info}/RECORD,,")
    z.writestr(f"{info}/RECORD", "\n".join(record) + "\n")
print(hashlib.sha256(open(path, "rb").read()).hexdigest())
"#;
    let python = resolve_interpreter(None).ok()?.command;
    let mut argv: Vec<std::ffi::OsString> = python.args.iter().map(Into::into).collect();
    argv.extend([
        "-c".into(),
        SCRIPT.into(),
        dir.into(),
        name.into(),
        version.into(),
    ]);
    let output = spawn(Path::new(&python.program), &argv).ok()?;
    assert!(
        output.status.success(),
        "wheel build failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

// ─── Reading the command line ──────────────────────────────────────────────

#[test]
fn a_bare_package_runs_its_own_console_script() {
    // GIVEN the arguments of `uvx mcp-server-time --local-timezone UTC`
    let list = args(&["mcp-server-time", "--local-timezone", "UTC"]);
    // WHEN they are read
    let inv = UvxInvocation::parse(&list).expect("parse");
    // THEN the package is the entry point and the rest is forwarded
    assert_eq!(inv.package, "mcp-server-time");
    assert_eq!(inv.version, None);
    assert_eq!(inv.entry_point, "mcp-server-time");
    assert_eq!(inv.forwarded, args(&["--local-timezone", "UTC"]));
}

#[test]
fn a_pinned_package_and_a_from_command_are_both_understood() {
    // GIVEN the two pinning spellings and the `--from` form
    // WHEN they are read
    let eq = UvxInvocation::parse(&args(&["pkg==1.2.3"])).expect("==");
    let at = UvxInvocation::parse(&args(&["pkg@1.2.3"])).expect("@");
    let from = UvxInvocation::parse(&args(&["--from", "pkg==2.0", "serve", "-v"])).expect("from");
    // THEN each yields the exact version and the right entry point
    assert_eq!(eq.version.as_deref(), Some("1.2.3"));
    assert_eq!(at.version.as_deref(), Some("1.2.3"));
    assert_eq!(from.package, "pkg");
    assert_eq!(from.version.as_deref(), Some("2.0"));
    assert_eq!(from.entry_point, "serve");
    assert_eq!(from.forwarded, args(&["-v"]));
}

#[test]
fn options_and_non_tokens_are_refused_rather_than_dropped() {
    // GIVEN command lines carrying an index option, a path, a range, `latest`
    let refused = [
        args(&["--index-url", "https://example.invalid", "pkg"]),
        args(&[]),
        args(&["../pkg"]),
        args(&["pkg>=1.0"]),
        args(&["pkg@latest"]),
        args(&["--from", "pkg"]),
    ];
    for list in refused {
        // WHEN each is read
        let result = UvxInvocation::parse(&list);
        // THEN it is refused
        assert!(
            matches!(result, Err(PythonServerError::Invocation(_))),
            "{list:?} should be refused"
        );
    }
}

// ─── Rendering a lock ──────────────────────────────────────────────────────

#[test]
fn a_lock_renders_as_one_hashed_requirement_per_line() {
    // GIVEN a two-line lock, one of them platform-restricted
    let mut second = locked("cryptography", "48.0.1", &"b".repeat(64));
    second.marker = Some(r#"sys_platform == "darwin" and platform_machine == "x86_64""#.into());
    let lock = vec![locked("pkg", "1.0", &"a".repeat(64)), second];
    // WHEN it is rendered
    let text = render_requirements("pkg", &lock).expect("render");
    // THEN pip reads exactly the pinned versions and their digests
    let expected = format!(
        "pkg==1.0 --hash=sha256:{}\ncryptography==48.0.1 ; sys_platform == \"darwin\" and platform_machine == \"x86_64\" --hash=sha256:{}\n",
        "a".repeat(64),
        "b".repeat(64)
    );
    assert_eq!(text, expected);
}

#[test]
fn a_lock_line_that_could_carry_a_pip_option_is_refused() {
    // GIVEN locks with no digest, a short digest, a range, and a marker
    //       literal smuggling an option
    let good = "a".repeat(64);
    let mut no_hash = locked("pkg", "1.0", &good);
    no_hash.hashes.clear();
    let mut smuggled = locked("pkg", "1.0", &good);
    smuggled.marker = Some(r#"sys_platform == " --index-url=http://x""#.into());
    let bad = [
        no_hash,
        locked("pkg", "1.0", "abc"),
        locked("pkg", ">=1.0", &good),
        locked("-r", "1.0", &good),
        smuggled,
    ];
    for req in bad {
        // WHEN it is rendered
        let result = render_requirements("pkg", std::slice::from_ref(&req));
        // THEN it is refused
        assert!(
            matches!(result, Err(PythonServerError::InvalidLock { .. })),
            "{req:?} should be refused"
        );
    }
}

// ─── Planning ──────────────────────────────────────────────────────────────

#[test]
fn only_a_stdio_uvx_command_is_planned() {
    // GIVEN an npx server and a remote one
    let mut npx = uvx_config("files", &["-y", "@scope/pkg"]);
    npx.command = "npx".into();
    let mut remote = uvx_config("remote", &[]);
    remote.transport = "streamable-http".into();
    // WHEN they are planned
    // THEN neither is touched
    assert_eq!(plan(&npx, &[]).expect("npx"), None);
    assert_eq!(plan(&remote, &[]).expect("remote"), None);
}

#[test]
fn a_curated_entry_brings_its_version_and_lock_and_refuses_another_version() {
    // GIVEN a curated entry pinned at 1.0 with its lock
    let lock = vec![locked("echo-pkg", "1.0", &"a".repeat(64))];
    let entries = [curated("echo_pkg", Some("1.0"), lock.clone())];
    // WHEN the bare command and a command asking for 2.0 are planned
    let planned = plan(&uvx_config("echo", &["Echo.Pkg"]), &entries)
        .expect("plan")
        .expect("uvx");
    let other = plan(&uvx_config("echo", &["echo-pkg==2.0"]), &entries);
    // THEN the bare one gets the lock, the other is refused
    assert_eq!(planned.version.as_deref(), Some("1.0"));
    assert_eq!(planned.lock, lock);
    assert!(matches!(other, Err(PythonServerError::Invocation(_))));
}

#[test]
fn a_lock_that_does_not_pin_its_own_package_is_refused() {
    // GIVEN a curated entry whose lock pins its package at another version
    let entries = [curated(
        "echo-pkg",
        Some("1.0"),
        vec![locked("echo-pkg", "0.9", &"a".repeat(64))],
    )];
    // WHEN it is planned
    let result = plan(&uvx_config("echo", &["echo-pkg"]), &entries);
    // THEN the lock is refused
    assert!(matches!(result, Err(PythonServerError::InvalidLock { .. })));
}

// ─── Installing, on a local package, without the network ───────────────────

#[tokio::test]
async fn adding_a_uvx_connection_installs_it_and_rewrites_the_command() {
    // GIVEN a local wheel, a curated entry locking it by its digest, and an
    //       installer that cannot reach any index
    let archives = tempfile::tempdir().expect("archives");
    let data = tempfile::tempdir().expect("data");
    let Some(hash) = build_wheel(archives.path(), TEST_PACKAGE, "0.1.0") else {
        return;
    };
    let entries = [curated(
        TEST_PACKAGE,
        Some("0.1.0"),
        vec![locked(TEST_PACKAGE, "0.1.0", &hash)],
    )];
    let installer =
        PythonServerInstaller::offline(data.path().to_path_buf(), archives.path().to_path_buf());
    let config = uvx_config("echo", &[TEST_PACKAGE, "--flag"]);

    // WHEN the connection is prepared for addition
    let prepared = prepare(config, &entries, &installer)
        .await
        .expect("prepare");

    // THEN the command is the absolute entry point inside the connection's venv
    let venv = data.path().join("echo").join("venv");
    let command = Path::new(&prepared.command);
    assert!(command.is_absolute(), "{}", prepared.command);
    assert!(command.starts_with(&venv), "{}", prepared.command);
    assert_eq!(prepared.args, args(&["--flag"]));
    // AND the venv was built by an interpreter, with the package in it
    assert!(!agent_venv_site_packages(data.path(), "echo").is_empty());
    // AND the rewritten command runs the server
    let out = spawn(command, &prepared.args).expect("run entry point");
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "ok --flag");
    // AND what was installed is recorded beside the venv
    let record =
        std::fs::read_to_string(data.path().join("echo").join("server.json")).expect("record");
    assert!(record.contains("\"version\": \"0.1.0\""), "{record}");
    assert!(record.contains("\"locked\": true"), "{record}");
}

#[tokio::test]
async fn an_altered_digest_refuses_the_installation_and_leaves_nothing_to_launch() {
    // GIVEN a local wheel and a lock whose digest is off by one character
    let archives = tempfile::tempdir().expect("archives");
    let data = tempfile::tempdir().expect("data");
    let Some(hash) = build_wheel(archives.path(), TEST_PACKAGE, "0.1.0") else {
        return;
    };
    let flipped = if hash.starts_with('0') { "1" } else { "0" };
    let altered = format!("{flipped}{}", &hash[1..]);
    let entries = [curated(
        TEST_PACKAGE,
        Some("0.1.0"),
        vec![locked(TEST_PACKAGE, "0.1.0", &altered)],
    )];
    let installer =
        PythonServerInstaller::offline(data.path().to_path_buf(), archives.path().to_path_buf());

    // WHEN the connection is prepared for addition
    let result = prepare(uvx_config("echo", &[TEST_PACKAGE]), &entries, &installer).await;

    // THEN pip refuses it, and no configuration comes back to be launched
    match result {
        Err(PythonServerError::Install { stderr, .. }) => {
            assert!(
                stderr.contains("HASH"),
                "pip should name the hash: {stderr}"
            );
        }
        other => panic!("expected a refused installation, got {other:?}"),
    }
    // AND the connection directory is gone
    assert!(!data.path().join("echo").exists());
}

#[tokio::test]
async fn a_registry_entry_keeps_the_version_resolved_at_addition_and_never_reruns_pip() {
    // GIVEN an unlocked entry, only 0.1.0 published, and a first addition
    let archives = tempfile::tempdir().expect("archives");
    let data = tempfile::tempdir().expect("data");
    if build_wheel(archives.path(), TEST_PACKAGE, "0.1.0").is_none() {
        return;
    }
    let installer =
        PythonServerInstaller::offline(data.path().to_path_buf(), archives.path().to_path_buf());
    let config = uvx_config("echo", &[TEST_PACKAGE]);
    let first = prepare(config.clone(), &[], &installer)
        .await
        .expect("first");
    let record = data.path().join("echo").join("server.json");
    assert!(std::fs::read_to_string(&record)
        .expect("record")
        .contains("\"version\": \"0.1.0\""));

    // WHEN 0.2.0 gets published, pip disappears from the venv, and the
    //      connection comes through the addition path again
    build_wheel(archives.path(), TEST_PACKAGE, "0.2.0").expect("second wheel");
    for site in agent_venv_site_packages(data.path(), "echo") {
        std::fs::remove_dir_all(site.join("pip")).expect("remove pip");
    }
    let second = prepare(config, &[], &installer).await.expect("second");

    // THEN the same entry point is returned, still at 0.1.0: pip never ran,
    //      it could not have
    assert_eq!(second.command, first.command);
    assert!(std::fs::read_to_string(&record)
        .expect("record")
        .contains("\"version\": \"0.1.0\""));
    let out = spawn(Path::new(&second.command), &[] as &[String]).expect("run");
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "ok");
}

// ─── The shipped catalogue, against the real package index ─────────────────

#[tokio::test]
#[ignore = "requires network access to the Python package index"]
async fn the_curated_python_entries_install_from_their_locks() {
    // GIVEN the shipped catalogue and an installer pointed at the real index
    let enrichments = crate::mcp::load_builtin_enrichments();
    let data = tempfile::tempdir().expect("data");
    let installer = PythonServerInstaller::new(data.path().to_path_buf());
    for package in ["mcp-server-time", "mcp-server-fetch"] {
        // WHEN the command the wizard builds is prepared for addition
        let prepared = prepare(uvx_config(package, &[package]), &enrichments, &installer)
            .await
            .unwrap_or_else(|e| panic!("{package}: {e}"));
        // THEN the entry point it names answers `--help`
        let out = spawn(Path::new(&prepared.command), &["--help"]).expect("run entry point");
        assert!(
            out.status.success(),
            "{package}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}
