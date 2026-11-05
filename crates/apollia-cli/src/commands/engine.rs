//! `apollia-os engine <verb>`: the inference engines of the machine the
//! runtime serves on.
//!
//! - `list [--all]` reads `GET /api/v1/engines`: the cards detected, the
//!   engines installed and the one in service, the variants the channel offers
//!   for this platform with their verdict, and Node.js. Without `--all` the
//!   variants that do not run here are counted, not listed.
//! - `check` asks the runtime to probe the cards again and verify the channel
//!   (`POST /api/v1/engines/check`). A channel out of reach or a refused
//!   signature is exit 2, with the message of the runtime.
//! - `use <variant|auto|embedded>` writes `engines.gguf` through the save route
//!   of the configuration (`GET`, then `PUT /api/v1/config`). A variant that is
//!   not installed is refused with exit 1; an installed one rated incompatible
//!   is written, with a warning. The choice applies the next time a model is
//!   loaded.
//! - `channel [stable|preview]` reads the followed channel, or changes it
//!   (`GET`, `PUT /api/v1/engines/settings`).
//! - `install <engine> [<variant>]` installs a variant of `llama-server`, the
//!   one rated recommended when none is named, or Node.js, then follows the
//!   installation until it ends ([`install`]). `import <dir>` installs from a
//!   directory copied from a release, followed the same way.
//! - `remove <engine> <tag> <variant> --confirm` removes one installed build
//!   and says what `engines.gguf` comes down to ([`remove`]).
//!
//! Daemon off, every leaf exits 2 with the one hint of
//! [`crate::output::emit_runtime_not_started`].

mod install;
mod remove;
mod render;
#[cfg(test)]
mod tests;
#[cfg(all(test, unix))]
mod tests_install;

use std::path::PathBuf;

use apollia_core::config::GGUF_ENGINE_CHOICES;
use clap::builder::PossibleValuesParser;
use clap::{Subcommand, ValueEnum};
use serde_json::Value;

use crate::client::{ClientError, RawResponse, RuntimeClient};
use crate::exit_codes;

/// Route of the engine list.
const ENGINES_ROUTE: &str = "/api/v1/engines";
/// Route of the channel check.
const CHECK_ROUTE: &str = "/api/v1/engines/check";
/// Route of the engine store settings.
const SETTINGS_ROUTE: &str = "/api/v1/engines/settings";
/// The save route of `apollia.toml`.
const CONFIG_ROUTE: &str = "/api/v1/config";
/// The key `use` writes.
const GGUF_KEY: &str = "engines.gguf";
/// When a choice written by `use` takes effect.
pub(crate) const APPLIES: &str = "applies the next time a model is loaded";

/// `apollia-os engine <verb>`.
#[derive(Debug, Subcommand)]
pub enum EngineCommand {
    /// The cards, the engines installed and in service, the variants the channel offers, Node.js.
    List {
        /// Also list the variants that do not run on this machine.
        #[arg(long)]
        all: bool,
    },
    /// Probe the cards again and verify the followed channel.
    Check,
    /// Choose the engine that serves GGUF models: a variant installed, `auto` or `embedded`.
    Use {
        /// `auto`, `embedded`, or an installed variant (`cuda`, `cuda-12`, `cuda-13`, `rocm`, `vulkan`, `cpu`, `metal`).
        #[arg(value_parser = PossibleValuesParser::new(GGUF_ENGINE_CHOICES.iter().copied()))]
        choice: String,
    },
    /// Show the followed channel, or follow another one.
    Channel {
        /// The channel to follow; without it, the followed one is shown.
        channel: Option<ChannelArg>,
    },
    /// Install a variant of llama-server (the recommended one when none is named) or Node.js.
    Install {
        /// The engine to install.
        engine: InstallEngine,
        /// The llama-server variant; without it, the one rated recommended for this machine.
        #[arg(value_parser = PossibleValuesParser::new(install::VARIANTS))]
        variant: Option<String>,
        /// Return once the runtime accepted the installation; `engine list` follows it.
        #[arg(long)]
        detach: bool,
        /// Install a variant rated incompatible with this machine.
        #[arg(long)]
        force: bool,
    },
    /// Install the engines of a directory copied from a release: manifest, signature and archives.
    Import {
        /// The directory holding `engines.json`, `engines.json.minisig` and the archives.
        dir: PathBuf,
        /// Return once the runtime accepted the import; `engine list` follows it.
        #[arg(long)]
        detach: bool,
    },
    /// Remove one installed build, as `engine list` names it.
    Remove {
        /// The engine: `llama-server`, `vllm-mlx` or `nodejs`.
        engine: String,
        /// The upstream tag of the build.
        tag: String,
        /// The variant of the build.
        variant: String,
        /// Remove without asking, also the build `engines.gguf` chooses.
        #[arg(long)]
        confirm: bool,
    },
}

/// An engine `engine install` installs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum InstallEngine {
    /// The llama.cpp server, which serves GGUF models.
    #[value(name = "llama-server")]
    LlamaServer,
    /// The Node.js runtime of the tool servers that run on it.
    #[value(name = "nodejs")]
    Nodejs,
}

/// A channel of the engine store.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ChannelArg {
    /// The builds the project vouches for.
    Stable,
    /// The builds being tried before they reach stable.
    Preview,
}

impl ChannelArg {
    fn as_str(self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Preview => "preview",
        }
    }
}

/// Entry point for `apollia-os engine <verb>`.
pub async fn run(cmd: &EngineCommand, socket: Option<PathBuf>, json: bool) -> i32 {
    let client = RuntimeClient::new(socket.unwrap_or_else(crate::client::default_socket_path));
    match cmd {
        EngineCommand::List { all } => list(&client, *all, json).await,
        EngineCommand::Check => check(&client, json).await,
        EngineCommand::Use { choice } => use_choice(&client, choice, json).await,
        EngineCommand::Channel { channel } => self::channel(&client, *channel, json).await,
        EngineCommand::Install {
            engine,
            variant,
            detach,
            force,
        } => {
            let wanted = install::Wanted {
                engine: *engine,
                variant: variant.as_deref(),
                detach: *detach,
                force: *force,
            };
            install::install(&client, wanted, json, install::interrupted()).await
        }
        EngineCommand::Import { dir, detach } => {
            install::import(&client, dir, *detach, json, install::interrupted()).await
        }
        EngineCommand::Remove {
            engine,
            tag,
            variant,
            confirm,
        } => remove::remove(&client, [engine, tag, variant], *confirm, json).await,
    }
}

/// A transport failure: the one refusal of a runtime that does not answer, or
/// the error itself.
fn transport(json: bool, err: &ClientError) -> i32 {
    match err {
        ClientError::ConnectionRefused => crate::output::emit_runtime_not_started(json),
        other => crate::output::emit_client_error(json, other),
    }
}

/// The message a refusal of the runtime carries in its `error` field.
fn runtime_message(resp: &RawResponse) -> String {
    serde_json::from_str::<Value>(&resp.body)
        .ok()
        .and_then(|v| v.get("error").and_then(Value::as_str).map(str::to_owned))
        .unwrap_or_else(|| format!("{} (status {})", resp.body, resp.status))
}

/// A refusal of the runtime: exit 1 when the request is at fault, 2 otherwise.
fn refusal(json: bool, resp: &RawResponse) -> i32 {
    let code = if resp.status < 500 {
        exit_codes::GENERAL_ERROR
    } else {
        exit_codes::RUNTIME_ERROR
    };
    crate::output::emit_error(json, code, &runtime_message(resp))
}

/// `GET route` read as JSON, or the exit code of its failure.
async fn get_json(client: &RuntimeClient, route: &str, json: bool) -> Result<Value, i32> {
    let resp = client.get(route).await.map_err(|e| transport(json, &e))?;
    if resp.status >= 400 {
        return Err(refusal(json, &resp));
    }
    serde_json::from_str(&resp.body).map_err(|e| {
        crate::output::emit_error(
            json,
            exit_codes::RUNTIME_ERROR,
            &format!("invalid JSON response: {e}"),
        )
    })
}

fn print_document(doc: &Value) {
    println!(
        "{}",
        serde_json::to_string_pretty(doc).unwrap_or_else(|_| doc.to_string())
    );
}

/// `engine list [--all]`.
async fn list(client: &RuntimeClient, all: bool, json: bool) -> i32 {
    let doc = match get_json(client, ENGINES_ROUTE, json).await {
        Ok(doc) => doc,
        Err(code) => return code,
    };
    if json {
        print_document(&doc);
        return exit_codes::SUCCESS;
    }
    for line in render::list(&doc, all, cfg!(windows)) {
        line.print();
    }
    exit_codes::SUCCESS
}

/// `engine check`.
async fn check(client: &RuntimeClient, json: bool) -> i32 {
    let resp = match client.post(CHECK_ROUTE, None).await {
        Ok(resp) => resp,
        Err(e) => return transport(json, &e),
    };
    if resp.status >= 400 {
        // Every refusal of the check is the channel or its signature, never
        // the request: the runtime is at fault, exit 2.
        return crate::output::emit_error(json, exit_codes::RUNTIME_ERROR, &runtime_message(&resp));
    }
    let doc: Value = serde_json::from_str(&resp.body).unwrap_or(Value::Null);
    if json {
        print_document(&doc);
        return exit_codes::SUCCESS;
    }
    for line in render::check(&doc) {
        line.print();
    }
    exit_codes::SUCCESS
}

/// `engine use <choice>`.
async fn use_choice(client: &RuntimeClient, choice: &str, json: bool) -> i32 {
    let engines = match get_json(client, ENGINES_ROUTE, json).await {
        Ok(doc) => doc,
        Err(code) => return code,
    };
    let warning = match render::judge_choice(&engines, choice) {
        render::ChoiceVerdict::NotInstalled => {
            return crate::output::emit_error(
                json,
                exit_codes::GENERAL_ERROR,
                &format!(
                    "no `{choice}` build of llama-server is installed; `apollia-os engine list` shows what is"
                ),
            );
        }
        render::ChoiceVerdict::Accepted { warning } => warning,
    };
    let config = match get_json(client, CONFIG_ROUTE, json).await {
        Ok(doc) => doc,
        Err(code) => return code,
    };
    let text = config.get("text").and_then(Value::as_str).unwrap_or("");
    let sha256 = config.get("sha256").and_then(Value::as_str).unwrap_or("");
    let edited = match with_gguf_choice(text, choice) {
        Ok(edited) => edited,
        Err(message) => {
            return crate::output::emit_error(json, exit_codes::GENERAL_ERROR, &message);
        }
    };
    let body = serde_json::json!({ "text": edited, "sha256": sha256, "origin": "cli" });
    let resp = match client.put(CONFIG_ROUTE, Some(&body)).await {
        Ok(resp) => resp,
        Err(e) => return transport(json, &e),
    };
    if resp.status >= 400 {
        return refusal(json, &resp);
    }
    if json {
        print_document(&serde_json::json!({
            "key": GGUF_KEY,
            "value": choice,
            "applies": APPLIES,
            "warning": warning,
        }));
        return exit_codes::SUCCESS;
    }
    if let Some(warning) = &warning {
        eprintln!("Warning: {warning}");
    }
    println!("{GGUF_KEY} = {choice}: {APPLIES}.");
    exit_codes::SUCCESS
}

/// `text` with `engines.gguf` set to `choice`, everything else kept as written.
pub(crate) fn with_gguf_choice(text: &str, choice: &str) -> Result<String, String> {
    let mut document = text
        .parse::<toml_edit::DocumentMut>()
        .map_err(|e| format!("apollia.toml does not parse: {e}"))?;
    let section = document
        .entry("engines")
        .or_insert_with(|| toml_edit::Item::Table(toml_edit::Table::new()));
    let Some(table) = section.as_table_like_mut() else {
        return Err("`engines` in apollia.toml is not a table".to_owned());
    };
    table.insert("gguf", toml_edit::value(choice));
    Ok(document.to_string())
}

/// `engine channel [stable|preview]`.
async fn channel(client: &RuntimeClient, wanted: Option<ChannelArg>, json: bool) -> i32 {
    let settings = match get_json(client, SETTINGS_ROUTE, json).await {
        Ok(doc) => doc,
        Err(code) => return code,
    };
    let Some(wanted) = wanted else {
        if json {
            print_document(&settings);
        } else {
            let followed = settings
                .get("channel")
                .and_then(Value::as_str)
                .unwrap_or("?");
            println!("{followed}");
        }
        return exit_codes::SUCCESS;
    };
    let base_url = settings
        .get("base_url")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let body = serde_json::json!({ "channel": wanted.as_str(), "base_url": base_url });
    let resp = match client.put(SETTINGS_ROUTE, Some(&body)).await {
        Ok(resp) => resp,
        Err(e) => return transport(json, &e),
    };
    if resp.status >= 400 {
        return refusal(json, &resp);
    }
    if json {
        let saved: Value = serde_json::from_str(&resp.body).unwrap_or(body);
        print_document(&saved);
        return exit_codes::SUCCESS;
    }
    println!("{}", wanted.as_str());
    exit_codes::SUCCESS
}
