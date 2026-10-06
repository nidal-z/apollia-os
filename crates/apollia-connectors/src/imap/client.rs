//! The IMAP client: implicit TLS, `LOGIN`, and read commands only.
//!
//! Every folder is opened with `EXAMINE`, which the protocol defines as
//! read-only: nothing changes in it, flags included. `SELECT` is never sent,
//! and neither is any command that writes (`STORE`, `EXPUNGE`, `COPY`,
//! `MOVE`, `APPEND`). A message is fetched with `BODY.PEEK`, which leaves it
//! unread, and a folder is counted with `STATUS`, which does not open it. Each
//! call opens its own session and logs out at the end.
//!
//! The server's certificate is checked against the root certificates of the
//! system. The password goes on the wire inside the TLS session and nowhere
//! else; the protocol library traces the commands it sends, `LOGIN` included,
//! through the `log` facade, which is why every binary of the product turns
//! the `async_imap` target off (see [`LOG_DIRECTIVE`]).

use std::sync::Arc;
use std::time::Duration;

use async_imap::types::NameAttribute;
use async_imap::{Client, Session};
use futures::future::BoxFuture;
use futures::TryStreamExt;
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;
use tokio_rustls::rustls::pki_types::ServerName;
use tokio_rustls::rustls::{ClientConfig, RootCertStore};
use tokio_rustls::TlsConnector;

use super::account::ImapAccount;
use super::error::ImapError;
use super::mail::{
    message_from_raw, summary_from_header, FolderCount, FolderUids, MailFolder, MailMessage,
    MailQuery, MailSummary,
};
use super::store::ConnectionProbe;

/// Filter directive every binary adds to its log filter, so that the
/// protocol library's trace of the commands it sends, `LOGIN` and its
/// password included, never reaches a log.
pub const LOG_DIRECTIVE: &str = "async_imap=off";

/// Longest wait for the server to accept the connection and finish the TLS
/// handshake.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

/// Longest a whole call (login, commands, logout) may take.
const CALL_TIMEOUT: Duration = Duration::from_secs(60);

type ImapSession = Session<TlsStream<TcpStream>>;

/// A client that reads mailboxes over IMAP.
#[derive(Clone)]
pub struct ImapClient {
    tls: Arc<ClientConfig>,
}

impl ImapClient {
    /// A client trusting the root certificates of this system.
    ///
    /// # Errors
    /// [`ImapError::Tls`] when the system holds no root certificate the TLS
    /// library accepts.
    pub fn with_system_roots() -> Result<Self, ImapError> {
        let loaded = rustls_native_certs::load_native_certs();
        let mut roots = RootCertStore::empty();
        let (_added, _ignored) = roots.add_parsable_certificates(loaded.certs);
        if roots.is_empty() {
            return Err(ImapError::Tls {
                host: String::new(),
                detail: "no root certificate could be read from this system".to_owned(),
            });
        }
        Self::with_roots(roots)
    }

    /// A client trusting `roots` only.
    ///
    /// # Errors
    /// [`ImapError::Tls`] when the TLS configuration cannot be built.
    pub fn with_roots(roots: RootCertStore) -> Result<Self, ImapError> {
        let provider = Arc::new(tokio_rustls::rustls::crypto::ring::default_provider());
        let config = ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .map_err(|e| ImapError::Tls {
                host: String::new(),
                detail: e.to_string(),
            })?
            .with_root_certificates(roots)
            .with_no_client_auth();
        Ok(Self {
            tls: Arc::new(config),
        })
    }

    /// Log in, open the inbox read-only, log out.
    ///
    /// # Errors
    /// [`ImapError::Unreachable`], [`ImapError::Tls`],
    /// [`ImapError::CredentialsRejected`] or [`ImapError::Protocol`].
    pub async fn check(&self, account: &ImapAccount, password: &str) -> Result<(), ImapError> {
        self.call(account, password, |session| {
            Box::pin(examine(session, super::mail::DEFAULT_FOLDER))
        })
        .await
    }

    /// Every folder of the mailbox.
    ///
    /// # Errors
    /// The connection failures of [`Self::check`].
    pub async fn list_folders(
        &self,
        account: &ImapAccount,
        password: &str,
    ) -> Result<Vec<MailFolder>, ImapError> {
        self.call(account, password, move |session| {
            Box::pin(async move {
                let names: Vec<_> = session
                    .list(None, Some("*"))
                    .await
                    .map_err(protocol)?
                    .try_collect()
                    .await
                    .map_err(protocol)?;
                Ok(names
                    .iter()
                    .map(|name| MailFolder {
                        name: name.name().to_owned(),
                        delimiter: name.delimiter().map(str::to_owned),
                        no_select: name
                            .attributes()
                            .iter()
                            .any(|a| matches!(a, NameAttribute::NoSelect)),
                    })
                    .collect())
            })
        })
        .await
    }

    /// The messages of a folder matching `query`, newest first.
    ///
    /// # Errors
    /// [`ImapError::InvalidQuery`], [`ImapError::FolderRefused`], and the
    /// connection failures of [`Self::check`].
    pub async fn search(
        &self,
        account: &ImapAccount,
        password: &str,
        query: &MailQuery,
    ) -> Result<Vec<MailSummary>, ImapError> {
        let criteria = query.criteria()?;
        let folder = query.folder().to_owned();
        let limit = query.limit();
        self.call(account, password, move |session| {
            Box::pin(async move {
                examine(session, &folder).await?;
                let mut uids: Vec<u32> = session
                    .uid_search(&criteria)
                    .await
                    .map_err(protocol)?
                    .into_iter()
                    .collect();
                uids.sort_unstable_by_key(|u| std::cmp::Reverse(*u));
                uids.truncate(limit);
                if uids.is_empty() {
                    return Ok(Vec::new());
                }
                let set = uid_set(&uids);
                let fetches: Vec<_> = session
                    .uid_fetch(&set, "(UID FLAGS BODY.PEEK[HEADER])")
                    .await
                    .map_err(protocol)?
                    .try_collect()
                    .await
                    .map_err(protocol)?;
                let mut summaries: Vec<MailSummary> = fetches
                    .iter()
                    .filter_map(|fetch| {
                        let uid = fetch.uid?;
                        let unread = !fetch
                            .flags()
                            .any(|f| matches!(f, async_imap::types::Flag::Seen));
                        Some(summary_from_header(uid, fetch.header()?, unread))
                    })
                    .collect();
                summaries.sort_unstable_by_key(|s| std::cmp::Reverse(s.uid));
                Ok(summaries)
            })
        })
        .await
    }

    /// The message `uid` of `folder`, decoded. It stays unread.
    ///
    /// # Errors
    /// [`ImapError::FolderRefused`], [`ImapError::MessageNotFound`], and the
    /// connection failures of [`Self::check`].
    pub async fn read(
        &self,
        account: &ImapAccount,
        password: &str,
        folder: &str,
        uid: u32,
    ) -> Result<MailMessage, ImapError> {
        let folder = folder.to_owned();
        self.call(account, password, move |session| {
            Box::pin(async move {
                examine(session, &folder).await?;
                let fetches: Vec<_> = session
                    .uid_fetch(uid.to_string(), "(UID BODY.PEEK[])")
                    .await
                    .map_err(protocol)?
                    .try_collect()
                    .await
                    .map_err(protocol)?;
                let raw = fetches
                    .iter()
                    .find(|f| f.uid == Some(uid))
                    .and_then(async_imap::types::Fetch::body)
                    .ok_or_else(|| ImapError::MessageNotFound {
                        folder: folder.clone(),
                        uid,
                    })?;
                message_from_raw(uid, &folder, raw)
            })
        })
        .await
    }

    /// How many messages `folder` holds, and how many are unread, as the
    /// server's `STATUS` answers it. The folder is not opened.
    ///
    /// # Errors
    /// [`ImapError::FolderRefused`], and the connection failures of
    /// [`Self::check`].
    pub async fn folder_count(
        &self,
        account: &ImapAccount,
        password: &str,
        folder: &str,
    ) -> Result<FolderCount, ImapError> {
        let folder = folder.to_owned();
        self.call(account, password, move |session| {
            Box::pin(async move {
                let status = session
                    .status(&folder, "(MESSAGES UNSEEN)")
                    .await
                    .map_err(|e| folder_failure(&folder, e))?;
                Ok(FolderCount {
                    messages: status.exists,
                    unread: status.unseen.unwrap_or(0),
                })
            })
        })
        .await
    }

    /// Every message of a folder matching `query`, newest first, whatever
    /// the limit of the query, with the validity of the folder's identifiers.
    ///
    /// # Errors
    /// [`ImapError::InvalidQuery`], [`ImapError::FolderRefused`],
    /// [`ImapError::Protocol`] when the server gives no `UIDVALIDITY`, and the
    /// connection failures of [`Self::check`].
    pub async fn matching_uids(
        &self,
        account: &ImapAccount,
        password: &str,
        query: &MailQuery,
    ) -> Result<FolderUids, ImapError> {
        let criteria = query.criteria()?;
        let folder = query.folder().to_owned();
        self.call(account, password, move |session| {
            Box::pin(async move {
                let uid_validity = examine_validity(session, &folder).await?;
                let mut uids: Vec<u32> = session
                    .uid_search(&criteria)
                    .await
                    .map_err(protocol)?
                    .into_iter()
                    .collect();
                uids.sort_unstable_by_key(|u| std::cmp::Reverse(*u));
                Ok(FolderUids { uid_validity, uids })
            })
        })
        .await
    }

    /// The messages `listed` names in `folder`, decoded, in the order listed.
    /// They stay unread. A message removed since its identifier was listed is
    /// left out.
    ///
    /// # Errors
    /// [`ImapError::Protocol`] when the folder's `UIDVALIDITY` is no longer
    /// the one the identifiers were listed under, which means they name other
    /// messages now; [`ImapError::FolderRefused`], and the connection failures
    /// of [`Self::check`].
    pub async fn read_many(
        &self,
        account: &ImapAccount,
        password: &str,
        folder: &str,
        listed: &FolderUids,
    ) -> Result<Vec<MailMessage>, ImapError> {
        if listed.uids.is_empty() {
            return Ok(Vec::new());
        }
        let folder = folder.to_owned();
        let uid_validity = listed.uid_validity;
        let uids = listed.uids.clone();
        self.call(account, password, move |session| {
            Box::pin(async move {
                let current = examine_validity(session, &folder).await?;
                if current != uid_validity {
                    return Err(ImapError::Protocol {
                        detail: format!(
                            "the identifiers of `{folder}` changed while it was read \
                             (UIDVALIDITY {uid_validity} became {current})"
                        ),
                    });
                }
                let fetches: Vec<_> = session
                    .uid_fetch(uid_set(&uids), "(UID BODY.PEEK[])")
                    .await
                    .map_err(protocol)?
                    .try_collect()
                    .await
                    .map_err(protocol)?;
                let mut messages = Vec::with_capacity(uids.len());
                for uid in &uids {
                    let raw = fetches
                        .iter()
                        .find(|f| f.uid == Some(*uid))
                        .and_then(async_imap::types::Fetch::body);
                    if let Some(raw) = raw {
                        messages.push(message_from_raw(*uid, &folder, raw)?);
                    }
                }
                Ok(messages)
            })
        })
        .await
    }

    /// Open a session, run `work` in it, log out, all within
    /// [`CALL_TIMEOUT`].
    async fn call<T, F>(
        &self,
        account: &ImapAccount,
        password: &str,
        work: F,
    ) -> Result<T, ImapError>
    where
        F: for<'s> FnOnce(&'s mut ImapSession) -> BoxFuture<'s, Result<T, ImapError>>,
    {
        let run = async {
            let mut session = self.open(account, password).await?;
            let result = work(&mut session).await;
            if let Err(e) = session.logout().await {
                tracing::event!(
                    tracing::Level::DEBUG,
                    host = %account.host,
                    error = %e,
                    "imap.logout_failed"
                );
            }
            result
        };
        tokio::time::timeout(CALL_TIMEOUT, run)
            .await
            .map_err(|_| ImapError::Unreachable {
                host: account.host.clone(),
                port: account.port,
                detail: format!("no answer within {}s", CALL_TIMEOUT.as_secs()),
            })?
    }

    /// Connect, finish the TLS handshake, read the greeting and log in.
    async fn open(&self, account: &ImapAccount, password: &str) -> Result<ImapSession, ImapError> {
        let unreachable = |detail: String| ImapError::Unreachable {
            host: account.host.clone(),
            port: account.port,
            detail,
        };
        let tls_failure = |detail: String| ImapError::Tls {
            host: account.host.clone(),
            detail,
        };
        let name =
            ServerName::try_from(account.host.clone()).map_err(|_| ImapError::InvalidHost {
                host: account.host.clone(),
            })?;
        let tcp = tokio::time::timeout(
            CONNECT_TIMEOUT,
            TcpStream::connect((account.host.as_str(), account.port)),
        )
        .await
        .map_err(|_| unreachable(format!("no answer within {}s", CONNECT_TIMEOUT.as_secs())))?
        .map_err(|e| unreachable(e.to_string()))?;
        let tls = tokio::time::timeout(
            CONNECT_TIMEOUT,
            TlsConnector::from(self.tls.clone()).connect(name, tcp),
        )
        .await
        .map_err(|_| tls_failure("the handshake did not finish".to_owned()))?
        .map_err(|e| tls_failure(e.to_string()))?;
        let mut client = Client::new(tls);
        match client.read_response().await {
            Ok(Some(_greeting)) => {}
            Ok(None) => {
                return Err(ImapError::Protocol {
                    detail: "the server closed the connection before greeting".to_owned(),
                })
            }
            Err(e) => return Err(unreachable(e.to_string())),
        }
        client
            .login(&account.username, password)
            .await
            .map_err(|(e, _client)| match e {
                async_imap::error::Error::No(_) | async_imap::error::Error::Bad(_) => {
                    ImapError::CredentialsRejected
                }
                async_imap::error::Error::Io(e) => unreachable(e.to_string()),
                other => protocol(other),
            })
    }
}

impl ConnectionProbe for ImapClient {
    async fn verify(&self, account: &ImapAccount, password: &str) -> Result<(), ImapError> {
        self.check(account, password).await
    }
}

/// Open `folder` read-only.
async fn examine(session: &mut ImapSession, folder: &str) -> Result<(), ImapError> {
    session
        .examine(folder)
        .await
        .map(|_mailbox| ())
        .map_err(|e| folder_failure(folder, e))
}

/// Open `folder` read-only and answer the validity of its identifiers.
async fn examine_validity(session: &mut ImapSession, folder: &str) -> Result<u32, ImapError> {
    session
        .examine(folder)
        .await
        .map_err(|e| folder_failure(folder, e))?
        .uid_validity
        .ok_or_else(|| ImapError::Protocol {
            detail: format!("the server gave no UIDVALIDITY for `{folder}`"),
        })
}

/// A refusal of `folder` by the server, or a protocol failure.
fn folder_failure(folder: &str, err: async_imap::error::Error) -> ImapError {
    match err {
        async_imap::error::Error::No(detail) | async_imap::error::Error::Bad(detail) => {
            ImapError::FolderRefused {
                folder: folder.to_owned(),
                detail,
            }
        }
        other => protocol(other),
    }
}

fn protocol(err: async_imap::error::Error) -> ImapError {
    ImapError::Protocol {
        detail: err.to_string(),
    }
}

/// `uids` as an IMAP sequence set: `7,5,3`.
fn uid_set(uids: &[u32]) -> String {
    uids.iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(",")
}
