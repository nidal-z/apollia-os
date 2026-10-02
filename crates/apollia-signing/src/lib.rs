//! Offline signatures for Apollia artefacts.
//!
//! Two schemes live here, each with its own keys and its own use:
//!
//! - **Ed25519 over a digest list**, for agent packages and organisation
//!   policies. The signed object is a [`DigestList`], the canonical list of
//!   `(path, SHA-256)` entries of an artefact, sorted by the bytes of the path.
//!   [`sign`] returns a detached signature, [`verify`] says whether it holds,
//!   and [`SignatureFile`] is the versioned `SIGNATURE` file that carries it.
//!   Keys are generated in the application ([`SigningKeyPair::generate`]).
//!   The private half of a package author key lives only in a
//!   [`SecretStore`](apollia_auth::SecretStore) ([`store_signing_key`],
//!   [`load_signing_key`]); the private half of an organisation key, which IT
//!   keeps, lives in a file created with owner-only rights
//!   ([`write_private_key_file`], [`read_private_key_file`]).
//! - **minisign**, for distribution metadata published by the Apollia release
//!   pipeline. [`distribution::verify_minisign`] checks a minisign signature
//!   against a minisign public key. Nothing here signs in that format: the
//!   release pipeline does, with a dedicated distribution key.
//!
//! Both are verifiable without network access. The decision and its trade-offs
//! are stated under the `signing-schemes` anchor of the architecture decisions
//! page.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

mod digest_list;
pub mod distribution;
mod error;
mod key_file;
mod key_storage;
mod keys;
mod signature_file;

pub use digest_list::{sha256, DigestList};
pub use error::SigningError;
pub use key_file::{read_private_key_file, write_private_key_file};
pub use key_storage::{load_signing_key, store_signing_key, SIGNING_SECRET_SERVICE};
pub use keys::{sign, verify, PublicKey, Signature, SigningKeyPair, Verdict};
pub use signature_file::{SignatureFile, SIGNATURE_FORMAT_VERSION};
