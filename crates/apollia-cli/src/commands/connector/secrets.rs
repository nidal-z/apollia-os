//! `client-id`, `client-secret` and `api-key`: where each value comes from.

use apollia_auth::ConnectorProvider;

use crate::exit_codes;
use crate::note;

use super::{emit_error, parse_provider, ApiKeyCommand, ClientIdCommand, ClientSecretCommand};

// ─── client-id / client-secret / api-key ──────────────────────────────────────

/// Detect the active source of `<provider>.client_id`. Mirrors the Tauri
/// `detect_source` helper so CLI and Desktop agree on what they report.
pub(super) fn detect_client_id_source(provider: ConnectorProvider) -> &'static str {
    if std::env::var(provider.client_id_env_var())
        .ok()
        .filter(|v| !v.is_empty())
        .is_some()
    {
        return "env";
    }
    if apollia_auth::oauth_clients_file::lookup_client_id(provider.id()).is_some() {
        return "file";
    }
    if !provider.default_client_id().is_empty() {
        return "builtin";
    }
    "none"
}

/// Detect the active source of `<provider>.client_secret`.
pub(super) fn detect_client_secret_source(provider: ConnectorProvider) -> &'static str {
    if std::env::var(provider.client_secret_env_var())
        .ok()
        .filter(|v| !v.is_empty())
        .is_some()
    {
        return "env";
    }
    if apollia_auth::oauth_clients_file::lookup_client_secret(provider.id()).is_some() {
        return "file";
    }
    if !provider.default_client_secret().is_empty() {
        return "builtin";
    }
    "none"
}

/// Detect the active source of `<provider>.api_key`.
pub(super) fn detect_api_key_source(provider: ConnectorProvider) -> &'static str {
    if std::env::var(provider.api_key_env_var())
        .ok()
        .filter(|v| !v.is_empty())
        .is_some()
    {
        return "env";
    }
    if apollia_auth::oauth_clients_file::lookup_api_key(provider.id()).is_some() {
        return "file";
    }
    if !provider.default_api_key().is_empty() {
        return "builtin";
    }
    "none"
}

pub(super) fn run_client_id(cmd: &ClientIdCommand, json: bool) -> i32 {
    match cmd {
        ClientIdCommand::List => run_client_id_list(json),
        ClientIdCommand::Set {
            provider,
            client_id,
        } => run_client_id_set(provider, client_id, json),
    }
}

pub(super) fn run_client_id_list(json: bool) -> i32 {
    let providers = [ConnectorProvider::Google, ConnectorProvider::Microsoft];
    let mut rows = Vec::with_capacity(providers.len());
    for provider in providers {
        let effective = provider.resolve_client_id().unwrap_or_default();
        let source = detect_client_id_source(provider);
        let override_value = apollia_auth::oauth_clients_file::lookup_client_id(provider.id());
        let secret_source = detect_client_secret_source(provider);
        let has_secret = provider.resolve_client_secret().is_some();
        let api_key_source = detect_api_key_source(provider);
        let has_api_key = provider.resolve_api_key().is_some();
        rows.push((
            provider,
            effective,
            source,
            override_value,
            secret_source,
            has_secret,
            api_key_source,
            has_api_key,
        ));
    }

    if json {
        let array: Vec<serde_json::Value> = rows
            .iter()
            .map(
                |(p, effective, source, ov, sec_src, has_sec, key_src, has_key)| {
                    serde_json::json!({
                        "provider": p.id(),
                        "effective_client_id": effective,
                        "client_id_source": source,
                        "client_id_override": ov,
                        "client_secret_source": sec_src,
                        "has_client_secret": has_sec,
                        "api_key_source": key_src,
                        "has_api_key": has_key,
                    })
                },
            )
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::Value::Array(array)).unwrap_or_default()
        );
    } else {
        note!("  OAuth client configuration:");
        for (p, effective, source, ov, sec_src, has_sec, key_src, has_key) in &rows {
            let masked_id = mask_secret(effective);
            println!("  * {} ({}):", p.id(), source);
            println!("      client_id : {masked_id}");
            if let Some(o) = ov {
                let masked_o = mask_secret(o);
                println!("      override  : {masked_o}");
            }
            println!(
                "      secret    : {sec_src} ({})",
                if *has_sec { "set" } else { "absent" }
            );
            println!(
                "      api_key   : {key_src} ({})",
                if *has_key { "set" } else { "absent" }
            );
        }
    }
    exit_codes::SUCCESS
}

pub(super) fn run_client_id_set(provider: &str, client_id: &str, json: bool) -> i32 {
    let provider_id = match parse_provider(provider) {
        Ok(p) => p,
        Err(e) => {
            emit_error(e, json);
            return exit_codes::GENERAL_ERROR;
        }
    };
    let trimmed = client_id.trim();
    match apollia_auth::oauth_clients_file::set_client_id(provider_id.id(), trimmed) {
        Ok(()) => {
            emit_set_ok(provider_id.id(), "client_id", trimmed.is_empty(), json);
            exit_codes::SUCCESS
        }
        Err(e) => {
            emit_error(format!("failed to write oauth-clients.toml: {e}"), json);
            exit_codes::GENERAL_ERROR
        }
    }
}

pub(super) fn run_client_secret(cmd: &ClientSecretCommand, json: bool) -> i32 {
    match cmd {
        ClientSecretCommand::Set {
            provider,
            client_secret,
        } => {
            let provider_id = match parse_provider(provider) {
                Ok(p) => p,
                Err(e) => {
                    emit_error(e, json);
                    return exit_codes::GENERAL_ERROR;
                }
            };
            let trimmed = client_secret.trim();
            match apollia_auth::oauth_clients_file::set_client_secret(provider_id.id(), trimmed) {
                Ok(()) => {
                    emit_set_ok(provider_id.id(), "client_secret", trimmed.is_empty(), json);
                    exit_codes::SUCCESS
                }
                Err(e) => {
                    emit_error(format!("failed to write oauth-clients.toml: {e}"), json);
                    exit_codes::GENERAL_ERROR
                }
            }
        }
    }
}

pub(super) fn run_api_key(cmd: &ApiKeyCommand, json: bool) -> i32 {
    match cmd {
        ApiKeyCommand::Set { provider, api_key } => {
            let provider_id = match parse_provider(provider) {
                Ok(p) => p,
                Err(e) => {
                    emit_error(e, json);
                    return exit_codes::GENERAL_ERROR;
                }
            };
            let trimmed = api_key.trim();
            match apollia_auth::oauth_clients_file::set_api_key(provider_id.id(), trimmed) {
                Ok(()) => {
                    emit_set_ok(provider_id.id(), "api_key", trimmed.is_empty(), json);
                    exit_codes::SUCCESS
                }
                Err(e) => {
                    emit_error(format!("failed to write oauth-clients.toml: {e}"), json);
                    exit_codes::GENERAL_ERROR
                }
            }
        }
    }
}

pub(super) fn emit_set_ok(provider_id: &str, key: &str, cleared: bool, json: bool) {
    if json {
        let body = serde_json::json!({
            "provider": provider_id,
            "key": key,
            "cleared": cleared,
            "updated": !cleared,
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&body).unwrap_or_default()
        );
    } else if cleared {
        println!("  * {} / {} override cleared", provider_id, key);
    } else {
        println!("  * {} / {} override updated", provider_id, key);
    }
}

/// Mask a secret-like string for terminal display.
///
/// Returns the first and last two chars separated by `...` when the input is
/// long enough, otherwise a fully redacted marker. Never used in `--json`
/// output: the JSON shape exposes presence flags rather than the values.
pub(super) fn mask_secret(s: &str) -> String {
    if s.is_empty() {
        return "<empty>".to_string();
    }
    if s.len() <= 8 {
        return "********".to_string();
    }
    let prefix: String = s.chars().take(4).collect();
    let suffix: String = s
        .chars()
        .rev()
        .take(2)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    format!("{prefix}...{suffix}")
}
