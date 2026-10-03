//! `apollia-os stt` subcommands: Speech-to-Text management.
//!
//! Provides `status`, `transcribe`, `transcriptions`, and `model` subcommands
//! for interacting with the STT engine from the terminal.

use std::path::{Path, PathBuf};

use clap::Subcommand;

use crate::client::{default_socket_path, ClientError, RuntimeClient};
use crate::exit_codes;
use crate::note;

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

        /// Skip the interactive confirmation prompt.
        #[arg(long)]
        confirm: bool,
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
            TranscriptionsCommand::Delete { id, confirm } => {
                run_transcription_delete(id, *confirm, socket, json).await
            }
        },
        SttCommand::Model { command } => match command {
            SttModelCommand::List => run_model_list(socket, json).await,
            SttModelCommand::Download { name } => run_model_download(name, json).await,
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

/// `apollia-os stt status`: display STT engine status.
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

/// `apollia-os stt transcribe <file>`: transcribe an audio file via the runtime API.
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
        return emit_stt_error(&format!("file not found: {}", file.display()), json);
    }

    let audio_bytes = match std::fs::read(file) {
        Ok(b) => b,
        Err(e) => {
            return emit_stt_error(&format!("failed to read {}: {e}", file.display()), json);
        }
    };

    let client = make_client(socket);

    // Pre-check: bail early with a clear message if STT is disabled.
    if let Ok(status) = client.stt_status().await {
        if status["enabled"].as_bool() == Some(false) {
            return emit_stt_error(
                "STT is disabled in apollia.toml - set stt.enabled = true to use transcription",
                json,
            );
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
        Ok(resp) => emit_transcribe_result(&resp.body, output, json),
        Err(e) => handle_error(e, json),
    }
}

/// Render a successful transcription response: write to `output` if given,
/// otherwise print the JSON envelope or the plain `full_text`.
fn emit_transcribe_result(body: &str, output: Option<&std::path::Path>, json: bool) -> i32 {
    if let Some(out_path) = output {
        if let Err(e) = std::fs::write(out_path, body) {
            return emit_stt_error(
                &format!("failed to write {}: {e}", out_path.display()),
                json,
            );
        }
        if !json {
            println!("Transcription saved to {}", out_path.display());
        }
        return exit_codes::SUCCESS;
    }

    let parsed: serde_json::Value = serde_json::from_str(body).unwrap_or(serde_json::Value::Null);
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&parsed).unwrap_or_default()
        );
    } else {
        let text = parsed["full_text"].as_str().unwrap_or("(no text)");
        println!("{text}");
    }
    exit_codes::SUCCESS
}

/// `apollia-os stt transcriptions list`: list transcription history.
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
                print_transcriptions_human(&resp);
            }
            exit_codes::SUCCESS
        }
        Err(e) => handle_error(e, json),
    }
}

/// Print the transcription history table in human-readable form.
fn print_transcriptions_human(resp: &serde_json::Value) {
    let Some(items) = resp["transcriptions"].as_array() else {
        println!("  (unexpected response format)");
        return;
    };
    if items.is_empty() {
        println!("  (no transcriptions)");
        return;
    }
    println!("  {:<34} {:<8} {:<6} TEXT", "ID", "SOURCE", "LANG");
    for item in items {
        let id = item["id"].as_str().unwrap_or("?");
        let source = item["source"].as_str().unwrap_or("?");
        let lang = item["language"].as_str().unwrap_or("-");
        let text = item["full_text"].as_str().unwrap_or("");
        let truncated = crate::commands::truncate_for_display(text, 60, 57, "...");
        println!("  {:<34} {:<8} {:<6} {}", id, source, lang, truncated);
    }
}

/// `apollia-os stt model list`: list `.bin` model files via the runtime API.
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

/// `apollia-os stt model download <name>`: download a model from HuggingFace.
///
/// The file goes through the verified downloader every model goes through: a
/// dictation entry of the catalogue is checked against the digest the table
/// pins, any other file against the SHA-256 its repository tree publishes, and
/// a file that does not match is deleted. No runtime connection is required.
/// Default model: the French fine-tune `whisper-large-v3-french-q5_0`.
async fn run_model_download(name: &str, json: bool) -> i32 {
    let spec = resolve_model_spec(name);

    let dest_dir = default_models_dir();
    if let Err(e) = std::fs::create_dir_all(&dest_dir) {
        return emit_stt_error(
            &format!("failed to create {}: {e}", dest_dir.display()),
            json,
        );
    }

    let dest_path = dest_dir.join(&spec.file);
    if dest_path.exists() {
        report_model_already_exists(&dest_path, json);
        return exit_codes::SUCCESS;
    }

    let expected_sha256 = match spec.pinned_sha256.clone() {
        Some(sha) => sha,
        None => match apollia_llm::HfRegistryClient::new(None)
            .lfs_sha256(&spec.repo, &spec.file)
            .await
        {
            Ok(sha) => sha,
            Err(e) => {
                return emit_stt_error(
                    &format!("cannot verify the download, refusing it: {e}"),
                    json,
                )
            }
        },
    };

    if !json {
        println!("Downloading {} from {}...", spec.file, spec.repo);
    }
    let pb = build_download_progress(&spec.file, json);
    let on_progress: apollia_llm::ProgressCallback = {
        let bar = pb.clone();
        std::sync::Arc::new(move |progress: apollia_llm::DownloadProgress| {
            if let Some(bar) = &bar {
                let mb = progress.downloaded_bytes as f64 / (1024.0 * 1024.0);
                bar.set_message(format!("Downloading... {mb:.1} MB"));
            }
        })
    };

    let request = download_request(&spec, dest_dir, expected_sha256);
    let result = apollia_llm::DownloadManager::new()
        .download(request, on_progress)
        .await;
    if let Some(ref bar) = pb {
        bar.finish_and_clear();
    }
    match result {
        Ok(outcome) => {
            report_model_downloaded(&outcome.dest_path, outcome.bytes, json);
            exit_codes::SUCCESS
        }
        Err(e) => emit_stt_error(&format!("download failed: {e}"), json),
    }
}

/// The request handed to the verified downloader for `spec`, checked against
/// `expected_sha256`.
fn download_request(
    spec: &ModelSpec,
    dest_dir: PathBuf,
    expected_sha256: String,
) -> apollia_llm::DownloadRequest {
    apollia_llm::DownloadRequest {
        url: format!(
            "https://huggingface.co/{}/resolve/main/{}",
            spec.repo, spec.file
        ),
        dest_dir,
        filename: Some(spec.file.clone()),
        hf_token: None,
        repo_id: None,
        expected_sha256: Some(expected_sha256),
    }
}

/// Report that the target model file is already present on disk.
fn report_model_already_exists(dest_path: &Path, json: bool) {
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
}

/// Report a successful model download (size + restart hint).
fn report_model_downloaded(dest_path: &Path, size_bytes: u64, json: bool) {
    let size_mb = size_bytes as f64 / (1024.0 * 1024.0);
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "status": "downloaded",
                "path": dest_path.display().to_string(),
                "size_bytes": size_bytes,
                "size_mb": format!("{size_mb:.1}"),
                "verified": true,
            }))
            .unwrap_or_default()
        );
    } else {
        println!("Downloaded: {} ({size_mb:.1} MB)", dest_path.display());
        println!("Restart Apollia OS to load the new model.");
    }
}

/// Build the spinner shown during a model download (none in JSON mode).
fn build_download_progress(filename: &str, json: bool) -> Option<indicatif::ProgressBar> {
    if json {
        return None;
    }
    let bar = indicatif::ProgressBar::new_spinner();
    bar.set_style(
        indicatif::ProgressStyle::with_template("{spinner:.green} {msg} [{elapsed_precise}]")
            .unwrap_or_else(|_| indicatif::ProgressStyle::default_spinner()),
    );
    bar.set_message(format!("Downloading {filename}..."));
    bar.enable_steady_tick(std::time::Duration::from_millis(120));
    Some(bar)
}

/// `apollia-os stt transcriptions delete <id>`: delete a transcription.
async fn run_transcription_delete(
    id: &str,
    confirm: bool,
    socket: Option<PathBuf>,
    json: bool,
) -> i32 {
    if let Some(code) =
        crate::output::require_confirmation(confirm, json, &format!("delete transcription '{id}'"))
    {
        return code;
    }
    let client = make_client(socket);

    match client.delete_transcription(id).await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                note!("✔ Transcription '{id}' deleted");
            }
            exit_codes::SUCCESS
        }
        Err(ClientError::ServerError { status: 404, .. }) => crate::output::emit_error(
            json,
            exit_codes::GENERAL_ERROR,
            &format!("transcription '{id}' not found"),
        ),
        Err(e) => handle_error(e, json),
    }
}

/// `apollia-os stt config get`: show the current STT configuration.
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

/// `apollia-os stt config update`: change the STT configuration.
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
                note!("✔ STT configuration updated");
            }
            exit_codes::SUCCESS
        }
        Err(e) => handle_error(e, json),
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────

/// Create a [`RuntimeClient`] from the optional socket path.
fn make_client(socket: Option<PathBuf>) -> RuntimeClient {
    let socket_path = socket.unwrap_or_else(default_socket_path);
    RuntimeClient::new(socket_path)
}

/// Emit an error message (JSON or human form) and return [`GENERAL_ERROR`].
fn emit_stt_error(msg: &str, json: bool) -> i32 {
    crate::output::emit_error(json, exit_codes::GENERAL_ERROR, msg)
}

/// Handle a [`ClientError`] with consistent output formatting.
fn handle_error(err: ClientError, json: bool) -> i32 {
    match &err {
        ClientError::ConnectionRefused => {
            let code = crate::output::emit_error(
                json,
                exit_codes::RUNTIME_ERROR,
                "runtime not started (connection refused)",
            );
            if !json {
                eprintln!("Start the runtime first: apollia-os start");
            }
            code
        }
        _ => crate::output::emit_error(json, exit_codes::GENERAL_ERROR, &err.to_string()),
    }
}

/// Return the default models directory: `~/.apollia/models/`. Dictation
/// models stay there whatever `[models] dir` says.
fn default_models_dir() -> PathBuf {
    apollia_core::paths::home_dir()
        .map(|h| apollia_core::config::default_models_dir(&apollia_core::paths::data_dir_under(h)))
        .unwrap_or_else(|| PathBuf::from("/tmp/apollia/models"))
}

/// The file a model name designates, and the digest the catalogue pins for
/// it when it is one of its dictation entries.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ModelSpec {
    /// HuggingFace repository, `owner/name`.
    repo: String,
    /// File within the repository, also its name in the models directory.
    file: String,
    /// The SHA-256 the catalogue pins; `None` for a file outside it, whose
    /// digest is read from the repository tree before the download.
    pinned_sha256: Option<String>,
}

/// Identifier of the catalogue entry the `default` alias designates.
const DEFAULT_ENTRY: &str = "whisper-large-v3-french-q5_0";

/// Resolve a model name to the file it designates.
///
/// `default` and the former alias `whisper-large-v3-fr-q5_0` name the French
/// fine-tune. A catalogue entry is named by its identifier (`whisper-base`)
/// or by its file stem (`ggml-base`). `owner/name/file` names any file of a
/// repository, the catalogue's digest applying when it is one of its entries.
/// Any other name is a `.bin` file of the default repository.
fn resolve_model_spec(name: &str) -> ModelSpec {
    let catalogue = apollia_llm::recommend::DictationCatalogue::embedded().ok();
    let entry_spec = |entry: &apollia_llm::recommend::DictationModel| ModelSpec {
        repo: entry.repo.clone(),
        file: entry.file.clone(),
        pinned_sha256: Some(entry.sha256.clone()),
    };
    let find = |pred: &dyn Fn(&apollia_llm::recommend::DictationModel) -> bool| {
        catalogue
            .as_ref()
            .and_then(|c| c.models().iter().find(|m| pred(m)).map(entry_spec))
    };
    let alias = match name {
        "default" | "whisper-large-v3-fr-q5_0" => DEFAULT_ENTRY,
        other => other,
    };
    if let Some(spec) = find(&|m| m.id == alias || m.file == format!("{alias}.bin")) {
        return spec;
    }
    let default = find(&|m| m.id == DEFAULT_ENTRY);
    let (default_repo, default_file) = default.map_or_else(
        || {
            (
                "bofenghuang/whisper-large-v3-french".to_owned(),
                "ggml-model-q5_0.bin".to_owned(),
            )
        },
        |d| (d.repo, d.file),
    );
    let parts: Vec<&str> = name.splitn(3, '/').collect();
    let (repo, file) = match parts.as_slice() {
        [owner, repo, file] => (format!("{owner}/{repo}"), (*file).to_owned()),
        [owner, repo] => (format!("{owner}/{repo}"), default_file),
        _ => (default_repo, format!("{name}.bin")),
    };
    find(&|m| m.repo == repo && m.file == file).unwrap_or(ModelSpec {
        repo,
        file,
        pinned_sha256: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_aliases_name_the_french_entry_and_its_pinned_digest() {
        // GIVEN the two names the default model has always answered to
        for name in ["default", "whisper-large-v3-fr-q5_0"] {
            // WHEN each is resolved
            let spec = resolve_model_spec(name);

            // THEN it is the French file the repository really carries, with
            // the digest the catalogue pins for it
            assert_eq!(spec.repo, "bofenghuang/whisper-large-v3-french", "{name}");
            assert_eq!(spec.file, "ggml-model-q5_0.bin", "{name}");
            assert_eq!(
                spec.pinned_sha256.as_deref(),
                Some("d8da7cb6cdadfac47829abf46fe8cd36fcfef06db5601bfd8acbcd40579010a5"),
                "{name}"
            );
        }
    }

    #[test]
    fn a_catalogue_entry_is_found_by_id_by_stem_and_by_its_full_path() {
        // GIVEN the three ways to name the Base model of whisper.cpp
        let names = [
            "whisper-base",
            "ggml-base",
            "ggerganov/whisper.cpp/ggml-base.bin",
        ];

        // WHEN each is resolved
        let specs: Vec<ModelSpec> = names.iter().map(|n| resolve_model_spec(n)).collect();

        // THEN all three are the same pinned file
        for spec in &specs {
            assert_eq!(spec.repo, "ggerganov/whisper.cpp");
            assert_eq!(spec.file, "ggml-base.bin");
            assert_eq!(spec.pinned_sha256.as_ref().map(String::len), Some(64));
        }
    }

    #[test]
    fn a_name_outside_the_catalogue_has_no_pinned_digest() {
        // GIVEN a bare name and a full path the catalogue does not carry
        // WHEN they are resolved
        let bare = resolve_model_spec("e2e-stt-present");
        let foreign = resolve_model_spec("acme/whisper-x/ggml-x.bin");

        // THEN each is its file, and its digest is left to the repository tree
        assert_eq!(bare.repo, "bofenghuang/whisper-large-v3-french");
        assert_eq!(bare.file, "e2e-stt-present.bin");
        assert_eq!(bare.pinned_sha256, None);
        assert_eq!(foreign.repo, "acme/whisper-x");
        assert_eq!(foreign.file, "ggml-x.bin");
        assert_eq!(foreign.pinned_sha256, None);
    }

    #[test]
    fn the_download_request_carries_the_digest_it_is_checked_against() {
        // GIVEN the resolved default model and a models directory
        let spec = resolve_model_spec("default");
        let sha = spec.pinned_sha256.clone().expect("pinned");
        let dir = PathBuf::from("/tmp/apollia-models");

        // WHEN the request for the verified downloader is built
        let request = download_request(&spec, dir.clone(), sha.clone());

        // THEN it fetches the repository file and refuses any other bytes
        assert_eq!(
            request.url,
            "https://huggingface.co/bofenghuang/whisper-large-v3-french/resolve/main/ggml-model-q5_0.bin"
        );
        assert_eq!(request.dest_dir, dir);
        assert_eq!(request.filename.as_deref(), Some("ggml-model-q5_0.bin"));
        assert_eq!(request.expected_sha256, Some(sha));
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

    // ─── Config and TranscriptionsDelete ──────────────────────────

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
        // THEN SttConfigCommand::Update with the expected fields
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
                TranscriptionsCommand::Delete { id, confirm } => {
                    assert_eq!(id, "t-abc123");
                    assert!(!confirm, "the confirmation is opt-in, never the default");
                }
                other => panic!("expected Delete, got {other:?}"),
            },
            other => panic!("expected Transcriptions, got {other:?}"),
        }
    }

    #[test]
    fn parses_transcribe_default_no_output() {
        use clap::Parser;
        #[derive(Parser, Debug)]
        struct LocalCli {
            #[command(subcommand)]
            cmd: SttCommand,
        }
        // GIVEN "stt transcribe /tmp/audio.wav", with no --output
        let cli = LocalCli::parse_from(["x", "transcribe", "/tmp/audio.wav"]);
        // WHEN clap parses the argument line
        // THEN the file is captured and the transcript goes to the terminal
        match cli.cmd {
            SttCommand::Transcribe { file, output } => {
                assert_eq!(file, PathBuf::from("/tmp/audio.wav"));
                assert!(output.is_none());
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parses_config_get() {
        use clap::Parser;
        #[derive(Parser, Debug)]
        struct LocalCli {
            #[command(subcommand)]
            cmd: SttCommand,
        }
        // GIVEN "stt config get"
        let cli = LocalCli::parse_from(["x", "config", "get"]);
        // WHEN clap parses the argument line
        // THEN the nested get subcommand is selected
        match cli.cmd {
            SttCommand::Config { command } => match command {
                SttConfigCommand::Get => {}
                other => panic!("unexpected: {other:?}"),
            },
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parses_config_update_with_backend_only() {
        use clap::Parser;
        #[derive(Parser, Debug)]
        struct LocalCli {
            #[command(subcommand)]
            cmd: SttCommand,
        }
        // GIVEN a config update carrying only --backend
        let cli = LocalCli::parse_from(["x", "config", "update", "--backend", "whisper"]);
        // WHEN clap parses the argument line
        // THEN the backend is captured and the untouched fields stay unset, so an update changes one thing
        match cli.cmd {
            SttCommand::Config { command } => match command {
                SttConfigCommand::Update {
                    backend,
                    model_path,
                    language,
                } => {
                    assert_eq!(backend.as_deref(), Some("whisper"));
                    assert!(model_path.is_none());
                    assert!(language.is_none());
                }
                other => panic!("unexpected: {other:?}"),
            },
            other => panic!("unexpected: {other:?}"),
        }
    }
}
