//! Local OAuth2 callback HTTP server (RFC 6749 §3.1.2).
//!
//! Binds an ephemeral TCP port on localhost, serves the OAuth callback at
//! both `/callback` and `/oauth/callback` (unified router so
//! the same listener serves the connector flows on `/callback` and the
//! MCP HTTP OAuth flow which advertises `/oauth/callback` in its CIMD),
//! validates the `state` parameter (CSRF protection), and returns the
//! authorization code to the caller via a channel.

use std::sync::{Arc, Mutex};

use axum::{extract::Query, response::Html, routing::get, Router};
use serde::Deserialize;
use tokio::{net::TcpListener, sync::oneshot};

use crate::error::AuthError;

type ResultCell = Arc<Mutex<Option<oneshot::Sender<Result<String, AuthError>>>>>;

// ─── HTML pages ───────────────────────────────────────────────────────────────

const SUCCESS_HTML: &str = concat!(
    "<!DOCTYPE html><html lang=\"en\">",
    "<head><meta charset=\"utf-8\">",
    "<title>Apollia - Authentication Successful</title></head>",
    "<body style=\"font-family:sans-serif;text-align:center;padding:2rem\">",
    "<h1>Authentication successful ✓</h1>",
    "<p>You can close this tab and return to the terminal.</p>",
    "</body></html>"
);

const ERROR_HTML: &str = concat!(
    "<!DOCTYPE html><html lang=\"en\">",
    "<head><meta charset=\"utf-8\">",
    "<title>Apollia - Authentication Failed</title></head>",
    "<body style=\"font-family:sans-serif;text-align:center;padding:2rem\">",
    "<h1>Authentication failed</h1>",
    "<p>An error occurred. Check the terminal for details.</p>",
    "</body></html>"
);

// ─── Internal types ───────────────────────────────────────────────────────────

/// Query parameters on the OAuth2 callback URL.
#[derive(Debug, Deserialize)]
struct CallbackParams {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
    #[serde(default)]
    error_description: Option<String>,
}

/// Longest provider error description carried into an error, in characters.
const MAX_ERROR_DESCRIPTION_CHARS: usize = 512;

// ─── Public API ───────────────────────────────────────────────────────────────

/// Bind `127.0.0.1:0` and return the listener together with its assigned port.
pub async fn bind_ephemeral_port() -> Result<(TcpListener, u16), AuthError> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| AuthError::CallbackServer(e.to_string()))?;
    let port = listener
        .local_addr()
        .map_err(|e| AuthError::CallbackServer(e.to_string()))?
        .port();
    Ok((listener, port))
}

/// Start a local HTTP server on `listener` and block until the OAuth2 callback arrives.
///
/// Validates the `state` query parameter against `expected_state` to prevent CSRF.
/// On success returns the authorization code. The server shuts down gracefully
/// after the first callback is processed.
pub async fn wait_for_callback(
    listener: TcpListener,
    expected_state: &str,
) -> Result<String, AuthError> {
    let (result_tx, result_rx) = oneshot::channel::<Result<String, AuthError>>();
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

    // Wrapped in Mutex<Option<...>> so the handler can take them exactly once
    // without requiring async locking (the guard is never held across an await).
    let result_cell: ResultCell = Arc::new(Mutex::new(Some(result_tx)));
    let shutdown_cell: Arc<Mutex<Option<oneshot::Sender<()>>>> =
        Arc::new(Mutex::new(Some(shutdown_tx)));

    let expected = expected_state.to_string();

    // Build a handler that captures the result + triggers shutdown, then
    // register it on both `/callback` (connector flows) and `/oauth/callback`
    // (MCP HTTP OAuth, per CIMD `redirect_uris`). Same semantics for both;
    // the path only differs because the MCP spec mandates the longer form.
    let result_cell_for_handler = result_cell.clone();
    let shutdown_cell_for_handler = shutdown_cell.clone();
    let handler = move |Query(params): Query<CallbackParams>| {
        let result_cell = result_cell_for_handler.clone();
        let shutdown_cell = shutdown_cell_for_handler.clone();
        let expected = expected.clone();
        async move {
            let result = resolve_callback(&params, &expected);
            let html = if result.is_ok() {
                SUCCESS_HTML
            } else {
                ERROR_HTML
            };

            if let Ok(mut guard) = result_cell.lock() {
                if let Some(tx) = guard.take() {
                    let _ = tx.send(result);
                }
            }
            if let Ok(mut guard) = shutdown_cell.lock() {
                if let Some(sd) = guard.take() {
                    let _ = sd.send(());
                }
            }

            Html(html)
        }
    };

    let app = Router::new()
        .route("/callback", get(handler.clone()))
        .route("/oauth/callback", get(handler));

    let server = axum::serve(listener, app).with_graceful_shutdown(async move {
        let _ = shutdown_rx.await;
    });

    tokio::spawn(async move {
        let _ = server.await;
    });

    result_rx
        .await
        .map_err(|_| AuthError::CallbackServer("server terminated before callback".into()))?
}

// ─── Internal helpers ─────────────────────────────────────────────────────────

/// Extract and validate the authorization code from the callback parameters.
fn resolve_callback(params: &CallbackParams, expected_state: &str) -> Result<String, AuthError> {
    if let Some(err) = &params.error {
        let text = match params.error_description.as_deref() {
            Some(desc) if !desc.is_empty() => {
                let desc: String = desc.chars().take(MAX_ERROR_DESCRIPTION_CHARS).collect();
                format!("{err}: {desc}")
            }
            _ => err.clone(),
        };
        // Only an answer to the consent this listener waits for may say an
        // administrator must approve: a page that forged the redirect without
        // the state would otherwise mark the connection blocked.
        let ours = params.state.as_deref() == Some(expected_state);
        if ours && crate::admin_approval::requires_admin_approval(&text) {
            return Err(AuthError::AdminApprovalRequired(text));
        }
        if ours && crate::admin_approval::declined_in_organisation(&text) {
            return Err(AuthError::ConsentDeclinedInOrganisation(text));
        }
        return Err(AuthError::ProviderError(text));
    }
    if params.state.as_deref() != Some(expected_state) {
        return Err(AuthError::StateMismatch);
    }
    params.code.clone().ok_or(AuthError::MissingCode)
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_callback_state_mismatch() {
        // GIVEN params with a state different from expected
        let params = CallbackParams {
            code: Some("mycode".into()),
            state: Some("xyz789".into()),
            error: None,
            error_description: None,
        };
        // WHEN resolved against "abc123"
        let result = resolve_callback(&params, "abc123");
        // THEN StateMismatch
        assert!(matches!(result, Err(AuthError::StateMismatch)));
    }

    #[test]
    fn test_resolve_callback_missing_code() {
        // GIVEN params with correct state but no code
        let params = CallbackParams {
            code: None,
            state: Some("abc123".into()),
            error: None,
            error_description: None,
        };
        // WHEN the callback is resolved against the expected state
        let result = resolve_callback(&params, "abc123");
        // THEN it fails with MissingCode
        assert!(matches!(result, Err(AuthError::MissingCode)));
    }

    #[test]
    fn test_resolve_callback_provider_error() {
        // GIVEN params with an error field
        let params = CallbackParams {
            code: None,
            state: Some("abc123".into()),
            error: Some("access_denied".into()),
            error_description: None,
        };
        // WHEN the callback is resolved against the expected state
        let result = resolve_callback(&params, "abc123");
        // THEN the provider error is surfaced as such, not as a missing code
        assert!(matches!(result, Err(AuthError::ProviderError(_))));
    }

    #[test]
    fn test_an_administrator_approval_on_the_redirect_is_named_with_its_description() {
        // GIVEN the redirect Microsoft sends when an administrator must approve
        let params = CallbackParams {
            code: None,
            state: Some("abc123".into()),
            error: Some("consent_required".into()),
            error_description: Some("AADSTS90094: The grant requires admin permission.".into()),
        };
        // WHEN the callback is resolved
        let result = resolve_callback(&params, "abc123");
        // THEN it is an approval answer, carrying the provider's words
        match result {
            Err(AuthError::AdminApprovalRequired(text)) => {
                assert!(text.starts_with("consent_required: AADSTS90094"), "{text}");
            }
            other => panic!("expected an approval answer, got {other:?}"),
        }
    }

    #[test]
    fn test_a_personal_account_that_declines_is_a_provider_error_not_an_approval() {
        // GIVEN an Outlook.com user who declined the consent page, answered
        // without an AADSTS code
        let params = CallbackParams {
            code: None,
            state: Some("abc123".into()),
            error: Some("access_denied".into()),
            error_description: Some(
                "The user has denied access to the scope requested by the client application."
                    .into(),
            ),
        };
        // WHEN the callback is resolved
        let result = resolve_callback(&params, "abc123");
        // THEN it stays a provider error the user can retry
        assert!(matches!(result, Err(AuthError::ProviderError(t)) if t.contains("denied access")));
    }

    #[test]
    fn test_a_work_account_that_declines_is_named_so_and_only_with_its_state() {
        // GIVEN a work account's refusal, once with the state of the consent and
        // once forged without it
        let refusal = |state: &str| CallbackParams {
            code: None,
            state: Some(state.into()),
            error: Some("access_denied".into()),
            error_description: Some("AADSTS65004: User declined to consent.".into()),
        };
        // WHEN each is resolved
        let ours = resolve_callback(&refusal("abc123"), "abc123");
        let forged = resolve_callback(&refusal("forged"), "abc123");
        // THEN ours may hide an approval, and the forged one is a plain error
        assert!(
            matches!(ours, Err(AuthError::ConsentDeclinedInOrganisation(t)) if t.contains("AADSTS65004"))
        );
        assert!(matches!(forged, Err(AuthError::ProviderError(_))));
    }

    #[test]
    fn test_an_approval_answer_without_the_state_is_not_believed() {
        // GIVEN an approval answer that does not carry the state of the consent
        let params = CallbackParams {
            code: None,
            state: Some("forged".into()),
            error: Some("consent_required".into()),
            error_description: Some("AADSTS90094: The grant requires admin permission.".into()),
        };
        // WHEN the callback is resolved
        let result = resolve_callback(&params, "abc123");
        // THEN it is a plain provider error, never an approval answer
        assert!(matches!(result, Err(AuthError::ProviderError(_))));
    }

    #[tokio::test]
    async fn test_the_callback_server_reads_the_error_description_of_the_redirect() {
        // GIVEN a callback server expecting state "abc123"
        let (listener, port) = bind_ephemeral_port().await.expect("bind should succeed");
        let server_task = tokio::spawn(wait_for_callback(listener, "abc123"));

        // WHEN the loopback receives an approval answer, as a browser would
        // bring it back from the provider; the listener is bound already, so
        // the request waits in its backlog
        let url = format!(
            "http://127.0.0.1:{port}/callback?state=abc123&error=consent_required\
             &error_description=AADSTS90094%3A+The+grant+requires+admin+permission."
        );
        let _ = reqwest::get(&url).await;

        // THEN the waiter answers that an administrator must approve
        let result = server_task.await.expect("task should not panic");
        assert!(matches!(result, Err(AuthError::AdminApprovalRequired(_))));
    }

    #[test]
    fn test_resolve_callback_success() {
        // GIVEN matching state and a valid code
        let params = CallbackParams {
            code: Some("auth_code_value".into()),
            state: Some("abc123".into()),
            error: None,
            error_description: None,
        };
        // WHEN the callback is resolved against the expected state
        let result = resolve_callback(&params, "abc123");
        // THEN the authorisation code comes back
        assert_eq!(result.unwrap(), "auth_code_value");
    }

    #[tokio::test]
    async fn test_oauth_callback_path_serves_same_handler() {
        // GIVEN a callback server expecting state "abc123"
        let (listener, port) = bind_ephemeral_port().await.expect("bind should succeed");
        let server_task = tokio::spawn(wait_for_callback(listener, "abc123"));

        tokio::time::sleep(std::time::Duration::from_millis(20)).await;

        // WHEN the MCP-style callback arrives at /oauth/callback (not /callback)
        let url = format!("http://127.0.0.1:{port}/oauth/callback?code=mycode&state=abc123");
        let _ = reqwest::get(&url).await;

        // THEN the code is captured exactly like on /callback
        let result = server_task.await.expect("task should not panic");
        assert_eq!(result.unwrap(), "mycode");
    }

    #[tokio::test]
    async fn test_callback_server_state_mismatch() {
        // GIVEN a callback server expecting state "abc123"
        let (listener, port) = bind_ephemeral_port().await.expect("bind should succeed");
        let server_task = tokio::spawn(wait_for_callback(listener, "abc123"));

        // Let the server accept connections before sending the request
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;

        // WHEN the callback arrives with a different state "xyz789"
        let url = format!("http://127.0.0.1:{port}/callback?code=mycode&state=xyz789");
        let _ = reqwest::get(&url).await;

        // THEN StateMismatch is returned
        let result = server_task.await.expect("task should not panic");
        assert!(matches!(result, Err(AuthError::StateMismatch)));
    }
}
