//! The mailbox connections of this machine: a list without secrets, and the
//! passwords in the secret store.
//!
//! The list is a JSON file in the data directory, `imap-accounts.json`, read
//! and rewritten whole. It holds what [`ImapAccount`] holds and nothing else.
//! The password goes to the configured [`SecretStore`] under the service
//! [`IMAP_SECRET_SERVICE`] and the address, never to the list and never to a
//! log line.
//!
//! A connection enters the list only after it has been verified: see
//! [`ImapAccountStore::add_verified`].

use std::future::Future;
use std::path::{Path, PathBuf};

use apollia_auth::{select_secret_store, SecretStore};
use serde::{Deserialize, Serialize};

use super::account::ImapAccount;
use super::error::ImapError;

/// Secret store service under which mailbox passwords are kept; the user slot
/// is the address.
pub const IMAP_SECRET_SERVICE: &str = "apollia-imap";

/// Whether `a` and `b` name the same mailbox: an address is found whatever
/// the case it is written in, so that every reader of a mailbox (the store,
/// the executor, the decision on a call) matches it the same way.
#[must_use]
pub fn same_address(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

/// File name of the connection list in the data directory.
pub const IMAP_INDEX_FILE: &str = "imap-accounts.json";

/// Proves that a mailbox accepts an identifier and password before anything
/// is stored.
pub trait ConnectionProbe: Send + Sync {
    /// Open the mailbox and close it again.
    ///
    /// # Errors
    /// The typed failure: [`ImapError::Unreachable`], [`ImapError::Tls`],
    /// [`ImapError::CredentialsRejected`] or [`ImapError::Protocol`].
    fn verify(
        &self,
        account: &ImapAccount,
        password: &str,
    ) -> impl Future<Output = Result<(), ImapError>> + Send;
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Index {
    #[serde(default)]
    accounts: Vec<ImapAccount>,
}

/// A mailbox connection removed from the store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemovedMailbox {
    /// The connection removed from the list.
    pub account: ImapAccount,
    /// Whether its password was deleted from the secret store.
    pub password_deleted: bool,
}

/// The mailbox connections of this machine.
pub struct ImapAccountStore {
    index_path: PathBuf,
    secrets: Box<dyn SecretStore>,
}

impl ImapAccountStore {
    /// The store of the data directory `data_dir`, with the secret store
    /// selected by `APOLLIA_TOKEN_STORAGE`.
    ///
    /// # Errors
    /// [`ImapError::Secret`] when the configured secret store cannot be built.
    pub fn open(data_dir: &Path) -> Result<Self, ImapError> {
        Ok(Self::with_store(
            data_dir.join(IMAP_INDEX_FILE),
            select_secret_store().map_err(ImapError::Secret)?,
        ))
    }

    /// A store over an explicit list file and secret store.
    #[must_use]
    pub fn with_store(index_path: PathBuf, secrets: Box<dyn SecretStore>) -> Self {
        Self {
            index_path,
            secrets,
        }
    }

    /// Every connection stored in the data directory `data_dir`, read without
    /// opening the secret store.
    ///
    /// # Errors
    /// [`ImapError::Index`] when the list exists and cannot be read.
    pub fn accounts_in(data_dir: &Path) -> Result<Vec<ImapAccount>, ImapError> {
        Ok(read_index_at(&data_dir.join(IMAP_INDEX_FILE))?.accounts)
    }

    /// Every stored connection, in the order they were added.
    ///
    /// # Errors
    /// [`ImapError::Index`] when the list exists and cannot be read.
    pub fn list(&self) -> Result<Vec<ImapAccount>, ImapError> {
        Ok(self.read_index()?.accounts)
    }

    /// The stored connection for `email`.
    ///
    /// # Errors
    /// [`ImapError::NotFound`] when there is none.
    pub fn get(&self, email: &str) -> Result<ImapAccount, ImapError> {
        self.list()?
            .into_iter()
            .find(|a| same_address(&a.email, email))
            .ok_or_else(|| ImapError::NotFound {
                email: email.to_owned(),
            })
    }

    /// The password of the stored connection for `email`.
    ///
    /// # Errors
    /// [`ImapError::NotFound`] when no password is stored for it,
    /// [`ImapError::Secret`] when the secret store fails.
    pub fn password(&self, email: &str) -> Result<String, ImapError> {
        self.secrets
            .get(IMAP_SECRET_SERVICE, email)
            .map_err(ImapError::Secret)?
            .ok_or_else(|| ImapError::NotFound {
                email: email.to_owned(),
            })
    }

    /// Verify the connection with `probe`, then store it.
    ///
    /// Nothing is written when the verification fails. When it succeeds, the
    /// password goes to the secret store first and the list second; a list
    /// that cannot be written takes the password back out, so a failure
    /// leaves no half connection behind. Adding an address already stored
    /// replaces it.
    ///
    /// # Errors
    /// The probe's typed failure, or [`ImapError::Secret`] /
    /// [`ImapError::Index`] when storing fails.
    pub async fn add_verified<P: ConnectionProbe>(
        &self,
        account: ImapAccount,
        password: &str,
        probe: &P,
    ) -> Result<ImapAccount, ImapError> {
        if let Err(err) = probe.verify(&account, password).await {
            tracing::event!(
                tracing::Level::INFO,
                host = %account.host,
                port = account.port,
                code = err.code(),
                "imap.account.verify_failed"
            );
            return Err(err);
        }
        let mut index = self.read_index()?;
        self.secrets
            .set(IMAP_SECRET_SERVICE, &account.email, password)
            .map_err(ImapError::Secret)?;
        index
            .accounts
            .retain(|a| !same_address(&a.email, &account.email));
        index.accounts.push(account.clone());
        if let Err(err) = self.write_index(&index) {
            if let Err(rollback) = self.secrets.delete(IMAP_SECRET_SERVICE, &account.email) {
                tracing::event!(
                    tracing::Level::WARN,
                    error = %rollback,
                    "imap.account.secret_rollback_failed"
                );
            }
            return Err(err);
        }
        tracing::event!(
            tracing::Level::INFO,
            host = %account.host,
            port = account.port,
            backend = self.secrets.backend_id(),
            "imap.account.added"
        );
        Ok(account)
    }

    /// Remove the stored connection for `email`: out of the list first, then
    /// its password out of the secret store. Answers the connection removed,
    /// and whether its password was deleted.
    ///
    /// A password that cannot be deleted is not an error: the mailbox is gone
    /// from the list, and nothing reads the password of a mailbox the list no
    /// longer holds. It is logged, and answered so the caller can say it was
    /// left behind.
    ///
    /// # Errors
    /// [`ImapError::NotFound`] when there is none, [`ImapError::Index`] when
    /// the list cannot be read or written.
    pub fn remove(&self, email: &str) -> Result<RemovedMailbox, ImapError> {
        let mut index = self.read_index()?;
        let position = index
            .accounts
            .iter()
            .position(|a| same_address(&a.email, email))
            .ok_or_else(|| ImapError::NotFound {
                email: email.to_owned(),
            })?;
        let account = index.accounts.remove(position);
        self.write_index(&index)?;
        let password_deleted = match self.secrets.delete(IMAP_SECRET_SERVICE, &account.email) {
            Ok(()) => true,
            Err(e) => {
                tracing::event!(
                    tracing::Level::WARN,
                    error = %e,
                    "imap.account.secret_delete_failed"
                );
                false
            }
        };
        tracing::event!(
            tracing::Level::INFO,
            host = %account.host,
            "imap.account.removed"
        );
        Ok(RemovedMailbox {
            account,
            password_deleted,
        })
    }

    fn read_index(&self) -> Result<Index, ImapError> {
        read_index_at(&self.index_path)
    }

    fn write_index(&self, index: &Index) -> Result<(), ImapError> {
        if let Some(parent) = self.index_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| index_error("written", e))?;
        }
        let bytes = serde_json::to_vec_pretty(index).map_err(|e| index_error("written", e))?;
        let tmp = self.index_path.with_extension("json.tmp");
        std::fs::write(&tmp, bytes).map_err(|e| index_error("written", e))?;
        std::fs::rename(&tmp, &self.index_path).map_err(|e| index_error("written", e))
    }
}

fn read_index_at(path: &Path) -> Result<Index, ImapError> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Index::default()),
        Err(e) => return Err(index_error("read", e)),
    };
    serde_json::from_slice(&bytes).map_err(|e| index_error("read", e))
}

fn index_error(action: &'static str, err: impl std::fmt::Display) -> ImapError {
    ImapError::Index {
        action,
        detail: err.to_string(),
    }
}
