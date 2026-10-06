//! `apollia-os package new`: write the starting folder of a package.
//!
//! One template, and the only one: a package of the current contract. An
//! agent is data in `apollia.toml` (its mission, its examples, what it may
//! reach); the Python the package brings is a tool, an `async` function under
//! `@tool`. The folder builds as it is written: `apollia-os package build
//! <folder>` turns it into a `.apo` archive, with no Python run.
//!
//! Nothing but files is written, and nothing is read from the runtime.

use std::path::{Path, PathBuf};

use apollia_aip::apollia_manifest::is_valid_identifier;

use crate::exit_codes;
use crate::output::emit_error;

/// The SDK the template is written against: the version of this binary.
const SDK_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Why a package folder was not written.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub(super) enum NewPackageError {
    /// The name is not one a package may carry.
    #[error(
        "invalid package name '{0}': lowercase letters, digits, '-' and '_' only \
         (for example: apollia-os package new invoice-sorter)"
    )]
    InvalidName(String),
    /// The folder is already there: nothing is overwritten.
    #[error("'{0}' already exists: choose another name or another --output-dir")]
    Exists(String),
    /// A file or a folder could not be written.
    #[error("cannot write {path}: {source}")]
    Write {
        /// What was being written.
        path: String,
        /// The cause.
        #[source]
        source: std::io::Error,
    },
}

/// The files of the template, relative to the package folder, with their text.
///
/// The tool is named after the package, with `_` for `-`: a Python module and
/// a tool identifier both take it.
pub(super) fn template(name: &str) -> Vec<(PathBuf, String)> {
    let tool = name.replace('-', "_");
    let manifest = format!(
        r#"# {name}: a package for Apollia.
#
# An agent is data: its mission, its examples and what it may reach are
# written here. The Python of the package brings tools (tools/{tool}.py).
# Build the archive with `apollia-os package build .`, then install it with
# `apollia-os package install {name}-0.1.0.apo`.

[package]
name = "{name}"
version = "0.1.0"
publisher = ""
description = "What this package is for, in one sentence."
sdk = ">={SDK_VERSION}"

[[agents]]
id = "{name}"
name = "{name}"
mission = """
Say here what the agent is for and how it works, as you would brief a
colleague. This text is its instructions.
"""
examples = [
    "A request this agent answers well.",
]

[agents.defaults]
# What the agent may call, tool by tool. The user can grant or revoke each
# one on the sheet of the agent.
access = ["pkg:{name}/{tool}"]
autonomy = "assisted"

# One entry per tool the Python defines. `python -m apollia describe .`
# rewrites these entries from the code.
[[tools]]
name = "{tool}"
module = "tools/{tool}.py"
description = "Say what the tool does, for the model and for the sheet of the agent."
action = "read"
inputs = {{ type = "object", properties = {{ text = {{ type = "string", description = "The text to work on." }} }}, required = ["text"], additionalProperties = false }}
"#
    );
    let module = format!(
        r#""""The tool of the package {name}.

A tool is an async function under ``@tool``. Its first parameter is the
context the runtime hands it; the others are its inputs, described with
``Annotated`` so the model knows what to pass.
"""

from typing import Annotated, Any

from apollia import tool
from apollia.types import Ctx


@tool(
    "{tool}",
    "Say what the tool does, for the model and for the sheet of the agent.",
    action="read",
)
async def {tool}(
    ctx: Ctx,
    text: Annotated[str, "The text to work on."],
) -> dict[str, Any]:
    ctx.logger.info("{tool} called")
    return {{"length": len(text)}}
"#
    );
    vec![
        (PathBuf::from("apollia.toml"), manifest),
        (PathBuf::from("tools").join(format!("{tool}.py")), module),
    ]
}

/// Writes the template of the package `name` under `parent`, in a folder of
/// that name, and returns the folder and the files written, in order.
///
/// # Errors
///
/// [`NewPackageError`] for a name a package cannot carry, a folder that is
/// already there, or a write that fails. A failed write leaves no folder.
pub(super) fn write_package(
    parent: &Path,
    name: &str,
) -> Result<(PathBuf, Vec<String>), NewPackageError> {
    if !is_valid_identifier(name) {
        return Err(NewPackageError::InvalidName(name.to_owned()));
    }
    let root = parent.join(name);
    if root.exists() {
        return Err(NewPackageError::Exists(root.display().to_string()));
    }
    let written = write_files(&root, name);
    if written.is_err() {
        let _ = std::fs::remove_dir_all(&root);
    }
    written.map(|files| (root, files))
}

fn write_files(root: &Path, name: &str) -> Result<Vec<String>, NewPackageError> {
    let failed = |path: &Path| {
        let path = path.display().to_string();
        move |source| NewPackageError::Write { path, source }
    };
    let mut files = Vec::new();
    for (relative, text) in template(name) {
        let path = root.join(&relative);
        if let Some(folder) = path.parent() {
            std::fs::create_dir_all(folder).map_err(failed(folder))?;
        }
        std::fs::write(&path, text).map_err(failed(&path))?;
        files.push(relative.to_string_lossy().replace('\\', "/"));
    }
    Ok(files)
}

/// `apollia-os package new <name>`.
pub(super) fn run_new(name: &str, output_dir: Option<&Path>, json: bool) -> i32 {
    let parent = match output_dir {
        Some(dir) => dir.to_path_buf(),
        None => match std::env::current_dir() {
            Ok(dir) => dir,
            Err(e) => {
                return emit_error(
                    json,
                    exit_codes::GENERAL_ERROR,
                    &format!("cannot read the current folder: {e}"),
                )
            }
        },
    };
    let (root, files) = match write_package(&parent, name) {
        Ok(written) => written,
        Err(e) => return emit_error(json, exit_codes::GENERAL_ERROR, &e.to_string()),
    };
    if json {
        let doc = serde_json::json!({
            "name": name,
            "path": root.display().to_string(),
            "files": files,
        });
        println!("{}", serde_json::to_string_pretty(&doc).unwrap_or_default());
    } else {
        println!("{}", root.display());
        for file in &files {
            crate::note!("  {file}");
        }
        crate::note!("  Next: apollia-os package build {}", root.display());
    }
    exit_codes::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_aip::apo_archive::{build_from_dir, BuildOptions};

    #[test]
    fn test_the_template_is_a_package_of_the_current_contract_that_builds() {
        // GIVEN an empty folder
        let dir = tempfile::tempdir().expect("tempdir");

        // WHEN a package is written there, then built as it is
        let (root, files) = write_package(dir.path(), "invoice-sorter").expect("written");
        let archive = dir.path().join("out.apo");
        let report = build_from_dir(&root, &archive, &BuildOptions::default()).expect("builds");

        // THEN the folder holds the manifest and one tool module, the
        // manifest declares one agent granted that tool, and the archive is
        // written without any Python run
        assert_eq!(files, ["apollia.toml", "tools/invoice_sorter.py"]);
        let manifest = report.manifest;
        assert_eq!(manifest.package.name, "invoice-sorter");
        assert_eq!(manifest.agents.len(), 1);
        assert_eq!(manifest.agents[0].id, "invoice-sorter");
        assert_eq!(manifest.tools.len(), 1);
        assert_eq!(manifest.tools[0].name, "invoice_sorter");
        assert_eq!(manifest.tools[0].module, "tools/invoice_sorter.py");
        assert!(archive.is_file());
    }

    #[test]
    fn test_the_template_carries_nothing_of_the_former_contract() {
        // GIVEN the files of the template
        let files = template("invoice-sorter");

        // WHEN their text is read
        let text: String = files.iter().map(|(_, text)| text.as_str()).collect();

        // THEN the agent is data and the code is a tool: no agent class, no
        // skill, no message handler, no dispatch hook
        for former in [
            "@agent",
            "@skill",
            "@on_message",
            "@orchestrated",
            "__apollia_dispatch__",
            "tools_required",
        ] {
            assert!(!text.contains(former), "the template carries {former}");
        }
        assert!(text.contains("@tool("));
        assert!(text.contains("[[agents]]"));
    }

    #[test]
    fn test_a_name_a_package_cannot_carry_and_an_existing_folder_are_refused() {
        // GIVEN a folder that already holds `taken`
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir(dir.path().join("taken")).expect("mkdir");

        // WHEN a package is asked under a name with a space, an uppercase
        // name, a path, and the name already taken
        let spaced = write_package(dir.path(), "my agent");
        let upper = write_package(dir.path(), "MyAgent");
        let path = write_package(dir.path(), "a/b");
        let taken = write_package(dir.path(), "taken");

        // THEN each is refused and nothing was written beside what was there
        assert!(matches!(spaced, Err(NewPackageError::InvalidName(_))));
        assert!(matches!(upper, Err(NewPackageError::InvalidName(_))));
        assert!(matches!(path, Err(NewPackageError::InvalidName(_))));
        assert!(matches!(taken, Err(NewPackageError::Exists(_))));
        let entries = std::fs::read_dir(dir.path()).expect("read").count();
        assert_eq!(entries, 1);
        assert!(std::fs::read_dir(dir.path().join("taken"))
            .expect("read")
            .next()
            .is_none());
    }
}
