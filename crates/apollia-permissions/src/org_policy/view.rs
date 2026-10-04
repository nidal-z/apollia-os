//! The machine form of a policy verdict, one shape for every reader.
//!
//! `apollia-os policy show --json` prints it for the folder it reads now, and
//! the runtime serves it for the verdict it read at startup. Both say the same
//! thing in the same keys, so a script written against one reads the other.
//! The form is structure only: the sentences a person reads are written by
//! whoever renders it, in their own language.

use std::path::Path;

use serde::Serialize;

use crate::org_policy::{OrgPolicy, PolicyVerdict};

/// A policy verdict as machine output, tagged by `state`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum PolicyView {
    /// No policy folder: nothing is imposed.
    #[serde(rename = "none")]
    Absent {
        /// The folder read.
        dir: String,
    },
    /// A policy folder that is present but refused.
    Invalid {
        /// The folder read.
        dir: String,
        /// The reason class, `schema`, `signature`, `key` or `owner`.
        reason: String,
        /// What exactly was refused.
        detail: String,
    },
    /// A policy whose rights, signature and schema hold.
    Active {
        /// The folder read.
        dir: String,
        /// Fingerprint of the organisation key that signed.
        key_fingerprint: String,
        /// Publisher name declared in the signature.
        publisher: String,
        /// When the organisation signed, as `policy.toml` declares it.
        signed_at: String,
        /// The policy, section by section, with `signed_at` as text.
        policy: serde_json::Value,
    },
}

impl PolicyView {
    /// The view of `verdict`, read from the folder `dir`.
    #[must_use]
    pub fn of(verdict: &PolicyVerdict, dir: &Path) -> Self {
        let dir = dir.display().to_string();
        match verdict {
            PolicyVerdict::Absent => Self::Absent { dir },
            PolicyVerdict::Invalid(invalid) => Self::Invalid {
                dir,
                reason: invalid.reason.as_str().to_owned(),
                detail: invalid.detail.clone(),
            },
            PolicyVerdict::Valid(valid) => Self::Active {
                dir,
                key_fingerprint: valid.key_fingerprint.clone(),
                publisher: valid.publisher.clone(),
                signed_at: valid.signed_at.to_string(),
                policy: policy_json(&valid.policy),
            },
        }
    }
}

/// The policy as JSON, with `signed_at` as its TOML text rather than the
/// private form a TOML date takes through serde.
fn policy_json(policy: &OrgPolicy) -> serde_json::Value {
    let mut value = serde_json::to_value(policy).unwrap_or(serde_json::Value::Null);
    if let Some(object) = value.as_object_mut() {
        object.insert(
            "signed_at".to_owned(),
            serde_json::Value::String(policy.signed_at.to_string()),
        );
    }
    value
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::org_policy::{parse_policy, PolicyInvalid, PolicyInvalidReason, ValidPolicy};

    fn dir() -> PathBuf {
        PathBuf::from("/etc/apollia/policy")
    }

    #[test]
    fn test_an_absent_policy_is_state_none_with_its_folder() {
        // GIVEN no policy folder
        let verdict = PolicyVerdict::Absent;
        // WHEN it is viewed
        let value = serde_json::to_value(PolicyView::of(&verdict, &dir())).unwrap();
        // THEN only the state and the folder are said
        assert_eq!(
            value,
            serde_json::json!({"state": "none", "dir": "/etc/apollia/policy"})
        );
    }

    #[test]
    fn test_an_invalid_policy_carries_its_reason_class_and_detail() {
        // GIVEN a policy refused for its signature
        let verdict = PolicyVerdict::Invalid(PolicyInvalid {
            reason: PolicyInvalidReason::Signature,
            detail: "the signature does not hold".to_owned(),
        });
        // WHEN it is viewed
        let value = serde_json::to_value(PolicyView::of(&verdict, &dir())).unwrap();
        // THEN the reason is the stable class name, with the detail beside it
        assert_eq!(value["state"], "invalid");
        assert_eq!(value["reason"], "signature");
        assert_eq!(value["detail"], "the signature does not hold");
    }

    #[test]
    fn test_an_active_policy_carries_key_date_and_sections() {
        // GIVEN a valid policy refusing remote servers
        let policy = parse_policy(
            "schema_version = 1\nsigned_at = 2026-10-02T09:30:00Z\n[connections]\nremote = \"deny\"\n",
        )
        .unwrap();
        let verdict = PolicyVerdict::Valid(Box::new(ValidPolicy {
            signed_at: policy.signed_at,
            policy,
            key_fingerprint: "ab".repeat(32),
            policy_digest: "00".into(),
            publisher: "Example IT".to_owned(),
        }));
        // WHEN it is viewed
        let value = serde_json::to_value(PolicyView::of(&verdict, &dir())).unwrap();
        // THEN the key, the date as text and each section are present
        assert_eq!(value["state"], "active");
        assert_eq!(value["key_fingerprint"], "ab".repeat(32));
        assert_eq!(value["signed_at"], "2026-10-02T09:30:00Z");
        assert_eq!(value["policy"]["signed_at"], "2026-10-02T09:30:00Z");
        assert_eq!(value["policy"]["connections"]["remote"], "deny");
        assert!(value["policy"]["journal"].is_object());
    }
}
