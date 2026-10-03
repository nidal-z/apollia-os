//! Where the CLI finds `apollia.toml`, and what it says about the places it no
//! longer looks.
//!
//! Every command resolves the file through [`resolve`], which answers the path
//! given on the command line or [`apollia_core::paths::config_file`]. The
//! working directory, `$XDG_CONFIG_HOME/apollia/` and `~/.config/apollia/` are
//! never read: a file left there is named by `doctor` and by a warning at
//! startup, with the command that moves it, and never imported.

use std::path::{Path, PathBuf};

use apollia_core::paths::{self, CONFIG_FILE_NAME};

/// The configuration file a command reads or writes: `explicit` when the
/// operator gave one, otherwise `<home>/.apollia/apollia.toml`.
pub(crate) fn resolve(explicit: Option<&Path>) -> PathBuf {
    match explicit {
        Some(p) => p.to_path_buf(),
        None => paths::config_file_under(paths::home_dir_or_temp()),
    }
}

/// Configuration files sitting at a location no surface reads any more, for
/// the given home, working directory and `$XDG_CONFIG_HOME`.
///
/// A file in the working directory that declares a `[package]` table is the
/// manifest of an agent package, which shares the name, and is not listed:
/// it was never a configuration file and must not be moved.
pub(crate) fn legacy_files(
    home: &Path,
    cwd: Option<&Path>,
    xdg_config_home: Option<&Path>,
) -> Vec<PathBuf> {
    let canonical = paths::config_file_under(home);
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(dir) = cwd {
        let local = dir.join(CONFIG_FILE_NAME);
        if !is_package_manifest(&local) {
            candidates.push(local);
        }
    }
    if let Some(xdg) = xdg_config_home.filter(|p| p.is_absolute()) {
        candidates.push(xdg.join("apollia").join(CONFIG_FILE_NAME));
    }
    candidates.push(home.join(".config").join("apollia").join(CONFIG_FILE_NAME));

    let mut found: Vec<PathBuf> = Vec::new();
    for path in candidates {
        if path.is_file() && !same_file(&path, &canonical) && !found.contains(&path) {
            found.push(path);
        }
    }
    found
}

/// [`legacy_files`] for the running process.
pub(crate) fn legacy_files_here() -> Vec<PathBuf> {
    let home = paths::home_dir_or_temp();
    let cwd = std::env::current_dir().ok();
    let xdg = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from);
    legacy_files(&home, cwd.as_deref(), xdg.as_deref())
}

/// The shell command that moves `from` to the canonical location `to`.
pub(crate) fn move_command(from: &Path, to: &Path) -> String {
    if cfg!(windows) {
        format!("move \"{}\" \"{}\"", from.display(), to.display())
    } else {
        format!("mv \"{}\" \"{}\"", from.display(), to.display())
    }
}

/// One warning per file left at a location no surface reads, except the file
/// actually in use when the operator named it explicitly.
pub(crate) fn warn_legacy_files(files: &[PathBuf], in_use: Option<&Path>, canonical: &Path) {
    for path in files {
        if in_use.is_some_and(|used| same_file(used, path)) {
            continue;
        }
        tracing::event!(
            tracing::Level::WARN,
            path = %path.display(),
            canonical = %canonical.display(),
            command = %move_command(path, canonical),
            "config.legacy_location.ignored"
        );
    }
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(x), Ok(y)) => x == y,
        _ => a == b,
    }
}

fn is_package_manifest(path: &Path) -> bool {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|raw| raw.parse::<toml::Table>().ok())
        .is_some_and(|table| table.contains_key("package"))
}

#[cfg(test)]
mod tests;
