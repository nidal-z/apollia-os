//! The schema of `policy.toml`, version 1.
//!
//! Strict TOML: every table refuses a key it does not know, and a document
//! that declares another `schema_version` is refused whole. A section that is
//! left out imposes nothing.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// The only schema version this build reads.
pub const POLICY_SCHEMA_VERSION: u32 = 1;

/// Why a `policy.toml` document was refused by the schema.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum PolicySchemaError {
    /// The text is not TOML, or it breaks the shape of the schema (an unknown
    /// key, a value of the wrong type, a missing `schema_version`).
    #[error("policy schema violation: {0}")]
    Invalid(String),
    /// The document declares a schema version this build does not read.
    #[error("unsupported policy schema version {found}, expected {POLICY_SCHEMA_VERSION}")]
    UnsupportedVersion {
        /// The version declared by the document.
        found: u32,
    },
    /// `signed_at` is not a full date and time with an offset.
    #[error("signed_at {value} is not a date and time with an offset")]
    IncompleteSignedAt {
        /// The value as written.
        value: String,
    },
    /// A trusted publisher is not a key fingerprint.
    #[error("trusted publisher {value:?} is not 64 lowercase hexadecimal characters")]
    InvalidPublisherFingerprint {
        /// The value as written.
        value: String,
    },
}

/// An organisation policy, as written in `policy.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OrgPolicy {
    /// Version of the schema the document is written in.
    pub schema_version: u32,
    /// When the organisation signed this policy: a date, a time and an offset.
    /// It sits in the signed file, so the signature covers it.
    pub signed_at: toml::value::Datetime,
    /// Connections and tools allowed or refused.
    #[serde(default)]
    pub connections: ConnectionsPolicy,
    /// Action families refused outright.
    #[serde(default)]
    pub families: FamiliesPolicy,
    /// Inference servers and models.
    #[serde(default)]
    pub inference: InferencePolicy,
    /// Settings fixed by the organisation, by key.
    #[serde(default)]
    pub settings: BTreeMap<String, toml::Value>,
    /// Agent package requirements.
    #[serde(default)]
    pub packages: PackagesPolicy,
    /// Audit journal requirements.
    #[serde(default)]
    pub journal: JournalPolicy,
}

/// A rule that can only refuse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DenyOnly {
    /// Refused.
    Deny,
}

/// `[connections]`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectionsPolicy {
    /// Identifiers allowed. A non-empty list refuses everything it does not name.
    #[serde(default)]
    pub allow: Vec<String>,
    /// Identifiers refused.
    #[serde(default)]
    pub deny: Vec<String>,
    /// `remote = "deny"` refuses every remote server.
    #[serde(default)]
    pub remote: Option<DenyOnly>,
}

/// An action family a policy can refuse.
///
/// The names are the serialized names of the action families a tool declares.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyFamily {
    /// Read data that is not personal.
    Read,
    /// Read personal data.
    ReadPersonal,
    /// Write or delete.
    WriteOrDelete,
    /// Send something outside the machine.
    SendExternal,
    /// Execute code.
    ExecuteCode,
}

/// `[families]`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FamiliesPolicy {
    /// Families refused whatever the agent or the consent.
    #[serde(default)]
    pub deny: Vec<PolicyFamily>,
}

/// `[inference]`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InferencePolicy {
    /// Inference servers allowed. A non-empty list refuses the others.
    #[serde(default)]
    pub allowed_servers: Vec<String>,
    /// `remote_endpoints = "deny"` refuses every cloud endpoint.
    #[serde(default)]
    pub remote_endpoints: Option<DenyOnly>,
    /// Refuses a model whose file has no known digest.
    #[serde(default)]
    pub require_model_hash: bool,
}

/// `[packages]`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackagesPolicy {
    /// Refuses an agent package that carries no valid signature.
    #[serde(default)]
    pub require_signature: bool,
    /// Key fingerprints of the publishers trusted, 64 lowercase hex characters.
    #[serde(default)]
    pub trusted_publishers: Vec<String>,
}

/// Whether an export carries the detail of each entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportDetail {
    /// The detail can never be exported.
    Forbid,
    /// The detail is always exported.
    Require,
}

/// `[journal.export]`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JournalExportPolicy {
    /// Forbids or imposes the detail in an export. Absent, the user chooses.
    #[serde(default)]
    pub detail: Option<ExportDetail>,
    /// Local folder that receives a daily export of the summary.
    #[serde(default)]
    pub daily_summary_dir: Option<String>,
}

/// `[journal]`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JournalPolicy {
    /// The journal must be open for sensitive actions to run.
    #[serde(default)]
    pub required: bool,
    /// Days an entry is kept.
    #[serde(default)]
    pub retention_days: Option<u32>,
    /// Export rules.
    #[serde(default)]
    pub export: JournalExportPolicy,
}

#[derive(Deserialize)]
struct VersionProbe {
    schema_version: Option<toml::Value>,
}

/// Parses `policy.toml` in the version 1 schema.
///
/// # Errors
///
/// Returns [`PolicySchemaError::UnsupportedVersion`] when the document declares
/// another version, [`PolicySchemaError::IncompleteSignedAt`] when `signed_at`
/// lacks its date, its time or its offset,
/// [`PolicySchemaError::InvalidPublisherFingerprint`] when a
/// trusted publisher is not a fingerprint, and [`PolicySchemaError::Invalid`]
/// for any other deviation.
pub fn parse_policy(text: &str) -> Result<OrgPolicy, PolicySchemaError> {
    // The version is read first, on its own, so a document written for a
    // later schema is reported as such rather than as a list of unknown keys.
    let probe: VersionProbe =
        toml::from_str(text).map_err(|e| PolicySchemaError::Invalid(e.message().to_owned()))?;
    match probe.schema_version {
        Some(toml::Value::Integer(v)) if v == i64::from(POLICY_SCHEMA_VERSION) => {}
        Some(toml::Value::Integer(v)) => {
            return Err(PolicySchemaError::UnsupportedVersion {
                found: u32::try_from(v).unwrap_or(u32::MAX),
            })
        }
        Some(_) => {
            return Err(PolicySchemaError::Invalid(
                "schema_version is not an integer".to_owned(),
            ))
        }
        None => {
            return Err(PolicySchemaError::Invalid(
                "missing schema_version".to_owned(),
            ))
        }
    }

    let policy: OrgPolicy =
        toml::from_str(text).map_err(|e| PolicySchemaError::Invalid(e.message().to_owned()))?;
    let signed_at = &policy.signed_at;
    if signed_at.date.is_none() || signed_at.time.is_none() || signed_at.offset.is_none() {
        return Err(PolicySchemaError::IncompleteSignedAt {
            value: signed_at.to_string(),
        });
    }
    if let Some(bad) = policy
        .packages
        .trusted_publishers
        .iter()
        .find(|f| !is_fingerprint(f))
    {
        return Err(PolicySchemaError::InvalidPublisherFingerprint { value: bad.clone() });
    }
    Ok(policy)
}

fn is_fingerprint(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}
