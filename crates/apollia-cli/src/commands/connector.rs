//! `apollia-os connector` subcommands — manage native SaaS connectors.
//!
//! Operates directly on `apollia-auth::AuthManager` (multi-account keyring)
//! and `apollia-connectors::{GoogleConnector, MicrosoftConnector}` without
//! requiring the runtime to be running. This is the local-first counterpart
//! to the Desktop `integrations` Tauri commands.

use std::sync::Arc;

use clap::Subcommand;

use apollia_auth::{AccountId, AuthManager, ConnectorProvider};
use apollia_connectors::{ConnectorRegistry, GoogleConnector, MicrosoftConnector};

use crate::exit_codes;

/// Subcommands of `apollia-os connector`.
#[derive(Debug, Subcommand)]
pub enum ConnectorCommand {
    /// List all native SaaS connectors registered in this build.
    ///
    /// Output covers the connector id, display name, publisher, and the
    /// services it exposes (e.g. `gmail`, `gcal`, `gdrive`).
    List,

    /// List OAuth-connected accounts for one or all providers.
    Accounts {
        /// Filter by provider: `google` or `microsoft`. Omit to list both.
        #[arg(long)]
        provider: Option<String>,
    },

    /// Probe the connector for an account by calling the userinfo endpoint.
    ///
    /// Returns the live identity claim and the scopes the upstream Authorization
    /// Server reports as granted — the same shape used by `connector.check()`
    /// inside the runtime.
    Test {
        /// Provider id: `google` or `microsoft`.
        provider: String,
        /// Account identifier (email when supplied during OAuth login).
        account: String,
    },

    /// Revoke the stored token for `(provider, account)`.
    ///
    /// Only the local keyring entry is cleared — the upstream Authorization
    /// Server is not notified. Use the provider's web revocation page for a
    /// server-side revocation.
    Revoke {
        /// Provider id.
        provider: String,
        /// Account id to revoke.
        account: String,
        /// Skip the confirmation prompt (required for scripts).
        #[arg(long)]
        confirm: bool,
    },
}

/// Entry point for `apollia-os connector <verb>`.
pub async fn run(cmd: &ConnectorCommand, json: bool) -> i32 {
    match cmd {
        ConnectorCommand::List => run_list(json).await,
        ConnectorCommand::Accounts { provider } => run_accounts(provider.as_deref(), json).await,
        ConnectorCommand::Test { provider, account } => run_test(provider, account, json).await,
        ConnectorCommand::Revoke {
            provider,
            account,
            confirm,
        } => run_revoke(provider, account, *confirm, json).await,
    }
}

/// Build a [`ConnectorRegistry`] populated with the bundled connectors.
async fn build_registry(auth: Arc<AuthManager>) -> Result<ConnectorRegistry, String> {
    let registry = ConnectorRegistry::new();
    let google = GoogleConnector::new(auth.clone())
        .map_err(|e| format!("failed to build google connector: {e}"))?;
    registry.register(google).await;
    let microsoft = MicrosoftConnector::new(auth)
        .map_err(|e| format!("failed to build microsoft connector: {e}"))?;
    registry.register(microsoft).await;
    Ok(registry)
}

/// Parse a CLI-provided provider id into a strongly-typed [`ConnectorProvider`].
fn parse_provider(id: &str) -> Result<ConnectorProvider, String> {
    match id {
        "google" => Ok(ConnectorProvider::Google),
        "microsoft" => Ok(ConnectorProvider::Microsoft),
        other => Err(format!(
            "unknown provider '{other}' (expected: google, microsoft)"
        )),
    }
}

fn open_auth_manager(json: bool) -> Option<Arc<AuthManager>> {
    match AuthManager::new() {
        Ok(m) => Some(Arc::new(m)),
        Err(e) => {
            emit_error(format!("failed to open auth storage: {e}"), json);
            None
        }
    }
}

fn emit_error(msg: String, json: bool) {
    if json {
        let out = serde_json::json!({ "error": msg });
        println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
    } else {
        eprintln!("Error: {msg}");
    }
}

// ─── list ─────────────────────────────────────────────────────────────────────

async fn run_list(json: bool) -> i32 {
    let Some(auth) = open_auth_manager(json) else {
        return exit_codes::GENERAL_ERROR;
    };
    let registry = match build_registry(auth).await {
        Ok(r) => r,
        Err(e) => {
            emit_error(e, json);
            return exit_codes::GENERAL_ERROR;
        }
    };

    let summaries = registry.manifests().await;
    if json {
        let array: Vec<serde_json::Value> = summaries
            .iter()
            .map(|s| {
                serde_json::json!({
                    "id": s.manifest.id,
                    "name": s.manifest.name,
                    "description": s.manifest.description,
                    "publisher": s.manifest.publisher,
                    "services": s.manifest.services,
                    "operations_count": s.operations.len(),
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::Value::Array(array)).unwrap_or_default()
        );
    } else if summaries.is_empty() {
        println!("No connectors registered in this build.");
    } else {
        println!("  Available connectors:");
        for s in &summaries {
            println!(
                "  * {:<10} {} ({} services, {} operations)",
                s.manifest.id,
                s.manifest.name,
                s.manifest.services.len(),
                s.operations.len()
            );
            println!("      services: {}", s.manifest.services.join(", "));
        }
    }
    exit_codes::SUCCESS
}

// ─── accounts ─────────────────────────────────────────────────────────────────

async fn run_accounts(filter: Option<&str>, json: bool) -> i32 {
    let Some(auth) = open_auth_manager(json) else {
        return exit_codes::GENERAL_ERROR;
    };

    let providers: Vec<ConnectorProvider> = match filter {
        Some(id) => match parse_provider(id) {
            Ok(p) => vec![p],
            Err(e) => {
                emit_error(e, json);
                return exit_codes::GENERAL_ERROR;
            }
        },
        None => vec![ConnectorProvider::Google, ConnectorProvider::Microsoft],
    };

    let mut rows: Vec<(ConnectorProvider, Vec<AccountId>)> = Vec::new();
    for provider in &providers {
        match auth.list_accounts(*provider).await {
            Ok(accounts) => rows.push((*provider, accounts)),
            Err(e) => {
                emit_error(
                    format!("failed to list accounts for {}: {e}", provider.id()),
                    json,
                );
                return exit_codes::GENERAL_ERROR;
            }
        }
    }

    if json {
        let array: Vec<serde_json::Value> = rows
            .iter()
            .flat_map(|(provider, accounts)| {
                let p_id = provider.id();
                accounts.iter().map(move |a| {
                    serde_json::json!({
                        "provider": p_id,
                        "account_id": a.as_str(),
                    })
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::Value::Array(array)).unwrap_or_default()
        );
    } else {
        let total: usize = rows.iter().map(|(_, a)| a.len()).sum();
        if total == 0 {
            println!("No connected accounts.");
            println!(
                "  -> Run `apollia-os auth login <provider>` to connect an account."
            );
            return exit_codes::SUCCESS;
        }
        println!("  Connected accounts:");
        for (provider, accounts) in &rows {
            for account in accounts {
                println!("  * {:<10} {}", provider.id(), account.as_str());
            }
        }
    }

    exit_codes::SUCCESS
}

// ─── test ─────────────────────────────────────────────────────────────────────

async fn run_test(provider: &str, account: &str, json: bool) -> i32 {
    let provider_id = match parse_provider(provider) {
        Ok(p) => p,
        Err(e) => {
            emit_error(e, json);
            return exit_codes::GENERAL_ERROR;
        }
    };
    let Some(auth) = open_auth_manager(json) else {
        return exit_codes::GENERAL_ERROR;
    };
    let registry = match build_registry(auth).await {
        Ok(r) => r,
        Err(e) => {
            emit_error(e, json);
            return exit_codes::GENERAL_ERROR;
        }
    };
    let connector = match registry.get(provider_id.id()).await {
        Some(c) => c,
        None => {
            emit_error(format!("connector '{provider}' not registered"), json);
            return exit_codes::GENERAL_ERROR;
        }
    };

    let account_id = AccountId::new(account.to_string());
    match connector.check(&account_id).await {
        Ok(report) => {
            if json {
                let body = serde_json::json!({
                    "provider": provider_id.id(),
                    "account": account,
                    "ok": report.reachable,
                    "reachable": report.reachable,
                    "detail": report.detail,
                    "granted_scopes": report.granted_scopes,
                });
                println!("{}", serde_json::to_string_pretty(&body).unwrap_or_default());
            } else {
                let glyph = if report.reachable { "*" } else { "x" };
                println!(
                    "  {glyph} {} / {} reachable={}",
                    provider_id.id(),
                    account,
                    report.reachable
                );
                if !report.detail.is_empty() {
                    println!("    detail: {}", report.detail);
                }
                if !report.granted_scopes.is_empty() {
                    println!("    scopes ({}):", report.granted_scopes.len());
                    for s in &report.granted_scopes {
                        println!("      - {s}");
                    }
                }
            }
            if report.reachable {
                exit_codes::SUCCESS
            } else {
                exit_codes::GENERAL_ERROR
            }
        }
        Err(e) => {
            if json {
                let body = serde_json::json!({
                    "provider": provider_id.id(),
                    "account": account,
                    "ok": false,
                    "error": e.to_string(),
                });
                println!("{}", serde_json::to_string_pretty(&body).unwrap_or_default());
            } else {
                eprintln!("Error: connector check failed: {e}");
            }
            exit_codes::GENERAL_ERROR
        }
    }
}

// ─── revoke ───────────────────────────────────────────────────────────────────

async fn run_revoke(provider: &str, account: &str, confirm: bool, json: bool) -> i32 {
    let provider_id = match parse_provider(provider) {
        Ok(p) => p,
        Err(e) => {
            emit_error(e, json);
            return exit_codes::GENERAL_ERROR;
        }
    };
    if !confirm {
        emit_error(
            format!(
                "pass --confirm to revoke {} / {} without prompt",
                provider_id.id(),
                account
            ),
            json,
        );
        return exit_codes::GENERAL_ERROR;
    }
    let Some(auth) = open_auth_manager(json) else {
        return exit_codes::GENERAL_ERROR;
    };
    let account_id = AccountId::new(account.to_string());
    match auth.revoke(provider_id, &account_id).await {
        Ok(()) => {
            if json {
                let body = serde_json::json!({
                    "provider": provider_id.id(),
                    "account": account,
                    "revoked": true,
                });
                println!("{}", serde_json::to_string_pretty(&body).unwrap_or_default());
            } else {
                println!(
                    "  * {} / {} token revoked locally",
                    provider_id.id(),
                    account
                );
                println!(
                    "    Note: upstream AS not notified. Visit the provider revocation page if needed."
                );
            }
            exit_codes::SUCCESS
        }
        Err(e) => {
            emit_error(format!("revoke failed: {e}"), json);
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
        cmd: ConnectorCommand,
    }

    #[test]
    fn parse_provider_known_ids() {
        assert_eq!(parse_provider("google").unwrap(), ConnectorProvider::Google);
        assert_eq!(
            parse_provider("microsoft").unwrap(),
            ConnectorProvider::Microsoft
        );
    }

    #[test]
    fn parse_provider_rejects_unknown() {
        assert!(parse_provider("notion").is_err());
    }

    #[test]
    fn parses_list() {
        let cli = TestCli::parse_from(["x", "list"]);
        assert!(matches!(cli.cmd, ConnectorCommand::List));
    }

    #[test]
    fn parses_accounts_no_filter() {
        let cli = TestCli::parse_from(["x", "accounts"]);
        match cli.cmd {
            ConnectorCommand::Accounts { provider } => assert!(provider.is_none()),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parses_accounts_with_provider() {
        let cli = TestCli::parse_from(["x", "accounts", "--provider", "google"]);
        match cli.cmd {
            ConnectorCommand::Accounts { provider } => {
                assert_eq!(provider.as_deref(), Some("google"));
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parses_test() {
        let cli = TestCli::parse_from(["x", "test", "google", "alice@example.com"]);
        match cli.cmd {
            ConnectorCommand::Test { provider, account } => {
                assert_eq!(provider, "google");
                assert_eq!(account, "alice@example.com");
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parses_revoke_with_confirm() {
        let cli = TestCli::parse_from(["x", "revoke", "google", "alice@example.com", "--confirm"]);
        match cli.cmd {
            ConnectorCommand::Revoke {
                provider,
                account,
                confirm,
            } => {
                assert_eq!(provider, "google");
                assert_eq!(account, "alice@example.com");
                assert!(confirm);
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[tokio::test]
    async fn revoke_without_confirm_returns_error() {
        let code = run_revoke("google", "x@example.com", false, true).await;
        assert_eq!(code, exit_codes::GENERAL_ERROR);
    }

    #[tokio::test]
    async fn test_with_unknown_provider_errors() {
        let code = run_test("dropbox", "x@example.com", true).await;
        assert_eq!(code, exit_codes::GENERAL_ERROR);
    }
}
