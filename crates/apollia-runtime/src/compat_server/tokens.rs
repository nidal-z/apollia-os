//! The actor that owns the tokens of the compatible server.
//!
//! A token is drawn by the generator of the internal token, prefixed `apo_`,
//! handed back once to whoever created it, and forgotten: the actor gives the
//! store its SHA-256 and its last four characters, never the value. To check a
//! token, the actor hashes what the client presented and compares it with the
//! hash of every token in use, in constant time; a match stamps the token's
//! last use.
//!
//! The internal token of the desktop (`<data_dir>/api-token`) is not one of
//! these, so it never opens the compatible server.

use std::path::PathBuf;

use apollia_core::server_token::{
    ServerTokenError, ServerTokenRepository, ServerTokenRow, TokenSelector,
};
use sha2::{Digest, Sha256};
use tokio::sync::{mpsc, oneshot};

use crate::api::middleware::generate_hex_token;

/// Prefix of every token of the compatible server, so a person can tell one
/// from any other key at a glance.
pub const SERVER_TOKEN_PREFIX: &str = "apo_";

/// Capacity of the actor's command channel. Every request to a server that
/// requires a token checks one, so this absorbs a burst of client calls.
const COMMAND_CAPACITY: usize = 256;

/// Why the token store could not answer.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ServerTokensError {
    /// The store refused the operation.
    #[error(transparent)]
    Store(#[from] ServerTokenError),
    /// `system.db` could not be opened when the runtime started.
    #[error("the token store is unavailable: {reason}")]
    Unavailable {
        /// Why it could not be opened.
        reason: String,
    },
    /// The actor has stopped.
    #[error("the token store has stopped with the runtime")]
    ActorGone,
}

/// A token just created: its row and its value, which nothing keeps.
///
/// `Debug` leaves the value out, so the token never reaches a log by mistake.
pub struct CreatedServerToken {
    /// The row the store keeps.
    pub row: ServerTokenRow,
    /// The value, shown once.
    pub value: String,
}

impl std::fmt::Debug for CreatedServerToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CreatedServerToken")
            .field("row", &self.row)
            .field("value", &"<redacted>")
            .finish()
    }
}

type Reply<T> = oneshot::Sender<Result<T, ServerTokensError>>;

enum Command {
    Create(String, Reply<CreatedServerToken>),
    List(Reply<Vec<ServerTokenRow>>),
    Revoke(TokenSelector, Reply<ServerTokenRow>),
    ActiveCount(Reply<u64>),
    Verify(String, oneshot::Sender<bool>),
}

/// Handle to the token actor.
#[derive(Debug, Clone)]
pub struct ServerTokensHandle {
    tx: mpsc::Sender<Command>,
}

/// The hex SHA-256 of a token value.
fn hash_hex(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}

impl ServerTokensHandle {
    /// Spawns the actor over the `system.db` at `system_db`.
    ///
    /// A file that cannot be opened is logged; the actor then refuses every
    /// token and answers [`ServerTokensError::Unavailable`] to the routes. It
    /// stops when every handle is dropped.
    pub fn spawn(system_db: PathBuf) -> Self {
        let (tx, rx) = mpsc::channel(COMMAND_CAPACITY);
        let store = ServerTokenRepository::open(&system_db).map_err(|error| {
            tracing::event!(
                tracing::Level::WARN,
                path = %system_db.display(),
                error = %error,
                "compat_server.tokens.open_failed"
            );
            error.to_string()
        });
        tokio::spawn(run(store, rx));
        Self { tx }
    }

    async fn request<T>(
        &self,
        command: impl FnOnce(Reply<T>) -> Command,
    ) -> Result<T, ServerTokensError> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(command(reply))
            .await
            .map_err(|_| ServerTokensError::ActorGone)?;
        rx.await.map_err(|_| ServerTokensError::ActorGone)?
    }

    /// Creates a token named `name` and returns its value, the only time it
    /// is ever given.
    ///
    /// # Errors
    ///
    /// [`ServerTokensError::Store`] for a refused name or a database failure,
    /// [`ServerTokensError::Unavailable`] or [`ServerTokensError::ActorGone`]
    /// when the store cannot answer.
    pub async fn create(&self, name: String) -> Result<CreatedServerToken, ServerTokensError> {
        self.request(|reply| Command::Create(name, reply)).await
    }

    /// Every token, in use and revoked, without any value.
    ///
    /// # Errors
    ///
    /// As [`Self::create`].
    pub async fn list(&self) -> Result<Vec<ServerTokenRow>, ServerTokensError> {
        self.request(Command::List).await
    }

    /// Revokes the token `selector` names.
    ///
    /// # Errors
    ///
    /// As [`Self::create`]; [`ServerTokenError::NotFound`] when nothing
    /// matches.
    pub async fn revoke(
        &self,
        selector: TokenSelector,
    ) -> Result<ServerTokenRow, ServerTokensError> {
        self.request(|reply| Command::Revoke(selector, reply)).await
    }

    /// How many tokens are in use.
    ///
    /// # Errors
    ///
    /// As [`Self::create`].
    pub async fn active_count(&self) -> Result<u64, ServerTokensError> {
        self.request(Command::ActiveCount).await
    }

    /// Whether `presented` is a token in use; a match stamps its last use.
    /// Any failure answers `false`.
    pub async fn verify(&self, presented: String) -> bool {
        let (reply, rx) = oneshot::channel();
        if self
            .tx
            .send(Command::Verify(presented, reply))
            .await
            .is_err()
        {
            return false;
        }
        rx.await.unwrap_or(false)
    }
}

async fn run(store: Result<ServerTokenRepository, String>, mut rx: mpsc::Receiver<Command>) {
    while let Some(command) = rx.recv().await {
        let store = store
            .as_ref()
            .map_err(|reason| ServerTokensError::Unavailable {
                reason: reason.clone(),
            });
        match command {
            Command::Create(name, reply) => {
                let _ = reply.send(store.and_then(|s| create(s, &name)));
            }
            Command::List(reply) => {
                let _ = reply.send(store.and_then(|s| Ok(s.list()?)));
            }
            Command::Revoke(selector, reply) => {
                let _ = reply.send(store.and_then(|s| revoke(s, &selector)));
            }
            Command::ActiveCount(reply) => {
                let _ = reply.send(store.and_then(|s| Ok(s.active_count()?)));
            }
            Command::Verify(presented, reply) => {
                let _ = reply.send(store.is_ok_and(|s| verify(s, &presented)));
            }
        }
    }
}

fn create(
    store: &ServerTokenRepository,
    name: &str,
) -> Result<CreatedServerToken, ServerTokensError> {
    let value = format!("{SERVER_TOKEN_PREFIX}{}", generate_hex_token());
    let last_four = &value[value.len() - 4..];
    let row = store.insert(name, &hash_hex(&value), last_four)?;
    tracing::event!(
        tracing::Level::INFO,
        token_id = row.id,
        name = %row.name,
        "compat_server.token.created"
    );
    Ok(CreatedServerToken { row, value })
}

fn revoke(
    store: &ServerTokenRepository,
    selector: &TokenSelector,
) -> Result<ServerTokenRow, ServerTokensError> {
    let row = store.revoke(selector)?;
    tracing::event!(
        tracing::Level::INFO,
        token_id = row.id,
        name = %row.name,
        "compat_server.token.revoked"
    );
    Ok(row)
}

/// Compares the hash of `presented` with every hash in use, each in constant
/// time and without stopping at the first match.
fn verify(store: &ServerTokenRepository, presented: &str) -> bool {
    let active = match store.active_hashes() {
        Ok(active) => active,
        Err(error) => {
            tracing::event!(
                tracing::Level::WARN,
                error = %error,
                "compat_server.tokens.read_failed"
            );
            return false;
        }
    };
    let presented = hash_hex(presented);
    let mut matched = None;
    for (id, hash) in &active {
        if constant_time_eq::constant_time_eq(presented.as_bytes(), hash.as_bytes()) {
            matched = Some(*id);
        }
    }
    let Some(id) = matched else {
        return false;
    };
    if let Err(error) = store.touch(id) {
        tracing::event!(
            tracing::Level::WARN,
            token_id = id,
            error = %error,
            "compat_server.token.touch_failed"
        );
    }
    true
}
