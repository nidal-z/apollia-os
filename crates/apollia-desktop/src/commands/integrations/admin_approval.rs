//! A Microsoft organisation that requires an administrator to approve the
//! application before its user can connect.
//!
//! The answer comes back on the loopback redirect or from the token exchange
//! (see `apollia_auth::admin_approval`). Either way the connection is kept
//! blocked by the organisation, with the address of the approval page, through
//! the connection state the runtime keeps and announces, and the dialog is
//! handed the same address to copy. A personal account consents for itself and
//! never receives that answer.
//!
//! A work or school account that refuses the consent (`AADSTS65004`) may have
//! left an administrator's approval unasked, which Microsoft does not tell
//! apart from a plain refusal: the dialog offers the same address to copy, as a
//! possibility, and the connection is not marked blocked. A personal account's
//! refusal carries no `AADSTS` code and stays a plain refusal.
//!
//! Attached to the daemon, the state is the daemon's and is not written from
//! the window; the dialog still shows the approval to send.

use std::path::{Path, PathBuf};

use apollia_auth::{AuthError, ConnectorProvider};
use apollia_core::EventBusSender;
use serde::Serialize;

/// What a refused consent says of an administrator's approval.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Approval {
    /// Microsoft answered that an administrator must approve: the connection
    /// is kept blocked by the organisation.
    Required,
    /// A work or school account refused the consent: an approval may be what
    /// is missing, offered and not asserted.
    Possible,
}

/// What the dialog is told when the loopback callback ended on `err`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CallbackOutcome {
    /// The approval page to offer, when one applies.
    pub address: Option<String>,
    /// How sure the answer is that an approval is missing.
    pub approval: Option<Approval>,
}

impl CallbackOutcome {
    /// Keep the connection blocked: only when an approval is required.
    pub(super) fn blocks(&self) -> bool {
        self.approval == Some(Approval::Required) && self.address.is_some()
    }

    /// The payload of the `oauth://error` event of consent `state`.
    pub(super) fn payload(&self, state: &str, err: &AuthError) -> serde_json::Value {
        serde_json::json!({
            "state": state,
            "error": err.to_string(),
            "admin_consent_url": self.address,
            "admin_approval": self.address.as_ref().and(self.approval),
        })
    }
}

/// What the callback error `err` of a consent with `provider` tells the
/// dialog, the approval page being `resolved`.
pub(super) fn callback_outcome(
    err: &AuthError,
    provider: Option<ConnectorProvider>,
    resolved: Option<String>,
) -> CallbackOutcome {
    let approval = match err {
        AuthError::AdminApprovalRequired(_) => Some(Approval::Required),
        AuthError::ConsentDeclinedInOrganisation(_) => Some(Approval::Possible),
        _ => None,
    };
    let address = approval_address(provider, approval.is_some(), resolved);
    CallbackOutcome {
        address: address.clone(),
        approval: address.and(approval),
    }
}

/// Where the state of a blocked connection is kept and announced: the data
/// directory and the bus of the embedded runtime.
#[derive(Clone)]
pub(super) struct StateSink {
    pub data_dir: PathBuf,
    pub bus: EventBusSender,
}

impl StateSink {
    /// The sink of the embedded runtime whose bus is `bus`, under the data
    /// directory of this session; `None` when no home resolves.
    pub(super) fn embedded(bus: &EventBusSender) -> Option<Self> {
        apollia_core::paths::data_dir().map(|data_dir| Self {
            data_dir,
            bus: bus.clone(),
        })
    }
}

/// The approval page to show when `provider` answered `approval`, `None` when
/// the answer is not an administrator approval of a Microsoft client or no
/// client resolves.
pub(super) fn approval_address(
    provider: Option<ConnectorProvider>,
    approval: bool,
    resolved: Option<String>,
) -> Option<String> {
    (approval && provider == Some(ConnectorProvider::Microsoft))
        .then_some(resolved)
        .flatten()
}

/// Keep `provider` blocked by its organisation, with `detail` and `address`,
/// and announce it. A failure is logged: the dialog still shows the approval.
pub(super) async fn keep_blocked(
    sink: Option<StateSink>,
    provider: ConnectorProvider,
    detail: String,
    address: String,
) {
    let Some(sink) = sink else {
        return;
    };
    let written = tokio::task::spawn_blocking(move || {
        apollia_runtime::connections::record_admin_approval_required(
            &sink.data_dir,
            &sink.bus,
            provider,
            Some(detail),
            &address,
        )
    })
    .await;
    log_failure(
        provider,
        "integrations.admin_approval.record_failed",
        written,
    );
}

/// An account of `provider` connected: lift a block kept for the provider.
pub(super) async fn lift_block(sink: Option<StateSink>, provider: ConnectorProvider) {
    let Some(sink) = sink else {
        return;
    };
    let written =
        tokio::task::spawn_blocking(move || lift(&sink.data_dir, &sink.bus, provider)).await;
    log_failure(provider, "integrations.admin_approval.lift_failed", written);
}

fn lift(
    data_dir: &Path,
    bus: &EventBusSender,
    provider: ConnectorProvider,
) -> Result<(), apollia_core::ConnectionStateError> {
    apollia_runtime::connections::lift_provider_block(data_dir, bus, provider)
}

fn log_failure(
    provider: ConnectorProvider,
    what: &'static str,
    written: Result<Result<(), apollia_core::ConnectionStateError>, tokio::task::JoinError>,
) {
    let error = match written {
        Ok(Ok(())) => return,
        Ok(Err(e)) => e.to_string(),
        Err(e) => e.to_string(),
    };
    tracing::event!(
        tracing::Level::WARN,
        provider = %provider.id(),
        operation = what,
        error = %error,
        "integrations.admin_approval.state_unwritten"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    const ADDRESS: &str =
        "https://login.microsoftonline.com/organizations/adminconsent?client_id=c4f95bc5";

    #[test]
    fn test_only_a_microsoft_approval_answer_with_a_client_gets_an_address() {
        // GIVEN the answers a connection can end on
        let resolved = || Some(ADDRESS.to_owned());

        // WHEN the address to show is decided for each
        let microsoft = approval_address(Some(ConnectorProvider::Microsoft), true, resolved());
        let declined = approval_address(Some(ConnectorProvider::Microsoft), false, resolved());
        let google = approval_address(Some(ConnectorProvider::Google), true, resolved());
        let no_client = approval_address(Some(ConnectorProvider::Microsoft), true, None);

        // THEN only the Microsoft approval answer shows one
        assert_eq!(microsoft.as_deref(), Some(ADDRESS));
        assert_eq!(declined, None);
        assert_eq!(google, None);
        assert_eq!(no_client, None);
    }

    /// Resolve the redirect `query` on a real loopback listener, as the
    /// browser would bring it back, and answer what the dialog is told. No
    /// Microsoft endpoint is reached: the redirect is written here.
    async fn outcome_of_redirect(query: &str) -> (CallbackOutcome, serde_json::Value) {
        let (listener, port) = apollia_auth::callback::bind_ephemeral_port().await.unwrap();
        let waiter = tokio::spawn(apollia_auth::callback::wait_for_callback(listener, "s1"));
        let _ = reqwest::get(format!("http://127.0.0.1:{port}/callback?state=s1&{query}")).await;
        let err = waiter.await.unwrap().unwrap_err();
        let outcome = callback_outcome(
            &err,
            Some(ConnectorProvider::Microsoft),
            Some(ADDRESS.to_owned()),
        );
        let payload = outcome.payload("s1", &err);
        (outcome, payload)
    }

    #[tokio::test]
    async fn test_a_work_account_that_refuses_is_offered_the_approval_without_being_blocked() {
        // GIVEN the refusal Microsoft Entra ID answers a work account
        let query =
            "error=access_denied&error_description=AADSTS65004%3A+User+declined+to+consent.";

        // WHEN the redirect comes back on the loopback
        let (outcome, payload) = outcome_of_redirect(query).await;

        // THEN the dialog is offered the approval page as a possibility, and
        // the connection is not kept blocked
        assert_eq!(outcome.approval, Some(Approval::Possible));
        assert!(!outcome.blocks());
        assert_eq!(payload["admin_consent_url"], ADDRESS);
        assert_eq!(payload["admin_approval"], "possible");
    }

    #[tokio::test]
    async fn test_a_personal_account_that_refuses_keeps_the_plain_refusal() {
        // GIVEN the refusal a personal account receives, with no AADSTS code
        let query = "error=access_denied&error_description=The+user+has+denied+access+to+the+\
                     scope+requested+by+the+client+application.";

        // WHEN the redirect comes back on the loopback
        let (outcome, payload) = outcome_of_redirect(query).await;

        // THEN nothing about an approval is said, and the refusal is shown as is
        assert_eq!(
            outcome,
            CallbackOutcome {
                address: None,
                approval: None
            }
        );
        assert!(payload["admin_consent_url"].is_null());
        assert!(payload["admin_approval"].is_null());
        assert!(payload["error"].as_str().unwrap().contains("denied access"));
    }

    #[tokio::test]
    async fn test_a_required_approval_blocks_the_connection() {
        // GIVEN Microsoft's answer that an administrator must approve
        let query = "error=consent_required&error_description=AADSTS90094%3A+Admin+permission.";

        // WHEN the redirect comes back on the loopback
        let (outcome, payload) = outcome_of_redirect(query).await;

        // THEN the connection is kept blocked and the dialog says it is
        assert_eq!(outcome.approval, Some(Approval::Required));
        assert!(outcome.blocks());
        assert_eq!(payload["admin_approval"], "required");
    }

    #[tokio::test]
    async fn test_a_blocked_connection_is_kept_announced_then_lifted_when_an_account_connects() {
        // GIVEN the sink of an embedded runtime over an empty data directory
        let dir = tempfile::tempdir().unwrap();
        let (bus, _keep) = tokio::sync::broadcast::channel(8);
        let mut events = bus.subscribe();
        let sink = StateSink {
            data_dir: dir.path().to_path_buf(),
            bus: bus.clone(),
        };

        // WHEN Microsoft is kept blocked, then an account of it connects
        keep_blocked(
            Some(sink.clone()),
            ConnectorProvider::Microsoft,
            "consent_required: AADSTS90094".to_owned(),
            ADDRESS.to_owned(),
        )
        .await;
        let blocked = events.recv().await.unwrap();
        lift_block(Some(sink), ConnectorProvider::Microsoft).await;
        let lifted = events.recv().await.unwrap();

        // THEN the block is announced with its address, then lifted
        match blocked {
            apollia_core::RuntimeEvent::ConnectionStateChanged {
                id, state, reason, ..
            } => {
                assert_eq!(id, "native:microsoft");
                assert_eq!(state, apollia_core::ConnectionStatus::Blocked);
                assert_eq!(reason.and_then(|r| r.address).as_deref(), Some(ADDRESS));
            }
            other => panic!("expected a state change, got {other:?}"),
        }
        assert!(matches!(
            lifted,
            apollia_core::RuntimeEvent::ConnectionStateChanged {
                state: apollia_core::ConnectionStatus::Working,
                ..
            }
        ));
    }
}
