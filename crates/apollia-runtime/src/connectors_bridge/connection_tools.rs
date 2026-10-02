//! A connector tool per connected account.
//!
//! A native connector holds several accounts, and each account is a
//! connection of its own. Its tools are published once per account, named
//! `conn:<provider>:<account>/<operation>` (`conn:google:ada@example.com/gmail.send`),
//! the same shape as `mcp:<server>/<tool>`: an access granted to an agent
//! names one account, and a call runs with the token of that account and no
//! other. `<provider>:<account>` is the identifier of the connection without
//! its `native:` prefix.
//!
//! A provider with no connected account publishes no tool.

use apollia_auth::{AccountId, ConnectorProvider};
use apollia_tools::executor::ToolExecutor;
use apollia_tools::{ToolDescriptor, ToolRegistryError, ToolRegistryHandle};

use super::availability::connector_of;
use super::descriptors::{google_tool_descriptors, microsoft_tool_descriptors};
use crate::connections::NativeAccounts;

/// Prefix of the tool of a connected account.
pub const CONNECTION_TOOL_PREFIX: &str = "conn:";

/// The name of `operation` of `provider` for `account`:
/// `conn:<provider>:<account>/<operation>`.
#[must_use]
pub fn connection_tool_name(provider: ConnectorProvider, account: &str, operation: &str) -> String {
    format!(
        "{CONNECTION_TOOL_PREFIX}{}:{account}/{operation}",
        provider.id()
    )
}

/// A tool name of a connected account, read back into its parts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionToolName {
    /// The connector the tool belongs to.
    pub provider: ConnectorProvider,
    /// The account the tool acts as.
    pub account: AccountId,
    /// The connector operation (`gmail.send`).
    pub operation: String,
}

/// Read `name` as the tool of a connected account.
///
/// `None` unless `name` is `conn:<provider>:<account>/<operation>` with a
/// known provider, a non-empty account and an operation of that provider.
#[must_use]
pub fn parse_connection_tool(name: &str) -> Option<ConnectionToolName> {
    let rest = name.strip_prefix(CONNECTION_TOOL_PREFIX)?;
    let (connection, operation) = rest.rsplit_once('/')?;
    let (provider, account) = connection.split_once(':')?;
    let provider = ConnectorProvider::from_id(provider)?;
    if account.is_empty() || connector_of(operation) != Some(provider) {
        return None;
    }
    Some(ConnectionToolName {
        provider,
        account: AccountId::new(account),
        operation: operation.to_owned(),
    })
}

/// The descriptors of every operation of `provider`, published for `account`.
///
/// Each one is the operation's descriptor under the account's name, its
/// description opened by the account so the model tells two accounts apart.
#[must_use]
pub fn account_descriptors(provider: ConnectorProvider, account: &str) -> Vec<ToolDescriptor> {
    let operations = match provider {
        ConnectorProvider::Google => google_tool_descriptors(),
        ConnectorProvider::Microsoft => microsoft_tool_descriptors(),
    };
    operations
        .into_iter()
        .map(|mut d| {
            d.description = format!("Account {account}. {}", d.description);
            d.name = connection_tool_name(provider, account, &d.name);
            d
        })
        .collect()
}

/// The descriptors of every connected account in `accounts`.
#[must_use]
pub fn connector_descriptors_for(accounts: &[NativeAccounts]) -> Vec<ToolDescriptor> {
    accounts
        .iter()
        .flat_map(|(provider, names)| {
            names
                .iter()
                .flat_map(|account| account_descriptors(*provider, account))
        })
        .collect()
}

/// The connected accounts of every native provider.
///
/// A storage that cannot be opened or read answers no account for that
/// provider: a tool could not authenticate without it.
pub async fn connected_accounts() -> Vec<NativeAccounts> {
    let auth = match super::get_auth().await {
        Ok(auth) => auth,
        Err(error) => {
            tracing::event!(
                tracing::Level::WARN,
                error = %error,
                "connector.accounts.unreadable"
            );
            return Vec::new();
        }
    };
    let mut out = Vec::new();
    for provider in ConnectorProvider::ALL {
        match auth.list_accounts(provider).await {
            Ok(accounts) => out.push((provider, accounts.into_iter().map(|a| a.0).collect())),
            Err(error) => tracing::event!(
                tracing::Level::WARN,
                provider = %provider.id(),
                error = %error,
                "connector.accounts.unreadable"
            ),
        }
    }
    out
}

/// The accounts of `provider` in `accounts`.
fn accounts_of(accounts: &[NativeAccounts], provider: ConnectorProvider) -> Vec<AccountId> {
    accounts
        .iter()
        .filter(|(p, _)| *p == provider)
        .flat_map(|(_, names)| names.iter().map(AccountId::new))
        .collect()
}

/// Every connector executor of `accounts`: one per operation and per
/// connected account, plus the mailbox tools over IMAP.
#[must_use]
pub fn connector_executors_for(accounts: &[NativeAccounts]) -> Vec<Box<dyn ToolExecutor>> {
    let mut executors =
        super::build_google_executors(&accounts_of(accounts, ConnectorProvider::Google));
    executors.extend(super::build_microsoft_executors(&accounts_of(
        accounts,
        ConnectorProvider::Microsoft,
    )));
    executors.extend(super::build_imap_executors());
    executors
}

/// Every connector executor of the accounts connected now.
pub async fn build_connector_executors() -> Vec<Box<dyn ToolExecutor>> {
    connector_executors_for(&connected_accounts().await)
}

/// Resolve the bare connector names of `access`, the access list of `agent`.
///
/// An access to a connector operation without its account (`gmail.send`)
/// names no connection. It is rewritten to the tool of the provider's account
/// when that provider has exactly one; otherwise it is removed, and the
/// removal is logged with the access it named. Every other entry is kept, in
/// order, and an entry is never listed twice.
#[must_use]
pub fn resolve_bare_connector_access(
    agent: &str,
    access: Vec<String>,
    accounts: &[NativeAccounts],
) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(access.len());
    for entry in access {
        let resolved = match connector_of(&entry) {
            None => Some(entry),
            Some(provider) => match accounts_of(accounts, provider).as_slice() {
                [account] => Some(connection_tool_name(provider, account.as_str(), &entry)),
                others => {
                    tracing::event!(
                        tracing::Level::WARN,
                        agent = %agent,
                        access = %entry,
                        accounts = others.len(),
                        detail = "a connector tool names one account, and this access names none",
                        "agent.access.connector.removed"
                    );
                    None
                }
            },
        };
        if let Some(name) = resolved {
            if !out.contains(&name) {
                out.push(name);
            }
        }
    }
    out
}

/// Resolve the bare connector names of `access` against the accounts
/// connected now. See [`resolve_bare_connector_access`].
pub async fn resolve_connector_access(agent: &str, access: Vec<String>) -> Vec<String> {
    if !access.iter().any(|entry| connector_of(entry).is_some()) {
        return access;
    }
    resolve_bare_connector_access(agent, access, &connected_accounts().await)
}

/// Register in `registry` the tools of `account`, connected to `provider`.
///
/// A tool the registry already holds is kept: the account was connected
/// before.
///
/// # Errors
///
/// [`ToolRegistryError`] when the registry refuses a descriptor for another
/// reason, or has stopped.
pub async fn register_account_tools(
    registry: &ToolRegistryHandle,
    provider: ConnectorProvider,
    account: &str,
) -> Result<(), ToolRegistryError> {
    for descriptor in account_descriptors(provider, account) {
        match registry.register(descriptor).await {
            Ok(()) | Err(ToolRegistryError::AlreadyRegistered(_)) => {}
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
