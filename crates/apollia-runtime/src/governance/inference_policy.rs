//! The `[inference]` section of the organisation policy, read where an
//! inference server is added and where a request is sent to one.
//!
//! The runtime reads the policy once at boot; [`InferenceGovernance`] holds
//! that verdict. Without a policy every server is allowed, the user is free.
//! With a valid one:
//!
//! - a non-empty `allowed_servers` refuses every address it does not list;
//! - `remote_endpoints = "deny"` refuses every address outside this machine
//!   that `allowed_servers` does not list, so a server the organisation runs on
//!   its own network stays allowed when it is listed.
//!
//! A policy that is present but refused cannot be read, so every server is
//! refused: the decision fails closed.

use std::sync::Arc;

use apollia_core::config::PolicySettings;
use apollia_core::inference_server::openai_api_base;
use apollia_permissions::org_policy::DenyOnly;
use apollia_permissions::PolicyVerdict;

/// Reason of a refusal under a non-empty `inference.allowed_servers`.
pub const SERVER_NOT_ALLOWED_REASON: &str =
    "refused by the organisation policy: this inference server is not one the organisation allows";

/// Reason of a refusal under `inference.remote_endpoints = "deny"`.
pub const REMOTE_SERVER_DENIED_REASON: &str =
    "refused by the organisation policy: inference servers outside this machine are not allowed";

/// The `[inference]` section of the policy the runtime read at boot.
#[derive(Debug, Clone)]
pub struct InferenceGovernance {
    verdict: Arc<PolicyVerdict>,
}

impl Default for InferenceGovernance {
    /// No policy deposited: every server is allowed and nothing is fixed.
    fn default() -> Self {
        Self::new(Arc::new(PolicyVerdict::Absent))
    }
}

impl InferenceGovernance {
    /// The governance over `verdict`.
    #[must_use]
    pub fn new(verdict: Arc<PolicyVerdict>) -> Self {
        Self { verdict }
    }

    /// Whether the server at `base_url` may be added or reached: `Err` holds
    /// the reason of the refusal, for the user.
    ///
    /// # Errors
    ///
    /// The reason, when the policy refuses the address or cannot be read.
    pub fn server_decision(&self, base_url: &str) -> Result<(), String> {
        let valid = match self.verdict.as_ref() {
            PolicyVerdict::Absent => return Ok(()),
            PolicyVerdict::Invalid(invalid) => {
                return Err(format!(
                    "the organisation policy is refused ({}): {}",
                    invalid.reason, invalid.detail
                ))
            }
            PolicyVerdict::Valid(valid) => valid,
        };
        let inference = &valid.policy.inference;
        let address = normalized(base_url);
        if inference
            .allowed_servers
            .iter()
            .any(|allowed| normalized(allowed) == address)
        {
            return Ok(());
        }
        if !inference.allowed_servers.is_empty() {
            return Err(SERVER_NOT_ALLOWED_REASON.to_owned());
        }
        let remote = !apollia_core::net::is_loopback_host_str(&openai_api_base(base_url));
        if remote && inference.remote_endpoints == Some(DenyOnly::Deny) {
            return Err(REMOTE_SERVER_DENIED_REASON.to_owned());
        }
        Ok(())
    }

    /// The settings the policy fixes, `[settings]` and the import rule that
    /// `require_model_hash` fixes.
    #[must_use]
    pub fn imposed_settings(&self) -> PolicySettings {
        self.verdict.imposed_settings()
    }

    /// The verdict the runtime read at boot, the whole policy behind it.
    #[must_use]
    pub fn verdict(&self) -> &PolicyVerdict {
        &self.verdict
    }
}

/// An address as the policy compares it: the OpenAI base (`.../v1`) with its
/// scheme and host lowercased, so `HTTP://GPU.corp:8000` and
/// `http://gpu.corp:8000/v1/` designate one server.
fn normalized(address: &str) -> String {
    let base = openai_api_base(address);
    let Some((scheme, rest)) = base.split_once("://") else {
        return base;
    };
    let (authority, path) = rest.split_at(rest.find('/').unwrap_or(rest.len()));
    format!(
        "{}://{}{path}",
        scheme.to_ascii_lowercase(),
        authority.to_ascii_lowercase()
    )
}

#[cfg(test)]
pub(crate) fn valid_verdict_with_inference(inference: &str) -> Arc<PolicyVerdict> {
    let text =
        format!("schema_version = 1\nsigned_at = 2026-10-01T09:00:00Z\n[inference]\n{inference}\n");
    let policy = apollia_permissions::parse_policy(&text).expect("policy parses");
    Arc::new(PolicyVerdict::Valid(Box::new(
        apollia_permissions::ValidPolicy {
            signed_at: policy.signed_at,
            policy,
            key_fingerprint: "00".into(),
            publisher: "it".into(),
        },
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn governance(inference: &str) -> InferenceGovernance {
        InferenceGovernance::new(valid_verdict_with_inference(inference))
    }

    #[test]
    fn test_without_a_policy_every_server_is_allowed() {
        // GIVEN no deposited policy
        let governance = InferenceGovernance::default();

        // WHEN a cloud address is submitted
        let decision = governance.server_decision("https://api.example.com");

        // THEN it is allowed
        assert_eq!(decision, Ok(()));
    }

    #[test]
    fn test_allowed_servers_refuses_an_address_it_does_not_list() {
        // GIVEN a policy listing one server of the organisation
        let governance = governance("allowed_servers = [\"http://gpu.corp.example:8000\"]");

        // WHEN that server, written differently, and another one are submitted
        let listed = governance.server_decision("HTTP://GPU.corp.example:8000/v1/");
        let other = governance.server_decision("http://127.0.0.1:11434");

        // THEN the listed one is allowed and the other refused by the policy
        assert_eq!(listed, Ok(()));
        assert_eq!(other, Err(SERVER_NOT_ALLOWED_REASON.to_owned()));
    }

    #[test]
    fn test_remote_deny_refuses_an_unlisted_server_off_this_machine() {
        // GIVEN a policy refusing remote endpoints and listing no server
        let governance = governance("remote_endpoints = \"deny\"");

        // WHEN a cloud address, a LAN address and a loopback address are submitted
        let cloud = governance.server_decision("https://api.example.com/v1");
        let lan = governance.server_decision("http://192.168.1.20:8000");
        let local = governance.server_decision("http://localhost:11434");

        // THEN only the loopback one is allowed
        assert_eq!(cloud, Err(REMOTE_SERVER_DENIED_REASON.to_owned()));
        assert_eq!(lan, Err(REMOTE_SERVER_DENIED_REASON.to_owned()));
        assert_eq!(local, Ok(()));
    }

    #[test]
    fn test_remote_deny_allows_a_listed_server_of_the_organisation() {
        // GIVEN a policy refusing remote endpoints that lists one server
        let governance = governance(
            "remote_endpoints = \"deny\"\nallowed_servers = [\"http://gpu.corp.example:8000\"]",
        );

        // WHEN that server is submitted
        let decision = governance.server_decision("http://gpu.corp.example:8000");

        // THEN it is allowed
        assert_eq!(decision, Ok(()));
    }

    #[test]
    fn test_a_refused_policy_refuses_every_server() {
        // GIVEN a present policy refused for its signature
        let governance = InferenceGovernance::new(Arc::new(PolicyVerdict::Invalid(
            apollia_permissions::PolicyInvalid {
                reason: apollia_permissions::PolicyInvalidReason::Signature,
                detail: "bad signature".into(),
            },
        )));

        // WHEN a loopback server is submitted
        let decision = governance.server_decision("http://127.0.0.1:8080");

        // THEN it is refused, naming the policy
        assert!(decision.is_err_and(|reason| reason.contains("organisation policy")));
    }
}
