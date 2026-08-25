//! `apollia-os model` subcommands: manage local .gguf model files.
//!
//! `model list` reads `~/.apollia/models/` directly, no runtime connection required.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use clap::Subcommand;

use crate::exit_codes;

/// Model subcommands: `apollia-os model <verb>`.
#[derive(Debug, Subcommand)]
pub enum ModelCommand {
    /// List available .gguf model files in ~/.apollia/models/.
    List,
    /// Search the HuggingFace registry through the runtime.
    Search {
        /// Free-text query (matches model id, description, tags).
        query: String,
        /// Maximum number of hits to return (default: 20).
        #[arg(long, default_value_t = 20)]
        limit: u32,
    },
    /// Fetch metadata + file list for a HuggingFace model.
    Show {
        /// Repository identifier in `org/repo` form.
        repo: String,
    },
    /// Report the runtime's detected hardware profile (RAM, CPU, GPU).
    Hardware,
    /// Remove a local model file from `~/.apollia/models/`.
    Delete {
        /// File name relative to the models directory.
        name: String,
        /// Skip the confirmation prompt.
        #[arg(long)]
        confirm: bool,
    },
}

/// Result of a scan of the `~/.apollia/models/` directory.
///
/// Represents either a single-file GGUF model or a series of shards grouped
/// by a common prefix (standard llama.cpp format:
/// `<prefix>-NNNNN-of-NNNNN.gguf`).
#[derive(Debug)]
pub enum ModelListing {
    /// Single-file model.
    Single {
        /// Full file name (with the `.gguf` extension).
        name: String,
        /// File size in bytes.
        size_bytes: u64,
    },
    /// GGUF model split into several shards following the standard pattern.
    Split {
        /// Common shard prefix (without the `-NNNNN-of-NNNNN` suffix).
        prefix: String,
        /// Number of shards actually present in the directory.
        shard_count: usize,
        /// Total number of expected shards (the `-of-NNNNN` value).
        total: usize,
        /// Cumulative size of all present shards, in bytes.
        size_bytes: u64,
        /// File names of the present shards, sorted by ascending index.
        shards: Vec<String>,
        /// `true` when `shard_count < total` (incomplete series).
        incomplete: bool,
    },
}

impl ModelListing {
    /// Display sort key (prefix for [`ModelListing::Split`], file name for
    /// [`ModelListing::Single`]).
    fn sort_key(&self) -> &str {
        match self {
            ModelListing::Single { name, .. } => name,
            ModelListing::Split { prefix, .. } => prefix,
        }
    }

    /// Cumulative size in bytes (single file or sum of shards).
    fn size_bytes(&self) -> u64 {
        match self {
            ModelListing::Single { size_bytes, .. } => *size_bytes,
            ModelListing::Split { size_bytes, .. } => *size_bytes,
        }
    }
}

/// Length (in ASCII bytes) of the `NNNNN-of-NNNNN` suffix, excluding the `-`
/// that separates it from the prefix.
const SHARD_SUFFIX_LEN: usize = 14;

/// Metadata for a shard detected during the initial scan.
///
/// Intermediate between `fs::read_dir` and [`ModelListing::Split`]: lets us
/// aggregate entries by common prefix before producing the final list.
struct ShardEntry {
    index: u32,
    file_name: String,
    size_bytes: u64,
}

/// Parse a GGUF file name to extract `(prefix, index, total)` when it follows
/// the standard `<prefix>-NNNNN-of-NNNNN.gguf` pattern.
///
/// Returns `None` for any other format: single file, non zero-padded indices,
/// missing extension, etc. Does not touch the filesystem.
///
/// This parser intentionally duplicates the equivalent runner logic: the CLI
/// does not depend on the runner, and the shard format rule is a public
/// invariant of the GGUF format.
fn parse_shard_name(file_name: &str) -> Option<(String, u32, u32)> {
    let stem = file_name.strip_suffix(".gguf")?;
    if stem.len() < SHARD_SUFFIX_LEN + 2 {
        return None;
    }
    let split_at = stem.len() - SHARD_SUFFIX_LEN;
    let (prefix_with_dash, trailing) = stem.split_at(split_at);
    let prefix = prefix_with_dash.strip_suffix('-')?;
    if prefix.is_empty() {
        return None;
    }
    let (idx_str, rest) = trailing.split_at(5);
    let total_str = rest.strip_prefix("-of-")?;
    if total_str.len() != 5 {
        return None;
    }
    let index = idx_str.parse::<u32>().ok()?;
    let total = total_str.parse::<u32>().ok()?;
    Some((prefix.to_string(), index, total))
}

/// Execute a `model` subcommand.
///
/// This command does **not** require a running runtime: it reads the local
/// filesystem directly (fail fast, zero deps).
///
/// Returns the process exit code: `0` = success, non-zero = error.
pub async fn run(cmd: &ModelCommand, socket: Option<PathBuf>, json: bool) -> i32 {
    match cmd {
        ModelCommand::List => run_list(json),
        ModelCommand::Search { query, limit } => run_search(socket, query, *limit, json).await,
        ModelCommand::Show { repo } => run_show(socket, repo, json).await,
        ModelCommand::Hardware => run_hardware(socket, json).await,
        ModelCommand::Delete { name, confirm } => run_delete(name, *confirm, json),
    }
}

// ─── Search ───────────────────────────────────────────────────────────────────

async fn run_search(socket: Option<PathBuf>, query: &str, limit: u32, json: bool) -> i32 {
    let socket = socket.unwrap_or_else(|| PathBuf::from(crate::client::DEFAULT_SOCKET_PATH));
    let client = crate::client::RuntimeClient::new(socket);
    let q = urlencode(query);
    let uri = format!("/api/v1/llm/registry/search?q={q}&limit={limit}");
    match client.get(&uri).await {
        Ok(resp) if resp.status < 400 => {
            // Both modes print the raw JSON body returned by the registry API;
            // the branch is kept explicit pending a human-formatted view.
            #[allow(clippy::if_same_then_else)]
            if json {
                println!("{}", resp.body);
            } else {
                println!("{}", resp.body);
            }
            exit_codes::SUCCESS
        }
        Ok(resp) => {
            eprintln!("Error: HTTP {}: {}", resp.status, resp.body);
            exit_codes::GENERAL_ERROR
        }
        Err(e) => {
            eprintln!("Error: {e}");
            exit_codes::GENERAL_ERROR
        }
    }
}

async fn run_show(socket: Option<PathBuf>, repo: &str, json: bool) -> i32 {
    let Some((org, name)) = repo.split_once('/') else {
        eprintln!("Error: expected org/repo, got '{repo}'");
        return exit_codes::GENERAL_ERROR;
    };
    let socket = socket.unwrap_or_else(|| PathBuf::from(crate::client::DEFAULT_SOCKET_PATH));
    let client = crate::client::RuntimeClient::new(socket);
    let uri = format!("/api/v1/llm/registry/model/{org}/{name}");
    match client.get(&uri).await {
        Ok(resp) if resp.status < 400 => {
            println!("{}", resp.body);
            let _ = json;
            exit_codes::SUCCESS
        }
        Ok(resp) => {
            eprintln!("Error: HTTP {}: {}", resp.status, resp.body);
            exit_codes::GENERAL_ERROR
        }
        Err(e) => {
            eprintln!("Error: {e}");
            exit_codes::GENERAL_ERROR
        }
    }
}

async fn run_hardware(socket: Option<PathBuf>, json: bool) -> i32 {
    let socket = socket.unwrap_or_else(|| PathBuf::from(crate::client::DEFAULT_SOCKET_PATH));
    let client = crate::client::RuntimeClient::new(socket);
    match client.get("/api/v1/llm/hardware").await {
        Ok(resp) if resp.status < 400 => {
            if json {
                println!("{}", resp.body);
            } else {
                render_hardware_text(&resp.body);
            }
            exit_codes::SUCCESS
        }
        Ok(resp) => {
            eprintln!("Error: HTTP {}: {}", resp.status, resp.body);
            exit_codes::GENERAL_ERROR
        }
        Err(crate::client::ClientError::ConnectionRefused) => {
            // Daemon-off is exit 2 (RUNTIME_ERROR), distinct from generic CLI
            // errors (exit 1). Aligns with tools describe / llm status /
            // audit list / etc.
            eprintln!("Error: runtime not started (connection refused)");
            exit_codes::RUNTIME_ERROR
        }
        Err(e) => {
            eprintln!("Error: {e}");
            exit_codes::GENERAL_ERROR
        }
    }
}

/// Pretty-prints a hardware summary, falling back to the raw body if it does
/// not parse as JSON.
fn render_hardware_text(body: &str) {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(body) else {
        println!("{body}");
        return;
    };
    if let Some(ram) = v.get("ram_gb").and_then(|x| x.as_f64()) {
        println!("  RAM       {ram:.1} GB");
    }
    if let Some(cpu) = v.get("cpu_threads").and_then(|x| x.as_i64()) {
        println!("  CPU       {cpu} threads");
    }
    if let Some(acc) = v.get("accelerator").and_then(|x| x.as_str()) {
        println!("  Accelerator {acc}");
    }
}

fn run_delete(name: &str, confirm: bool, json: bool) -> i32 {
    if !confirm {
        if json {
            let out = serde_json::json!({"error": "use --confirm to delete without prompt"});
            println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
        } else {
            eprintln!("Use --confirm to delete '{name}' without prompt.");
        }
        return exit_codes::GENERAL_ERROR;
    }
    let dir = default_models_dir();
    let path = dir.join(name);
    if !path.exists() {
        if json {
            let out =
                serde_json::json!({"error": format!("{name} not found in {}", dir.display())});
            println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
        } else {
            eprintln!("Error: {} not found in {}", name, dir.display());
        }
        return exit_codes::GENERAL_ERROR;
    }
    match std::fs::remove_file(&path) {
        Ok(()) => {
            if json {
                let out = serde_json::json!({
                    "removed": path.display().to_string(),
                });
                println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
            } else {
                println!("  * removed {}", path.display());
            }
            exit_codes::SUCCESS
        }
        Err(e) => {
            eprintln!("Error: failed to remove {}: {e}", path.display());
            exit_codes::GENERAL_ERROR
        }
    }
}

/// Minimal percent-encode for the query string.
fn urlencode(s: &str) -> String {
    s.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

/// `apollia-os model list`: list `.gguf` model files in `~/.apollia/models/`.
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
        let entries: Vec<serde_json::Value> = models.iter().map(listing_to_json).collect();
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
        println!("  {:<48} {:<28} SIZE", "NAME", "LAYOUT");
        if models.is_empty() {
            println!("  (no .gguf models found)");
        } else {
            for m in &models {
                print_listing_row(m);
            }
        }
    }
    exit_codes::SUCCESS
}

/// Serialize a single [`ModelListing`] as a JSON value for `--json` output.
fn listing_to_json(m: &ModelListing) -> serde_json::Value {
    let size_mb = (m.size_bytes() as f64) / (1024.0 * 1024.0);
    match m {
        ModelListing::Single { name, size_bytes } => serde_json::json!({
            "kind":       "single",
            "name":       name,
            "size_bytes": size_bytes,
            "size_mb":    size_mb,
        }),
        ModelListing::Split {
            prefix,
            shard_count,
            total,
            size_bytes,
            shards,
            incomplete,
        } => serde_json::json!({
            "kind":        "split",
            "prefix":      prefix,
            "shard_count": shard_count,
            "total":       total,
            "incomplete":  incomplete,
            "size_bytes":  size_bytes,
            "size_mb":     size_mb,
            "shards":      shards,
        }),
    }
}

/// Render a single [`ModelListing`] as a human-readable row.
fn print_listing_row(m: &ModelListing) {
    let size_mb = (m.size_bytes() as f64) / (1024.0 * 1024.0);
    match m {
        ModelListing::Single { name, .. } => {
            println!("  {:<48} {:<28} {size_mb:.1} MB", name, "mono");
        }
        ModelListing::Split {
            prefix,
            shard_count,
            total,
            incomplete,
            ..
        } => {
            let layout = if *incomplete {
                format!("{shard_count}/{total} shards (INCOMPLETE)")
            } else {
                format!("{shard_count} shards")
            };
            println!("  {prefix:<48} {layout:<28} {size_mb:.1} MB");
        }
    }
}

// ─────────────────────────────────────────────
// Filesystem helpers
// ─────────────────────────────────────────────

/// Return the default models directory: `~/.apollia/models/`.
///
/// Falls back to `/tmp/apollia/models` if `$HOME` is not set.
fn default_models_dir() -> PathBuf {
    home_dir()
        .map(|h| apollia_core::paths::data_dir_under(h).join("models"))
        .unwrap_or_else(|| PathBuf::from("/tmp/apollia/models"))
}

/// Resolve the user's home directory via `$HOME`.
///
/// Using the environment variable directly avoids adding a `dirs` crate dependency.
fn home_dir() -> Option<PathBuf> {
    apollia_core::paths::home_dir()
}

/// Scan `dir` for `*.gguf` files, group shards by prefix and return a sorted
/// list of [`ModelListing`].
///
/// - A file that does **not** match the shard pattern becomes a [`ModelListing::Single`].
/// - Files sharing the same `(prefix, total)` are aggregated into a [`ModelListing::Split`].
/// - A series with fewer files than `total` (including an orphan shard without
///   index `00001`) is flagged `incomplete = true` without raising an error;
///   the CLI is resilient to partial downloads.
///
/// Returns an empty `Vec` (not an error) when `dir` does not exist. Any I/O
/// failure on an individual entry (permissions, broken symlink, missing
/// metadata) is propagated immediately.
pub fn list_gguf_files(dir: &Path) -> Result<Vec<ModelListing>, std::io::Error> {
    if !dir.exists() {
        return Ok(vec![]);
    }

    let mut out: Vec<ModelListing> = Vec::new();
    let mut groups: HashMap<(String, u32), Vec<ShardEntry>> = HashMap::new();

    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("gguf") {
            continue;
        }
        let Some(file_name) = path
            .file_name()
            .and_then(|n| n.to_str())
            .map(str::to_string)
        else {
            continue;
        };
        let size_bytes = entry.metadata()?.len();

        match parse_shard_name(&file_name) {
            Some((prefix, index, total)) => {
                groups.entry((prefix, total)).or_default().push(ShardEntry {
                    index,
                    file_name,
                    size_bytes,
                });
            }
            None => out.push(ModelListing::Single {
                name: file_name,
                size_bytes,
            }),
        }
    }

    for ((prefix, total), mut shards) in groups {
        shards.sort_by_key(|s| s.index);
        let shard_count = shards.len();
        let total_usize = total as usize;
        let size_bytes: u64 = shards.iter().map(|s| s.size_bytes).sum();
        let shard_names: Vec<String> = shards.into_iter().map(|s| s.file_name).collect();
        out.push(ModelListing::Split {
            prefix,
            shard_count,
            total: total_usize,
            size_bytes,
            shards: shard_names,
            incomplete: shard_count < total_usize,
        });
    }

    out.sort_by(|a, b| a.sort_key().cmp(b.sort_key()));
    Ok(out)
}

// ─────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use tempfile::TempDir;

    #[derive(Parser, Debug)]
    struct TestCli {
        #[command(subcommand)]
        cmd: ModelCommand,
    }

    #[test]
    fn parses_list() {
        let cli = TestCli::parse_from(["x", "list"]);
        assert!(matches!(cli.cmd, ModelCommand::List));
    }

    #[test]
    fn parses_search_default_limit() {
        let cli = TestCli::parse_from(["x", "search", "qwen"]);
        match cli.cmd {
            ModelCommand::Search { query, limit } => {
                assert_eq!(query, "qwen");
                assert_eq!(limit, 20);
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parses_search_with_limit() {
        let cli = TestCli::parse_from(["x", "search", "llama", "--limit", "5"]);
        match cli.cmd {
            ModelCommand::Search { query, limit } => {
                assert_eq!(query, "llama");
                assert_eq!(limit, 5);
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parses_show() {
        let cli = TestCli::parse_from(["x", "show", "Qwen/Qwen3-30B"]);
        match cli.cmd {
            ModelCommand::Show { repo } => assert_eq!(repo, "Qwen/Qwen3-30B"),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parses_hardware() {
        let cli = TestCli::parse_from(["x", "hardware"]);
        assert!(matches!(cli.cmd, ModelCommand::Hardware));
    }

    #[test]
    fn parses_delete_with_confirm() {
        let cli = TestCli::parse_from(["x", "delete", "model.gguf", "--confirm"]);
        match cli.cmd {
            ModelCommand::Delete { name, confirm } => {
                assert_eq!(name, "model.gguf");
                assert!(confirm);
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    fn touch(dir: &Path, name: &str, bytes: &[u8]) {
        std::fs::write(dir.join(name), bytes).expect("write fixture file");
    }

    // GIVEN a directory with 2 .gguf files and 1 .txt file
    // WHEN list_gguf_files() is called
    // THEN only the 2 .gguf files are returned
    #[test]
    fn test_model_list_scans_gguf_files() {
        let dir = TempDir::new().expect("tempdir");
        touch(dir.path(), "llama3.gguf", b"fake model");
        touch(dir.path(), "mistral.gguf", b"fake model data extra");
        touch(dir.path(), "readme.txt", b"not a model");

        let models = list_gguf_files(dir.path()).expect("scan ok");

        assert_eq!(models.len(), 2);
        assert!(models.iter().any(|m| matches!(
            m,
            ModelListing::Single { name, .. } if name == "llama3.gguf"
        )));
        assert!(models.iter().any(|m| matches!(
            m,
            ModelListing::Single { name, .. } if name == "mistral.gguf"
        )));
    }

    // GIVEN a non-existent directory
    // WHEN list_gguf_files() is called
    // THEN an empty Vec is returned (not an io::Error)
    #[test]
    fn test_model_list_nonexistent_dir_returns_empty() {
        let dir = PathBuf::from("/tmp/apollia-test-no-models-dir-xyz-999");

        let models = list_gguf_files(&dir).expect("scan ok");

        assert!(models.is_empty());
    }

    // GIVEN a directory with only non-.gguf files
    // WHEN list_gguf_files() is called
    // THEN an empty Vec is returned
    #[test]
    fn test_model_list_ignores_non_gguf_files() {
        let dir = TempDir::new().expect("tempdir");
        touch(dir.path(), "config.toml", b"[config]");
        touch(dir.path(), "weights.bin", b"binary");

        let models = list_gguf_files(dir.path()).expect("scan ok");

        assert!(models.is_empty());
    }

    // GIVEN a directory with mono-file models in non-alphabetical order
    // WHEN list_gguf_files() is called
    // THEN results are sorted by file name
    #[test]
    fn test_model_list_sorted_by_name() {
        let dir = TempDir::new().expect("tempdir");
        touch(dir.path(), "zzz.gguf", b"z");
        touch(dir.path(), "aaa.gguf", b"a");
        touch(dir.path(), "mmm.gguf", b"m");

        let models = list_gguf_files(dir.path()).expect("scan ok");

        assert_eq!(models.len(), 3);
        assert_eq!(models[0].sort_key(), "aaa.gguf");
        assert_eq!(models[1].sort_key(), "mmm.gguf");
        assert_eq!(models[2].sort_key(), "zzz.gguf");
    }

    // GIVEN a single .gguf file with known contents
    // WHEN list_gguf_files() is called
    // THEN the Single listing carries the correct size
    #[test]
    fn test_model_listing_carries_size() {
        let dir = TempDir::new().expect("tempdir");
        let content = b"fake gguf bytes 12345";
        touch(dir.path(), "model.gguf", content);

        let models = list_gguf_files(dir.path()).expect("scan ok");

        assert_eq!(models.len(), 1);
        match &models[0] {
            ModelListing::Single { name, size_bytes } => {
                assert_eq!(name, "model.gguf");
                assert_eq!(*size_bytes, content.len() as u64);
            }
            other => panic!("expected Single, got {other:?}"),
        }
    }

    // GIVEN 3 complete shards of the same split model
    // WHEN list_gguf_files() is called
    // THEN a single Split listing aggregates them
    #[test]
    fn test_split_complete_groups_as_one_entry() {
        let dir = TempDir::new().expect("tempdir");
        touch(dir.path(), "Llama-00001-of-00003.gguf", &[0u8; 1000]);
        touch(dir.path(), "Llama-00002-of-00003.gguf", &[0u8; 1000]);
        touch(dir.path(), "Llama-00003-of-00003.gguf", &[0u8; 1000]);

        let models = list_gguf_files(dir.path()).expect("scan ok");

        assert_eq!(models.len(), 1);
        match &models[0] {
            ModelListing::Split {
                prefix,
                shard_count,
                total,
                incomplete,
                size_bytes,
                shards,
            } => {
                assert_eq!(prefix, "Llama");
                assert_eq!(*shard_count, 3);
                assert_eq!(*total, 3);
                assert!(!*incomplete);
                assert_eq!(*size_bytes, 3000);
                assert_eq!(
                    shards,
                    &vec![
                        "Llama-00001-of-00003.gguf".to_string(),
                        "Llama-00002-of-00003.gguf".to_string(),
                        "Llama-00003-of-00003.gguf".to_string(),
                    ]
                );
            }
            other => panic!("expected Split, got {other:?}"),
        }
    }

    // GIVEN 2 shards of a 3-shard series
    // WHEN list_gguf_files() is called
    // THEN the listing is marked incomplete
    #[test]
    fn test_split_incomplete_is_flagged() {
        let dir = TempDir::new().expect("tempdir");
        touch(dir.path(), "Llama-00001-of-00003.gguf", &[0u8; 1000]);
        touch(dir.path(), "Llama-00002-of-00003.gguf", &[0u8; 1000]);

        let models = list_gguf_files(dir.path()).expect("scan ok");

        assert_eq!(models.len(), 1);
        match &models[0] {
            ModelListing::Split {
                shard_count,
                total,
                incomplete,
                ..
            } => {
                assert_eq!(*shard_count, 2);
                assert_eq!(*total, 3);
                assert!(*incomplete);
            }
            other => panic!("expected Split incomplete, got {other:?}"),
        }
    }

    // GIVEN an orphan shard (index 00002 only)
    // WHEN list_gguf_files() is called
    // THEN it is reported as a split series marked incomplete, no error
    #[test]
    fn test_orphan_shard_without_first_is_incomplete() {
        let dir = TempDir::new().expect("tempdir");
        touch(dir.path(), "weird-00002-of-00003.gguf", &[0u8; 100]);

        let models = list_gguf_files(dir.path()).expect("scan ok");

        assert_eq!(models.len(), 1);
        match &models[0] {
            ModelListing::Split {
                prefix,
                shard_count,
                total,
                incomplete,
                ..
            } => {
                assert_eq!(prefix, "weird");
                assert_eq!(*shard_count, 1);
                assert_eq!(*total, 3);
                assert!(*incomplete);
            }
            other => panic!("expected Split incomplete, got {other:?}"),
        }
    }

    // GIVEN a mixed directory of split and mono models
    // WHEN list_gguf_files() is called
    // THEN entries are sorted by display key (prefix for split, stem for mono)
    #[test]
    fn test_listing_sorted_by_display_key() {
        let dir = TempDir::new().expect("tempdir");
        touch(dir.path(), "Zeta-mono.gguf", &[0u8; 1]);
        touch(dir.path(), "Alpha-00001-of-00002.gguf", &[0u8; 1]);
        touch(dir.path(), "Alpha-00002-of-00002.gguf", &[0u8; 1]);
        touch(dir.path(), "Mid.gguf", &[0u8; 1]);

        let models = list_gguf_files(dir.path()).expect("scan ok");

        assert_eq!(models.len(), 3);
        assert_eq!(models[0].sort_key(), "Alpha");
        assert_eq!(models[1].sort_key(), "Mid.gguf");
        assert_eq!(models[2].sort_key(), "Zeta-mono.gguf");
    }

    // GIVEN names that do not match the shard pattern
    // WHEN parse_shard_name() is called
    // THEN None is returned for each of them
    #[test]
    fn test_parse_shard_name_negative_cases() {
        assert!(parse_shard_name("model.gguf").is_none());
        assert!(parse_shard_name("model-1-of-3.gguf").is_none());
        assert!(parse_shard_name("model-00001-of-3.gguf").is_none());
        assert!(parse_shard_name("model-00001-0f-00003.gguf").is_none());
        assert!(parse_shard_name("-00001-of-00003.gguf").is_none());
        assert!(parse_shard_name("model-00001-of-00003").is_none());
    }

    // GIVEN a valid shard file name
    // WHEN parse_shard_name() is called
    // THEN the tuple (prefix, index, total) is returned
    #[test]
    fn test_parse_shard_name_positive_cases() {
        assert_eq!(
            parse_shard_name("Llama-70B-Q5_K_M-00001-of-00009.gguf"),
            Some(("Llama-70B-Q5_K_M".to_string(), 1, 9))
        );
        assert_eq!(
            parse_shard_name("A-00042-of-00099.gguf"),
            Some(("A".to_string(), 42, 99))
        );
    }

    // GIVEN two different split series sharing a prefix but not a total
    // WHEN list_gguf_files() is called
    // THEN both are returned as separate Split entries
    #[test]
    fn test_two_series_same_prefix_different_total() {
        let dir = TempDir::new().expect("tempdir");
        touch(dir.path(), "Model-00001-of-00002.gguf", &[0u8; 10]);
        touch(dir.path(), "Model-00002-of-00002.gguf", &[0u8; 10]);
        touch(dir.path(), "Model-00001-of-00003.gguf", &[0u8; 10]);

        let models = list_gguf_files(dir.path()).expect("scan ok");

        assert_eq!(models.len(), 2);
        for m in &models {
            match m {
                ModelListing::Split { prefix, .. } => assert_eq!(prefix, "Model"),
                other => panic!("expected Split, got {other:?}"),
            }
        }
    }

    // GIVEN a Single listing and a Split listing
    // WHEN size_bytes() is called
    // THEN the correct sum is returned
    #[test]
    fn test_model_listing_size_bytes_accessor() {
        let single = ModelListing::Single {
            name: "foo.gguf".to_string(),
            size_bytes: 42,
        };
        assert_eq!(single.size_bytes(), 42);

        let split = ModelListing::Split {
            prefix: "bar".to_string(),
            shard_count: 2,
            total: 2,
            size_bytes: 200,
            shards: vec![
                "bar-00001-of-00002.gguf".to_string(),
                "bar-00002-of-00002.gguf".to_string(),
            ],
            incomplete: false,
        };
        assert_eq!(split.size_bytes(), 200);
    }

    // GIVEN a mix of listings
    // WHEN listing_to_json() serializes them
    // THEN the JSON carries the documented kind-discriminated fields
    #[test]
    fn test_listing_to_json_shapes() {
        let single = ModelListing::Single {
            name: "Qwen.gguf".to_string(),
            size_bytes: 2_570_000_000,
        };
        let v = listing_to_json(&single);
        assert_eq!(v["kind"], "single");
        assert_eq!(v["name"], "Qwen.gguf");
        assert_eq!(v["size_bytes"], 2_570_000_000_u64);
        assert!(v["size_mb"].as_f64().is_some());

        let split = ModelListing::Split {
            prefix: "Llama".to_string(),
            shard_count: 2,
            total: 3,
            size_bytes: 2000,
            shards: vec![
                "Llama-00001-of-00003.gguf".to_string(),
                "Llama-00002-of-00003.gguf".to_string(),
            ],
            incomplete: true,
        };
        let v = listing_to_json(&split);
        assert_eq!(v["kind"], "split");
        assert_eq!(v["prefix"], "Llama");
        assert_eq!(v["shard_count"], 2);
        assert_eq!(v["total"], 3);
        assert_eq!(v["incomplete"], true);
        assert_eq!(v["shards"].as_array().map(Vec::len), Some(2));
    }
}
