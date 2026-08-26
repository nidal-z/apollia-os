//! `apollia-os llm` subcommands: diagnose and test LLM backends via the runtime API.
//!
//! Provides `status`, `ping`, `chat`, `costs` and `backends` (CRUD) operations
//! for LLM backend management.

use std::path::PathBuf;

use clap::Subcommand;

use crate::client::{default_socket_path, ClientError, RuntimeClient};
use crate::exit_codes;

mod backends;
mod backends_crud;
mod costs;
mod runtime;

use backends::run_backends;
use costs::{run_costs, run_setup, SetupArgs};
use runtime::{run_chat, run_ping, run_reload, run_status};

/// LLM subcommands: `apollia-os llm <verb>`.
#[derive(Debug, Subcommand)]
pub enum LlmCommand {
    /// Display the status of all configured LLM backends.
    Status,
    /// Measure the latency of a specific LLM backend.
    Ping {
        /// Backend name (default: the router's configured default backend).
        backend: Option<String>,
    },
    /// Send a direct prompt to an LLM backend and print the response.
    Chat {
        /// The prompt text to send to the LLM.
        prompt: String,
        /// Backend to use (optional, uses the configured default if omitted).
        #[arg(long)]
        backend: Option<String>,
    },
    /// Display aggregated usage and costs (tokens and estimated cost per backend).
    ///
    /// Without flags, prints the cost table. Use `--get-threshold` to print
    /// `[llm] cost_alert_threshold_usd` from `apollia.toml`, or
    /// `--threshold N` to set it.
    Costs {
        /// Read the cost alert threshold from `apollia.toml` instead of the
        /// cost table.
        #[arg(long, conflicts_with = "threshold")]
        get_threshold: bool,
        /// Set the cost alert threshold (USD). Writes
        /// `[llm] cost_alert_threshold_usd = N` to `apollia.toml`. Pass `0`
        /// or a negative value to clear the threshold.
        #[arg(long, value_name = "USD")]
        threshold: Option<f64>,
        /// Optional config file path override (default: `~/.apollia/apollia.toml`).
        #[arg(long, value_name = "PATH")]
        config: Option<PathBuf>,
    },
    /// Manage configured LLM backends (list, create, update, delete, set-default).
    Backends {
        /// Backends subcommand.
        #[command(subcommand)]
        command: LlmBackendsCommand,
    },
    /// Reload the LLM router from `system.db` without restarting the runtime.
    ///
    /// `backends create/update/delete/set-default` write to the database but
    /// the in-memory router stays frozen until a reload. This command swaps
    /// the active router in place without interrupting running tasks.
    Reload,

    /// First-run helper: configure a local LLM in one step.
    ///
    /// `--local` is the only mode supported today. It expects a `.gguf` model
    /// path, copies it into `~/.apollia/models/` (no copy when already there),
    /// and creates a backend named `local` with `provider=llama-cpp` and
    /// `is_default=true` in `system.db`. The runtime picks the new default
    /// on the next `llm reload` (or daemon restart).
    Setup {
        /// Use the local llama-cpp backend (required for v0.1.0; a cloud
        /// provider is declared with `llm backends create --api-key`).
        #[arg(long)]
        local: bool,
        /// Path to the `.gguf` model file.
        #[arg(long, value_name = "PATH")]
        model: PathBuf,
        /// Backend name (default: `local`). Overwrites the existing entry of
        /// the same name.
        #[arg(long, value_name = "NAME", default_value = "local")]
        name: String,
        /// Device hint for llama-cpp: `metal` (macOS default), `cuda`, `cpu`.
        /// When omitted, picks `metal` on macOS and `cpu` elsewhere.
        #[arg(long, value_name = "DEVICE")]
        device: Option<String>,
        /// Override the system database path (default: `~/.apollia/system.db`).
        #[arg(long, value_name = "PATH")]
        system_db: Option<PathBuf>,
        /// Override the models directory (default: `~/.apollia/models/`).
        #[arg(long, value_name = "DIR")]
        models_dir: Option<PathBuf>,
    },
}

/// Backends CRUD subcommands: `apollia-os llm backends <verb>`.
#[derive(Debug, Subcommand)]
pub enum LlmBackendsCommand {
    /// List all configured LLM backends.
    List,
    /// Show the full configuration of a backend (including config_json).
    Show {
        /// Backend name.
        name: String,
    },
    /// Create a new LLM backend.
    ///
    /// `--provider` drives the shape of the `config_json` sent to the
    /// runtime. For `llama-cpp` (local GGUF models), `--model` must be the
    /// absolute path to the .gguf file. For cloud providers, `--model` is
    /// the identifier (e.g. `claude-sonnet-4-6`, `gpt-4o`).
    Create {
        /// Unique backend name (snake_case or kebab-case).
        name: String,
        /// Provider: `llama-cpp` (local GGUF), `anthropic`, `openai`,
        /// `mistral`, `ollama`. `--kind` is accepted as an alias for
        /// backward compatibility.
        #[arg(long, alias = "kind", value_name = "PROVIDER")]
        provider: String,
        /// Model identifier or path (absolute path for `llama-cpp`).
        #[arg(long)]
        model: String,
        /// API key (cloud providers only). Stored as-is in
        /// `config_json.api_key`. Prefer `--api-key-env VAR_NAME` to avoid
        /// persisting the key in system.db.
        #[arg(long, value_name = "KEY")]
        api_key: Option<String>,
        /// Environment variable name holding the API key.
        ///
        /// The runtime reads `std::env::var(NAME)` at boot. Recommended to
        /// keep the key out of system.db.
        #[arg(long, value_name = "VAR_NAME")]
        api_key_env: Option<String>,
        /// Base URL (Ollama, self-hosted OpenAI-compatible gateway, ...).
        #[arg(long, value_name = "URL")]
        base_url: Option<String>,
        /// Device for `llama-cpp` models: `metal` (Apple), `cuda`, `cpu`.
        #[arg(long, value_name = "DEVICE", default_value = "metal")]
        device: String,
        /// How long the backend may stay silent before the call is abandoned.
        ///
        /// This is a backstop against a wedged backend, not a latency policy.
        /// On the non-streaming path a server sends nothing until generation is
        /// complete, so this budget has to cover the slowest honest answer: a
        /// large model on modest hardware legitimately takes minutes. Values
        /// below 60 seconds are raised to 60.
        #[arg(long, value_name = "SECS", default_value = "600")]
        timeout_sec: u64,
        /// Usable context window of this backend, in tokens.
        ///
        /// Sizes conversation compaction. A self-hosted OpenAI-compatible
        /// server does not report its window, and Ollama sizes its own from the
        /// machine's memory, so without this the runtime falls back to a generic
        /// limit that can exceed what the server actually loaded. Ollama
        /// backends are probed automatically when the model is loaded; set this
        /// to pin the value.
        #[arg(long, value_name = "TOKENS")]
        context_window: Option<usize>,
        /// Create the backend disabled.
        #[arg(long)]
        disabled: bool,
        /// Mark this backend as the default (only one at a time).
        #[arg(long, alias = "is-default")]
        default: bool,
    },
    /// Update an existing LLM backend.
    ///
    /// Works in merge mode: flags that are not supplied keep their current
    /// value. The runtime exposes `PUT` as replace, so the CLI fetches the
    /// existing config first and only overwrites the fields the operator
    /// changed.
    Update {
        /// Backend name to update.
        name: String,
        /// New provider (rarely useful, changes the backend implementation).
        #[arg(long, alias = "kind", value_name = "PROVIDER")]
        provider: Option<String>,
        /// New model (absolute path for `llama-cpp`).
        #[arg(long)]
        model: Option<String>,
        /// New API key (cloud providers).
        #[arg(long, value_name = "KEY")]
        api_key: Option<String>,
        /// New environment variable name for the API key.
        #[arg(long, value_name = "VAR_NAME")]
        api_key_env: Option<String>,
        /// New base URL.
        #[arg(long, value_name = "URL")]
        base_url: Option<String>,
        /// New device for `llama-cpp`.
        #[arg(long, value_name = "DEVICE")]
        device: Option<String>,
        /// New inference timeout in seconds.
        #[arg(long, value_name = "SECS")]
        timeout_sec: Option<u64>,
        /// Enable the backend.
        #[arg(long, conflicts_with = "disable")]
        enable: bool,
        /// Disable the backend.
        #[arg(long, conflicts_with = "enable")]
        disable: bool,
        /// Mark as default.
        #[arg(long, alias = "is-default")]
        default: bool,
    },
    /// Delete an LLM backend.
    Delete {
        /// Backend name to delete.
        name: String,
        /// Confirm deletion without an interactive prompt.
        #[arg(long)]
        confirm: bool,
    },
    /// Set a backend as the default backend.
    SetDefault {
        /// Backend name to mark as default.
        name: String,
    },
}

/// Execute a `llm` subcommand.
///
/// Returns the process exit code: `0` = success, non-zero = error.
pub async fn run(cmd: &LlmCommand, socket: Option<PathBuf>, json: bool) -> i32 {
    let socket_path = socket.unwrap_or_else(default_socket_path);
    let client = RuntimeClient::new(socket_path);

    match cmd {
        LlmCommand::Status => run_status(&client, json).await,
        LlmCommand::Ping { backend } => run_ping(&client, backend.as_deref(), json).await,
        LlmCommand::Chat { prompt, backend } => {
            run_chat(&client, prompt, backend.as_deref(), json).await
        }
        LlmCommand::Costs {
            get_threshold,
            threshold,
            config,
        } => run_costs(&client, *get_threshold, *threshold, config.as_deref(), json).await,
        LlmCommand::Backends { command } => run_backends(&client, command, json).await,
        LlmCommand::Reload => run_reload(&client, json).await,
        LlmCommand::Setup {
            local,
            model,
            name,
            device,
            system_db,
            models_dir,
        } => run_setup(SetupArgs {
            local: *local,
            model,
            backend_name: name,
            device_override: device.as_deref(),
            system_db_override: system_db.as_deref(),
            models_dir_override: models_dir.as_deref(),
            json,
        }),
    }
}
// ─────────────────────────────────────────────
// Error helpers
// ─────────────────────────────────────────────

/// Handle client-level errors uniformly.
fn handle_error(err: ClientError, json: bool) -> i32 {
    match err {
        ClientError::ConnectionRefused => crate::output::emit_error(
            json,
            exit_codes::RUNTIME_ERROR,
            "runtime not started (connection refused)",
        ),
        other => crate::output::emit_error(json, exit_codes::GENERAL_ERROR, &other.to_string()),
    }
}

/// Handle HTTP server errors uniformly.
fn handle_server_error(status: u16, body: &str, json: bool) -> i32 {
    let error_msg = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(String::from))
        .unwrap_or_else(|| format!("server error ({status})"));

    crate::output::emit_error(json, exit_codes::GENERAL_ERROR, &error_msg.to_string())
}
#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::backends::{
        backend_security_notes, build_config_json, is_ollama_cloud_model, BuildConfigArgs,
    };
    use super::costs::infer_quantization;
    use super::*;

    #[derive(Debug, Parser)]
    struct TestCli {
        #[command(subcommand)]
        command: LlmCommand,
    }

    #[test]
    fn test_llm_status_parses() {
        // GIVEN "status"
        // WHEN
        let cli = TestCli::parse_from(["apollia-os", "status"]);
        // THEN LlmCommand::Status
        assert!(matches!(cli.command, LlmCommand::Status));
    }

    #[test]
    fn test_llm_ping_no_backend_parses() {
        // GIVEN "ping" without an argument
        // WHEN
        let cli = TestCli::parse_from(["apollia-os", "ping"]);
        // THEN LlmCommand::Ping { backend: None }
        match &cli.command {
            LlmCommand::Ping { backend } => assert!(backend.is_none()),
            other => panic!("expected Ping, got {other:?}"),
        }
    }

    #[test]
    fn test_llm_ping_with_backend_parses() {
        // GIVEN "ping anthropic"
        // WHEN
        let cli = TestCli::parse_from(["apollia-os", "ping", "anthropic"]);
        // THEN LlmCommand::Ping { backend: Some("anthropic") }
        match &cli.command {
            LlmCommand::Ping { backend } => assert_eq!(backend.as_deref(), Some("anthropic")),
            other => panic!("expected Ping, got {other:?}"),
        }
    }

    #[test]
    fn test_llm_costs_parses() {
        // GIVEN "costs"
        // WHEN
        let cli = TestCli::parse_from(["apollia-os", "costs"]);
        // THEN LlmCommand::Costs (no threshold flags)
        assert!(matches!(
            cli.command,
            LlmCommand::Costs {
                get_threshold: false,
                threshold: None,
                ..
            }
        ));
    }

    #[test]
    fn test_llm_costs_get_threshold_parses() {
        let cli = TestCli::parse_from(["apollia-os", "costs", "--get-threshold"]);
        match &cli.command {
            LlmCommand::Costs {
                get_threshold,
                threshold,
                ..
            } => {
                assert!(*get_threshold);
                assert!(threshold.is_none());
            }
            other => panic!("expected Costs, got {other:?}"),
        }
    }

    #[test]
    fn test_llm_costs_set_threshold_parses() {
        let cli = TestCli::parse_from(["apollia-os", "costs", "--threshold", "0.5"]);
        match &cli.command {
            LlmCommand::Costs {
                get_threshold,
                threshold,
                ..
            } => {
                assert!(!*get_threshold);
                assert_eq!(*threshold, Some(0.5));
            }
            other => panic!("expected Costs, got {other:?}"),
        }
    }

    #[test]
    fn test_llm_costs_threshold_conflicts_with_get_threshold() {
        let result = TestCli::try_parse_from([
            "apollia-os",
            "costs",
            "--threshold",
            "0.5",
            "--get-threshold",
        ]);
        assert!(
            result.is_err(),
            "--threshold + --get-threshold must conflict"
        );
    }

    #[test]
    fn test_llm_setup_local_parses() {
        let cli = TestCli::parse_from([
            "apollia-os",
            "setup",
            "--local",
            "--model",
            "/tmp/model.gguf",
        ]);
        match &cli.command {
            LlmCommand::Setup {
                local, model, name, ..
            } => {
                assert!(*local);
                assert_eq!(model, &PathBuf::from("/tmp/model.gguf"));
                assert_eq!(name, "local");
            }
            other => panic!("expected Setup, got {other:?}"),
        }
    }

    #[test]
    fn test_llm_setup_with_custom_name_and_device() {
        let cli = TestCli::parse_from([
            "apollia-os",
            "setup",
            "--local",
            "--model",
            "/tmp/m.gguf",
            "--name",
            "tiny",
            "--device",
            "cpu",
        ]);
        match &cli.command {
            LlmCommand::Setup {
                local,
                name,
                device,
                ..
            } => {
                assert!(*local);
                assert_eq!(name, "tiny");
                assert_eq!(device.as_deref(), Some("cpu"));
            }
            other => panic!("expected Setup, got {other:?}"),
        }
    }

    #[test]
    fn test_setup_without_local_flag_errors() {
        // Pretend file exists by referencing the binary itself (any extant file
        // with a non-.gguf extension would still pass the existence check, but
        // we exit before that on --local missing).
        let code = run_setup(SetupArgs {
            local: false,
            model: std::path::Path::new("/tmp/never.gguf"),
            backend_name: "local",
            device_override: None,
            system_db_override: None,
            models_dir_override: None,
            json: true,
        });
        assert_eq!(code, exit_codes::GENERAL_ERROR);
    }

    #[test]
    fn test_setup_rejects_missing_model_file() {
        let code = run_setup(SetupArgs {
            local: true,
            model: std::path::Path::new("/definitely/missing/model.gguf"),
            backend_name: "local",
            device_override: None,
            system_db_override: None,
            models_dir_override: None,
            json: true,
        });
        assert_eq!(code, exit_codes::GENERAL_ERROR);
    }

    #[test]
    fn test_setup_rejects_non_gguf_extension() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        // Write to a path with a non-gguf extension.
        let path = tmp.path().with_extension("bin");
        std::fs::copy(tmp.path(), &path).unwrap();
        let code = run_setup(SetupArgs {
            local: true,
            model: &path,
            backend_name: "local",
            device_override: None,
            system_db_override: None,
            models_dir_override: None,
            json: true,
        });
        assert_eq!(code, exit_codes::GENERAL_ERROR);
    }

    #[cfg(unix)]
    #[test]
    fn test_setup_does_not_truncate_model_when_models_dir_symlinks_to_source() {
        // GIVEN a .gguf model in a real dir, and a models_dir that symlinks to
        // that same dir, so the copy destination resolves to the source file.
        let real = tempfile::tempdir().unwrap();
        let model = real.path().join("m.gguf");
        std::fs::write(&model, b"REAL-MODEL-BYTES").unwrap();
        let holder = tempfile::tempdir().unwrap();
        let link_dir = holder.path().join("models");
        std::os::unix::fs::symlink(real.path(), &link_dir).unwrap();
        let sysdb = holder.path().join("system.db");

        // WHEN setup runs with the models dir pointing (via symlink) at the
        // directory already holding the source model.
        let _ = run_setup(SetupArgs {
            local: true,
            model: &model,
            backend_name: "local",
            device_override: None,
            system_db_override: Some(&sysdb),
            models_dir_override: Some(&link_dir),
            json: true,
        });

        // THEN the source model is never truncated (regression: a naive path
        // compare let std::fs::copy zero the file).
        assert_eq!(std::fs::read(&model).unwrap(), b"REAL-MODEL-BYTES");
    }

    #[test]
    fn infer_quantization_picks_known_pattern() {
        assert_eq!(infer_quantization("llama-Q4_K_M.gguf"), "q4_k_m");
        assert_eq!(infer_quantization("Qwen3-0.6B-Q8_0.gguf"), "q8_0");
        assert_eq!(infer_quantization("unknown.gguf"), "q4_k_m");
    }

    #[test]
    fn test_llm_backends_list_parses() {
        // GIVEN "backends list"
        // WHEN
        let cli = TestCli::parse_from(["apollia-os", "backends", "list"]);
        // THEN LlmCommand::Backends { command: LlmBackendsCommand::List }
        match &cli.command {
            LlmCommand::Backends { command } => {
                assert!(matches!(command, LlmBackendsCommand::List))
            }
            other => panic!("expected Backends, got {other:?}"),
        }
    }

    #[test]
    fn test_llm_backends_create_parses() {
        // GIVEN "backends create anthropic --kind anthropic --model claude-3-5-sonnet-20241022"
        // WHEN
        let cli = TestCli::parse_from([
            "apollia-os",
            "backends",
            "create",
            "anthropic",
            "--kind",
            "anthropic",
            "--model",
            "claude-3-5-sonnet-20241022",
        ]);
        // THEN Create with the right fields
        match &cli.command {
            LlmCommand::Backends { command } => match command {
                LlmBackendsCommand::Create {
                    name,
                    provider,
                    model,
                    api_key,
                    base_url,
                    ..
                } => {
                    assert_eq!(name, "anthropic");
                    assert_eq!(provider, "anthropic");
                    assert_eq!(model, "claude-3-5-sonnet-20241022");
                    assert!(api_key.is_none());
                    assert!(base_url.is_none());
                }
                other => panic!("expected Create, got {other:?}"),
            },
            other => panic!("expected Backends, got {other:?}"),
        }
    }

    #[test]
    fn test_llm_backends_create_with_api_key_parses() {
        // GIVEN "backends create openai --kind openai --model gpt-4o --api-key sk-test"
        // WHEN
        let cli = TestCli::parse_from([
            "apollia-os",
            "backends",
            "create",
            "openai",
            "--kind",
            "openai",
            "--model",
            "gpt-4o",
            "--api-key",
            "sk-test",
        ]);
        // THEN api_key = Some("sk-test")
        match &cli.command {
            LlmCommand::Backends { command } => match command {
                LlmBackendsCommand::Create { api_key, .. } => {
                    assert_eq!(api_key.as_deref(), Some("sk-test"))
                }
                other => panic!("expected Create, got {other:?}"),
            },
            other => panic!("expected Backends, got {other:?}"),
        }
    }

    #[test]
    fn test_llm_backends_update_parses() {
        // GIVEN "backends update anthropic --model claude-3-opus-20240229"
        // WHEN
        let cli = TestCli::parse_from([
            "apollia-os",
            "backends",
            "update",
            "anthropic",
            "--model",
            "claude-3-opus-20240229",
        ]);
        // THEN Update { name: "anthropic", model: Some("claude-3-opus-20240229"), api_key: None }
        match &cli.command {
            LlmCommand::Backends { command } => match command {
                LlmBackendsCommand::Update {
                    name,
                    model,
                    api_key,
                    ..
                } => {
                    assert_eq!(name, "anthropic");
                    assert_eq!(model.as_deref(), Some("claude-3-opus-20240229"));
                    assert!(api_key.is_none());
                }
                other => panic!("expected Update, got {other:?}"),
            },
            other => panic!("expected Backends, got {other:?}"),
        }
    }

    #[test]
    fn test_llm_backends_delete_confirm_parses() {
        // GIVEN "backends delete anthropic --confirm"
        // WHEN
        let cli =
            TestCli::parse_from(["apollia-os", "backends", "delete", "anthropic", "--confirm"]);
        // THEN Delete { name: "anthropic", confirm: true }
        match &cli.command {
            LlmCommand::Backends { command } => match command {
                LlmBackendsCommand::Delete { name, confirm } => {
                    assert_eq!(name, "anthropic");
                    assert!(confirm);
                }
                other => panic!("expected Delete, got {other:?}"),
            },
            other => panic!("expected Backends, got {other:?}"),
        }
    }

    #[test]
    fn test_llm_backends_set_default_parses() {
        // GIVEN "backends set-default anthropic"
        // WHEN
        let cli = TestCli::parse_from(["apollia-os", "backends", "set-default", "anthropic"]);
        // THEN SetDefault { name: "anthropic" }
        match &cli.command {
            LlmCommand::Backends { command } => match command {
                LlmBackendsCommand::SetDefault { name } => assert_eq!(name, "anthropic"),
                other => panic!("expected SetDefault, got {other:?}"),
            },
            other => panic!("expected Backends, got {other:?}"),
        }
    }

    fn ollama_config(base_url: Option<&str>) -> serde_json::Value {
        build_config_json(BuildConfigArgs {
            provider: "ollama",
            model: "qwen2.5:14b",
            api_key: None,
            api_key_env: None,
            base_url,
            device: "auto",
            timeout_sec: 60,
            context_window: None,
        })
    }

    #[test]
    fn test_ollama_default_base_url_carries_the_v1_suffix() {
        // GIVEN no --base-url, so the built-in default is used
        // WHEN the backend config is built
        // THEN the base ends in /v1: the OpenAI-compatible client appends
        // /chat/completions to it, and Ollama serves that route under /v1.
        // Without the suffix every completion returns 404.
        let cfg = ollama_config(None);
        assert_eq!(cfg["base_url"], "http://localhost:11434/v1");
    }

    #[test]
    fn test_security_notes_flag_a_cleartext_key_on_the_network() {
        // GIVEN a remote endpoint over plain http, carrying an API key
        // WHEN the notes are built
        // THEN both the cleartext transport and the credential are named
        let notes = backend_security_notes(
            "openai",
            "gpt-4o",
            Some("http://192.168.1.55:8000/v1"),
            Some("sk-secret"),
            None,
        );
        let joined = notes.join(" ");
        assert!(joined.contains("plain http://"), "notes: {joined}");
        assert!(joined.contains("API key will travel"), "notes: {joined}");
        assert!(joined.contains("leave this machine"), "notes: {joined}");
    }

    #[test]
    fn test_security_notes_flag_an_ollama_cloud_model_behind_a_local_url() {
        // GIVEN an Ollama backend on loopback whose model tag is a hosted one
        // WHEN the notes are built
        // THEN the egress is reported even though the URL says localhost, which
        // is the only place this fact can surface before the first prompt
        let notes = backend_security_notes(
            "ollama",
            "qwen3-coder:480b-cloud",
            Some("http://localhost:11434/v1"),
            None,
            None,
        );
        let joined = notes.join(" ");
        assert!(joined.contains("leave this machine"), "notes: {joined}");
        assert!(joined.contains("ollama.com"), "notes: {joined}");
    }

    #[test]
    fn test_cloud_model_detection_is_scoped_to_ollama_and_the_tag_suffix() {
        // GIVEN tags that merely contain the word, and the same tag on another
        // provider
        // WHEN the suffix is tested
        // THEN only a genuine Ollama -cloud tag matches
        assert!(is_ollama_cloud_model("ollama", "gpt-oss:120b-cloud"));
        assert!(is_ollama_cloud_model("ollama", "Qwen3-Coder:480B-Cloud"));
        assert!(!is_ollama_cloud_model("ollama", "cloud-llama:8b"));
        assert!(!is_ollama_cloud_model("ollama", "qwen3:8b"));
        assert!(!is_ollama_cloud_model("openai", "gpt-4o-cloud"));
    }

    #[test]
    fn test_context_window_is_persisted_only_when_set() {
        // GIVEN a backend created with an explicit window, and one without
        // WHEN the config is built
        // THEN the key appears only in the first, so an unset window stays
        // unknown rather than being pinned to a default nobody chose
        let with = build_config_json(BuildConfigArgs {
            provider: "ollama",
            model: "qwen3:8b",
            api_key: None,
            api_key_env: None,
            base_url: None,
            device: "auto",
            timeout_sec: 60,
            context_window: Some(32768),
        });
        assert_eq!(with["context_window"], 32768);
        assert!(ollama_config(None).get("context_window").is_none());
    }

    #[test]
    fn test_security_notes_stay_quiet_on_loopback() {
        // GIVEN a local backend
        // WHEN the notes are built
        // THEN nothing is said: no network hop, no data egress
        assert!(backend_security_notes(
            "ollama",
            "qwen3:8b",
            Some("http://localhost:11434/v1"),
            None,
            None
        )
        .is_empty());
        assert!(backend_security_notes(
            "ollama",
            "qwen3:8b",
            Some("http://127.0.0.1:11434/v1"),
            None,
            None
        )
        .is_empty());
    }

    #[test]
    fn test_security_notes_report_egress_without_inventing_a_key() {
        // GIVEN a LAN Ollama, which needs no credential
        // WHEN the notes are built
        // THEN the data leaving the machine is reported, but no key is claimed
        let notes = backend_security_notes(
            "ollama",
            "qwen3:8b",
            Some("http://192.168.1.55:11434/v1"),
            None,
            None,
        );
        let joined = notes.join(" ");
        assert!(joined.contains("leave this machine"), "notes: {joined}");
        assert!(
            !joined.contains("API key"),
            "must not mention a key that does not exist: {joined}"
        );
    }

    #[test]
    fn test_security_notes_stay_quiet_over_https_transport() {
        // GIVEN an encrypted transport
        // WHEN the notes are built
        // THEN only the egress note applies, never the cleartext one
        let notes = backend_security_notes(
            "anthropic",
            "claude-sonnet-4-6",
            Some("https://api.anthropic.com"),
            Some("sk-ant-x"),
            None,
        );
        let joined = notes.join(" ");
        assert!(!joined.contains("plain http://"), "notes: {joined}");
        assert!(joined.contains("leave this machine"), "notes: {joined}");
    }

    #[test]
    fn test_ollama_explicit_base_url_is_used_verbatim() {
        // GIVEN an explicit --base-url, for a remote or non-standard Ollama
        // WHEN the backend config is built
        // THEN it is stored as given, with no suffix guessing
        let cfg = ollama_config(Some("http://192.168.1.20:11434/v1"));
        assert_eq!(cfg["base_url"], "http://192.168.1.20:11434/v1");
    }
}
