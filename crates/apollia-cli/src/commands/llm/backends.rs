//! `llm backends` dispatch, the config payload it builds and its security notes.

use crate::client::{ClientError, RuntimeClient};

use super::backends_crud::{
    run_backends_create, run_backends_delete, run_backends_list, run_backends_set_default,
    run_backends_show, run_backends_update,
};
use super::LlmBackendsCommand;

// ─────────────────────────────────────────────
// Backends CRUD handlers
// ─────────────────────────────────────────────

/// Route `apollia-os llm backends <verb>` to the appropriate handler.
pub(super) async fn run_backends(
    client: &RuntimeClient,
    command: &LlmBackendsCommand,
    json: bool,
) -> i32 {
    match command {
        LlmBackendsCommand::List => run_backends_list(client, json).await,
        LlmBackendsCommand::Show { name } => run_backends_show(client, name, json).await,
        LlmBackendsCommand::Create {
            name,
            provider,
            model,
            api_key,
            api_key_env,
            base_url,
            device,
            timeout_sec,
            context_window,
            disabled,
            default,
        } => {
            run_backends_create(
                client,
                name,
                provider,
                model,
                api_key.as_deref(),
                api_key_env.as_deref(),
                base_url.as_deref(),
                device,
                *timeout_sec,
                *context_window,
                !*disabled,
                *default,
                json,
            )
            .await
        }
        LlmBackendsCommand::Update {
            name,
            provider,
            model,
            api_key,
            api_key_env,
            base_url,
            device,
            timeout_sec,
            enable,
            disable,
            default,
        } => {
            let enabled = if *enable {
                Some(true)
            } else if *disable {
                Some(false)
            } else {
                None
            };
            let is_default = if *default { Some(true) } else { None };
            run_backends_update(
                client,
                name,
                provider.as_deref(),
                model.as_deref(),
                api_key.as_deref(),
                api_key_env.as_deref(),
                base_url.as_deref(),
                device.as_deref(),
                *timeout_sec,
                enabled,
                is_default,
                json,
            )
            .await
        }
        LlmBackendsCommand::Delete { name, confirm } => {
            run_backends_delete(client, name, *confirm, json).await
        }
        LlmBackendsCommand::SetDefault { name } => {
            run_backends_set_default(client, name, json).await
        }
    }
}

/// Map a CLI-friendly provider name to the runtime's canonical identifier.
///
/// The runtime accepts: `llama-cpp`, `openai`, `mistral`, `anthropic`, `ollama`.
/// `local` is a common shortcut for `llama-cpp`. Unknown providers are
/// passed through unchanged so the runtime can return its own validation
/// error with the full list of valid values.
pub(super) fn canonicalize_provider(p: &str) -> &str {
    match p {
        "local" | "llama" | "llama_cpp" | "llamacpp" => "llama-cpp",
        other => other,
    }
}

/// Build the `config_json` object that the runtime stores alongside the
/// backend row. Shape varies per provider:
///
/// - `llama-cpp`: `{model_path, device, quantization?, timeout_sec}`
/// - `anthropic` / `openai` / `mistral`: `{api_key?, api_key_env?, base_url?, timeout_sec}`
/// - `ollama`: `{base_url, timeout_sec}`
///
/// Inputs for [`build_config_json`], grouped to keep the signature small.
pub(super) struct BuildConfigArgs<'a> {
    pub(super) provider: &'a str,
    pub(super) model: &'a str,
    pub(super) api_key: Option<&'a str>,
    pub(super) api_key_env: Option<&'a str>,
    pub(super) base_url: Option<&'a str>,
    pub(super) device: &'a str,
    pub(super) timeout_sec: u64,
    pub(super) context_window: Option<usize>,
}

/// Whether an Ollama model tag names a model executed on Ollama's servers.
///
/// Ollama routes these through the same local daemon and the same loopback URL
/// as local models, so nothing about the endpoint reveals that the inference is
/// remote. The tag suffix is the only signal available before a call is made.
pub(super) fn is_ollama_cloud_model(provider: &str, model: &str) -> bool {
    provider == "ollama" && model.trim_end().to_ascii_lowercase().ends_with("-cloud")
}

/// Hosts for which cleartext HTTP never reaches a network interface.
pub(super) fn is_loopback_host(host: &str) -> bool {
    let h = host.trim_start_matches('[').trim_end_matches(']');
    h.eq_ignore_ascii_case("localhost")
        || h == "127.0.0.1"
        || h == "::1"
        || h.to_ascii_lowercase().ends_with(".localhost")
}

/// Notes printed after a backend is created, when its transport deserves one.
///
/// Returned rather than printed so the wording is testable. Two distinct facts,
/// neither of which the product stated anywhere before:
///
/// - a credential sent over plain HTTP to another host travels in the clear, and
/// - a remote backend moves prompt content off this machine, which is worth
///   saying out loud for a runtime whose first principle is local-first.
///
/// A third fact does not follow from the URL at all: Ollama serves hosted models
/// through the same loopback endpoint as local ones, distinguished only by a
/// `-cloud` model tag. Judging on the host alone would clear that setup in
/// silence while every prompt leaves the machine, so the model name is examined
/// too.
///
/// Advisory, never blocking: plain HTTP over a trusted LAN or through a tunnel
/// is a legitimate deployment.
pub(super) fn backend_security_notes(
    provider: &str,
    model: &str,
    base_url: Option<&str>,
    api_key: Option<&str>,
    api_key_env: Option<&str>,
) -> Vec<String> {
    let mut notes = Vec::new();
    if is_ollama_cloud_model(provider, model) {
        notes.push(
            "    NOTE: this is an Ollama hosted model. Despite the local URL, prompts are"
                .to_string(),
        );
        notes.push(
            "          executed on ollama.com servers and leave this machine, which can"
                .to_string(),
        );
        notes.push("          include file contents, memory and workspace data.".to_string());
        notes.push(
            "          Use a model tag without the -cloud suffix to stay on this machine."
                .to_string(),
        );
    }
    let Some(url) = base_url else {
        return notes;
    };
    if provider == "llama-cpp" {
        return notes;
    }

    let host = url.strip_prefix("http://").map(|r| {
        let authority = r.split('/').next().unwrap_or(r);
        // Strip the port, keeping bracketed IPv6 literals intact.
        match authority.rsplit_once(':') {
            Some((h, _)) if !h.is_empty() && !h.contains(':') => h,
            _ => authority,
        }
    });

    let has_credential =
        api_key.is_some_and(|k| !k.is_empty()) || api_key_env.is_some_and(|v| !v.is_empty());

    if let Some(host) = host {
        if !is_loopback_host(host) {
            notes.push("    NOTE: this endpoint is plain http:// to another host.".to_string());
            if has_credential {
                notes.push(
                    "          The API key will travel unencrypted on that network. Prefer \
                     https:// or a tunnel."
                        .to_string(),
                );
            }
        }
    }

    let is_remote_host = host.is_none_or(|h| !is_loopback_host(h))
        && !url.starts_with("http://localhost")
        && !url.starts_with("http://127.0.0.1");
    if is_remote_host {
        notes.push(
            "    NOTE: prompts sent to this backend leave this machine, which can include \
             file contents,"
                .to_string(),
        );
        notes.push("          memory and workspace data.".to_string());
    }
    notes
}

pub(super) fn build_config_json(args: BuildConfigArgs<'_>) -> serde_json::Value {
    let BuildConfigArgs {
        provider,
        model,
        api_key,
        api_key_env,
        base_url,
        device,
        timeout_sec,
        context_window,
    } = args;

    let mut cfg = serde_json::Map::new();
    cfg.insert("timeout_sec".into(), serde_json::Value::from(timeout_sec));
    if let Some(window) = context_window {
        cfg.insert(
            "context_window".into(),
            serde_json::Value::from(window as u64),
        );
    }

    match provider {
        "llama-cpp" => {
            cfg.insert(
                "model_path".into(),
                serde_json::Value::String(model.to_string()),
            );
            cfg.insert(
                "device".into(),
                serde_json::Value::String(device.to_string()),
            );
        }
        "ollama" => {
            // The `/v1` suffix is required: the OpenAI-compatible client appends
            // `/chat/completions` to this base, and Ollama serves that route
            // under `/v1`. Without the suffix every completion returns 404.
            cfg.insert(
                "base_url".into(),
                serde_json::Value::String(
                    base_url.unwrap_or("http://localhost:11434/v1").to_string(),
                ),
            );
        }
        _ => {
            // Cloud providers: API key + optional base_url for self-hosted gateways.
            if let Some(k) = api_key {
                cfg.insert("api_key".into(), serde_json::Value::String(k.to_string()));
            }
            if let Some(v) = api_key_env {
                cfg.insert(
                    "api_key_env".into(),
                    serde_json::Value::String(v.to_string()),
                );
            }
            if let Some(u) = base_url {
                cfg.insert("base_url".into(), serde_json::Value::String(u.to_string()));
            }
        }
    }
    serde_json::Value::Object(cfg)
}

/// Trigger an in-place LlmRouter reload via `POST /api/v1/llm/reload` so the
/// runtime picks up the freshly mutated `system.db` without restarting.
///
/// Mutations (`create`, `update`, `delete`, `set-default`) call this after a
/// successful write. Reload failures are reported but do not change the exit
/// code: the database mutation succeeded; only the in-memory swap is missing.
/// Callers can re-run `apollia-os llm reload` to retry, or restart the daemon.
pub(super) async fn auto_reload_after_mutation(client: &RuntimeClient, json: bool) {
    match client.reload_llm_router().await {
        Ok(resp) => {
            if !json {
                let default = resp.get("default").and_then(|v| v.as_str()).unwrap_or("");
                let count = resp
                    .get("backends")
                    .and_then(|v| v.as_array())
                    .map(|a| a.len())
                    .unwrap_or(0);
                println!(
                    "    router reloaded in place ({count} backend(s) active, default: {default})"
                );
            }
        }
        Err(ClientError::ServerError { status, body }) => {
            if !json {
                eprintln!("    warning: router reload failed ({status}): {body}");
                eprintln!("    the mutation is persisted; run `apollia-os llm reload` to retry");
            }
        }
        Err(e) => {
            if !json {
                eprintln!("    warning: router reload failed: {e}");
                eprintln!("    the mutation is persisted; run `apollia-os llm reload` to retry");
            }
        }
    }
}
