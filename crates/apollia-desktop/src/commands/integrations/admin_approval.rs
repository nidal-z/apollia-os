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
//! Attached to the daemon, the state is the daemon's and is not written from
//! the window; the dialog still shows the approval to send.

use std::path::{Path, PathBuf};

use apollia_auth::ConnectorProvider;
use apollia_core::EventBusSender;

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
