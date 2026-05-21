//! `apollia-os mcp oauth` subcommands — interactive PKCE flow from the CLI.
//!
//! Mirrors the Desktop `mcp_oauth_login` Tauri command but drives the browser
//! via the `open` crate. The orchestrator (`apollia_auth::negotiate_token`)
//! accepts an injected `open_browser` closure precisely so the same flow can
//! run headless from a terminal: the URL is printed to stdout *and* opened in
//! the system browser, then the OAuth provider redirects to a loopback
//! listener that the orchestrator manages.
//!
//! Tokens are stored under the canonical keychain service
//! `apollia_auth::MCP_OAUTH_SERVICE` (`apollia-mcp-oauth`), so they are
//! transparently shared with the Desktop runtime (subject to OS keychain ACL
//! between the two binaries).

use std::path::PathBuf;

use clap::Subcommand;

use apollia_auth::{
    delete_mcp_token, load_mcp_token, negotiate_token, select_secret_store, AuthError,
    McpOAuthError, NegotiateRequest,
};

use crate::client::{ClientError, RuntimeClient, DEFAULT_SOCKET_PATH};
use crate::exit_codes;

/// Subcommands of `apollia-os mcp oauth`.
#[derive(Debug, Subcommand)]
pub enum McpOauthCommand {
    /// Run the interactive PKCE login flow for `<server>` and persist the token.
    ///
    /// Opens the OAuth authorisation URL in the system browser, prints it on
    /// stdout (so headless setups can copy / paste it), waits for the
    /// authorisation server to redirect back to the loopback listener, then
    /// stores the resulting access + refresh tokens in the OS keychain under
    /// `apollia-mcp-oauth/<server>`.
    Login {
        /// Server name as declared in `mcp.db` (matches the Desktop wizard).
        server: String,
        /// Optional comma-separated scope list. Omit to defer to the AS's
        /// `scopes_supported` (recommended).
        #[arg(long, value_delimiter = ',', value_name = "SCOPE")]
        scopes: Vec<String>,
        /// Override the OAuth client id resolution (for tenants running their
        /// own AS app — usually unnecessary).
        #[arg(long, value_name = "ID")]
        client_id: Option<String>,
        /// Override the path to `mcp.db` (default: `~/.apollia/mcp.db`).
        #[arg(long, value_name = "PATH")]
        db: Option<PathBuf>,
    },

    /// Report the persisted-token status for one or every configured server.
    ///
    /// Surfaces token expiry, granted scopes, and identity claims (`sub`,
    /// `email`) without revealing the access token itself.
    Status {
        /// Optional server name. When omitted, lists every server with a
        /// stored token plus those declared in `mcp.db` but unauthenticated.
        server: Option<String>,
        #[arg(long, value_name = "PATH")]
        db: Option<PathBuf>,
    },

    /// Delete the persisted token for `<server>` from the OS keychain.
    ///
    /// The authorisation server is **not** notified — call the provider's
    /// revocation endpoint manually if a server-side revocation is required.
    Logout {
        /// Server name to forget.
        server: String,
        /// Skip the confirmation prompt.
        #[arg(long)]
        confirm: bool,
    },
}

/// Entry point for `apollia-os mcp oauth <verb>`.
pub async fn run(cmd: &McpOauthCommand, json: bool) -> i32 {
    match cmd {
        McpOauthCommand::Login {
            server,
            scopes,
            client_id,
            db,
        } => run_login(server, scopes, client_id.as_deref(), db.as_deref(), json).await,
        McpOauthCommand::Status { server, db } => {
            run_status(server.as_deref(), db.as_deref(), json).await
        }
        McpOauthCommand::Logout { server, confirm } => run_logout(server, *confirm, json),
    }
}

fn emit_error(msg: impl Into<String>, json: bool) -> i32 {
    let s = msg.into();
    if json {
        let out = serde_json::json!({"error": s});
        println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
    } else {
        eprintln!("Error: {s}");
    }
    exit_codes::GENERAL_ERROR
}

/// Outcome of the post-login runtime reconnect attempt. Recorded so the
/// human-readable output can give precise next-step guidance instead of a
/// generic "token stored" without telling the operator whether the server is
/// actually live.
#[derive(Debug)]
enum ReconnectOutcome {
    /// Runtime answered, server is now connected.
    Connected,
    /// Runtime not running — token is stored for the next boot.
    RuntimeOffline,
    /// Runtime answered but the reconnect attempt failed (parsed error in field).
    Failed(String),
    /// Runtime answered but said the server is unknown (404 fallback).
    Skipped,
}

/// Hit `POST /api/v1/mcp/servers/<name>/restart` so the runtime picks up the
/// freshly stored token. The route falls back to `add_server` when no session
/// was ever started (the normal post-OAuth case for a server that boot-failed),
/// so a single call covers both "session exists" and "first connection" paths.
async fn reconnect_runtime_session(server: &str) -> ReconnectOutcome {
    let client = RuntimeClient::new(PathBuf::from(DEFAULT_SOCKET_PATH));
    let uri = format!("/api/v1/mcp/servers/{server}/restart");
    match client.post(&uri, None).await {
        Ok(resp) if resp.status < 400 => ReconnectOutcome::Connected,
        Ok(resp) if resp.status == 404 => ReconnectOutcome::Skipped,
        Ok(resp) => ReconnectOutcome::Failed(format!("HTTP {}: {}", resp.status, resp.body)),
        Err(ClientError::ConnectionRefused) => ReconnectOutcome::RuntimeOffline,
        Err(e) => ReconnectOutcome::Failed(e.to_string()),
    }
}

impl serde::Serialize for ReconnectOutcome {
    fn serialize<S: serde::Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        match self {
            ReconnectOutcome::Connected => ser.serialize_str("connected"),
            ReconnectOutcome::RuntimeOffline => ser.serialize_str("runtime_offline"),
            ReconnectOutcome::Skipped => ser.serialize_str("skipped"),
            ReconnectOutcome::Failed(reason) => {
                use serde::ser::SerializeStruct;
                let mut s = ser.serialize_struct("ReconnectOutcome", 2)?;
                s.serialize_field("status", "failed")?;
                s.serialize_field("reason", reason)?;
                s.end()
            }
        }
    }
}

/// Probe `server_url` to capture the `WWW-Authenticate` header advertised
/// when the server requires OAuth. Returns `Ok(Some(header))` when the server
/// answered 401 with the header, `Ok(None)` when it answered anything else,
/// and `Err(...)` only when the request itself never reached the server.
///
/// Required for hosts whose Protected Resource Metadata lives behind the
/// MCP endpoint (Notion, Linear) rather than at the well-known origin path.
async fn probe_www_authenticate(server_url: &str) -> Result<Option<String>, reqwest::Error> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;
    // Use a minimal POST so the server's auth middleware engages even when GET
    // is unauthenticated; many MCP HTTP transports answer 200 on GET (health
    // probe) but 401 on POST. The body is intentionally minimal — we only
    // need the response headers.
    let resp = client
        .post(server_url)
        .header("content-type", "application/json")
        .body("{}")
        .send()
        .await?;
    if resp.status() != reqwest::StatusCode::UNAUTHORIZED {
        return Ok(None);
    }
    Ok(resp
        .headers()
        .get(reqwest::header::WWW_AUTHENTICATE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string()))
}

/// Resolve `~/.apollia/mcp.db` or honour an explicit override.
fn resolve_mcp_db(override_path: Option<&std::path::Path>) -> PathBuf {
    if let Some(p) = override_path {
        return p.to_path_buf();
    }
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".apollia")
        .join("mcp.db")
}

/// Look up the URL stored for `server` in `mcp.db`. The orchestrator needs it
/// to drive resource-metadata discovery — failing fast here yields a far
/// clearer error than a cryptic `fetch_prm` failure deep in the auth crate.
fn load_server_url(db_override: Option<&std::path::Path>, server: &str) -> Result<String, String> {
    let path = resolve_mcp_db(db_override);
    if !path.exists() {
        return Err(format!(
            "mcp database not found at {} (Desktop or `apollia-os mcp add` must have run first)",
            path.display()
        ));
    }
    let repo = apollia_mcp::McpServerRepository::open(&path)
        .map_err(|e| format!("open {} failed: {e}", path.display()))?;
    let entry = repo
        .list()
        .map_err(|e| format!("read mcp servers failed: {e}"))?
        .into_iter()
        .find(|s| s.name == server)
        .ok_or_else(|| format!("server '{server}' not configured in {}", path.display()))?;
    entry
        .url
        .clone()
        .filter(|u| !u.is_empty())
        .ok_or_else(|| {
            format!(
                "server '{server}' has no URL — only HTTP/streamable-http MCP servers \
                 require OAuth (stdio servers run a local subprocess)"
            )
        })
}

// ─── login ────────────────────────────────────────────────────────────────────

async fn run_login(
    server: &str,
    scopes: &[String],
    client_id: Option<&str>,
    db_override: Option<&std::path::Path>,
    json: bool,
) -> i32 {
    let server_url = match load_server_url(db_override, server) {
        Ok(u) => u,
        Err(e) => return emit_error(e, json),
    };

    let store = match select_secret_store() {
        Ok(s) => s,
        Err(e) => return emit_error(format!("keychain unavailable: {e}"), json),
    };

    let scopes_opt: Option<Vec<String>> = if scopes.is_empty() {
        None
    } else {
        Some(scopes.to_vec())
    };
    let client_id_override = client_id
        .map(str::trim)
        .filter(|s| !s.is_empty());

    // Probe the server first to capture the WWW-Authenticate header. Many MCP
    // hosts (Notion, Linear, …) hide their Protected Resource Metadata behind
    // an authenticated path and only advertise it via the 401 challenge, so
    // letting `negotiate_token` discover PRM blindly fails with "PRM fetch
    // returned 401". The probe is a cheap unauthenticated request — when the
    // server returns 401 we read the header verbatim; on 200 / other we leave
    // it as None and let the orchestrator's origin fallback do the work.
    let www_authenticate = match probe_www_authenticate(&server_url).await {
        Ok(Some(header)) => {
            tracing::debug!(server = %server, "probe captured WWW-Authenticate header");
            Some(header)
        }
        Ok(None) => {
            tracing::debug!(server = %server, "probe returned non-401, no WWW-Authenticate to extract");
            None
        }
        Err(e) => {
            // Surface this clearly on stderr because the orchestrator's
            // origin fallback often points at an auth-gated 401, leaving
            // the operator wondering why a perfectly reachable server
            // refuses to authorise. The probe error is the real cause.
            eprintln!(
                "  ! probe of {server_url} failed: {e}\n    OAuth dance will use origin-based PRM discovery; may fail for hosts whose PRM lives behind /mcp."
            );
            None
        }
    };

    let req = NegotiateRequest {
        server_name: server,
        server_url: &server_url,
        www_authenticate: www_authenticate.as_deref(),
        scopes: scopes_opt,
        client_id_override,
    };

    if !json {
        eprintln!(
            "Starting MCP OAuth flow for '{server}' against {server_url}.\n  \
             A browser will open with the authorisation URL — log in there to complete the flow."
        );
    }

    let result = negotiate_token(req, &*store, |url| {
        // Print + open. Printing first guarantees the URL is reachable in
        // truly headless contexts (SSH, CI) where the `open` crate is a
        // no-op or fails silently.
        if !json {
            println!("\n  -> open in browser:\n     {url}\n");
        }
        match open::that(url) {
            Ok(()) => Ok(()),
            Err(e) => Err(AuthError::CallbackServer(format!(
                "failed to open browser ({e}); copy the URL above manually"
            ))),
        }
    })
    .await;

    match result {
        Ok(token) => {
            if json {
                let reconnect = reconnect_runtime_session(server).await;
                let body = serde_json::json!({
                    "server": server,
                    "stored": true,
                    "scopes": token.scope,
                    "identity": {
                        "sub": token.identity_sub,
                        "email": token.identity_email,
                    },
                    "expires_at": token.expires_at,
                    "runtime_reconnect": reconnect,
                });
                println!("{}", serde_json::to_string_pretty(&body).unwrap_or_default());
            } else {
                println!("  * OAuth token stored for '{server}'");
                if let Some(email) = &token.identity_email {
                    println!("    identity : {email}");
                } else if let Some(sub) = &token.identity_sub {
                    println!("    identity : {sub}");
                }
                if !token.scope.is_empty() {
                    println!("    scopes   : {}", token.scope.join(", "));
                }
                if let Some(exp) = token.expires_at {
                    println!("    expires  : unix {exp}");
                }
                // Triggers an immediate runtime reconnect — at boot the server
                // failed because no token was stored yet, so without this the
                // operator has to also stop+start the daemon manually.
                match reconnect_runtime_session(server).await {
                    ReconnectOutcome::Connected => {
                        println!("  * runtime reconnected '{server}' (new token applied)");
                    }
                    ReconnectOutcome::RuntimeOffline => {
                        println!(
                            "  ! daemon offline — start it with `apollia-os start` and the server will pick up the token."
                        );
                    }
                    ReconnectOutcome::Failed(reason) => {
                        println!(
                            "  ! token stored but runtime reconnect failed: {reason}\n    Retry with `apollia-os mcp restart {server}`."
                        );
                    }
                    ReconnectOutcome::Skipped => {
                        // Daemon present but the server was unknown to the
                        // repo as well — extremely unlikely once we reached
                        // this point because load_server_url succeeded.
                        println!(
                            "  ! token stored; could not reconnect (server not in runtime repo)."
                        );
                    }
                }
            }
            exit_codes::SUCCESS
        }
        Err(McpOAuthError::ReauthRequired { .. }) => {
            emit_error(
                "the authorisation server rejected the refresh — stored token deleted, re-run login",
                json,
            )
        }
        Err(e) => emit_error(format!("OAuth flow failed: {e}"), json),
    }
}

// ─── status ───────────────────────────────────────────────────────────────────

async fn run_status(
    server: Option<&str>,
    db_override: Option<&std::path::Path>,
    json: bool,
) -> i32 {
    let store = match select_secret_store() {
        Ok(s) => s,
        Err(e) => return emit_error(format!("keychain unavailable: {e}"), json),
    };

    let servers = match server {
        Some(name) => vec![name.to_string()],
        None => {
            // Enumerate every server declared in mcp.db so unauthenticated
            // entries appear too, not just those with a stored token.
            let path = resolve_mcp_db(db_override);
            if !path.exists() {
                if json {
                    println!("[]");
                } else {
                    println!("  (mcp.db absent — no configured servers)");
                }
                return exit_codes::SUCCESS;
            }
            match apollia_mcp::McpServerRepository::open(&path) {
                Ok(repo) => repo
                    .list()
                    .map(|rows| rows.into_iter().map(|r| r.name).collect::<Vec<_>>())
                    .unwrap_or_default(),
                Err(e) => return emit_error(format!("open {} failed: {e}", path.display()), json),
            }
        }
    };

    let mut report: Vec<serde_json::Value> = Vec::with_capacity(servers.len());
    for name in &servers {
        match load_mcp_token(&*store, name) {
            Ok(Some(token)) => {
                report.push(serde_json::json!({
                    "server": name,
                    "stored": true,
                    "scopes": token.scope,
                    "expires_at": token.expires_at,
                    "identity": {
                        "sub": token.identity_sub,
                        "email": token.identity_email,
                    },
                }));
            }
            Ok(None) => {
                report.push(serde_json::json!({
                    "server": name,
                    "stored": false,
                }));
            }
            Err(e) => {
                report.push(serde_json::json!({
                    "server": name,
                    "stored": false,
                    "error": e.to_string(),
                }));
            }
        }
    }

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).unwrap_or_default()
        );
    } else if report.is_empty() {
        println!("  (no servers found in mcp.db)");
    } else {
        println!("  {:<24} {:<10} {:<30} IDENTITY", "SERVER", "STORED", "SCOPES");
        for entry in &report {
            let s = entry.get("server").and_then(|v| v.as_str()).unwrap_or("?");
            let stored = entry
                .get("stored")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let stored_glyph = if stored { "*" } else { "-" };
            let scopes = entry
                .get("scopes")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join(",")
                })
                .unwrap_or_default();
            let identity = entry
                .get("identity")
                .and_then(|v| v.get("email").or_else(|| v.get("sub")))
                .and_then(|v| v.as_str())
                .unwrap_or("-");
            let scopes_truncated = if scopes.len() > 28 {
                format!("{}…", &scopes[..27])
            } else {
                scopes
            };
            println!(
                "  {s:<24} {stored_glyph:<10} {scopes_truncated:<30} {identity}"
            );
        }
    }
    exit_codes::SUCCESS
}

// ─── logout ───────────────────────────────────────────────────────────────────

fn run_logout(server: &str, confirm: bool, json: bool) -> i32 {
    if !confirm {
        return emit_error(
            format!(
                "use --confirm to delete the stored token for '{server}'. \
                 The authorisation server is not notified."
            ),
            json,
        );
    }
    let store = match select_secret_store() {
        Ok(s) => s,
        Err(e) => return emit_error(format!("keychain unavailable: {e}"), json),
    };
    match delete_mcp_token(&*store, server) {
        Ok(()) => {
            if json {
                let out = serde_json::json!({"server": server, "deleted": true});
                println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
            } else {
                println!("  * token removed for '{server}'");
            }
            exit_codes::SUCCESS
        }
        Err(e) => emit_error(format!("delete failed: {e}"), json),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser, Debug)]
    struct TestCli {
        #[command(subcommand)]
        cmd: McpOauthCommand,
    }

    #[test]
    fn parses_login_minimal() {
        let cli = TestCli::parse_from(["x", "login", "notion"]);
        match cli.cmd {
            McpOauthCommand::Login {
                server,
                scopes,
                client_id,
                ..
            } => {
                assert_eq!(server, "notion");
                assert!(scopes.is_empty());
                assert!(client_id.is_none());
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parses_login_with_scopes() {
        let cli = TestCli::parse_from([
            "x",
            "login",
            "notion",
            "--scopes",
            "read,write,admin",
        ]);
        match cli.cmd {
            McpOauthCommand::Login { scopes, .. } => {
                assert_eq!(scopes, vec!["read", "write", "admin"]);
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parses_status_optional_server() {
        let cli_all = TestCli::parse_from(["x", "status"]);
        let cli_one = TestCli::parse_from(["x", "status", "notion"]);
        assert!(matches!(
            cli_all.cmd,
            McpOauthCommand::Status { server: None, .. }
        ));
        match cli_one.cmd {
            McpOauthCommand::Status { server, .. } => {
                assert_eq!(server.as_deref(), Some("notion"));
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parses_logout_requires_confirm_flag() {
        let cli = TestCli::parse_from(["x", "logout", "notion"]);
        match cli.cmd {
            McpOauthCommand::Logout { server, confirm } => {
                assert_eq!(server, "notion");
                assert!(!confirm);
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parses_logout_with_confirm() {
        let cli = TestCli::parse_from(["x", "logout", "notion", "--confirm"]);
        match cli.cmd {
            McpOauthCommand::Logout { confirm, .. } => assert!(confirm),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn logout_without_confirm_errors() {
        let code = run_logout("notion", false, true);
        assert_eq!(code, exit_codes::GENERAL_ERROR);
    }

    #[test]
    fn resolve_mcp_db_honours_override() {
        let p = std::path::Path::new("/tmp/custom-mcp.db");
        assert_eq!(resolve_mcp_db(Some(p)), PathBuf::from("/tmp/custom-mcp.db"));
    }

    #[test]
    fn resolve_mcp_db_default_ends_with_canonical_filename() {
        let p = resolve_mcp_db(None);
        assert!(
            p.to_string_lossy().ends_with(".apollia/mcp.db"),
            "unexpected default path: {p:?}"
        );
    }
}
