//! The organisation policy: a signed `policy.toml` that IT drops in a system
//! folder, which the runtime reads once at startup.
//!
//! The folder holds three files:
//!
//! - `policy.toml`, the policy, in the schema of [`schema`], including the
//!   date the organisation signed it (`signed_at`);
//! - `SIGNATURE`, an [`apollia_signing::SignatureFile`] over a digest list of
//!   one entry, `policy.toml` and its SHA-256;
//! - `org.pub`, the organisation public key, 64 lowercase hexadecimal
//!   characters (the 32 raw Ed25519 bytes), optionally followed by a line feed.
//!
//! [`load_policy`] answers [`PolicyVerdict::Absent`] when the folder does not
//! exist, [`PolicyVerdict::Valid`] when the three files are present, owned by
//! the system, and the signature holds for `org.pub`, and
//! [`PolicyVerdict::Invalid`] with its reason otherwise. Applying the verdict
//! (rules, fail-closed state) is the caller's business.
//!
//! The checks run in a fixed order, and the first failure is the reason:
//! rights of the folder and of each file present, then `policy.toml` present,
//! then `org.pub`, then `SIGNATURE` and the signature, then the schema. The
//! policy text is parsed only once its signature holds.

mod authoring;
mod ownership;
pub mod schema;
mod view;

#[cfg(test)]
mod authoring_tests;
#[cfg(test)]
mod tests;

use std::fmt;
use std::path::{Path, PathBuf};

use apollia_core::config::PolicySettings;
use apollia_signing::{sha256, DigestList, PublicKey, SignatureFile, Verdict};

pub use authoring::{
    is_system_policy_dir, policy_template, refuse_system_dir, sign_policy_dir,
    write_policy_template, PolicyAuthoringError, SignedPolicy, ORG_PRIVATE_KEY_FILE,
};
pub use ownership::OwnerRequirement;
pub use schema::{
    parse_policy, ConnectionsPolicy, DenyOnly, ExportDetail, FamiliesPolicy, InferencePolicy,
    JournalExportPolicy, JournalPolicy, OrgPolicy, PackagesPolicy, PolicyFamily, PolicySchemaError,
    POLICY_SCHEMA_VERSION,
};
pub use view::PolicyView;

/// The setting of the model import rule that `inference.require_model_hash`
/// fixes: a model whose digest matches no known one is refused.
pub const REQUIRE_VERIFIED_SETTING: &str = "models.import.require_verified";

/// The system folder of the organisation policy on macOS.
#[cfg(target_os = "macos")]
pub const SYSTEM_POLICY_DIR: &str = "/Library/Application Support/Apollia/policy";

/// The system folder of the organisation policy on Windows.
///
/// Written out rather than read from `%ProgramData%`: an environment variable
/// is set by the user, and a policy the user can point elsewhere is no policy.
#[cfg(windows)]
pub const SYSTEM_POLICY_DIR: &str = r"C:\ProgramData\Apollia\policy";

/// The system folder of the organisation policy on Linux and other Unix systems.
#[cfg(all(not(target_os = "macos"), not(windows)))]
pub const SYSTEM_POLICY_DIR: &str = "/etc/apollia/policy";

/// File name of the policy.
pub const POLICY_FILE: &str = "policy.toml";
/// File name of the signature over the policy.
pub const SIGNATURE_FILE: &str = "SIGNATURE";
/// File name of the organisation public key.
pub const ORG_KEY_FILE: &str = "org.pub";

/// Largest `policy.toml` read, in bytes.
pub const MAX_POLICY_BYTES: u64 = 1024 * 1024;

/// Environment variable that points the runtime at another policy folder.
///
/// Read in debug builds only (tests and the desktop automaton); a release
/// build does not contain the code that reads it.
#[cfg(debug_assertions)]
pub const POLICY_DIR_OVERRIDE_ENV: &str = "APOLLIA_POLICY_DIR";

/// Where the runtime looks for the policy, and whose rights it requires.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyLocation {
    /// The policy folder.
    pub dir: PathBuf,
    /// Who must be the only one able to modify it.
    pub owner: OwnerRequirement,
}

/// Returns where the runtime reads the policy.
///
/// A release build always answers [`SYSTEM_POLICY_DIR`] with
/// [`OwnerRequirement::System`]. A debug build honours
/// `POLICY_DIR_OVERRIDE_ENV` when it is set, with
/// `OwnerRequirement::CurrentUser`, so a seeded test home can carry a policy.
pub fn policy_location() -> PolicyLocation {
    #[cfg(debug_assertions)]
    if let Some(dir) = std::env::var_os(POLICY_DIR_OVERRIDE_ENV).filter(|v| !v.is_empty()) {
        return PolicyLocation {
            dir: PathBuf::from(dir),
            owner: OwnerRequirement::CurrentUser,
        };
    }
    PolicyLocation {
        dir: PathBuf::from(SYSTEM_POLICY_DIR),
        owner: OwnerRequirement::System,
    }
}

/// Why a present policy is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum PolicyInvalidReason {
    /// `policy.toml` is missing, too large, or breaks the schema.
    Schema,
    /// `SIGNATURE` is missing or malformed, or the signature does not hold.
    Signature,
    /// `org.pub` is missing, unreadable, or not a key, or the signature names
    /// another key.
    Key,
    /// The folder or a file is a symbolic link, or can be modified by someone
    /// other than the system administrator.
    Owner,
}

impl PolicyInvalidReason {
    /// Stable lowercase name, for logs and machine output.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Schema => "schema",
            Self::Signature => "signature",
            Self::Key => "key",
            Self::Owner => "owner",
        }
    }
}

impl fmt::Display for PolicyInvalidReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A present policy that was refused, with its reason and a readable detail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyInvalid {
    /// The reason class.
    pub reason: PolicyInvalidReason,
    /// What exactly was refused.
    pub detail: String,
}

/// A policy whose rights, signature and schema all hold.
#[derive(Debug, Clone, PartialEq)]
pub struct ValidPolicy {
    /// The policy.
    pub policy: OrgPolicy,
    /// Fingerprint of `org.pub`, the key that signed.
    pub key_fingerprint: String,
    /// Publisher name declared in `SIGNATURE`.
    pub publisher: String,
    /// When the organisation signed the policy, as `signed_at` declares it in
    /// `policy.toml`. The signature covers the whole file, so this date is
    /// signed: changing it breaks the signature.
    pub signed_at: toml::value::Datetime,
}

/// The outcome of reading a policy folder.
#[derive(Debug, Clone, PartialEq)]
pub enum PolicyVerdict {
    /// No policy folder: the user is free.
    Absent,
    /// A policy that holds.
    Valid(Box<ValidPolicy>),
    /// A policy folder that is present but refused.
    Invalid(PolicyInvalid),
}

impl PolicyVerdict {
    /// Stable lowercase name of the verdict, for logs and machine output.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Absent => "absent",
            Self::Valid(_) => "valid",
            Self::Invalid(_) => "invalid",
        }
    }

    /// The settings the policy fixes, as the settings contract of
    /// `apollia.toml` reads them: each key of `[settings]` with its value.
    ///
    /// `inference.require_model_hash = true` fixes the import rule
    /// [`REQUIRE_VERIFIED_SETTING`] to `true` on top of them, whatever
    /// `[settings]` says of that key.
    ///
    /// Without a valid policy nothing is fixed. A refused policy fixes nothing
    /// either: what it shuts down is decided where the policy is applied.
    pub fn imposed_settings(&self) -> PolicySettings {
        let Self::Valid(valid) = self else {
            return PolicySettings::none();
        };
        let imposed =
            valid
                .policy
                .settings
                .iter()
                .fold(PolicySettings::none(), |imposed, (key, value)| {
                    // The parser already turned every value into JSON once, in
                    // `check_setting`; a value that could not be is a refused
                    // policy, never a valid one.
                    match serde_json::to_value(value) {
                        Ok(value) => imposed.impose(key.clone(), value),
                        Err(_) => imposed,
                    }
                });
        if valid.policy.inference.require_model_hash {
            return imposed.impose(REQUIRE_VERIFIED_SETTING, serde_json::Value::Bool(true));
        }
        imposed
    }

    /// The `[packages]` section of a valid policy; the empty section, which
    /// requires nothing, otherwise.
    pub fn packages(&self) -> PackagesPolicy {
        match self {
            Self::Valid(valid) => valid.policy.packages.clone(),
            Self::Absent | Self::Invalid(_) => PackagesPolicy::default(),
        }
    }
}

fn invalid(reason: PolicyInvalidReason, detail: impl Into<String>) -> PolicyVerdict {
    PolicyVerdict::Invalid(PolicyInvalid {
        reason,
        detail: detail.into(),
    })
}

/// Reads the policy folder `dir`, requiring the rights `owner`.
pub fn load_policy(dir: &Path, owner: OwnerRequirement) -> PolicyVerdict {
    let dir_metadata = match std::fs::symlink_metadata(dir) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return PolicyVerdict::Absent,
        Err(e) => {
            return invalid(
                PolicyInvalidReason::Owner,
                format!("cannot read the rights of {}: {e}", dir.display()),
            )
        }
    };
    if !dir_metadata.file_type().is_symlink() && !dir_metadata.is_dir() {
        return invalid(
            PolicyInvalidReason::Schema,
            format!("{} is not a folder", dir.display()),
        );
    }
    if let Err(detail) = ownership::check(dir, owner) {
        return invalid(PolicyInvalidReason::Owner, detail);
    }
    for name in [POLICY_FILE, SIGNATURE_FILE, ORG_KEY_FILE] {
        let path = dir.join(name);
        match std::fs::symlink_metadata(&path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            _ => {
                if let Err(detail) = ownership::check(&path, owner) {
                    return invalid(PolicyInvalidReason::Owner, detail);
                }
            }
        }
    }
    verify_contents(dir)
}

/// Reads the policy folder `dir` without looking at its rights: the schema,
/// the key and the signature only.
///
/// This is the check IT runs on a folder it prepares, before deploying it to
/// the system folder; the runtime never trusts a policy read this way, it
/// reads through [`load_policy`].
pub fn verify_policy_dir(dir: &Path) -> PolicyVerdict {
    match std::fs::metadata(dir) {
        Ok(m) if m.is_dir() => verify_contents(dir),
        Ok(_) => invalid(
            PolicyInvalidReason::Schema,
            format!("{} is not a folder", dir.display()),
        ),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => PolicyVerdict::Absent,
        Err(e) => invalid(
            PolicyInvalidReason::Schema,
            format!("cannot read {}: {e}", dir.display()),
        ),
    }
}

fn verify_contents(dir: &Path) -> PolicyVerdict {
    let policy_bytes = match read_policy_bytes(&dir.join(POLICY_FILE)) {
        Ok(bytes) => bytes,
        Err(detail) => return invalid(PolicyInvalidReason::Schema, detail),
    };
    let public_key = match read_org_key(&dir.join(ORG_KEY_FILE)) {
        Ok(key) => key,
        Err(detail) => return invalid(PolicyInvalidReason::Key, detail),
    };

    let signature_path = dir.join(SIGNATURE_FILE);
    let signature = match std::fs::read_to_string(&signature_path) {
        Ok(text) => match SignatureFile::parse(&text) {
            Ok(file) => file,
            Err(e) => return invalid(PolicyInvalidReason::Signature, e.to_string()),
        },
        Err(e) => {
            return invalid(
                PolicyInvalidReason::Signature,
                format!("cannot read {SIGNATURE_FILE}: {e}"),
            )
        }
    };
    if signature.key_fingerprint() != public_key.fingerprint() {
        return invalid(
            PolicyInvalidReason::Key,
            format!(
                "{SIGNATURE_FILE} names key {}, {ORG_KEY_FILE} is {}",
                signature.key_fingerprint(),
                public_key.fingerprint()
            ),
        );
    }
    let mut list = DigestList::new();
    if let Err(e) = list.insert(POLICY_FILE, sha256(&policy_bytes)) {
        return invalid(PolicyInvalidReason::Signature, e.to_string());
    }
    if signature.verify(&list, &public_key) != Verdict::Valid {
        return invalid(
            PolicyInvalidReason::Signature,
            format!("the signature does not hold for {POLICY_FILE}"),
        );
    }

    let text = match String::from_utf8(policy_bytes) {
        Ok(text) => text,
        Err(_) => {
            return invalid(
                PolicyInvalidReason::Schema,
                format!("{POLICY_FILE} is not UTF-8"),
            )
        }
    };
    let policy = match parse_policy(&text) {
        Ok(policy) => policy,
        Err(e) => return invalid(PolicyInvalidReason::Schema, e.to_string()),
    };

    PolicyVerdict::Valid(Box::new(ValidPolicy {
        signed_at: policy.signed_at,
        policy,
        key_fingerprint: public_key.fingerprint(),
        publisher: signature.publisher().to_owned(),
    }))
}

pub(crate) fn read_policy_bytes(path: &Path) -> Result<Vec<u8>, String> {
    let metadata =
        std::fs::metadata(path).map_err(|e| format!("cannot read {POLICY_FILE}: {e}"))?;
    if metadata.len() > MAX_POLICY_BYTES {
        return Err(format!(
            "{POLICY_FILE} is larger than {MAX_POLICY_BYTES} bytes"
        ));
    }
    std::fs::read(path).map_err(|e| format!("cannot read {POLICY_FILE}: {e}"))
}

fn read_org_key(path: &Path) -> Result<PublicKey, String> {
    let text =
        std::fs::read_to_string(path).map_err(|e| format!("cannot read {ORG_KEY_FILE}: {e}"))?;
    let hex_key = text.strip_suffix('\n').unwrap_or(&text);
    if hex_key.len() != 64
        || !hex_key
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
    {
        return Err(format!(
            "{ORG_KEY_FILE} is not 64 lowercase hexadecimal characters"
        ));
    }
    let mut bytes = [0u8; 32];
    for (i, pair) in hex_key.as_bytes().chunks(2).enumerate() {
        let high = hex_value(pair[0]);
        let low = hex_value(pair[1]);
        bytes[i] = (high << 4) | low;
    }
    PublicKey::from_bytes(&bytes).map_err(|e| format!("{ORG_KEY_FILE}: {e}"))
}

fn hex_value(b: u8) -> u8 {
    match b {
        b'0'..=b'9' => b - b'0',
        _ => b - b'a' + 10,
    }
}
