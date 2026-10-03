//! The one-shot move of the tool credentials an older version kept encrypted
//! in `governance.db` into the secret store.
//!
//! Up to schema v2, `tool_credentials` held each value AES-256-GCM encrypted
//! (`nonce(12) || ciphertext`) with a 32-byte key in `.keyfile` beside the
//! database. Schema v3 renames that table `tool_credentials_legacy` and marks
//! the move pending in `governance_upgrades`; this module decrypts each value
//! once, writes it to the secret store, then drops the table and deletes the
//! key file. It is the only reader of the key file left.

use std::path::Path;

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use apollia_auth::SecretStore;
use rusqlite::{params, Connection, OptionalExtension};

use super::{credential_account, TOOL_CREDENTIALS_SERVICE};
use crate::tool_registry::ToolGovernanceError;

/// Name of the key file an older version wrote beside `governance.db`.
pub(crate) const LEGACY_KEYFILE_NAME: &str = ".keyfile";

const NONCE_LEN: usize = 12;

/// Moves the pending encrypted values of `conn` into `secrets`, once.
///
/// A value that cannot be decrypted (key file missing or damaged, value
/// tampered) is not moved: its index entry stays, so the credential is still
/// listed and the operator sees that it no longer works and stores it again.
/// A value the secret store refuses stops the move with an error, the table
/// and the key file untouched, so the next open replays it.
pub(super) fn move_pending_values(
    conn: &Connection,
    db_path: &Path,
    secrets: &dyn SecretStore,
) -> Result<(), ToolGovernanceError> {
    let pending = conn
        .query_row(
            "SELECT 1 FROM governance_upgrades WHERE name = ?1 AND done_at IS NULL",
            params![apollia_permissions::TOOL_CREDENTIALS_UPGRADE],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if !pending {
        return Ok(());
    }
    let keyfile = db_path.with_file_name(LEGACY_KEYFILE_NAME);
    let rows = legacy_rows(conn)?;
    let cipher = if rows.is_empty() {
        None
    } else {
        read_cipher(&keyfile)
    };
    let mut moved = 0usize;
    let mut unreadable = 0usize;
    for (tool, key, blob) in &rows {
        match cipher.as_ref().and_then(|c| decrypt(c, blob)) {
            Some(value) => {
                secrets.set(
                    TOOL_CREDENTIALS_SERVICE,
                    &credential_account(tool, key),
                    &value,
                )?;
                moved += 1;
            }
            None => {
                unreadable += 1;
                tracing::event!(
                    tracing::Level::WARN,
                    tool = %tool,
                    key = %key,
                    detail = "the value could not be decrypted; store it again",
                    "tool.credentials.upgrade.unreadable"
                );
            }
        }
    }
    conn.execute_batch("DROP TABLE IF EXISTS tool_credentials_legacy;")?;
    conn.execute(
        "UPDATE governance_upgrades SET done_at = unixepoch() WHERE name = ?1",
        params![apollia_permissions::TOOL_CREDENTIALS_UPGRADE],
    )?;
    match std::fs::remove_file(&keyfile) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => tracing::event!(
            tracing::Level::WARN,
            path = %keyfile.display(),
            error = %e,
            "tool.credentials.upgrade.keyfile_not_removed"
        ),
    }
    tracing::event!(
        tracing::Level::INFO,
        moved,
        unreadable,
        "tool.credentials.upgrade.done"
    );
    Ok(())
}

type LegacyRow = (String, String, Vec<u8>);

fn legacy_rows(conn: &Connection) -> Result<Vec<LegacyRow>, ToolGovernanceError> {
    let exists: bool = conn.query_row(
        "SELECT COUNT(*) > 0 FROM sqlite_master
         WHERE type = 'table' AND name = 'tool_credentials_legacy'",
        [],
        |row| row.get(0),
    )?;
    if !exists {
        return Ok(Vec::new());
    }
    let mut stmt = conn.prepare(
        "SELECT tool_name, key_name, value_encrypted FROM tool_credentials_legacy ORDER BY id",
    )?;
    let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

fn read_cipher(keyfile: &Path) -> Option<Aes256Gcm> {
    let bytes = match std::fs::read(keyfile) {
        Ok(bytes) => bytes,
        Err(e) => {
            tracing::event!(
                tracing::Level::WARN,
                path = %keyfile.display(),
                error = %e,
                "tool.credentials.upgrade.keyfile_unreadable"
            );
            return None;
        }
    };
    if bytes.len() != 32 {
        tracing::event!(
            tracing::Level::WARN,
            path = %keyfile.display(),
            found = bytes.len(),
            "tool.credentials.upgrade.keyfile_corrupted"
        );
        return None;
    }
    Some(Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&bytes)))
}

fn decrypt(cipher: &Aes256Gcm, blob: &[u8]) -> Option<String> {
    if blob.len() < NONCE_LEN {
        return None;
    }
    let (nonce, ciphertext) = blob.split_at(NONCE_LEN);
    let plaintext = cipher.decrypt(Nonce::from_slice(nonce), ciphertext).ok()?;
    String::from_utf8(plaintext).ok()
}
