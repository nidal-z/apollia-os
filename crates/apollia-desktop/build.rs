//! Build script of the desktop application.
//!
//! Besides the Tauri context, it packs the documentation site into the archive
//! the application embeds (`docs/site/scripts/embed_docs.py`), written into
//! `OUT_DIR` and never into the tree. Cargo reads this script's directives on
//! standard output, which is why it prints them.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Name of the archive in `OUT_DIR`, included by `src/embedded_docs.rs`.
const ARCHIVE_NAME: &str = "embedded-docs.zip";

fn main() {
    embed_docs();
    tauri_build::build();
}

/// Stops the build with `message`: the archive is part of the binary, and a
/// binary without its documentation is not one this build may produce.
fn fail(message: &str) -> ! {
    eprintln!("error: the embedded documentation could not be packed: {message}");
    std::process::exit(1)
}

fn env_path(name: &str) -> PathBuf {
    match std::env::var_os(name) {
        Some(value) => PathBuf::from(value),
        None => fail(&format!("{name} is not set")),
    }
}

fn embed_docs() {
    let repo = env_path("CARGO_MANIFEST_DIR").join("../..");
    let site = repo.join("docs/site");
    let script = site.join("scripts/embed_docs.py");
    let watched: [&Path; 6] = [
        &script,
        &site.join("docs"),
        &site.join("i18n/fr"),
        &site.join("sidebars.js"),
        &site.join("static/img"),
        &repo.join("clients/openapi.json"),
    ];
    for path in watched {
        println!("cargo::rerun-if-changed={}", path.display());
    }
    println!("cargo::rerun-if-env-changed=APOLLIA_PYTHON");

    let out = env_path("OUT_DIR").join(ARCHIVE_NAME);
    // `python3` first, `python` next: the second is the only name some Windows
    // installations answer to, and `APOLLIA_PYTHON` overrides both.
    let interpreters = match std::env::var("APOLLIA_PYTHON") {
        Ok(python) => vec![python],
        Err(_) => vec!["python3".to_owned(), "python".to_owned()],
    };
    let mut last_failure = String::from("no Python interpreter was found (set APOLLIA_PYTHON)");
    for python in interpreters {
        match Command::new(&python)
            .arg(&script)
            .arg("--out")
            .arg(&out)
            .status()
        {
            Ok(status) if status.success() => return,
            Ok(status) => last_failure = format!("{python} {} answered {status}", script.display()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => last_failure = format!("{python} could not be started: {error}"),
        }
    }
    fail(&last_failure)
}
