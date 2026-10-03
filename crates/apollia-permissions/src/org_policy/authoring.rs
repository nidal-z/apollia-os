//! Writing and signing a policy folder, on the machine where IT prepares it.
//!
//! Nothing here installs a policy. The folder is written wherever IT asks,
//! then deployed to the system folder by IT's own tools; writing into the
//! system folder itself is refused, so the deployment stays one deliberate
//! act by the administrator.

use std::path::{Path, PathBuf};

use apollia_signing::{sha256, DigestList, SignatureFile, SigningError, SigningKeyPair};

use super::schema::{parse_policy, PolicySchemaError};
use super::{read_policy_bytes, ORG_KEY_FILE, POLICY_FILE, SIGNATURE_FILE, SYSTEM_POLICY_DIR};

/// File name of the organisation private key written next to `org.pub`.
pub const ORG_PRIVATE_KEY_FILE: &str = "org.key";

/// Why a policy folder could not be written or signed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PolicyAuthoringError {
    /// The target is the system policy folder, or inside it.
    #[error("{} is the system policy folder: prepare the policy elsewhere and deploy it with your own tools", path.display())]
    SystemDir {
        /// The refused path.
        path: PathBuf,
    },
    /// The file to write already exists.
    #[error("{} already exists", path.display())]
    AlreadyExists {
        /// The existing file.
        path: PathBuf,
    },
    /// `policy.toml` cannot be read.
    #[error("{0}")]
    Unreadable(String),
    /// `policy.toml` breaks the schema, so it is not signed.
    #[error(transparent)]
    Schema(#[from] PolicySchemaError),
    /// The signature could not be built.
    #[error(transparent)]
    Signing(#[from] SigningError),
    /// A file could not be written.
    #[error("cannot write {}", path.display())]
    Write {
        /// The file.
        path: PathBuf,
        /// The underlying failure.
        #[source]
        source: std::io::Error,
    },
}

/// What `sign_policy_dir` signed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedPolicy {
    /// Fingerprint of the key that signed, the one written to `org.pub`.
    pub key_fingerprint: String,
    /// Publisher name declared in `SIGNATURE`.
    pub publisher: String,
    /// `signed_at`, as declared in `policy.toml`.
    pub signed_at: toml::value::Datetime,
}

/// Returns whether `path` is the system policy folder or lies inside it.
///
/// The comparison is made on the resolved path, so a relative path or a
/// symbolic link that lands in the system folder is recognised.
pub fn is_system_policy_dir(path: &Path) -> bool {
    let target = resolve(path);
    let system = resolve(Path::new(SYSTEM_POLICY_DIR));
    target.starts_with(&system) || target.starts_with(SYSTEM_POLICY_DIR)
}

/// Makes `path` absolute and resolves the symbolic links of its longest
/// existing prefix; the part that does not exist yet is appended as written.
fn resolve(path: &Path) -> PathBuf {
    let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let mut existing = absolute.as_path();
    let mut rest = Vec::new();
    loop {
        if let Ok(canonical) = existing.canonicalize() {
            let mut resolved = canonical;
            for part in rest.iter().rev() {
                resolved.push(part);
            }
            return resolved;
        }
        match (existing.parent(), existing.file_name()) {
            (Some(parent), Some(name)) => {
                rest.push(name.to_os_string());
                existing = parent;
            }
            _ => return absolute,
        }
    }
}

/// Refuses `dir` when it is the system policy folder.
///
/// # Errors
///
/// Returns [`PolicyAuthoringError::SystemDir`] when `dir` is the system
/// policy folder or lies inside it.
pub fn refuse_system_dir(dir: &Path) -> Result<(), PolicyAuthoringError> {
    if is_system_policy_dir(dir) {
        return Err(PolicyAuthoringError::SystemDir {
            path: dir.to_path_buf(),
        });
    }
    Ok(())
}

/// Returns a commented `policy.toml` that imposes nothing, dated `signed_at`.
///
/// Every section of the schema is present with the value that imposes
/// nothing, and each rule IT may set is shown in a comment above it.
/// `signed_at` must be a date, a time and an offset in TOML form, such as
/// `2026-10-02T09:30:00Z`.
pub fn policy_template(signed_at: &str) -> String {
    format!(
        r#"# Apollia organisation policy.
#
# Edit this file, sign the folder with `apollia-os policy sign`, check it with
# `apollia-os policy verify`, then deploy the folder (policy.toml, SIGNATURE,
# org.pub) to the system policy folder with your own tools, owned by the
# administrator and writable by nobody else.
#
# A key left out, or a list left empty, imposes nothing. Every other key is
# refused: a typing error is a policy that does not load.

schema_version = 1

# When the organisation signs this policy: a date, a time and an offset. It is
# covered by the signature; set it to the day you sign.
signed_at = {signed_at}

[connections]
# Connections allowed, by identifier: "selection:<entry>" for a server of the
# curated selection, "package:npm:<name>" for a server that runs an npm
# package, "native:<kind>" for google, microsoft, imap or folder. A non-empty
# list refuses everything else, a server of the user's own among it.
# allow = ["native:google", "selection:@notionhq/notion-mcp-server"]
allow = []
# Connections refused, by the same identifiers. A connection already added
# that becomes refused is not started again; it is never deleted.
# deny = ["package:npm:@modelcontextprotocol/server-filesystem"]
deny = []
# Refuse every remote server.
# remote = "deny"

[families]
# Action families refused whatever the agent or the consent. Names: read,
# read_personal, write_or_delete, send_external, execute_code.
# deny = ["execute_code", "send_external"]
deny = []

[inference]
# Addresses of the inference servers allowed. A non-empty list refuses every
# other address, when a server is added and before a request reaches it.
# allowed_servers = ["https://llm.example.internal"]
allowed_servers = []
# Refuse every server outside this machine that allowed_servers does not list.
# remote_endpoints = "deny"
# Refuse a model whose file has no known digest: fixes
# models.import.require_verified to true.
require_model_hash = false

[settings]
# Settings fixed by the organisation, by dotted key. Each key names a setting
# of apollia.toml and each value has its type; any other key refuses the policy.
# "desktop.developer_mode" = false
# The load guardrail: disabled, lenient, balanced, strict or ceiling.
# "llm.guardrails.level" = "strict"
# The model registry: an internal mirror, or the registry turned off.
# "models.registry_url" = "https://models.example.internal/index.json"
# "models.registry_enabled" = false

[packages]
# Refuse an agent package that carries no valid signature.
require_signature = false
# Key fingerprints of the trusted publishers, 64 lowercase hexadecimal
# characters each.
trusted_publishers = []

[journal]
# The journal must be open for sensitive actions to run.
required = false
# Days an entry is kept.
# retention_days = 365

[journal.export]
# Forbid or require the detail of each entry in an export: "forbid" or
# "require". Left out, the user chooses.
# detail = "forbid"
# Local folder that receives a daily export of the summary.
# daily_summary_dir = "/var/lib/apollia/journal-exports"
"#
    )
}

/// Writes the template into `dir`, created when missing, and returns the path
/// of the new `policy.toml`. An existing `policy.toml` is never replaced.
///
/// # Errors
///
/// Returns [`PolicyAuthoringError::SystemDir`] for the system policy folder,
/// [`PolicyAuthoringError::AlreadyExists`] when `policy.toml` is present, and
/// [`PolicyAuthoringError::Write`] when the folder or the file cannot be
/// written.
pub fn write_policy_template(dir: &Path, signed_at: &str) -> Result<PathBuf, PolicyAuthoringError> {
    refuse_system_dir(dir)?;
    std::fs::create_dir_all(dir).map_err(|source| PolicyAuthoringError::Write {
        path: dir.to_path_buf(),
        source,
    })?;
    let path = dir.join(POLICY_FILE);
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path);
    let mut file = match file {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            return Err(PolicyAuthoringError::AlreadyExists { path })
        }
        Err(source) => return Err(PolicyAuthoringError::Write { path, source }),
    };
    std::io::Write::write_all(&mut file, policy_template(signed_at).as_bytes()).map_err(
        |source| PolicyAuthoringError::Write {
            path: path.clone(),
            source,
        },
    )?;
    Ok(path)
}

/// Signs the `policy.toml` of `dir` with `key`, declaring `publisher`, and
/// writes `SIGNATURE` and `org.pub` next to it, replacing earlier ones.
///
/// A policy that breaks the schema is not signed.
///
/// # Errors
///
/// Returns [`PolicyAuthoringError::SystemDir`] for the system policy folder,
/// [`PolicyAuthoringError::Unreadable`] when `policy.toml` cannot be read,
/// [`PolicyAuthoringError::Schema`] when it breaks the schema,
/// [`PolicyAuthoringError::Signing`] when the publisher name is refused, and
/// [`PolicyAuthoringError::Write`] when a file cannot be written.
pub fn sign_policy_dir(
    dir: &Path,
    key: &SigningKeyPair,
    publisher: &str,
) -> Result<SignedPolicy, PolicyAuthoringError> {
    refuse_system_dir(dir)?;
    let bytes =
        read_policy_bytes(&dir.join(POLICY_FILE)).map_err(PolicyAuthoringError::Unreadable)?;
    let text = std::str::from_utf8(&bytes)
        .map_err(|_| PolicyAuthoringError::Unreadable(format!("{POLICY_FILE} is not UTF-8")))?;
    let policy = parse_policy(text)?;

    let mut list = DigestList::new();
    list.insert(POLICY_FILE, sha256(&bytes))?;
    let signature = SignatureFile::create(&list, publisher, key)?;
    let public_key = key.public_key();

    let write = |name: &str, contents: String| {
        let path = dir.join(name);
        std::fs::write(&path, contents)
            .map_err(|source| PolicyAuthoringError::Write { path, source })
    };
    write(SIGNATURE_FILE, signature.render())?;
    write(ORG_KEY_FILE, format!("{}\n", public_key.to_hex()))?;

    Ok(SignedPolicy {
        key_fingerprint: public_key.fingerprint(),
        publisher: publisher.to_owned(),
        signed_at: policy.signed_at,
    })
}
