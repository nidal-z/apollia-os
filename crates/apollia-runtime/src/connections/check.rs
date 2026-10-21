//! Check one connection, in the way its kind can be checked, and keep what the
//! check found.
//!
//! - A native account obtains a valid access token: a cached token still
//!   valid answers without the network; otherwise the token is refreshed at
//!   its provider's token endpoint, and nowhere else. A refused refresh asks
//!   to sign in again.
//! - A mailbox logs in and opens its inbox read-only, on its own server. A
//!   refused login reads as a refused password.
//! - A folder is looked up on this machine, without the network. A folder
//!   that is no longer there reads as not found.
//! - An MCP server is checked through its live session, started first when it
//!   has none: a fresh handshake that lists its tools, then the read-only
//!   probe it declares, when it declares one (see the route).
//!
//! Every check answers a [`ConnectionStateRecord`] dated now; [`record`] keeps
//! it in `connection_states` and announces it as
//! [`RuntimeEvent::ConnectionStateChanged`].

use std::path::Path;

use apollia_auth::{
    AccountId, AuthError, AuthManager, ConnectorProvider, ProviderConfig, TokenRefresh,
};
use apollia_connectors::imap::{ConnectionProbe, ImapAccount, ImapError};
use apollia_core::{
    ConnectionReason, ConnectionReasonKey, ConnectionStateError, ConnectionStateRecord,
    ConnectionStateStore, ConnectionStatus, EventBusSender, McpHealth, RuntimeEvent,
};
use apollia_mcp::manager::McpConnectionTestResult;
use apollia_mcp::session::McpSessionError;

/// Longest technical detail kept with a state, in characters.
const MAX_DETAIL_CHARS: usize = 300;

/// The state `id` was found in now: `reason` absent means working.
fn found(id: &str, reason: Option<(ConnectionReasonKey, String)>) -> ConnectionStateRecord {
    let (state, reason) = match reason {
        None => (ConnectionStatus::Working, None),
        Some((key, detail)) => (
            ConnectionStatus::NeedsRepair,
            Some(ConnectionReason::new(
                key,
                Some(detail.chars().take(MAX_DETAIL_CHARS).collect()),
            )),
        ),
    };
    ConnectionStateRecord {
        id: id.to_owned(),
        state,
        reason,
        checked_at: chrono::Utc::now().to_rfc3339(),
    }
}

/// What a failure to obtain a token says about the account.
#[must_use]
pub fn native_reason(error: &AuthError) -> ConnectionReasonKey {
    match error {
        AuthError::HttpError(_) | AuthError::ResponseTooLarge(_) => {
            ConnectionReasonKey::Unreachable
        }
        AuthError::Keyring(_) | AuthError::Serialization(_) => ConnectionReasonKey::Failing,
        _ => ConnectionReasonKey::SignInAgain,
    }
}

/// The provider configuration a check of a `provider` account refreshes
/// against: no scope is asked, a refresh keeps the ones granted.
#[must_use]
pub fn provider_config(provider: ConnectorProvider) -> ProviderConfig {
    match provider {
        ConnectorProvider::Google => apollia_auth::build_google_provider(&[]),
        ConnectorProvider::Microsoft => apollia_auth::build_microsoft_provider(&[]),
    }
}

/// Check the native account `account` of `provider`: obtain a valid token,
/// refreshed through `refresh` at the provider's token endpoint
/// ([`provider_config`]) when it expired.
pub async fn check_native<R: TokenRefresh>(
    id: &str,
    auth: &AuthManager,
    provider: ConnectorProvider,
    account: &str,
    refresh: &R,
) -> ConnectionStateRecord {
    let account = AccountId::new(account);
    let config = provider_config(provider);
    match auth
        .get_valid_token_with(provider, &account, &config, refresh)
        .await
    {
        Ok(_) => found(id, None),
        Err(e) => found(id, Some((native_reason(&e), e.to_string()))),
    }
}

/// What a failed mailbox check says about the mailbox.
#[must_use]
pub fn imap_reason(error: &ImapError) -> ConnectionReasonKey {
    match error {
        ImapError::CredentialsRejected => ConnectionReasonKey::PasswordRefused,
        ImapError::Unreachable { .. } | ImapError::Tls { .. } => ConnectionReasonKey::Unreachable,
        _ => ConnectionReasonKey::Failing,
    }
}

/// Check the mailbox `account` with `probe`: log in, open the inbox
/// read-only. `password` is the one kept for it, or the error that kept it
/// from being read, which asks for the password again.
pub async fn check_imap<P: ConnectionProbe>(
    id: &str,
    account: &ImapAccount,
    password: Result<String, ImapError>,
    probe: &P,
) -> ConnectionStateRecord {
    let password = match password {
        Ok(password) => password,
        Err(e) => {
            return found(
                id,
                Some((ConnectionReasonKey::PasswordRefused, e.to_string())),
            )
        }
    };
    match probe.verify(account, &password).await {
        Ok(()) => found(id, None),
        Err(e) => found(id, Some((imap_reason(&e), e.to_string()))),
    }
}

/// Check the folder at `path`: it is still a folder of this machine.
#[must_use]
pub fn check_folder(id: &str, path: &Path) -> ConnectionStateRecord {
    if path.is_dir() {
        found(id, None)
    } else {
        found(
            id,
            Some((
                ConnectionReasonKey::FolderNotFound,
                format!("{} is not a folder of this machine", path.display()),
            )),
        )
    }
}

/// What starting the session of an MCP server that had none says, when it
/// failed.
#[must_use]
pub fn mcp_start_failed(id: &str, error: &McpSessionError) -> ConnectionStateRecord {
    found(id, Some((mcp_error_reason(error), error.to_string())))
}

fn mcp_error_reason(error: &McpSessionError) -> ConnectionReasonKey {
    match error {
        McpSessionError::Unauthorized { .. } => ConnectionReasonKey::SignInAgain,
        _ => ConnectionReasonKey::Unreachable,
    }
}

/// What the test of a live MCP session says.
///
/// A test that answered proves the server: its fresh handshake listed its
/// tools. The health the session is left in then says whether its operations
/// fail, the declared probe included.
#[must_use]
pub fn mcp_tested(
    id: &str,
    test: &Result<McpConnectionTestResult, McpSessionError>,
) -> ConnectionStateRecord {
    let result = match test {
        Ok(result) => result,
        Err(e) => return mcp_start_failed(id, e),
    };
    let reason = match &result.live_health {
        None | Some(McpHealth::Healthy { .. }) => None,
        Some(McpHealth::Degraded { last_error, .. }) => {
            Some((ConnectionReasonKey::Failing, last_error.clone()))
        }
        Some(McpHealth::NeedsReauth { reason }) => {
            Some((ConnectionReasonKey::SignInAgain, reason.clone()))
        }
        Some(McpHealth::Unavailable { reason }) => {
            Some((ConnectionReasonKey::Unreachable, reason.clone()))
        }
    };
    found(id, reason)
}

/// The state of a connection the organisation policy forbids: a check does
/// not reach it.
#[must_use]
pub fn blocked(id: &str) -> ConnectionStateRecord {
    ConnectionStateRecord {
        id: id.to_owned(),
        state: ConnectionStatus::Blocked,
        reason: Some(ConnectionReason::new(
            ConnectionReasonKey::ForbiddenByPolicy,
            None,
        )),
        checked_at: chrono::Utc::now().to_rfc3339(),
    }
}

/// Keep `record` in `connection_states` of the `system.db` under `data_dir`,
/// then announce it on `bus`.
///
/// # Errors
/// [`ConnectionStateError`] when the store cannot be opened or written;
/// nothing is announced then.
pub fn record(
    data_dir: &Path,
    bus: &EventBusSender,
    record: &ConnectionStateRecord,
) -> Result<(), ConnectionStateError> {
    ConnectionStateStore::open(&apollia_core::paths::DataFile::System.path(data_dir))?
        .put(record)?;
    let _ = bus.send(RuntimeEvent::ConnectionStateChanged {
        id: record.id.clone(),
        state: record.state,
        reason: record.reason.clone(),
        checked_at: record.checked_at.clone(),
    });
    Ok(())
}

#[cfg(test)]
mod tests;
