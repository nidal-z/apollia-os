//! What `packaging/build-engine-manifest.py` publishes is what the engine
//! store accepts.
//!
//! The script and the store are written in two languages and meet nowhere but
//! in the files of a channel: the manifest, its signature and the archives.
//! These tests run the script on stand-in upstream archives and hand its
//! output to the store's own verification, so a drift on either side is a
//! failing test rather than a channel no installed application can read.
//!
//! The signed tests need `cargo tauri signer`, the tool that signs in the
//! release pipeline, and are ignored without it. `just test-engine-manifest`
//! runs them all, and the publication workflow runs that recipe before it
//! signs anything with the real key.
//!
//! `test_published_directory_is_signed_by_the_compiled_key` judges a real
//! output directory, named by `APOLLIA_ENGINE_MANIFEST_DIR`, against the
//! distribution key compiled into this tree: the last check before a
//! publication, since a manifest signed by any other key is one every
//! installed application refuses.

#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use apollia_runtime::engine_store::{
    archive_kind, find_binary, import_from_dir, verify_manifest, Channel, Engine, EngineStore,
    EngineStoreError, Manifest, Variant, MANIFEST_FILE, SIGNATURE_FILE,
};
use sha2::{Digest, Sha256};

const LLAMA_TAG: &str = "b4242";
const NODE_TAG: &str = "v1.2.3";

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn sha256_hex(path: &Path) -> String {
    let mut digest = Sha256::new();
    std::io::copy(&mut std::fs::File::open(path).unwrap(), &mut digest).unwrap();
    hex::encode(digest.finalize())
}

fn write_file(path: &Path, content: &str, mode: u32) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
}

/// A zip holding `files`, as an upstream Windows archive would.
fn make_zip(dest: &Path, files: &[(&str, &str)]) {
    let mut zip = zip::ZipWriter::new(std::fs::File::create(dest).unwrap());
    let options = zip::write::SimpleFileOptions::default();
    for (name, content) in files {
        zip.start_file(*name, options).unwrap();
        zip.write_all(content.as_bytes()).unwrap();
    }
    zip.finish().unwrap();
}

/// A `.tar.gz` of the directory `top` under `staging`, as upstream ships its
/// macOS and Linux builds.
fn make_tar_gz(dest: &Path, staging: &Path, top: &str) {
    let status = Command::new("tar")
        // Keeps macOS from adding its `._` companion files to the archive.
        .env("COPYFILE_DISABLE", "1")
        .arg("-czf")
        .arg(dest)
        .arg("-C")
        .arg(staging)
        .arg(top)
        .status()
        .unwrap();
    assert!(status.success(), "tar could not write {}", dest.display());
}

/// The stand-in upstream archives and the pin file that names them.
struct Upstream {
    /// Directory the archives sit in, given to the script as `--offline-dir`.
    dir: PathBuf,
    /// The pin file, given as `--sources`.
    sources: PathBuf,
}

fn upstream(root: &Path) -> Upstream {
    let dir = root.join("upstream");
    let staging = root.join("staging");
    std::fs::create_dir_all(&dir).unwrap();

    // Windows CUDA: the server in one zip, the runtime libraries in another.
    let win = format!("llama-{LLAMA_TAG}-bin-win-cuda-x64.zip");
    make_zip(
        &dir.join(&win),
        &[("llama-server.exe", "server"), ("ggml-cuda.dll", "kernels")],
    );
    let cudart = "cudart-llama-bin-win-cuda-x64.zip";
    make_zip(&dir.join(cudart), &[("cudart64_12.dll", "runtime")]);

    // Linux CUDA: a tar.gz with an executable and a link between two
    // sibling libraries, the shape of the upstream Linux builds.
    let linux_top = format!("llama-{LLAMA_TAG}");
    let linux_dir = staging.join(&linux_top);
    write_file(
        &linux_dir.join("llama-server"),
        "#!/bin/sh\necho server\n",
        0o755,
    );
    write_file(&linux_dir.join("libllama.so.0"), "library", 0o644);
    std::os::unix::fs::symlink("libllama.so.0", linux_dir.join("libllama.so")).unwrap();
    let linux = format!("llama-{LLAMA_TAG}-bin-ubuntu-cuda-x64.tar.gz");
    make_tar_gz(&dir.join(&linux), &staging, &linux_top);

    // Node.js for Linux: `bin/node`, npm's `npx-cli.js`, and the `bin/npx`
    // link that climbs out of `bin`, as every upstream release carries it.
    let node_top = format!("node-{NODE_TAG}-linux-x64");
    let node_dir = staging.join(&node_top);
    write_file(
        &node_dir.join("bin/node"),
        "#!/bin/sh\necho v1.2.3\n",
        0o755,
    );
    write_file(
        &node_dir.join("lib/node_modules/npm/bin/npx-cli.js"),
        "// npx",
        0o644,
    );
    std::os::unix::fs::symlink(
        "../lib/node_modules/npm/bin/npx-cli.js",
        node_dir.join("bin/npx"),
    )
    .unwrap();
    let node = format!("{node_top}.tar.gz");
    make_tar_gz(&dir.join(&node), &staging, &node_top);

    let source = |name: &str| {
        serde_json::json!({
            "url": format!("https://upstream.invalid/{name}"),
            "sha256": sha256_hex(&dir.join(name)),
        })
    };
    let mut runtime = source(cudart);
    runtime["beside"] = "llama-server.exe".into();
    let pins = serde_json::json!({
        "packages": [
            {
                "engine": "llama-server", "tag": LLAMA_TAG, "variant": "cuda",
                "platform": "windows-x64", "released": "2026-10-01",
                "sources": [source(&win), runtime],
            },
            {
                "engine": "llama-server", "tag": LLAMA_TAG, "variant": "cuda",
                "platform": "linux-x64",
                "sources": [source(&linux)],
            },
            {
                "engine": "nodejs", "tag": NODE_TAG, "variant": "cpu",
                "platform": "linux-x64",
                "sources": [source(&node)],
            },
        ]
    });
    let sources = root.join("engine-sources.json");
    std::fs::write(&sources, serde_json::to_vec_pretty(&pins).unwrap()).unwrap();
    Upstream { dir, sources }
}

/// The script, set to publish the stand-in upstream into `out`.
fn script(upstream: &Upstream, out: &Path, sequence: u64) -> Command {
    let mut command = Command::new("python3");
    command
        .arg(repo_root().join("packaging/build-engine-manifest.py"))
        .args(["--channel", "stable", "--llama-tag", LLAMA_TAG])
        .args(["--sequence", &sequence.to_string()])
        .arg("--sources")
        .arg(&upstream.sources)
        .arg("--offline-dir")
        .arg(&upstream.dir)
        .arg("--out")
        .arg(out)
        .args(["--require", "llama-server/cuda/windows-x64"])
        .args(["--require", "llama-server/cuda/linux-x64"])
        .args(["--require", "nodejs/cpu/linux-x64"])
        .env_remove("APOLLIA_DIST_SIGNING_KEY")
        .env_remove("APOLLIA_DIST_SIGNING_KEY_PASSWORD")
        .env_remove("TAURI_SIGNING_PRIVATE_KEY")
        .env_remove("TAURI_SIGNING_PRIVATE_KEY_PATH")
        .env_remove("TAURI_SIGNING_PRIVATE_KEY_PASSWORD");
    command
}

fn run(mut command: Command) -> (bool, String) {
    let output = command.output().unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    (output.status.success(), text)
}

/// A throwaway minisign pair made by the release tool: (private, public).
fn generate_key(root: &Path) -> (String, String) {
    let path = root.join("dist.key");
    let output = Command::new("cargo")
        .args(["tauri", "signer", "generate", "--ci", "--password", ""])
        .arg("--write-keys")
        .arg(&path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "cargo tauri signer generate failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let private = std::fs::read_to_string(&path).unwrap();
    let public = std::fs::read_to_string(path.with_extension("key.pub")).unwrap();
    (private.trim().to_owned(), public.trim().to_owned())
}

/// A signed publication of the stand-in upstream: its directory and the
/// public key that verifies it.
fn signed_publication(root: &Path) -> (PathBuf, String) {
    let upstream = upstream(root);
    let (private, public) = generate_key(root);
    let out = root.join("out");
    let mut command = script(&upstream, &out, 7);
    command
        .env("APOLLIA_DIST_SIGNING_KEY", private)
        .env("APOLLIA_DIST_SIGNING_KEY_PASSWORD", "");
    let (ok, text) = run(command);
    assert!(ok, "the script refused: {text}");
    (out, public)
}

fn store(root: &Path, key: &str, platform: &str) -> EngineStore {
    EngineStore {
        root: root.join(format!("engines-{platform}")),
        trusted_key: key.to_owned(),
        platform: platform.to_owned(),
    }
}

#[test]
fn test_unsigned_manifest_is_one_the_store_parses_and_lists_only_zips() {
    // GIVEN upstream archives, two of them tar.gz
    let tmp = tempfile::tempdir().unwrap();
    let upstream = upstream(tmp.path());
    let out = tmp.path().join("out");

    // WHEN the script builds the channel without signing
    let mut command = script(&upstream, &out, 3);
    command.arg("--no-sign");
    let (ok, text) = run(command);
    assert!(ok, "the script refused: {text}");

    // THEN the manifest is one the store parses, every package is a zip that
    // sits beside it with the stated size and digest, and nothing is signed
    let manifest = Manifest::parse(&std::fs::read(out.join(MANIFEST_FILE)).unwrap()).unwrap();
    assert_eq!(manifest.channel, Channel::Stable);
    assert_eq!(manifest.sequence, 3);
    assert_eq!(manifest.packages.len(), 3);
    for package in &manifest.packages {
        assert!(archive_kind(&package.url).is_some(), "{}", package.url);
        let archive = out.join(&package.url);
        assert_eq!(archive.metadata().unwrap().len(), package.size);
        assert_eq!(sha256_hex(&archive), package.sha256);
    }
    let windows = manifest
        .newest(Engine::LlamaServer, Variant::Cuda, "windows-x64")
        .unwrap();
    assert_eq!(windows.released.as_deref(), Some("2026-10-01"));
    assert!(!out.join(SIGNATURE_FILE).exists());
}

#[test]
fn test_source_altered_by_one_byte_is_refused_before_anything_is_written() {
    // GIVEN an upstream archive that no longer matches its pinned digest
    let tmp = tempfile::tempdir().unwrap();
    let upstream = upstream(tmp.path());
    let tampered = upstream.dir.join("cudart-llama-bin-win-cuda-x64.zip");
    let mut bytes = std::fs::read(&tampered).unwrap();
    bytes[10] ^= 0x01;
    std::fs::write(&tampered, bytes).unwrap();
    let out = tmp.path().join("out");

    // WHEN the script builds the channel
    let mut command = script(&upstream, &out, 3);
    command.arg("--no-sign");
    let (ok, text) = run(command);

    // THEN it refuses, names the archive, and leaves no manifest and no zip
    assert!(!ok);
    assert!(text.contains("differs from the pinned"), "{text}");
    assert!(!out.join(MANIFEST_FILE).exists());
    let zips = std::fs::read_dir(&out)
        .map(|d| {
            d.flatten()
                .filter(|e| e.path().extension().is_some_and(|x| x == "zip"))
                .count()
        })
        .unwrap_or(0);
    assert_eq!(zips, 0);
}

#[test]
fn test_missing_required_package_is_refused() {
    // GIVEN a tag none of the llama-server packages is pinned at
    let tmp = tempfile::tempdir().unwrap();
    let upstream = upstream(tmp.path());
    let out = tmp.path().join("out");

    // WHEN the script is asked to publish that tag
    let mut command = script(&upstream, &out, 3);
    command.args(["--llama-tag", "b1", "--no-sign"]);
    let (ok, text) = run(command);

    // THEN it refuses and names the packages the channel would lose
    assert!(!ok);
    assert!(text.contains("llama-server/cuda/linux-x64"), "{text}");
    assert!(!out.join(MANIFEST_FILE).exists());
}

#[test]
fn test_signing_without_the_key_is_refused() {
    // GIVEN no distribution key in the environment
    let tmp = tempfile::tempdir().unwrap();
    let upstream = upstream(tmp.path());
    let out = tmp.path().join("out");

    // WHEN the script builds and signs the channel
    let (ok, text) = run(script(&upstream, &out, 3));

    // THEN it refuses, names the variable, and writes no signature
    assert!(!ok);
    assert!(
        text.contains("APOLLIA_DIST_SIGNING_KEY is not set"),
        "{text}"
    );
    assert!(!out.join(SIGNATURE_FILE).exists());
}

#[test]
#[ignore = "requires cargo tauri signer"]
fn test_signed_manifest_is_accepted_then_refused_once_altered() {
    // GIVEN a channel built from two stand-in engine archives and signed
    // with a throwaway distribution key
    let tmp = tempfile::tempdir().unwrap();
    let (out, public) = signed_publication(tmp.path());
    let bytes = std::fs::read(out.join(MANIFEST_FILE)).unwrap();
    let signature = std::fs::read_to_string(out.join(SIGNATURE_FILE)).unwrap();

    // WHEN the store verifies it with the public half
    let manifest = verify_manifest(&bytes, &signature, &public, Channel::Stable).unwrap();

    // THEN the manifest is accepted with its packages
    assert_eq!(manifest.sequence, 7);
    assert!(manifest
        .newest(Engine::LlamaServer, Variant::Cuda, "windows-x64")
        .is_some());
    assert!(manifest
        .newest(Engine::LlamaServer, Variant::Cuda, "linux-x64")
        .is_some());

    // WHEN one byte of the manifest changes
    let mut altered = bytes.clone();
    let at = altered.iter().position(|b| *b == b'7').unwrap();
    altered[at] = b'8';

    // THEN the same signature no longer holds
    let refusal = verify_manifest(&altered, &signature, &public, Channel::Stable).unwrap_err();
    assert!(matches!(refusal, EngineStoreError::Signature { .. }));
}

#[test]
#[ignore = "requires cargo tauri signer"]
fn test_tar_gz_engine_comes_out_as_a_zip_the_store_installs() {
    // GIVEN a signed channel whose Linux engine came from a tar.gz
    let tmp = tempfile::tempdir().unwrap();
    let (out, public) = signed_publication(tmp.path());

    // WHEN a Linux store imports the directory
    let linux = store(tmp.path(), &public, "linux-x64");
    let installed = import_from_dir(&linux, &out).unwrap();

    // THEN the engine is filed, its binary still executable and the link
    // between the sibling libraries kept
    let engine = installed
        .iter()
        .find(|p| p.engine == Engine::LlamaServer)
        .unwrap();
    assert_eq!(engine.tag, LLAMA_TAG);
    assert_eq!(engine.variant, Variant::Cuda);
    let binary = find_binary(&engine.path, "llama-server").unwrap();
    assert_ne!(binary.metadata().unwrap().permissions().mode() & 0o111, 0);
    let link = binary.with_file_name("libllama.so");
    assert_eq!(
        std::fs::read_link(&link).unwrap(),
        PathBuf::from("libllama.so.0")
    );

    // WHEN a Windows store imports the same directory
    let windows = store(tmp.path(), &public, "windows-x64");
    let installed = import_from_dir(&windows, &out).unwrap();

    // THEN the runtime libraries sit beside the server that loads them
    let server = find_binary(&installed[0].path, "llama-server.exe").unwrap();
    assert!(server.with_file_name("cudart64_12.dll").is_file());
    assert!(server.with_file_name("ggml-cuda.dll").is_file());
}

#[test]
#[ignore = "requires cargo tauri signer"]
fn test_signed_nodejs_package_is_accepted_then_refused_once_altered() {
    // GIVEN a signed channel that carries a stand-in Node.js release
    let tmp = tempfile::tempdir().unwrap();
    let (out, public) = signed_publication(tmp.path());

    // WHEN the store imports it, as a machine without network does
    let linux = store(tmp.path(), &public, "linux-x64");
    let installed = import_from_dir(&linux, &out).unwrap();

    // THEN Node.js is filed under its tag with `node` and `npx-cli.js`, and
    // without the link the store would have refused
    let node = installed
        .iter()
        .find(|p| p.engine == Engine::NodeJs)
        .unwrap();
    assert_eq!(node.tag, NODE_TAG);
    assert_eq!(node.variant, Variant::Cpu);
    let binary = find_binary(&node.path, "bin/node").unwrap();
    assert_ne!(binary.metadata().unwrap().permissions().mode() & 0o111, 0);
    let prefix = binary.parent().unwrap().parent().unwrap();
    assert!(prefix.join("lib/node_modules/npm/bin/npx-cli.js").is_file());
    assert!(std::fs::symlink_metadata(prefix.join("bin/npx")).is_err());

    // WHEN one byte of the published manifest changes
    let path = out.join(MANIFEST_FILE);
    let mut altered = std::fs::read(&path).unwrap();
    let at = altered.iter().position(|b| *b == b'7').unwrap();
    altered[at] = b'8';
    std::fs::write(&path, altered).unwrap();

    // THEN a fresh store refuses the directory before reading any archive
    let fresh = store(&tmp.path().join("fresh"), &public, "linux-x64");
    let refusal = import_from_dir(&fresh, &out).unwrap_err();
    assert!(matches!(refusal, EngineStoreError::Signature { .. }));
    assert!(!fresh.root.join("nodejs").exists());
}

#[test]
#[ignore = "requires APOLLIA_ENGINE_MANIFEST_DIR, a directory written by packaging/build-engine-manifest.py"]
fn test_published_directory_is_signed_by_the_compiled_key() {
    // GIVEN the directory about to be published and the distribution key
    // compiled into this tree
    let dir = PathBuf::from(
        std::env::var("APOLLIA_ENGINE_MANIFEST_DIR").expect("APOLLIA_ENGINE_MANIFEST_DIR is set"),
    );
    let channel = match std::env::var("APOLLIA_ENGINE_CHANNEL").as_deref() {
        Ok("preview") => Channel::Preview,
        _ => Channel::Stable,
    };
    let key = apollia_signing::distribution::distribution_public_key().expect(
        "this tree compiles no distribution public key: no installed application could verify the manifest",
    );
    let bytes = std::fs::read(dir.join(MANIFEST_FILE)).unwrap();
    let signature = std::fs::read_to_string(dir.join(SIGNATURE_FILE)).unwrap();

    // WHEN the store verifies the manifest as an installed application will
    let manifest = verify_manifest(&bytes, &signature, key, channel)
        .expect("the manifest is not signed by the distribution key this tree compiles");

    // THEN every archive it lists is beside it, with the stated size and digest
    assert!(!manifest.packages.is_empty());
    for package in &manifest.packages {
        let archive = dir.join(&package.url);
        assert_eq!(
            archive.metadata().unwrap().len(),
            package.size,
            "{}",
            package.url
        );
        assert_eq!(sha256_hex(&archive), package.sha256, "{}", package.url);
    }
}
