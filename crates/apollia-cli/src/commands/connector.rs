//! `apollia-os connector` subcommands: manage native SaaS connectors.
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
use crate::note;

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
    /// Server reports as granted, the same shape used by `connector.check()`
    /// inside the runtime.
    Test {
        /// Provider id: `google` or `microsoft`.
        provider: String,
        /// Account identifier (email when supplied during OAuth login).
        account: String,
    },

    /// Revoke the stored token for `(provider, account)`.
    ///
    /// Only the local keyring entry is cleared, the upstream Authorization
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

    /// Manage OAuth client_id overrides in `~/.apollia/oauth-clients.toml`.
    ///
    /// Power-user / Expert Mode: lets a CLI operator plug in their own Google
    /// or Microsoft client_id without rebuilding the binary. Resolution chain
    /// per provider is `env var > oauth-clients.toml > compiled default`.
    #[command(name = "client-id", subcommand)]
    ClientId(ClientIdCommand),

    /// Manage OAuth client_secret overrides in `~/.apollia/oauth-clients.toml`.
    ///
    /// Required by Google (Installed App needs a secret) and a no-op for
    /// Microsoft (public client per spec). File is created on demand with
    /// `0o600` permissions on Unix.
    #[command(name = "client-secret", subcommand)]
    ClientSecret(ClientSecretCommand),

    /// Manage API key overrides (Google Picker) in `~/.apollia/oauth-clients.toml`.
    ///
    /// Google-only today. Microsoft slot is reserved for the OneDrive File
    /// Picker if added later.
    #[command(name = "api-key", subcommand)]
    ApiKey(ApiKeyCommand),

    /// Manage per-account Google Drive folder preferences.
    ///
    /// Operates on `~/.apollia/drive-prefs.toml` and is independent of the
    /// runtime. The `picked` sub-group lists folders captured via the Desktop
    /// Picker (the CLI cannot pick, no UI, but can review and remove them).
    Drive {
        /// Drive subcommand.
        #[command(subcommand)]
        command: DriveCommand,
    },
}

/// Subcommands of `apollia-os connector client-id`.
#[derive(Debug, Subcommand)]
pub enum ClientIdCommand {
    /// List every provider's effective client_id + source + override.
    List,

    /// Set the client_id override for `<provider>`.
    ///
    /// Pass an empty string (`""`) to clear the override.
    Set {
        /// Provider id: `google` or `microsoft`.
        provider: String,
        /// New client_id value. Empty string clears the override.
        client_id: String,
    },
}

/// Subcommands of `apollia-os connector client-secret`.
#[derive(Debug, Subcommand)]
pub enum ClientSecretCommand {
    /// Set the client_secret override for `<provider>`.
    ///
    /// Pass an empty string (`""`) to clear the override. The CLI does not
    /// echo the secret back, but it is written to `~/.apollia/oauth-clients.toml`.
    Set {
        /// Provider id: `google` or `microsoft`.
        provider: String,
        /// New client_secret value. Empty string clears the override.
        client_secret: String,
    },
}

/// Subcommands of `apollia-os connector api-key`.
#[derive(Debug, Subcommand)]
pub enum ApiKeyCommand {
    /// Set the API key override for `<provider>`.
    ///
    /// Pass an empty string (`""`) to clear the override.
    Set {
        /// Provider id: `google` or `microsoft`.
        provider: String,
        /// New API key value. Empty string clears the override.
        api_key: String,
    },
}

/// Subcommands of `apollia-os connector drive`.
#[derive(Debug, Subcommand)]
pub enum DriveCommand {
    /// Manage the per-account Drive root folder path.
    Folder {
        /// Folder subcommand.
        #[command(subcommand)]
        command: DriveFolderCommand,
    },
}

/// Subcommands of `apollia-os connector drive folder`.
#[derive(Debug, Subcommand)]
pub enum DriveFolderCommand {
    /// List the folder override + effective path for every Google account.
    List,

    /// Set the folder path override for `<account>`.
    Set {
        /// Account id (typically the Google email).
        account: String,
        /// New folder path (e.g. `Apollia/Workspace`).
        path: String,
    },

    /// Reset the folder override for `<account>` (falls back to the default).
    Reset {
        /// Account id.
        account: String,
    },

    /// Manage the picked-folder list captured via the Desktop Drive Picker.
    Picked {
        /// Picked-folder subcommand.
        #[command(subcommand)]
        command: PickedFolderCommand,
    },
}

/// Subcommands of `apollia-os connector drive folder picked`.
#[derive(Debug, Subcommand)]
pub enum PickedFolderCommand {
    /// List the picked Drive folders persisted for `<account>`.
    List {
        /// Account id.
        account: String,
    },

    /// Remove a picked folder from the persisted list.
    Remove {
        /// Account id.
        account: String,
        /// Drive folder id (the same id surfaced by `picked list`).
        folder_id: String,
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
        ConnectorCommand::ClientId(sub) => run_client_id(sub, json),
        ConnectorCommand::ClientSecret(sub) => run_client_secret(sub, json),
        ConnectorCommand::ApiKey(sub) => run_api_key(sub, json),
        ConnectorCommand::Drive { command } => run_drive(command, json).await,
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
    let _ = crate::output::emit_error(json, exit_codes::GENERAL_ERROR, &msg);
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
        note!("  Available connectors:");
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
            println!("  -> Connect an account from the desktop app, Settings > Integrations.");
            println!(
                "     There is no CLI command for this: the OAuth flow needs a browser redirect."
            );
            return exit_codes::SUCCESS;
        }
        note!("  Connected accounts:");
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
    let report = match connector.check(&account_id).await {
        Ok(report) => report,
        Err(e) => return render_test_error(provider_id.id(), account, &e, json),
    };
    render_test_report(provider_id.id(), account, &report, json)
}

/// Renders a successful connector check report and returns the exit code.
fn render_test_report(
    provider: &str,
    account: &str,
    report: &apollia_connectors::HealthReport,
    json: bool,
) -> i32 {
    if json {
        let body = serde_json::json!({
            "provider": provider,
            "account": account,
            "ok": report.reachable,
            "reachable": report.reachable,
            "detail": report.detail,
            "granted_scopes": report.granted_scopes,
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&body).unwrap_or_default()
        );
    } else {
        render_test_report_text(provider, account, report);
    }
    if report.reachable {
        exit_codes::SUCCESS
    } else {
        exit_codes::GENERAL_ERROR
    }
}

/// Human-readable rendering of a connector check report.
fn render_test_report_text(
    provider: &str,
    account: &str,
    report: &apollia_connectors::HealthReport,
) {
    let glyph = if report.reachable { "*" } else { "x" };
    println!(
        "  {glyph} {} / {} reachable={}",
        provider, account, report.reachable
    );
    if !report.detail.is_empty() {
        println!("    detail: {}", report.detail);
    }
    if !report.granted_scopes.is_empty() {
        note!("    scopes ({}):", report.granted_scopes.len());
        for s in &report.granted_scopes {
            println!("      - {s}");
        }
    }
}

/// Renders a failed connector check and returns the error exit code.
fn render_test_error(
    provider: &str,
    account: &str,
    error: &dyn std::fmt::Display,
    json: bool,
) -> i32 {
    crate::output::emit_error(
        json,
        exit_codes::GENERAL_ERROR,
        &format!("connector check failed for {provider} account '{account}': {error}"),
    )
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
    revoke_and_report(&auth, provider_id, account, json).await
}

/// Revoke the stored token and map the outcome to an exit code.
///
/// Revoking an account with no stored token exits [`exit_codes::SUCCESS`]:
/// the storage contract is idempotent (`MultiAccountStorage::delete` returns
/// `Ok(())` even if the token was already gone). Split from [`run_revoke`] so
/// the exit-code mapping is testable against an isolated [`AuthManager`],
/// without the platform keyring or the real `~/.apollia` index.
async fn revoke_and_report(
    auth: &AuthManager,
    provider_id: ConnectorProvider,
    account: &str,
    json: bool,
) -> i32 {
    let account_id = AccountId::new(account.to_string());
    match auth.revoke(provider_id, &account_id).await {
        Ok(()) => {
            if json {
                let body = serde_json::json!({
                    "provider": provider_id.id(),
                    "account": account,
                    "revoked": true,
                });
                println!(
                    "{}",
                    serde_json::to_string_pretty(&body).unwrap_or_default()
                );
            } else {
                println!(
                    "  * {} / {} token revoked locally",
                    provider_id.id(),
                    account
                );
                note!(
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

// ─── client-id / client-secret / api-key ──────────────────────────────────────

/// Detect the active source of `<provider>.client_id`. Mirrors the Tauri
/// `detect_source` helper so CLI and Desktop agree on what they report.
fn detect_client_id_source(provider: ConnectorProvider) -> &'static str {
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
fn detect_client_secret_source(provider: ConnectorProvider) -> &'static str {
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
fn detect_api_key_source(provider: ConnectorProvider) -> &'static str {
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

fn run_client_id(cmd: &ClientIdCommand, json: bool) -> i32 {
    match cmd {
        ClientIdCommand::List => run_client_id_list(json),
        ClientIdCommand::Set {
            provider,
            client_id,
        } => run_client_id_set(provider, client_id, json),
    }
}

fn run_client_id_list(json: bool) -> i32 {
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

fn run_client_id_set(provider: &str, client_id: &str, json: bool) -> i32 {
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

fn run_client_secret(cmd: &ClientSecretCommand, json: bool) -> i32 {
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

fn run_api_key(cmd: &ApiKeyCommand, json: bool) -> i32 {
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

fn emit_set_ok(provider_id: &str, key: &str, cleared: bool, json: bool) {
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
fn mask_secret(s: &str) -> String {
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

// ─── drive folder management ──────────────────────────────────────────────────

async fn run_drive(cmd: &DriveCommand, json: bool) -> i32 {
    match cmd {
        DriveCommand::Folder { command } => run_drive_folder(command, json).await,
    }
}

async fn run_drive_folder(cmd: &DriveFolderCommand, json: bool) -> i32 {
    match cmd {
        DriveFolderCommand::List => run_drive_folder_list(json).await,
        DriveFolderCommand::Set { account, path } => run_drive_folder_set(account, path, json),
        DriveFolderCommand::Reset { account } => run_drive_folder_reset(account, json),
        DriveFolderCommand::Picked { command } => run_drive_picked(command, json),
    }
}

async fn run_drive_folder_list(json: bool) -> i32 {
    let Some(auth) = open_auth_manager(json) else {
        return exit_codes::GENERAL_ERROR;
    };
    let accounts = match auth.list_accounts(ConnectorProvider::Google).await {
        Ok(a) => a,
        Err(e) => {
            emit_error(format!("failed to list google accounts: {e}"), json);
            return exit_codes::GENERAL_ERROR;
        }
    };

    let rows: Vec<(String, Option<String>, String)> = accounts
        .iter()
        .map(|a| {
            let override_path = apollia_auth::drive_prefs::lookup_folder_path("google", a.as_str());
            let effective = apollia_auth::drive_prefs::effective_folder_path("google", a.as_str());
            (a.0.clone(), override_path, effective)
        })
        .collect();

    if json {
        let array: Vec<serde_json::Value> = rows
            .iter()
            .map(|(account_id, override_path, effective)| {
                serde_json::json!({
                    "account_id": account_id,
                    "folder_path": override_path,
                    "effective_folder_path": effective,
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::Value::Array(array)).unwrap_or_default()
        );
    } else if rows.is_empty() {
        println!("No connected Google accounts. Connect one from the desktop app, Settings > Integrations.");
    } else {
        note!("  Drive folder configuration (google):");
        for (account_id, override_path, effective) in &rows {
            println!("  * {account_id}");
            match override_path {
                Some(p) => println!("      override : {p}"),
                None => println!("      override : <default>"),
            }
            println!("      effective: {effective}");
        }
    }
    exit_codes::SUCCESS
}

fn run_drive_folder_set(account: &str, path: &str, json: bool) -> i32 {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        emit_error(
            "folder path must not be empty (use `reset` to clear an override)".into(),
            json,
        );
        return exit_codes::GENERAL_ERROR;
    }
    match apollia_auth::drive_prefs::set_folder_path("google", account, trimmed) {
        Ok(()) => {
            if json {
                let body = serde_json::json!({
                    "provider": "google",
                    "account_id": account,
                    "folder_path": trimmed,
                    "updated": true,
                });
                println!(
                    "{}",
                    serde_json::to_string_pretty(&body).unwrap_or_default()
                );
            } else {
                println!("  * google / {account} folder set to: {trimmed}");
            }
            exit_codes::SUCCESS
        }
        Err(e) => {
            emit_error(format!("failed to write drive-prefs.toml: {e}"), json);
            exit_codes::GENERAL_ERROR
        }
    }
}

fn run_drive_folder_reset(account: &str, json: bool) -> i32 {
    match apollia_auth::drive_prefs::reset_folder_path("google", account) {
        Ok(()) => {
            if json {
                let body = serde_json::json!({
                    "provider": "google",
                    "account_id": account,
                    "reset": true,
                });
                println!(
                    "{}",
                    serde_json::to_string_pretty(&body).unwrap_or_default()
                );
            } else {
                println!("  * google / {account} folder override reset to default");
            }
            exit_codes::SUCCESS
        }
        Err(e) => {
            emit_error(format!("failed to write drive-prefs.toml: {e}"), json);
            exit_codes::GENERAL_ERROR
        }
    }
}

fn run_drive_picked(cmd: &PickedFolderCommand, json: bool) -> i32 {
    match cmd {
        PickedFolderCommand::List { account } => run_drive_picked_list(account, json),
        PickedFolderCommand::Remove { account, folder_id } => {
            run_drive_picked_remove(account, folder_id, json)
        }
    }
}

fn run_drive_picked_list(account: &str, json: bool) -> i32 {
    let folders = apollia_auth::drive_prefs::list_picked_folders("google", account);
    if json {
        let array: Vec<serde_json::Value> = folders
            .iter()
            .map(|f| {
                serde_json::json!({
                    "id": f.id,
                    "name": f.name,
                    "mime_type": f.mime_type,
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::Value::Array(array)).unwrap_or_default()
        );
    } else if folders.is_empty() {
        println!("No picked Drive folders for google / {account}.");
        println!("  -> Use the Desktop app to pick folders (the CLI has no Picker UI).");
    } else {
        println!("  Picked Drive folders (google / {account}):");
        for f in &folders {
            println!("  * {} ({})", f.name, f.id);
            println!("      mime: {}", f.mime_type);
        }
    }
    exit_codes::SUCCESS
}

fn run_drive_picked_remove(account: &str, folder_id: &str, json: bool) -> i32 {
    match apollia_auth::drive_prefs::remove_picked_folder("google", account, folder_id) {
        Ok(()) => {
            if json {
                let body = serde_json::json!({
                    "provider": "google",
                    "account_id": account,
                    "folder_id": folder_id,
                    "removed": true,
                });
                println!(
                    "{}",
                    serde_json::to_string_pretty(&body).unwrap_or_default()
                );
            } else {
                println!("  * google / {account} picked folder {folder_id} removed");
            }
            exit_codes::SUCCESS
        }
        Err(e) => {
            emit_error(format!("failed to write drive-prefs.toml: {e}"), json);
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
    async fn revoke_absent_account_exits_success() {
        // GIVEN an auth manager over an isolated index file and the mock
        // keyring (process-global builder; no other test in this binary
        // touches a keyring entry, so no ordering dependence), holding no
        // token for the account
        keyring::set_default_credential_builder(keyring::mock::default_credential_builder());
        let dir = tempfile::tempdir().unwrap();
        let storage = apollia_auth::multi_account::MultiAccountStorage::with_index_path(
            dir.path().join("idx.json"),
        );
        let auth = AuthManager::with_storage(storage);

        // WHEN revoking an account that was never connected
        let code = revoke_and_report(
            &auth,
            ConnectorProvider::Google,
            "absent@example.invalid",
            true,
        )
        .await;

        // THEN the documented idempotent contract decides the exit code
        // ("Returns Ok(()) even if the token was already gone",
        // MultiAccountStorage::delete): success, not an error
        assert_eq!(code, exit_codes::SUCCESS);
    }

    #[tokio::test]
    async fn test_with_unknown_provider_errors() {
        let code = run_test("dropbox", "x@example.com", true).await;
        assert_eq!(code, exit_codes::GENERAL_ERROR);
    }

    #[test]
    fn parses_client_id_list() {
        let cli = TestCli::parse_from(["x", "client-id", "list"]);
        assert!(matches!(
            cli.cmd,
            ConnectorCommand::ClientId(ClientIdCommand::List)
        ));
    }

    #[test]
    fn parses_client_id_set() {
        let cli =
            TestCli::parse_from(["x", "client-id", "set", "google", "abc123.apps.example.com"]);
        match cli.cmd {
            ConnectorCommand::ClientId(ClientIdCommand::Set {
                provider,
                client_id,
            }) => {
                assert_eq!(provider, "google");
                assert_eq!(client_id, "abc123.apps.example.com");
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parses_client_secret_set() {
        let cli = TestCli::parse_from(["x", "client-secret", "set", "google", "GOCSPX-xxx"]);
        match cli.cmd {
            ConnectorCommand::ClientSecret(ClientSecretCommand::Set {
                provider,
                client_secret,
            }) => {
                assert_eq!(provider, "google");
                assert_eq!(client_secret, "GOCSPX-xxx");
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parses_api_key_set() {
        let cli = TestCli::parse_from(["x", "api-key", "set", "google", "AIza..."]);
        match cli.cmd {
            ConnectorCommand::ApiKey(ApiKeyCommand::Set { provider, api_key }) => {
                assert_eq!(provider, "google");
                assert_eq!(api_key, "AIza...");
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parses_drive_folder_list() {
        let cli = TestCli::parse_from(["x", "drive", "folder", "list"]);
        assert!(matches!(
            cli.cmd,
            ConnectorCommand::Drive {
                command: DriveCommand::Folder {
                    command: DriveFolderCommand::List,
                },
            }
        ));
    }

    #[test]
    fn parses_drive_folder_set() {
        let cli = TestCli::parse_from([
            "x",
            "drive",
            "folder",
            "set",
            "alice@example.com",
            "Apollia/Workspace",
        ]);
        match cli.cmd {
            ConnectorCommand::Drive {
                command:
                    DriveCommand::Folder {
                        command: DriveFolderCommand::Set { account, path },
                    },
            } => {
                assert_eq!(account, "alice@example.com");
                assert_eq!(path, "Apollia/Workspace");
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parses_drive_folder_reset() {
        let cli = TestCli::parse_from(["x", "drive", "folder", "reset", "alice@example.com"]);
        match cli.cmd {
            ConnectorCommand::Drive {
                command:
                    DriveCommand::Folder {
                        command: DriveFolderCommand::Reset { account },
                    },
            } => {
                assert_eq!(account, "alice@example.com");
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parses_drive_folder_picked_list() {
        let cli = TestCli::parse_from([
            "x",
            "drive",
            "folder",
            "picked",
            "list",
            "alice@example.com",
        ]);
        match cli.cmd {
            ConnectorCommand::Drive {
                command:
                    DriveCommand::Folder {
                        command:
                            DriveFolderCommand::Picked {
                                command: PickedFolderCommand::List { account },
                            },
                    },
            } => assert_eq!(account, "alice@example.com"),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parses_drive_folder_picked_remove() {
        let cli = TestCli::parse_from([
            "x",
            "drive",
            "folder",
            "picked",
            "remove",
            "alice@example.com",
            "1abcDEFghi",
        ]);
        match cli.cmd {
            ConnectorCommand::Drive {
                command:
                    DriveCommand::Folder {
                        command:
                            DriveFolderCommand::Picked {
                                command: PickedFolderCommand::Remove { account, folder_id },
                            },
                    },
            } => {
                assert_eq!(account, "alice@example.com");
                assert_eq!(folder_id, "1abcDEFghi");
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn drive_folder_set_rejects_empty_path() {
        let code = run_drive_folder_set("alice@example.com", "   ", true);
        assert_eq!(code, exit_codes::GENERAL_ERROR);
    }

    #[test]
    fn mask_secret_short_input_is_fully_redacted() {
        assert_eq!(mask_secret(""), "<empty>");
        assert_eq!(mask_secret("short"), "********");
    }

    #[test]
    fn mask_secret_long_input_preserves_edges() {
        let masked = mask_secret("AIzaSyAbcdef1234567890");
        assert!(masked.starts_with("AIza"));
        assert!(masked.ends_with("90"));
        assert!(masked.contains("..."));
    }
}
