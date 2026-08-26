//! Connector inventory verbs: `list`, `accounts`, `test` and `revoke`.

use apollia_auth::{AccountId, AuthManager, ConnectorProvider};

use crate::exit_codes;
use crate::note;

use super::{build_registry, emit_error, open_auth_manager, parse_provider};

// ─── list ─────────────────────────────────────────────────────────────────────

pub(super) async fn run_list(json: bool) -> i32 {
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

pub(super) async fn run_accounts(filter: Option<&str>, json: bool) -> i32 {
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

pub(super) async fn run_test(provider: &str, account: &str, json: bool) -> i32 {
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
pub(super) fn render_test_report(
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
pub(super) fn render_test_report_text(
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
pub(super) fn render_test_error(
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

pub(super) async fn run_revoke(provider: &str, account: &str, confirm: bool, json: bool) -> i32 {
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
pub(super) async fn revoke_and_report(
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
