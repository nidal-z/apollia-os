//! What describes a mailbox connection, the password aside.

use serde::{Deserialize, Serialize};

use super::error::ImapError;
use super::providers::{email_domain, provider_for_email, DEFAULT_IMAP_PORT};

/// A mailbox reached over IMAP. Holds no secret: the password lives in the
/// secret store, under the address.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImapAccount {
    /// Address of the mailbox; identifies the connection.
    pub email: String,
    /// IMAP server.
    pub host: String,
    /// IMAP port, implicit TLS.
    pub port: u16,
    /// Identifier sent at login.
    pub username: String,
}

/// What an operator gives to describe a mailbox; only the address is
/// required.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImapAccountInput {
    /// Address of the mailbox.
    pub email: String,
    /// IMAP server; prefilled from the address for a known provider.
    #[serde(default)]
    pub host: Option<String>,
    /// IMAP port; the provider's, else 993.
    #[serde(default)]
    pub port: Option<u16>,
    /// Identifier sent at login; the address when absent.
    #[serde(default)]
    pub username: Option<String>,
}

impl ImapAccount {
    /// Describe a mailbox from what the operator gave, filling the server,
    /// the port and the identifier the operator left out.
    ///
    /// # Errors
    /// [`ImapError::InvalidEmail`] when the address is not one,
    /// [`ImapError::UnknownProvider`] when no server is given and the domain
    /// is not a known provider, [`ImapError::InvalidHost`] and
    /// [`ImapError::InvalidPort`] for an unusable server.
    pub fn from_input(input: &ImapAccountInput) -> Result<Self, ImapError> {
        let email = input.email.trim().to_owned();
        let domain = email_domain(&email).ok_or_else(|| ImapError::InvalidEmail {
            email: email.clone(),
        })?;
        let provider = provider_for_email(&email);
        let host = match non_blank(input.host.as_deref()) {
            Some(host) => host.to_owned(),
            None => provider
                .map(|p| p.host.to_owned())
                .ok_or(ImapError::UnknownProvider { domain })?,
        };
        if host
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || c == '/')
        {
            return Err(ImapError::InvalidHost { host });
        }
        let port = input
            .port
            .or(provider.map(|p| p.port))
            .unwrap_or(DEFAULT_IMAP_PORT);
        if port == 0 {
            return Err(ImapError::InvalidPort);
        }
        let username =
            non_blank(input.username.as_deref()).map_or_else(|| email.clone(), str::to_owned);
        Ok(Self {
            email,
            host,
            port,
            username,
        })
    }
}

fn non_blank(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|v| !v.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(email: &str) -> ImapAccountInput {
        ImapAccountInput {
            email: email.to_owned(),
            ..ImapAccountInput::default()
        }
    }

    #[test]
    fn test_known_provider_address_alone_is_enough() {
        // GIVEN only a Gmail address
        let given = input("ana@gmail.com");
        // WHEN the account is described
        let account = ImapAccount::from_input(&given).unwrap();
        // THEN server, port and identifier are filled in
        assert_eq!(account.host, "imap.gmail.com");
        assert_eq!(account.port, 993);
        assert_eq!(account.username, "ana@gmail.com");
    }

    #[test]
    fn test_given_server_wins_over_the_prefill() {
        // GIVEN a Gmail address with an explicit server, port and identifier
        let given = ImapAccountInput {
            email: "ana@gmail.com".into(),
            host: Some("mail.example.org".into()),
            port: Some(1993),
            username: Some("ana".into()),
        };
        // WHEN the account is described
        let account = ImapAccount::from_input(&given).unwrap();
        // THEN what the operator gave is kept
        assert_eq!(account.host, "mail.example.org");
        assert_eq!(account.port, 1993);
        assert_eq!(account.username, "ana");
    }

    #[test]
    fn test_unknown_domain_without_server_is_refused() {
        // GIVEN an address on an unlisted domain and no server
        let given = input("ana@example.org");
        // WHEN the account is described
        let err = ImapAccount::from_input(&given).unwrap_err();
        // THEN the operator is asked for the server
        assert_eq!(err.code(), "unknown_provider");
    }

    #[test]
    fn test_unknown_domain_with_server_defaults_to_993() {
        // GIVEN an address on an unlisted domain with a server and no port
        let given = ImapAccountInput {
            email: "ana@example.org".into(),
            host: Some("imap.example.org".into()),
            ..ImapAccountInput::default()
        };
        // WHEN the account is described
        let account = ImapAccount::from_input(&given).unwrap();
        // THEN the implicit TLS port is used
        assert_eq!(account.port, 993);
    }

    #[test]
    fn test_unusable_values_are_refused_with_their_code() {
        // GIVEN an address that is not one, a host with a space, and port 0
        let bad_email = input("ana");
        let bad_host = ImapAccountInput {
            host: Some("imap example.org".into()),
            ..input("ana@example.org")
        };
        let bad_port = ImapAccountInput {
            port: Some(0),
            ..input("ana@gmail.com")
        };
        // WHEN each is described
        // THEN each is refused with its own code
        let code = |i: &ImapAccountInput| ImapAccount::from_input(i).unwrap_err().code();
        assert_eq!(code(&bad_email), "invalid_email");
        assert_eq!(code(&bad_host), "invalid_host");
        assert_eq!(code(&bad_port), "invalid_port");
    }
}
