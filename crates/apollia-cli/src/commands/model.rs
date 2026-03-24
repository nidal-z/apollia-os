//! `apollia-os model` subcommands — manage local .gguf model files.
//!
//! `model list` reads `~/.apollia/models/` directly — no runtime connection required.

use std::path::{Path, PathBuf};

use clap::Subcommand;

use crate::exit_codes;

/// Model subcommands: `apollia-os model <verb>`.
#[derive(Debug, Subcommand)]
pub enum ModelCommand {
    /// List available .gguf model files in ~/.apollia/models/.
    List,
}

/// Information about a local `.gguf` model file.
#[derive(Debug)]
pub struct ModelInfo {
    /// File name (base name, no directory component).
    pub name: String,
    /// File size in bytes.
    pub size_bytes: u64,
}

/// Execute a `model` subcommand.
///
/// This command does **not** require a running runtime — it reads the local
/// filesystem directly (Principle #4 — fail fast; Principle #2 — zero deps).
///
/// Returns the process exit code: `0` = success, non-zero = error.
pub fn run(cmd: &ModelCommand, json: bool) -> i32 {
    match cmd {
        ModelCommand::List => run_list(json),
    }
}

/// `apollia-os model list` — list `.gguf` model files in `~/.apollia/models/`.
fn run_list(json: bool) -> i32 {
    let models_dir = default_models_dir();

    let models = match list_gguf_files(&models_dir) {
        Ok(m) => m,
        Err(e) => {
            if json {
                let output = serde_json::json!({"error": e.to_string()});
                println!(
                    "{}",
                    serde_json::to_string_pretty(&output).unwrap_or_default()
                );
            } else {
                eprintln!("Error reading models directory: {e}");
            }
            return exit_codes::GENERAL_ERROR;
        }
    };

    if json {
        let entries: Vec<serde_json::Value> = models
            .iter()
            .map(|m| {
                serde_json::json!({
                    "name":      m.name,
                    "size_bytes": m.size_bytes,
                    "size_mb":   (m.size_bytes as f64) / (1024.0 * 1024.0),
                })
            })
            .collect();
        let output = serde_json::json!({
            "models":    entries,
            "directory": models_dir.display().to_string(),
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&output).unwrap_or_default()
        );
    } else {
        println!("  Models directory: {}", models_dir.display());
        println!();
        println!("  {:<48} SIZE", "NAME");
        if models.is_empty() {
            println!("  (no .gguf models found)");
        } else {
            for m in &models {
                let size_mb = (m.size_bytes as f64) / (1024.0 * 1024.0);
                println!("  {:<48} {size_mb:.1} MB", m.name);
            }
        }
    }
    exit_codes::SUCCESS
}

// ─────────────────────────────────────────────
// Filesystem helpers
// ─────────────────────────────────────────────

/// Return the default models directory: `~/.apollia/models/`.
///
/// Falls back to `/tmp/apollia/models` if `$HOME` is not set.
fn default_models_dir() -> PathBuf {
    home_dir()
        .map(|h| h.join(".apollia").join("models"))
        .unwrap_or_else(|| PathBuf::from("/tmp/apollia/models"))
}

/// Resolve the user's home directory via `$HOME`.
///
/// Using the environment variable directly avoids adding a `dirs` crate dependency.
fn home_dir() -> Option<PathBuf> {
    std::env::var("HOME").ok().map(PathBuf::from)
}

/// Scan `dir` for `*.gguf` files and return their names and sizes, sorted by name.
///
/// Returns an empty `Vec` — not an error — when `dir` does not exist.
/// Any individual entry that cannot be read (permissions, broken symlink)
/// propagates its `io::Error` immediately.
pub fn list_gguf_files(dir: &Path) -> Result<Vec<ModelInfo>, std::io::Error> {
    if !dir.exists() {
        return Ok(vec![]);
    }

    let mut models = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("gguf") {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let size_bytes = entry.metadata()?.len();
            models.push(ModelInfo { name, size_bytes });
        }
    }
    models.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(models)
}

// ─────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // GIVEN a directory with 2 .gguf files and 1 .txt file
    // WHEN list_gguf_files() is called
    // THEN only the 2 .gguf files are returned
    #[test]
    fn test_ac6_model_list_scans_gguf_files() {
        // GIVEN
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("llama3.gguf"), b"fake model").unwrap();
        std::fs::write(dir.path().join("mistral.gguf"), b"fake model data extra").unwrap();
        std::fs::write(dir.path().join("readme.txt"), b"not a model").unwrap();

        // WHEN
        let models = list_gguf_files(dir.path()).unwrap();

        // THEN
        assert_eq!(models.len(), 2);
        assert!(models.iter().any(|m| m.name.contains("llama3")));
        assert!(models.iter().any(|m| m.name.contains("mistral")));
    }

    // GIVEN a non-existent directory
    // WHEN list_gguf_files() is called
    // THEN an empty Vec is returned (not an io::Error)
    #[test]
    fn test_model_list_nonexistent_dir_returns_empty() {
        // GIVEN
        let dir = PathBuf::from("/tmp/apollia-test-no-models-dir-xyz-999");

        // WHEN
        let models = list_gguf_files(&dir).unwrap();

        // THEN
        assert!(models.is_empty());
    }

    // GIVEN a directory with only non-.gguf files
    // WHEN list_gguf_files() is called
    // THEN an empty Vec is returned
    #[test]
    fn test_model_list_ignores_non_gguf_files() {
        // GIVEN
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("config.toml"), b"[config]").unwrap();
        std::fs::write(dir.path().join("weights.bin"), b"binary").unwrap();

        // WHEN
        let models = list_gguf_files(dir.path()).unwrap();

        // THEN
        assert!(models.is_empty());
    }

    // GIVEN a directory with .gguf files in non-alphabetical order
    // WHEN list_gguf_files() is called
    // THEN results are sorted alphabetically by file name
    #[test]
    fn test_model_list_sorted_by_name() {
        // GIVEN
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("zzz.gguf"), b"z").unwrap();
        std::fs::write(dir.path().join("aaa.gguf"), b"a").unwrap();
        std::fs::write(dir.path().join("mmm.gguf"), b"m").unwrap();

        // WHEN
        let models = list_gguf_files(dir.path()).unwrap();

        // THEN
        assert_eq!(models.len(), 3);
        assert_eq!(models[0].name, "aaa.gguf");
        assert_eq!(models[1].name, "mmm.gguf");
        assert_eq!(models[2].name, "zzz.gguf");
    }

    // GIVEN a directory with a .gguf file
    // WHEN list_gguf_files() is called
    // THEN the returned ModelInfo carries the correct file size
    #[test]
    fn test_model_info_carries_size() {
        // GIVEN
        let dir = TempDir::new().unwrap();
        let content = b"fake gguf bytes 12345";
        std::fs::write(dir.path().join("model.gguf"), content).unwrap();

        // WHEN
        let models = list_gguf_files(dir.path()).unwrap();

        // THEN
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].size_bytes, content.len() as u64);
    }
}
