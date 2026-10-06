//! A mailbox as a connection: any provider reached over IMAP with an
//! application password, Gmail included.
//!
//! - [`providers`]: the providers whose server is known from the address.
//! - [`ImapAccount`]: what describes a connection, the password aside.
//! - [`ImapAccountStore`]: the connections of this machine, the passwords in
//!   the secret store, and the rule that a connection is stored only once it
//!   has been verified.
//! - [`ImapClient`]: the verification and the read commands, over TLS.
//! - [`mail`]: what a search asks and what a read answers.

mod account;
mod client;
mod error;
pub mod mail;
pub mod providers;
mod store;

pub use account::{ImapAccount, ImapAccountInput};
pub use client::{ImapClient, LOG_DIRECTIVE};
pub use error::ImapError;
pub use mail::{
    FolderCount, FolderUids, MailFolder, MailMessage, MailQuery, MailSummary, DEFAULT_FOLDER,
};
pub use store::{ConnectionProbe, ImapAccountStore, IMAP_INDEX_FILE, IMAP_SECRET_SERVICE};

#[cfg(test)]
mod client_tests;
#[cfg(test)]
pub(crate) mod fake_server;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tls_test_material;
