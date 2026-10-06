//! The mailbox kind of the batch sources: a batch launched on a mailbox
//! connection reads one item per message.
//!
//! The location of the source names the connection: `imap:<address>` for a
//! mailbox connected over IMAP, `outlook:<account>` for a Microsoft account.
//! Its options are the filters of
//! [`apollia_connectors::mail_source::MailFilter`]: folder, period, unread
//! only.
//!
//! A mailbox is only read. A launch asking to write its results back into
//! the source is refused when the source is opened, before any connection is
//! made. A connection the organisation policy forbids is refused there too,
//! before its password or its token is read, through the admission a
//! connection goes through.

use std::io;
use std::path::PathBuf;

use apollia_auth::{AccountId, ConnectorProvider};
use apollia_connectors::imap::{ImapAccountStore, ImapClient};
use apollia_connectors::mail_source::{
    ImapMailbox, MailFilter, MailSource, Mailbox, OutlookMailbox, MAIL_KIND,
};
use apollia_core::batch_source::{BatchSource, SourceError, SourceRegistry, SourceSpec};
use tokio::runtime::Handle;

use super::imap::deposited_policy;
use super::microsoft::get_microsoft_connector;
use crate::governance::{
    mailbox_refusal, AdmissionDecision, AdmissionRequest, ConnectionAdmission,
    FORBIDDEN_CONNECTION_REASON,
};

/// Prefix of the location of a mailbox connected over IMAP.
pub const IMAP_LOCATION: &str = "imap:";
/// Prefix of the location of an Outlook account.
pub const OUTLOOK_LOCATION: &str = "outlook:";

/// Registers the mailbox kind ([`MAIL_KIND`]).
///
/// # Errors
///
/// [`SourceError::DuplicateKind`] when the kind is already registered.
pub fn register_mail_source(registry: &mut SourceRegistry) -> Result<(), SourceError> {
    registry.register(
        MAIL_KIND,
        Box::new(|spec: &SourceSpec| {
            let data_dir = apollia_core::paths::data_dir()
                .ok_or_else(|| refused(spec, "the home directory cannot be resolved"))?;
            let store =
                ImapAccountStore::open(&data_dir).map_err(|e| refused(spec, &e.to_string()))?;
            open(spec, &store, &deposited_policy())
        }),
    )
}

/// Opens the mailbox `spec` names, once `admission` admits its connection.
fn open(
    spec: &SourceSpec,
    store: &ImapAccountStore,
    admission: &dyn ConnectionAdmission,
) -> Result<Box<dyn BatchSource>, SourceError> {
    if spec
        .options
        .contains_key(apollia_sources::OPTION_DESTINATION)
    {
        return Err(SourceError::NotWritable);
    }
    let location = spec.location.trim();
    let imap = location.strip_prefix(IMAP_LOCATION).map(str::trim);
    let outlook = location.strip_prefix(OUTLOOK_LOCATION).map(str::trim);
    let filter = MailFilter::from_options(&spec.options).map_err(|reason| SourceError::Io {
        path: PathBuf::from(&spec.location),
        source: io::Error::new(io::ErrorKind::InvalidInput, reason),
    })?;
    let runtime = Handle::try_current().map_err(|e| refused(spec, &e.to_string()))?;
    let mailbox = match (imap, outlook) {
        (Some(email), _) if !email.is_empty() => {
            let account = store
                .get(email)
                .map_err(|e| refused(spec, &e.to_string()))?;
            if let Some(reason) = mailbox_refusal(admission, &account) {
                return Err(forbidden(spec, reason));
            }
            let password = store
                .password(&account.email)
                .map_err(|e| refused(spec, &e.to_string()))?;
            let client =
                ImapClient::with_system_roots().map_err(|e| refused(spec, &e.to_string()))?;
            Mailbox::Imap(ImapMailbox::new(client, account, password))
        }
        (_, Some(account)) if !account.is_empty() => {
            let request = AdmissionRequest::for_native_account(ConnectorProvider::Microsoft);
            if let Ok(AdmissionDecision::Refused { .. }) = admission.read_decision(&request) {
                return Err(forbidden(spec, FORBIDDEN_CONNECTION_REASON));
            }
            let connector = runtime
                .block_on(get_microsoft_connector())
                .map_err(|e| refused(spec, &e))?;
            Mailbox::Outlook(OutlookMailbox::new(connector, AccountId::new(account)))
        }
        _ => {
            return Err(SourceError::InvalidLocation {
                location: spec.location.clone(),
            })
        }
    };
    Ok(Box::new(MailSource::new(
        location, mailbox, filter, runtime,
    )))
}

fn refused(spec: &SourceSpec, reason: &str) -> SourceError {
    SourceError::Io {
        path: PathBuf::from(&spec.location),
        source: io::Error::other(reason.to_owned()),
    }
}

fn forbidden(spec: &SourceSpec, reason: &str) -> SourceError {
    tracing::event!(
        tracing::Level::WARN,
        location = %spec.location,
        "governance.connection.forbidden.source_refused"
    );
    SourceError::Io {
        path: PathBuf::from(&spec.location),
        source: io::Error::new(io::ErrorKind::PermissionDenied, reason.to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::governance::policy_admission::valid_verdict_with_connections;
    use crate::governance::PolicyAdmission;

    /// A secret store holding nothing.
    struct NoSecrets;

    impl apollia_auth::SecretStore for NoSecrets {
        fn set(&self, _: &str, _: &str, _: &str) -> Result<(), apollia_auth::AuthError> {
            Ok(())
        }
        fn get(&self, _: &str, _: &str) -> Result<Option<String>, apollia_auth::AuthError> {
            Ok(None)
        }
        fn delete(&self, _: &str, _: &str) -> Result<(), apollia_auth::AuthError> {
            Ok(())
        }
        fn backend_id(&self) -> &'static str {
            "none"
        }
    }

    /// A store holding the one mailbox `eve@example.com`, without password.
    fn store(dir: &std::path::Path) -> ImapAccountStore {
        let index = dir.join(apollia_connectors::imap::IMAP_INDEX_FILE);
        std::fs::write(
            &index,
            r#"{"accounts":[
                {"email":"eve@example.com","host":"imap.example.com","port":993,"username":"eve"}]}"#,
        )
        .unwrap();
        ImapAccountStore::with_store(index, Box::new(NoSecrets))
    }

    fn spec(location: &str, options: &[(&str, &str)]) -> SourceSpec {
        SourceSpec {
            location: location.to_owned(),
            options: options
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect(),
        }
    }

    fn admit() -> PolicyAdmission {
        PolicyAdmission::new(valid_verdict_with_connections(""))
    }

    fn failure(outcome: Result<Box<dyn BatchSource>, SourceError>) -> SourceError {
        match outcome {
            Err(error) => error,
            Ok(_) => panic!("the source opened"),
        }
    }

    #[test]
    fn test_the_mailbox_kind_is_registered_once() {
        // GIVEN an empty registry of batch sources
        let mut registry = SourceRegistry::new();
        // WHEN the mailbox kind is registered, twice
        let first = register_mail_source(&mut registry);
        let second = register_mail_source(&mut registry);
        // THEN the kind is there, and the second registration is refused
        assert!(first.is_ok());
        assert_eq!(registry.kinds(), [MAIL_KIND]);
        assert!(matches!(second, Err(SourceError::DuplicateKind { .. })));
    }

    #[tokio::test]
    async fn test_the_write_option_is_refused_when_the_source_is_opened() {
        // GIVEN a launch on an IMAP mailbox and one on an Outlook account,
        //       each asking to write the results back
        let dir = tempfile::tempdir().unwrap();
        let store = store(dir.path());
        for location in ["imap:eve@example.com", "outlook:eve@example.com"] {
            for destination in ["copy", "original"] {
                let launch = spec(
                    location,
                    &[(apollia_sources::OPTION_DESTINATION, destination)],
                );
                // WHEN the source is opened
                let error = failure(open(&launch, &store, &admit()));
                // THEN it is refused as read-only, before anything is read
                assert!(
                    matches!(error, SourceError::NotWritable),
                    "{location} {destination}: {error}"
                );
            }
        }
    }

    #[tokio::test]
    async fn test_a_forbidden_connection_is_refused_before_anything_is_read() {
        // GIVEN one mailbox without password, a policy denying the mailboxes
        //       and Microsoft accounts, and one admitting everything
        let dir = tempfile::tempdir().unwrap();
        let store = store(dir.path());
        let deny = PolicyAdmission::new(valid_verdict_with_connections(
            "deny = [\"native:imap\", \"native:microsoft\"]",
        ));

        // WHEN a source is opened on the mailbox and on an Outlook account
        //      under the first, and on the mailbox under the second
        let imap = failure(open(&spec("imap:eve@example.com", &[]), &store, &deny));
        let outlook = failure(open(&spec("outlook:eve@example.com", &[]), &store, &deny));
        let admitted = failure(open(&spec("imap:eve@example.com", &[]), &store, &admit()));

        // THEN the policy refuses both; the admitted mailbox goes past the
        //      gate and stops on its missing password
        for error in [imap, outlook] {
            match error {
                SourceError::Io { source, .. } => {
                    assert_eq!(source.kind(), io::ErrorKind::PermissionDenied);
                    assert_eq!(source.to_string(), FORBIDDEN_CONNECTION_REASON);
                }
                other => panic!("{other}"),
            }
        }
        match admitted {
            SourceError::Io { source, .. } => {
                assert_ne!(source.kind(), io::ErrorKind::PermissionDenied);
            }
            other => panic!("{other}"),
        }
    }

    #[tokio::test]
    async fn test_a_location_or_a_filter_that_cannot_be_read_is_refused() {
        // GIVEN locations naming no connection, an unknown mailbox, and a
        //       filter whose day is malformed
        let dir = tempfile::tempdir().unwrap();
        let store = store(dir.path());

        // WHEN each is opened
        let bare = failure(open(&spec("eve@example.com", &[]), &store, &admit()));
        let empty = failure(open(&spec("imap:", &[]), &store, &admit()));
        let unknown = failure(open(&spec("imap:bob@example.com", &[]), &store, &admit()));
        let bad_day = failure(open(
            &spec("imap:eve@example.com", &[("since", "soon")]),
            &store,
            &admit(),
        ));

        // THEN the first two are not locations, the third is no connection,
        //      and the last names the option
        assert!(matches!(bare, SourceError::InvalidLocation { .. }));
        assert!(matches!(empty, SourceError::InvalidLocation { .. }));
        assert!(matches!(unknown, SourceError::Io { .. }));
        match bad_day {
            SourceError::Io { source, .. } => {
                assert_eq!(source.kind(), io::ErrorKind::InvalidInput);
                assert!(source.to_string().contains("since"));
            }
            other => panic!("{other}"),
        }
    }
}
