//! Build script of the runtime.
//!
//! It embeds the example packages the application proposes: every folder of
//! `agents/starters/` that holds an `apollia.toml` becomes one entry of a
//! table written into `OUT_DIR`, each of its files pulled in by
//! `include_bytes!`. The runtime assembles an entry into a `.apo` archive when
//! the user chooses to install it. A second table, read by the tests only,
//! embeds the packages of `tests/fixtures/example-packages/` the same way.
//!
//! Cargo reads this script's directives on standard output, which is why it
//! prints them.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// Name of a package manifest.
const MANIFEST: &str = "apollia.toml";

/// One table to generate: where its packages are, and the file it goes to.
struct Table {
    /// Folder of package folders, relative to this crate.
    source: &'static str,
    /// File written into `OUT_DIR`.
    output: &'static str,
}

const TABLES: [Table; 2] = [
    Table {
        source: "../../agents/starters",
        output: "example_packages.rs",
    },
    Table {
        source: "tests/fixtures/example-packages",
        output: "example_packages_fixture.rs",
    },
];

fn main() {
    let crate_dir = env_path("CARGO_MANIFEST_DIR");
    let out_dir = env_path("OUT_DIR");
    for table in &TABLES {
        let source = crate_dir.join(table.source);
        println!("cargo::rerun-if-changed={}", source.display());
        let packages = packages_of(&source);
        let output = out_dir.join(table.output);
        if let Err(error) = std::fs::write(&output, render(&packages)) {
            fail(&format!("{}: {error}", output.display()));
        }
    }
}

/// Stops the build with `message`: a binary without the examples its sources
/// name is not one this build may produce.
fn fail(message: &str) -> ! {
    eprintln!("error: the example packages could not be embedded: {message}");
    std::process::exit(1)
}

fn env_path(name: &str) -> PathBuf {
    match std::env::var_os(name) {
        Some(value) => PathBuf::from(value),
        None => fail(&format!("{name} is not set")),
    }
}

/// A package folder: its name, and its files as (path inside the package with
/// `/` separators, absolute path on this machine), sorted.
struct Package {
    folder: String,
    files: Vec<(String, PathBuf)>,
}

/// The package folders of `source`, sorted by name. A folder without a
/// manifest is not a package, and a `source` that does not exist holds none:
/// the crate then builds with an empty table.
fn packages_of(source: &Path) -> Vec<Package> {
    let mut packages = Vec::new();
    for (name, path) in entries(source) {
        if !path.is_dir() || !path.join(MANIFEST).is_file() {
            continue;
        }
        let mut files = Vec::new();
        collect(&path, "", &mut files);
        packages.push(Package {
            folder: name,
            files,
        });
    }
    packages
}

/// The entries of `dir` whose name is valid UTF-8 and does not start with a
/// dot, sorted by name. A missing folder has none.
fn entries(dir: &Path) -> Vec<(String, PathBuf)> {
    let read = match std::fs::read_dir(dir) {
        Ok(read) => read,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
        Err(error) => fail(&format!("{}: {error}", dir.display())),
    };
    let mut found = Vec::new();
    for entry in read {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => fail(&format!("{}: {error}", dir.display())),
        };
        let Ok(name) = entry.file_name().into_string() else {
            fail(&format!(
                "{}: a file name is not valid UTF-8",
                dir.display()
            ));
        };
        if !name.starts_with('.') {
            found.push((name, entry.path()));
        }
    }
    found.sort();
    found
}

/// Adds every file under `dir` to `files`, `prefix` being the path of `dir`
/// inside the package.
fn collect(dir: &Path, prefix: &str, files: &mut Vec<(String, PathBuf)>) {
    for (name, path) in entries(dir) {
        let relative = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };
        if path.is_dir() {
            collect(&path, &relative, files);
        } else {
            files.push((relative, path));
        }
    }
}

/// The Rust expression of the table: a slice of `ExampleSource`.
fn render(packages: &[Package]) -> String {
    let mut out = String::from("&[\n");
    for package in packages {
        let _ = writeln!(out, "    ExampleSource {{");
        let _ = writeln!(out, "        folder: {:?},", package.folder);
        let _ = writeln!(out, "        files: &[");
        for (relative, path) in &package.files {
            let _ = writeln!(
                out,
                "            ({relative:?}, include_bytes!({:?}) as &[u8]),",
                path.display().to_string()
            );
        }
        let _ = writeln!(out, "        ],");
        let _ = writeln!(out, "    }},");
    }
    out.push_str("]\n");
    out
}
