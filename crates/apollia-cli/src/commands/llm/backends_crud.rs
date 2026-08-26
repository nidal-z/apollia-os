//! The `llm backends` verbs: list, show, create, update, delete, set-default.

use crate::client::{ClientError, RuntimeClient};
use crate::exit_codes;
use crate::note;

use super::backends::{
    auto_reload_after_mutation, backend_security_notes, build_config_json, canonicalize_provider,
    BuildConfigArgs,
};
use super::handle_error;

/// `apollia-os llm backends list`: list all configured backends.
/// Render `llm backends list`: a CONFIG listing (name, provider, configured
/// model, default marker), NOT a health probe. Availability lives in
/// `llm status` / `llm ping`; claiming it here would be misleading.
pub(super) fn format_backends_list(resp: &serde_json::Value) {
    let backends = resp
        .get("backends")
        .and_then(|b| b.as_array())
        .cloned()
        .unwrap_or_default();
    println!(
        "  {:<18} {:<12} {:<44} DEFAULT",
        "NAME", "PROVIDER", "MODEL"
    );
    if backends.is_empty() {
        println!("  (no LLM backends configured)");
        return;
    }
    for b in &backends {
        let name = b.get("name").and_then(|v| v.as_str()).unwrap_or("?");
        let provider = b.get("provider").and_then(|v| v.as_str()).unwrap_or("?");
        let model = b.get("model").and_then(|v| v.as_str()).unwrap_or("?");
        let is_default = b
            .get("is_default")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        let enabled = b
            .get("enabled")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true);
        let marker = if is_default {
            "* default"
        } else if !enabled {
            "disabled"
        } else {
            ""
        };
        println!("  {name:<18} {provider:<12} {model:<44} {marker}");
    }
}

pub(super) async fn run_backends_list(client: &RuntimeClient, json: bool) -> i32 {
    match client.list_llm_backends().await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                format_backends_list(&resp);
            }
            exit_codes::SUCCESS
        }
        Err(e) => handle_error(e, json),
    }
}

/// `apollia-os llm backends show <name>`: display the full configuration
/// of a backend, including the provider-specific `config_json` blob.
pub(super) async fn run_backends_show(client: &RuntimeClient, name: &str, json: bool) -> i32 {
    match client.get_llm_backend(name).await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
                return exit_codes::SUCCESS;
            }
            let provider = resp.get("provider").and_then(|v| v.as_str()).unwrap_or("?");
            let model = resp.get("model").and_then(|v| v.as_str()).unwrap_or("?");
            let enabled = resp
                .get("enabled")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            let is_default = resp
                .get("is_default")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            println!("Backend     : {name}");
            println!("Provider    : {provider}");
            println!("Model       : {model}");
            println!(
                "State       : {}{}",
                if enabled { "enabled" } else { "disabled" },
                if is_default { " (default)" } else { "" }
            );
            if let Some(cfg) = resp.get("config_json") {
                note!("Config      :");
                let rendered = serde_json::to_string_pretty(cfg).unwrap_or_default();
                for line in rendered.lines() {
                    println!("  {line}");
                }
            }
            exit_codes::SUCCESS
        }
        Err(ClientError::ServerError { status: 404, .. }) => {
            let code = crate::output::emit_error(
                json,
                exit_codes::GENERAL_ERROR,
                &format!("backend '{name}' not found"),
            );
            if !json {
                eprintln!("Hint: run `apollia-os llm backends list` to see existing backends.");
            }
            code
        }
        Err(e) => handle_error(e, json),
    }
}

/// `apollia-os llm backends create`: create a new LLM backend.
///
/// Builds the full payload expected by `POST /api/v1/llm/backends`
/// (canonical provider + provider-specific config_json) instead of sending
/// only the `kind/model` fields, which the runtime rejects with a 400.
// REASON: each argument is one clap flag of `llm backends create`; the parser hands them over one by one.
#[allow(clippy::too_many_arguments)]
pub(super) async fn run_backends_create(
    client: &RuntimeClient,
    name: &str,
    provider: &str,
    model: &str,
    api_key: Option<&str>,
    api_key_env: Option<&str>,
    base_url: Option<&str>,
    device: &str,
    timeout_sec: u64,
    context_window: Option<usize>,
    enabled: bool,
    is_default: bool,
    json: bool,
) -> i32 {
    let canonical = canonicalize_provider(provider);
    let config_json = build_config_json(BuildConfigArgs {
        provider: canonical,
        model,
        api_key,
        api_key_env,
        base_url,
        device,
        timeout_sec,
        context_window,
    });

    let body = serde_json::json!({
        "name": name,
        "provider": canonical,
        "model": model,
        "config_json": config_json,
        "enabled": enabled,
        "is_default": is_default,
    });

    match client.create_llm_backend(&body).await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                println!("OK backend '{name}' created (provider: {canonical}, model: {model})");
                if is_default {
                    println!("    marked as default backend");
                }
                for line in backend_security_notes(canonical, model, base_url, api_key, api_key_env)
                {
                    println!("{line}");
                }
                if !enabled {
                    println!("    disabled (pass --enable to activate)");
                }
                auto_reload_after_mutation(client, json).await;
            }
            exit_codes::SUCCESS
        }
        Err(ClientError::ServerError { status, body }) => {
            emit_backend_create_server_error(status, &body, json)
        }
        Err(e) => handle_error(e, json),
    }
}

/// Render a server-side error from `POST /api/v1/llm/backends`.
pub(super) fn emit_backend_create_server_error(status: u16, body: &str, json: bool) -> i32 {
    let _ = crate::output::emit_error(
        json,
        exit_codes::GENERAL_ERROR,
        &format!("{body} (status {status})"),
    );
    if !json && status == 422 {
        eprintln!();
        eprintln!("Hint: accepted providers: llama-cpp, anthropic, openai, mistral, ollama");
        eprintln!("      (the --kind alias is still accepted for backward compatibility)");
    }
    exit_codes::GENERAL_ERROR
}

/// `apollia-os llm backends update`: update an existing backend.
///
/// The runtime exposes `PUT` in replace mode (all fields required). The CLI
/// first reads the current state via `GET /api/v1/llm/backends/:name`, applies
/// the provided flags in merge mode, then sends the full payload. This allows
/// `--model X` without having to re-specify provider/config_json/enabled.
// REASON: each argument is one clap flag of `llm backends update`; the parser hands them over one by one.
#[allow(clippy::too_many_arguments)]
pub(super) async fn run_backends_update(
    client: &RuntimeClient,
    name: &str,
    provider: Option<&str>,
    model: Option<&str>,
    api_key: Option<&str>,
    api_key_env: Option<&str>,
    base_url: Option<&str>,
    device: Option<&str>,
    timeout_sec: Option<u64>,
    enabled: Option<bool>,
    is_default: Option<bool>,
    json: bool,
) -> i32 {
    // Step 1: fetch the current configuration so we can merge.
    let current = match client.get_llm_backend(name).await {
        Ok(v) => v,
        Err(ClientError::ServerError { status: 404, .. }) => {
            let code = crate::output::emit_error(
                json,
                exit_codes::GENERAL_ERROR,
                &format!("backend '{name}' not found"),
            );
            if !json {
                eprintln!("Hint: run `apollia-os llm backends list` to see existing backends.");
            }
            return code;
        }
        Err(e) => return handle_error(e, json),
    };

    // Step 2: extract current values as the merge baseline.
    let cur_provider = current
        .get("provider")
        .and_then(|v| v.as_str())
        .unwrap_or("anthropic");
    let cur_model = current.get("model").and_then(|v| v.as_str()).unwrap_or("");
    let cur_cfg = current
        .get("config_json")
        .cloned()
        .unwrap_or_else(|| serde_json::Value::Object(serde_json::Map::new()));
    let cur_enabled = current
        .get("enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let cur_default = current
        .get("is_default")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    // Step 3: compute the merged values.
    let new_provider_raw = provider.unwrap_or(cur_provider);
    let new_provider = canonicalize_provider(new_provider_raw);
    let new_model = model.unwrap_or(cur_model);
    let new_enabled = enabled.unwrap_or(cur_enabled);
    let new_default = is_default.unwrap_or(cur_default);

    // Step 4: merge config_json, start from the existing object, overlay
    // any field that the user explicitly changed.
    let merge = ConfigMerge {
        new_provider,
        new_model,
        model,
        api_key,
        api_key_env,
        base_url,
        device,
        timeout_sec,
    };
    let cfg_map = merge_backend_config(cur_cfg, &merge);

    let body = serde_json::json!({
        "provider": new_provider,
        "model": new_model,
        "config_json": serde_json::Value::Object(cfg_map),
        "enabled": new_enabled,
        "is_default": new_default,
    });

    match client.update_llm_backend(name, &body).await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                println!("OK backend '{name}' updated");
                println!(
                    "    provider={new_provider}, model={new_model}, enabled={new_enabled}, default={new_default}"
                );
                auto_reload_after_mutation(client, json).await;
            }
            exit_codes::SUCCESS
        }
        Err(ClientError::ServerError { status, body }) => crate::output::emit_error(
            json,
            exit_codes::GENERAL_ERROR,
            &format!("{body} (status {status})"),
        ),
        Err(e) => handle_error(e, json),
    }
}

/// Resolved-then-overlaid values used by [`merge_backend_config`].
pub(super) struct ConfigMerge<'a> {
    new_provider: &'a str,
    new_model: &'a str,
    model: Option<&'a str>,
    api_key: Option<&'a str>,
    api_key_env: Option<&'a str>,
    base_url: Option<&'a str>,
    device: Option<&'a str>,
    timeout_sec: Option<u64>,
}

/// Merge the existing `config_json` object with the user-supplied overrides,
/// applying only the fields that were explicitly changed.
pub(super) fn merge_backend_config(
    cur_cfg: serde_json::Value,
    m: &ConfigMerge<'_>,
) -> serde_json::Map<String, serde_json::Value> {
    let mut cfg_map = match cur_cfg {
        serde_json::Value::Object(map) => map,
        _ => serde_json::Map::new(),
    };
    if let Some(secs) = m.timeout_sec {
        cfg_map.insert("timeout_sec".into(), serde_json::Value::from(secs));
    }
    if m.new_provider == "llama-cpp" {
        // For local backends, the model PATH lives in config_json.model_path
        // and must stay in sync with the top-level `model` column.
        if m.model.is_some() {
            cfg_map.insert(
                "model_path".into(),
                serde_json::Value::String(m.new_model.to_string()),
            );
        }
        if let Some(d) = m.device {
            cfg_map.insert("device".into(), serde_json::Value::String(d.to_string()));
        }
    } else {
        if let Some(k) = m.api_key {
            cfg_map.insert("api_key".into(), serde_json::Value::String(k.to_string()));
        }
        if let Some(v) = m.api_key_env {
            cfg_map.insert(
                "api_key_env".into(),
                serde_json::Value::String(v.to_string()),
            );
        }
        if let Some(u) = m.base_url {
            cfg_map.insert("base_url".into(), serde_json::Value::String(u.to_string()));
        }
    }
    cfg_map
}

/// `apollia-os llm backends delete`: delete a backend.
pub(super) async fn run_backends_delete(
    client: &RuntimeClient,
    name: &str,
    confirm: bool,
    json: bool,
) -> i32 {
    if !confirm {
        return crate::output::emit_error(
            json,
            exit_codes::GENERAL_ERROR,
            &format!("use --confirm to delete backend '{name}' without prompt"),
        );
    }

    match client.delete_llm_backend(name).await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                println!("OK backend '{name}' deleted");
                auto_reload_after_mutation(client, json).await;
            }
            exit_codes::SUCCESS
        }
        Err(ClientError::ServerError { status: 404, .. }) => crate::output::emit_error(
            json,
            exit_codes::GENERAL_ERROR,
            &format!("backend '{name}' not found"),
        ),
        Err(e) => handle_error(e, json),
    }
}

/// `apollia-os llm backends set-default`: set the default backend.
pub(super) async fn run_backends_set_default(
    client: &RuntimeClient,
    name: &str,
    json: bool,
) -> i32 {
    match client.set_default_llm_backend(name).await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                println!("OK backend '{name}' set as default backend");
                auto_reload_after_mutation(client, json).await;
            }
            exit_codes::SUCCESS
        }
        Err(ClientError::ServerError { status: 404, .. }) => crate::output::emit_error(
            json,
            exit_codes::GENERAL_ERROR,
            &format!("backend '{name}' not found"),
        ),
        Err(e) => handle_error(e, json),
    }
}
