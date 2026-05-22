//! `apollia-os stt` subcommands — Speech-to-Text management.
//!
//! Provides `status`, `transcribe`, `transcriptions`, and `model` subcommands
//! for interacting with the STT engine from the terminal.

use std::path::PathBuf;

use clap::Subcommand;

use crate::client::{ClientError, RuntimeClient, DEFAULT_SOCKET_PATH};
use crate::exit_codes;

// ─── Subcommands ──────────────────────────────────────────────────────────

/// STT subcommands: `apollia-os stt <verb>`.
#[derive(Debug, Subcommand)]
pub enum SttCommand {
    /// Display the STT engine status (enabled, model, backend, acceleration).
    Status,

    /// Transcribe an audio file and print the resulting text.
    Transcribe {
        /// Path to the audio file (WAV format).
        file: PathBuf,

        /// Save the full TranscriptResult to a JSON file instead of stdout.
        #[arg(long, value_name = "PATH")]
        output: Option<PathBuf>,
    },

    /// List recent transcriptions from the local database.
    Transcriptions {
        /// Transcriptions subcommand.
        #[command(subcommand)]
        command: TranscriptionsCommand,
    },

    /// Manage local STT model files.
    Model {
        /// Model subcommand.
        #[command(subcommand)]
        command: SttModelCommand,
    },
    /// Manage STT configuration (backend, model, language).
    Config {
        /// Config subcommand.
        #[command(subcommand)]
        command: SttConfigCommand,
    },
}

/// Transcription history subcommands.
#[derive(Debug, Subcommand)]
pub enum TranscriptionsCommand {
    /// List recent transcriptions.
    List {
        /// Maximum number of entries to display.
        #[arg(long, default_value = "20")]
        limit: u32,
    },
    /// Delete a transcription by its ID.
    Delete {
        /// Transcription identifier.
        id: String,
    },
}

/// STT configuration subcommands.
#[derive(Debug, Subcommand)]
pub enum SttConfigCommand {
    /// Show the current STT configuration.
    Get,
    /// Update the STT configuration.
    Update {
        /// Backend to use (whisper, disabled).
        #[arg(long)]
        backend: Option<String>,
        /// Path to the Whisper model.
        #[arg(long)]
        model_path: Option<String>,
        /// Language (fr, en, auto).
        #[arg(long)]
        language: Option<String>,
    },
}

/// STT model management subcommands.
#[derive(Debug, Subcommand)]
pub enum SttModelCommand {
    /// List `.bin` model files in ~/.apollia/models/.
    List,

    /// Download a model from HuggingFace.
    Download {
        /// Model name (e.g. `whisper-large-v3-fr-q5_0`).
        name: String,
    },
}

// ─── Entry point ──────────────────────────────────────────────────────────

/// Execute an `stt` subcommand.
///
/// Returns the process exit code (0 = success, 1 = error, 2 = runtime offline).
pub async fn run(cmd: &SttCommand, socket: Option<PathBuf>, json: bool) -> i32 {
    match cmd {
        SttCommand::Status => run_status(socket, json).await,
        SttCommand::Transcribe { file, output } => {
            run_transcribe(file, output.as_deref(), socket, json).await
        }
        SttCommand::Transcriptions { command } => match command {
            TranscriptionsCommand::List { limit } => {
                run_transcriptions_list(*limit, socket, json).await
            }
            TranscriptionsCommand::Delete { id } => {
                run_transcription_delete(id, socket, json).await
            }
        },
        SttCommand::Model { command } => match command {
            SttModelCommand::List => run_model_list(socket, json).await,
            SttModelCommand::Download { name } => run_model_download(name, json),
        },
        SttCommand::Config { command } => match command {
            SttConfigCommand::Get => run_config_get(socket, json).await,
            SttConfigCommand::Update {
                backend,
                model_path,
                language,
            } => {
                run_config_update(
                    backend.as_deref(),
                    model_path.as_deref(),
                    language.as_deref(),
                    socket,
                    json,
                )
                .await
            }
        },
    }
}

// ─── Handlers ─────────────────────────────────────────────────────────────

/// `apollia-os stt status` — display STT engine status.
async fn run_status(socket: Option<PathBuf>, json: bool) -> i32 {
    let client = make_client(socket);

    match client.stt_status().await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                let enabled = resp["enabled"].as_bool().unwrap_or(false);
                let model_loaded = resp["model_loaded"].as_bool().unwrap_or(false);
                let model_name = resp["model_name"].as_str().unwrap_or("(none)");
                let model_path = resp["model_path"].as_str().unwrap_or("(none)");
                let backend = resp["backend_name"].as_str().unwrap_or("(none)");
                let metal = resp["metal_enabled"].as_bool().unwrap_or(false);
                let cuda = resp["cuda_enabled"].as_bool().unwrap_or(false);

                let status_icon = if enabled && model_loaded {
                    "ready"
                } else if enabled {
                    "loading"
                } else {
                    "disabled"
                };

                println!("  STT Engine: {status_icon}");
                println!("  Enabled:    {enabled}");
                println!("  Model:      {model_name}");
                println!("  Path:       {model_path}");
                println!("  Backend:    {backend}");

                let accel = match (metal, cuda) {
                    (true, _) => "Metal (Apple Silicon)",
                    (_, true) => "CUDA (NVIDIA)",
                    _ => "CPU",
                };
                println!("  Accel:      {accel}");
            }
            exit_codes::SUCCESS
        }
        Err(e) => handle_error(e, json),
    }
}

/// `apollia-os stt transcribe <file>` — transcribe an audio file via the runtime API.
///
/// Pre-checks that STT is enabled in the runtime config before attempting
/// the transcription. Returns a clear error if `stt.enabled = false`.
async fn run_transcribe(
    file: &PathBuf,
    output: Option<&std::path::Path>,
    socket: Option<PathBuf>,
    json: bool,
) -> i32 {
    if !file.exists() {
        let msg = format!("file not found: {}", file.display());
        if json {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({"error": msg}))
                    .unwrap_or_default()
            );
        } else {
            eprintln!("Error: {msg}");
        }
        return exit_codes::GENERAL_ERROR;
    }

    let audio_bytes = match std::fs::read(file) {
        Ok(b) => b,
        Err(e) => {
            let msg = format!("failed to read {}: {e}", file.display());
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({"error": msg}))
                        .unwrap_or_default()
                );
            } else {
                eprintln!("Error: {msg}");
            }
            return exit_codes::GENERAL_ERROR;
        }
    };

    let client = make_client(socket);

    // Pre-check: bail early with a clear message if STT is disabled.
    if let Ok(status) = client.stt_status().await {
        if status["enabled"].as_bool() == Some(false) {
            let msg =
                "STT is disabled in apollia.toml — set stt.enabled = true to use transcription";
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({"error": msg}))
                        .unwrap_or_default()
                );
            } else {
                eprintln!("Error: {msg}");
            }
            return exit_codes::GENERAL_ERROR;
        }
    }

    let boundary = "apollia-cli-boundary-7f3a";
    let mut body = Vec::new();
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(
        b"Content-Disposition: form-data; name=\"audio\"; filename=\"audio.wav\"\r\n",
    );
    body.extend_from_slice(b"Content-Type: audio/wav\r\n\r\n");
    body.extend_from_slice(&audio_bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());

    match client
        .post_multipart(
            "/api/v1/stt/transcribe",
            &body,
            &format!("multipart/form-data; boundary={boundary}"),
        )
        .await
    {
        Ok(resp) => {
            if let Some(out_path) = output {
                match std::fs::write(out_path, &resp.body) {
                    Ok(()) => {
                        if !json {
                            println!("Transcription saved to {}", out_path.display());
                        }
                    }
                    Err(e) => {
                        let msg = format!("failed to write {}: {e}", out_path.display());
                        if json {
                            println!(
                                "{}",
                                serde_json::to_string_pretty(&serde_json::json!({"error": msg}))
                                    .unwrap_or_default()
                            );
                        } else {
                            eprintln!("Error: {msg}");
                        }
                        return exit_codes::GENERAL_ERROR;
                    }
                }
            } else if json {
                let parsed: serde_json::Value =
                    serde_json::from_str(&resp.body).unwrap_or(serde_json::Value::Null);
                println!(
                    "{}",
                    serde_json::to_string_pretty(&parsed).unwrap_or_default()
                );
            } else {
                let parsed: serde_json::Value =
                    serde_json::from_str(&resp.body).unwrap_or(serde_json::Value::Null);
                let text = parsed["full_text"].as_str().unwrap_or("(no text)");
                println!("{text}");
            }
            exit_codes::SUCCESS
        }
        Err(e) => handle_error(e, json),
    }
}

/// `apollia-os stt transcriptions list` — list transcription history.
async fn run_transcriptions_list(limit: u32, socket: Option<PathBuf>, json: bool) -> i32 {
    let client = make_client(socket);

    match client.stt_transcriptions(limit, 0).await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                let transcriptions = resp["transcriptions"].as_array();
                match transcriptions {
                    Some(items) if items.is_empty() => {
                        println!("  (no transcriptions)");
                    }
                    Some(items) => {
                        println!("  {:<34} {:<8} {:<6} TEXT", "ID", "SOURCE", "LANG");
                        for item in items {
                            let id = item["id"].as_str().unwrap_or("?");
                            let source = item["source"].as_str().unwrap_or("?");
                            let lang = item["language"].as_str().unwrap_or("-");
                            let text = item["full_text"].as_str().unwrap_or("");
                            let truncated = if text.len() > 60 {
                                format!("{}...", &text[..57])
                            } else {
                                text.to_string()
                            };
                            println!("  {:<34} {:<8} {:<6} {}", id, source, lang, truncated);
                        }
                    }
                    None => {
                        println!("  (unexpected response format)");
                    }
                }
            }
            exit_codes::SUCCESS
        }
        Err(e) => handle_error(e, json),
    }
}

/// `apollia-os stt model list` — list `.bin` model files via the runtime API.
async fn run_model_list(socket: Option<PathBuf>, json: bool) -> i32 {
    let client = make_client(socket);

    match client.stt_models().await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                let models = resp["models"].as_array();
                match models {
                    Some(items) if items.is_empty() => {
                        println!("  (no .bin models found in ~/.apollia/models/)");
                    }
                    Some(items) => {
                        println!("  {:<48} SIZE", "NAME");
                        for m in items {
                            let name = m["name"].as_str().unwrap_or("?");
                            let size = m["size_bytes"].as_u64().unwrap_or(0);
                            let size_mb = size as f64 / (1024.0 * 1024.0);
                            println!("  {:<48} {size_mb:.1} MB", name);
                        }
                    }
                    None => {
                        println!("  (unexpected response format)");
                    }
                }
            }
            exit_codes::SUCCESS
        }
        Err(e) => handle_error(e, json),
    }
}

/// `apollia-os stt model download <name>` — download a model from HuggingFace.
///
/// Uses the `hf-hub` crate directly (no runtime connection required).
/// Default repository: `bofenghuang/whisper-large-v3-french`.
fn run_model_download(name: &str, json: bool) -> i32 {
    let (repo_id, filename) = resolve_model_spec(name);

    let dest_dir = default_models_dir();
    if let Err(e) = std::fs::create_dir_all(&dest_dir) {
        let msg = format!("failed to create {}: {e}", dest_dir.display());
        if json {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({"error": msg}))
                    .unwrap_or_default()
            );
        } else {
            eprintln!("Error: {msg}");
        }
        return exit_codes::GENERAL_ERROR;
    }

    let dest_path = dest_dir.join(&filename);
    if dest_path.exists() {
        if json {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "status": "already_exists",
                    "path": dest_path.display().to_string(),
                }))
                .unwrap_or_default()
            );
        } else {
            println!(
                "Model already exists: {}\nDelete it first to re-download.",
                dest_path.display()
            );
        }
        return exit_codes::SUCCESS;
    }

    if !json {
        println!("Downloading {filename} from {repo_id}...");
    }

    let api = hf_hub::api::sync::Api::new();
    let api = match api {
        Ok(a) => a,
        Err(e) => {
            let msg = format!("failed to initialize HuggingFace API: {e}");
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({"error": msg}))
                        .unwrap_or_default()
                );
            } else {
                eprintln!("Error: {msg}");
            }
            return exit_codes::GENERAL_ERROR;
        }
    };

    let repo = api.model(repo_id.to_string());

    let pb = if json {
        None
    } else {
        let bar = indicatif::ProgressBar::new_spinner();
        bar.set_style(
            indicatif::ProgressStyle::with_template("{spinner:.green} {msg} [{elapsed_precise}]")
                .unwrap_or_else(|_| indicatif::ProgressStyle::default_spinner()),
        );
        bar.set_message(format!("Downloading {filename}..."));
        bar.enable_steady_tick(std::time::Duration::from_millis(120));
        Some(bar)
    };

    let cached_path = match repo.get(&filename) {
        Ok(p) => p,
        Err(e) => {
            if let Some(ref bar) = pb {
                bar.finish_and_clear();
            }
            let msg = format!("download failed: {e}");
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({"error": msg}))
                        .unwrap_or_default()
                );
            } else {
                eprintln!("Error: {msg}");
            }
            return exit_codes::GENERAL_ERROR;
        }
    };

    if let Some(ref bar) = pb {
        bar.set_message("Copying to models directory...");
    }

    if let Err(e) = std::fs::copy(&cached_path, &dest_path) {
        if let Some(ref bar) = pb {
            bar.finish_and_clear();
        }
        let msg = format!(
            "failed to copy {} to {}: {e}",
            cached_path.display(),
            dest_path.display()
        );
        if json {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({"error": msg}))
                    .unwrap_or_default()
            );
        } else {
            eprintln!("Error: {msg}");
        }
        return exit_codes::GENERAL_ERROR;
    }

    if let Some(ref bar) = pb {
        bar.finish_and_clear();
    }

    let size_bytes = std::fs::metadata(&dest_path).map(|m| m.len()).unwrap_or(0);
    let size_mb = size_bytes as f64 / (1024.0 * 1024.0);

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "status": "downloaded",
                "path": dest_path.display().to_string(),
                "size_bytes": size_bytes,
                "size_mb": format!("{size_mb:.1}"),
            }))
            .unwrap_or_default()
        );
    } else {
        println!("Downloaded: {} ({size_mb:.1} MB)", dest_path.display());
        println!("Restart Apollia OS to load the new model.");
    }

    exit_codes::SUCCESS
}

/// `apollia-os stt transcriptions delete <id>` — supprimer une transcription.
async fn run_transcription_delete(id: &str, socket: Option<PathBuf>, json: bool) -> i32 {
    let client = make_client(socket);

    match client.delete_transcription(id).await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                println!("✔ Transcription '{id}' deleted");
            }
            exit_codes::SUCCESS
        }
        Err(ClientError::ServerError { status: 404, body }) => {
            if json {
                let out = serde_json::json!({"error": body});
                println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
            } else {
                eprintln!("Error: transcription '{id}' introuvable");
            }
            exit_codes::GENERAL_ERROR
        }
        Err(e) => handle_error(e, json),
    }
}

/// `apollia-os stt config get` — afficher la configuration STT courante.
async fn run_config_get(socket: Option<PathBuf>, json: bool) -> i32 {
    let client = make_client(socket);

    match client.get_stt_config().await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                let backend = resp.get("backend").and_then(|v| v.as_str()).unwrap_or("?");
                let model_path = resp
                    .get("model_path")
                    .and_then(|v| v.as_str())
                    .unwrap_or("(none)");
                let language = resp
                    .get("language")
                    .and_then(|v| v.as_str())
                    .unwrap_or("auto");
                println!("  Backend    : {backend}");
                println!("  Model path : {model_path}");
                println!("  Language   : {language}");
            }
            exit_codes::SUCCESS
        }
        Err(e) => handle_error(e, json),
    }
}

/// `apollia-os stt config update` — modifier la configuration STT.
async fn run_config_update(
    backend: Option<&str>,
    model_path: Option<&str>,
    language: Option<&str>,
    socket: Option<PathBuf>,
    json: bool,
) -> i32 {
    let client = make_client(socket);

    let mut body = serde_json::json!({});
    if let Some(b) = backend {
        body["backend"] = serde_json::Value::String(b.to_string());
    }
    if let Some(mp) = model_path {
        body["model_path"] = serde_json::Value::String(mp.to_string());
    }
    if let Some(l) = language {
        body["language"] = serde_json::Value::String(l.to_string());
    }

    match client.update_stt_config(&body).await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                println!("✔ STT configuration updated");
            }
            exit_codes::SUCCESS
        }
        Err(e) => handle_error(e, json),
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────

/// Create a [`RuntimeClient`] from the optional socket path.
fn make_client(socket: Option<PathBuf>) -> RuntimeClient {
    let socket_path = socket.unwrap_or_else(|| PathBuf::from(DEFAULT_SOCKET_PATH));
    RuntimeClient::new(socket_path)
}

/// Handle a [`ClientError`] with consistent output formatting.
fn handle_error(err: ClientError, json: bool) -> i32 {
    match &err {
        ClientError::ConnectionRefused => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "error": "runtime not started (connection refused)"
                    }))
                    .unwrap_or_default()
                );
            } else {
                eprintln!("Error: runtime not started (connection refused)");
                eprintln!("Start the runtime first: apollia-os start");
            }
            exit_codes::RUNTIME_ERROR
        }
        _ => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({"error": err.to_string()}))
                        .unwrap_or_default()
                );
            } else {
                eprintln!("Error: {err}");
            }
            exit_codes::GENERAL_ERROR
        }
    }
}

/// Return the default models directory: `~/.apollia/models/`.
fn default_models_dir() -> PathBuf {
    std::env::var("HOME")
        .ok()
        .map(|h| PathBuf::from(h).join(".apollia").join("models"))
        .unwrap_or_else(|| PathBuf::from("/tmp/apollia/models"))
}

/// Resolve a model name to a `(repo_id, filename)` pair.
///
/// Recognizes the built-in alias `whisper-large-v3-fr-q5_0` for the default
/// French model. Other names are treated as HuggingFace `repo_id/filename`
/// if they contain a `/`, or as a filename in the default repo otherwise.
fn resolve_model_spec(name: &str) -> (&str, String) {
    const DEFAULT_REPO: &str = "bofenghuang/whisper-large-v3-french";
    const DEFAULT_FILENAME: &str = "whisper-large-v3-fr-q5_0.bin";

    match name {
        "whisper-large-v3-fr-q5_0" | "default" => (DEFAULT_REPO, DEFAULT_FILENAME.to_string()),
        other if other.contains('/') => {
            let parts: Vec<&str> = other.splitn(3, '/').collect();
            if parts.len() >= 3 {
                let repo = &other[..parts[0].len() + 1 + parts[1].len()];
                let file = parts[2].to_string();
                (repo, file)
            } else {
                (other, DEFAULT_FILENAME.to_string())
            }
        }
        other => (DEFAULT_REPO, format!("{other}.bin")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // GIVEN the built-in alias "whisper-large-v3-fr-q5_0"
    // WHEN resolve_model_spec is called
    // THEN it returns the default repo and filename
    #[test]
    fn resolve_default_model_alias() {
        let (repo, file) = resolve_model_spec("whisper-large-v3-fr-q5_0");
        assert_eq!(repo, "bofenghuang/whisper-large-v3-french");
        assert_eq!(file, "whisper-large-v3-fr-q5_0.bin");
    }

    // GIVEN the "default" alias
    // WHEN resolve_model_spec is called
    // THEN it returns the default repo and filename
    #[test]
    fn resolve_default_keyword() {
        let (repo, file) = resolve_model_spec("default");
        assert_eq!(repo, "bofenghuang/whisper-large-v3-french");
        assert_eq!(file, "whisper-large-v3-fr-q5_0.bin");
    }

    // GIVEN a repo/owner/filename format
    // WHEN resolve_model_spec is called
    // THEN it splits correctly into repo and filename
    #[test]
    fn resolve_full_repo_path() {
        let (repo, file) = resolve_model_spec("ggerganov/whisper.cpp/ggml-base.bin");
        assert_eq!(repo, "ggerganov/whisper.cpp");
        assert_eq!(file, "ggml-base.bin");
    }

    // GIVEN a bare name without slashes
    // WHEN resolve_model_spec is called
    // THEN it uses the default repo and appends .bin
    #[test]
    fn resolve_bare_name() {
        let (repo, file) = resolve_model_spec("ggml-base");
        assert_eq!(repo, "bofenghuang/whisper-large-v3-french");
        assert_eq!(file, "ggml-base.bin");
    }

    // GIVEN the default models directory function
    // WHEN HOME is set
    // THEN the path ends with .apollia/models
    #[test]
    fn default_models_dir_uses_home() {
        let dir = default_models_dir();
        let dir_str = dir.to_string_lossy();
        assert!(
            dir_str.ends_with(".apollia/models") || dir_str.contains("apollia"),
            "expected models dir to contain apollia, got: {dir_str}"
        );
    }

    // ─── Nouveau: Config et TranscriptionsDelete ──────────────────────────

    use clap::Parser;

    #[derive(Debug, Parser)]
    struct TestCli {
        #[command(subcommand)]
        command: SttCommand,
    }

    #[test]
    fn test_stt_config_get_parses() {
        // GIVEN "config get"
        // WHEN
        let cli = TestCli::parse_from(["apollia-os", "config", "get"]);
        // THEN SttCommand::Config { command: SttConfigCommand::Get }
        match &cli.command {
            SttCommand::Config { command } => {
                assert!(matches!(command, SttConfigCommand::Get))
            }
            other => panic!("expected Config, got {other:?}"),
        }
    }

    #[test]
    fn test_stt_config_update_parses() {
        // GIVEN "config update --backend whisper --language fr"
        // WHEN
        let cli = TestCli::parse_from([
            "apollia-os",
            "config",
            "update",
            "--backend",
            "whisper",
            "--language",
            "fr",
        ]);
        // THEN SttConfigCommand::Update avec les bons champs
        match &cli.command {
            SttCommand::Config { command } => match command {
                SttConfigCommand::Update {
                    backend,
                    model_path,
                    language,
                } => {
                    assert_eq!(backend.as_deref(), Some("whisper"));
                    assert!(model_path.is_none());
                    assert_eq!(language.as_deref(), Some("fr"));
                }
                other => panic!("expected Update, got {other:?}"),
            },
            other => panic!("expected Config, got {other:?}"),
        }
    }

    #[test]
    fn test_stt_config_update_with_model_path_parses() {
        // GIVEN "config update --model-path /tmp/my.bin"
        // WHEN
        let cli = TestCli::parse_from([
            "apollia-os",
            "config",
            "update",
            "--model-path",
            "/tmp/my.bin",
        ]);
        // THEN model_path = Some("/tmp/my.bin")
        match &cli.command {
            SttCommand::Config { command } => match command {
                SttConfigCommand::Update { model_path, .. } => {
                    assert_eq!(model_path.as_deref(), Some("/tmp/my.bin"))
                }
                other => panic!("expected Update, got {other:?}"),
            },
            other => panic!("expected Config, got {other:?}"),
        }
    }

    #[test]
    fn test_stt_transcriptions_delete_parses() {
        // GIVEN "transcriptions delete t-abc123"
        // WHEN
        let cli = TestCli::parse_from(["apollia-os", "transcriptions", "delete", "t-abc123"]);
        // THEN TranscriptionsCommand::Delete { id: "t-abc123" }
        match &cli.command {
            SttCommand::Transcriptions { command } => match command {
                TranscriptionsCommand::Delete { id } => assert_eq!(id, "t-abc123"),
                other => panic!("expected Delete, got {other:?}"),
            },
            other => panic!("expected Transcriptions, got {other:?}"),
        }
    }
}
