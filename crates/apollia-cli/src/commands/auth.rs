//! `apollia-os auth` subcommands — OAuth2 PKCE authentication management.
//!
//! Manages authentication tokens for LLM cloud providers (Anthropic, OpenAI, Vertex AI)
//! via an interactive PKCE flow. Tokens are stored in the OS-native keyring.

use clap::Subcommand;

use crate::exit_codes;

// ─── Subcommands ──────────────────────────────────────────────────────────────

/// Auth subcommands: `apollia-os auth <verb>`.
#[derive(Debug, Subcommand)]
pub enum AuthCommand {
    /// Authenticate with a provider via OAuth2 PKCE and store the token in the OS keyring.
    ///
    /// Opens the browser to the provider authorization page. After the user grants access,
    /// the token is exchanged and stored locally. Requires `{PROVIDER}_CLIENT_ID` to be set.
    Login {
        /// Provider name: `anthropic`, `openai`, or `vertex`.
        provider: String,
    },

    /// Display the authentication status for all supported providers.
    ///
    /// For each provider reports whether a token is stored, valid, or expired.
    Status,

    /// Remove the stored token for the given provider.
    Logout {
        /// Provider name: `anthropic`, `openai`, or `vertex`.
        provider: String,
    },
}

// ─── Entry point ──────────────────────────────────────────────────────────────

/// Execute an `auth` subcommand. Returns a POSIX exit code.
pub async fn run(cmd: &AuthCommand, json: bool) -> i32 {
    match cmd {
        AuthCommand::Login { provider } => run_login(provider, json).await,
        AuthCommand::Status => run_status(json),
        AuthCommand::Logout { provider } => run_logout(provider, json),
    }
}

// ─── Login ────────────────────────────────────────────────────────────────────

async fn run_login(provider_name: &str, json: bool) -> i32 {
    let provider = match apollia_auth::get_provider(provider_name) {
        Some(p) => p,
        None => {
            let supported = apollia_auth::SUPPORTED_PROVIDERS.join(", ");
            eprintln!("Error: provider '{provider_name}' not supported. Supported: {supported}");
            return exit_codes::GENERAL_ERROR;
        }
    };

    if provider.client_id.is_empty() {
        let var_name = format!(
            "{}_CLIENT_ID",
            provider_name.to_uppercase().replace('-', "_")
        );
        eprintln!("Error: environment variable {var_name} is not set.");
        return exit_codes::GENERAL_ERROR;
    }

    let (listener, port) = match apollia_auth::callback::bind_ephemeral_port().await {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Error: failed to bind callback port: {e}");
            return exit_codes::GENERAL_ERROR;
        }
    };

    let flow = apollia_auth::OAuth2PkceFlow::new(port);
    let auth_url = apollia_auth::build_auth_url(&provider, &flow);

    if !json {
        println!("Opening browser for {} authentication…", provider.name);
        println!("If the browser does not open, visit:\n  {auth_url}");
    }

    if let Err(e) = open::that(&auth_url) {
        tracing::warn!(error = %e, "could not open browser automatically");
    }

    if !json {
        println!("Waiting for OAuth2 callback…");
    }

    let code = match apollia_auth::callback::wait_for_callback(listener, &flow.state).await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error: callback failed: {e}");
            return exit_codes::GENERAL_ERROR;
        }
    };

    let token = match apollia_auth::token::exchange_code(&provider, &flow, &code).await {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Error: token exchange failed: {e}");
            return exit_codes::GENERAL_ERROR;
        }
    };

    if let Err(e) = apollia_auth::KeyringStorage::store(provider_name, &token) {
        eprintln!("Error: could not store token in keyring: {e}");
        return exit_codes::GENERAL_ERROR;
    }

    if json {
        println!(r#"{{"status":"ok","provider":"{provider_name}"}}"#);
    } else {
        println!("Successfully authenticated with {}.", provider.name);
    }

    exit_codes::SUCCESS
}

// ─── Status ───────────────────────────────────────────────────────────────────

fn run_status(json: bool) -> i32 {
    let rows: Vec<(&str, &str)> = apollia_auth::SUPPORTED_PROVIDERS
        .iter()
        .map(|name| {
            let status = match apollia_auth::KeyringStorage::load(name) {
                Ok(Some(token)) => {
                    if token.is_expired() {
                        "expired"
                    } else {
                        "valid"
                    }
                }
                Ok(None) | Err(_) => "not configured",
            };
            (*name, status)
        })
        .collect();

    if json {
        let obj: serde_json::Map<String, serde_json::Value> = rows
            .iter()
            .map(|(name, status)| {
                (
                    name.to_string(),
                    serde_json::Value::String(status.to_string()),
                )
            })
            .collect();
        println!("{}", serde_json::Value::Object(obj));
    } else {
        println!("{:<15} Status", "Provider");
        println!("{}", "─".repeat(30));
        for (name, status) in &rows {
            println!("{name:<15} {status}");
        }
    }

    exit_codes::SUCCESS
}

// ─── Logout ───────────────────────────────────────────────────────────────────

fn run_logout(provider_name: &str, json: bool) -> i32 {
    if apollia_auth::get_provider(provider_name).is_none() {
        let supported = apollia_auth::SUPPORTED_PROVIDERS.join(", ");
        eprintln!("Error: provider '{provider_name}' not supported. Supported: {supported}");
        return exit_codes::GENERAL_ERROR;
    }

    match apollia_auth::KeyringStorage::delete(provider_name) {
        Ok(()) => {
            if json {
                println!(r#"{{"status":"ok","provider":"{provider_name}"}}"#);
            } else {
                println!("Logged out from {provider_name}.");
            }
            exit_codes::SUCCESS
        }
        Err(e) => {
            eprintln!("Error: {e}");
            exit_codes::GENERAL_ERROR
        }
    }
}
