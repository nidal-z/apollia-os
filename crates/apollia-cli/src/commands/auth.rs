//! `apollia-os auth` subcommands: OAuth2 PKCE authentication management.
//!
//! Manages authentication tokens for LLM cloud providers (Anthropic, OpenAI, Vertex AI)
//! via an interactive PKCE flow. Tokens are stored in the OS-native keyring.

use clap::Subcommand;

use crate::exit_codes;

/// Keychain service name historically used by `KeyringStorage`. We mirror
/// it here so CLI reads/writes through `select_secret_store()` end up at
/// the same entry on systems where both backends share storage.
const AUTH_SERVICE: &str = "apollia-auth";

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

    // Use the pluggable SecretStore so `APOLLIA_TOKEN_STORAGE=file` works
    // for `auth login` like it does for `connector` and `mcp oauth`. On
    // dev workstations the default (KeyringSecretStore) still hits the OS
    // keychain; in CI or under an isolated $HOME we honour the env var
    // and persist to `~/.apollia/secrets/…age`.
    let store = match apollia_auth::select_secret_store() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error: secret store unavailable: {e}");
            return exit_codes::GENERAL_ERROR;
        }
    };
    let json_payload = match serde_json::to_string(&token) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error: serialize token: {e}");
            return exit_codes::GENERAL_ERROR;
        }
    };
    if let Err(e) = store.set(AUTH_SERVICE, provider_name, &json_payload) {
        eprintln!("Error: could not store token: {e}");
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

/// Resolves the auth status string for a single provider against the store.
fn provider_status(
    store_result: &Result<Box<dyn apollia_auth::SecretStore>, apollia_auth::AuthError>,
    name: &str,
) -> &'static str {
    let Ok(store) = store_result else {
        return "not configured";
    };
    let payload = match store.get(AUTH_SERVICE, name) {
        Ok(Some(payload)) => payload,
        Ok(None) | Err(_) => return "not configured",
    };
    let Ok(token) = serde_json::from_str::<apollia_auth::StoredToken>(&payload) else {
        return "corrupted";
    };
    if token.is_expired() {
        "expired"
    } else {
        "valid"
    }
}

fn run_status(json: bool) -> i32 {
    let store_result = apollia_auth::select_secret_store();
    let rows: Vec<(&str, &str)> = apollia_auth::SUPPORTED_PROVIDERS
        .iter()
        .map(|name| (*name, provider_status(&store_result, name)))
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

    let store = match apollia_auth::select_secret_store() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error: secret store unavailable: {e}");
            return exit_codes::GENERAL_ERROR;
        }
    };
    match store.delete(AUTH_SERVICE, provider_name) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser, Debug)]
    struct TestCli {
        #[command(subcommand)]
        cmd: AuthCommand,
    }

    #[test]
    fn parses_login() {
        let cli = TestCli::parse_from(["x", "login", "anthropic"]);
        match cli.cmd {
            AuthCommand::Login { provider } => assert_eq!(provider, "anthropic"),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parses_status() {
        let cli = TestCli::parse_from(["x", "status"]);
        assert!(matches!(cli.cmd, AuthCommand::Status));
    }

    #[test]
    fn parses_logout() {
        let cli = TestCli::parse_from(["x", "logout", "openai"]);
        match cli.cmd {
            AuthCommand::Logout { provider } => assert_eq!(provider, "openai"),
            other => panic!("unexpected: {other:?}"),
        }
    }
}
