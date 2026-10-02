//! Tests of the IMAP client against the fake server: the verification, its
//! typed failures, the read commands, and the record proving only reading
//! commands are sent.

use super::fake_server::{test_client, FakeImap, GOOD_PASSWORD};
use super::*;

/// Commands that change a mailbox; none may ever reach the server.
const WRITING_COMMANDS: &[&str] = &["SELECT", "STORE", "EXPUNGE", "COPY", "MOVE", "APPEND"];

fn verbs(commands: &[String]) -> Vec<String> {
    commands
        .iter()
        .map(|c| {
            let mut words = c.split_whitespace();
            let first = words.next().unwrap_or_default().to_ascii_uppercase();
            if first == "UID" {
                format!(
                    "UID {}",
                    words.next().unwrap_or_default().to_ascii_uppercase()
                )
            } else {
                first
            }
        })
        .filter(|v| v != "CAPABILITY")
        .collect()
}

#[tokio::test]
async fn test_check_logs_in_examines_the_inbox_and_logs_out() {
    // GIVEN a fake mailbox accepting the password
    let server = FakeImap::start().await;
    // WHEN the connection is checked
    let result = test_client().check(&server.account(), GOOD_PASSWORD).await;
    // THEN it succeeds over TLS with LOGIN, EXAMINE INBOX and LOGOUT
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(verbs(&server.commands()), ["LOGIN", "EXAMINE", "LOGOUT"]);
    assert!(server.commands().contains(&"EXAMINE \"INBOX\"".to_owned()));
}

#[tokio::test]
async fn test_refused_password_is_a_typed_failure() {
    // GIVEN a fake mailbox and a wrong password
    let server = FakeImap::start().await;
    // WHEN the connection is checked
    let err = test_client()
        .check(&server.account(), "wrong")
        .await
        .unwrap_err();
    // THEN the failure says the credentials were refused, and nothing was
    // opened
    assert_eq!(err.code(), "credentials_rejected");
    assert_eq!(verbs(&server.commands()), ["LOGIN"]);
}

#[tokio::test]
async fn test_closed_port_is_unreachable() {
    // GIVEN a loopback port nobody listens on
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let account = ImapAccount {
        email: "ana@example.org".into(),
        host: "localhost".into(),
        port,
        username: "ana@example.org".into(),
    };
    // WHEN the connection is checked
    let err = test_client()
        .check(&account, GOOD_PASSWORD)
        .await
        .unwrap_err();
    // THEN the host is reported unreachable
    assert_eq!(err.code(), "unreachable");
}

#[tokio::test]
async fn test_server_without_tls_is_a_tls_failure() {
    // GIVEN a server that answers in clear text on its port
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        use tokio::io::AsyncWriteExt;
        if let Ok((mut tcp, _)) = listener.accept().await {
            let _ = tcp.write_all(b"* OK plain text IMAP\r\n").await;
        }
    });
    let account = ImapAccount {
        email: "ana@example.org".into(),
        host: "localhost".into(),
        port,
        username: "ana@example.org".into(),
    };
    // WHEN the connection is checked
    let err = test_client()
        .check(&account, GOOD_PASSWORD)
        .await
        .unwrap_err();
    // THEN the failure is the secure connection
    assert_eq!(err.code(), "tls");
}

#[tokio::test]
async fn test_untrusted_certificate_is_a_tls_failure() {
    // GIVEN a fake mailbox, and a client trusting another authority only
    let server = FakeImap::start().await;
    let client = super::fake_server::client_trusting(super::tls_test_material::TEST_OTHER_CA_PEM);
    // WHEN the connection is checked
    let err = client
        .check(&server.account(), GOOD_PASSWORD)
        .await
        .unwrap_err();
    // THEN the test authority is not trusted, and no command was sent
    assert_eq!(err.code(), "tls");
    assert!(server.commands().is_empty());
}

#[tokio::test]
async fn test_failed_check_through_the_store_writes_nothing() {
    let _events = super::tests::EVENTS.lock().await;
    // GIVEN an empty store and a fake mailbox
    let server = FakeImap::start().await;
    let dir = tempfile::tempdir().unwrap();
    let secrets = super::tests::MemorySecrets::default();
    let store =
        ImapAccountStore::with_store(dir.path().join(IMAP_INDEX_FILE), Box::new(secrets.clone()));
    // WHEN the account is added with a refused password
    let err = store
        .add_verified(server.account(), "wrong", &test_client())
        .await
        .unwrap_err();
    // THEN nothing is stored, neither the list nor the password
    assert_eq!(err.code(), "credentials_rejected");
    assert!(!dir.path().join(IMAP_INDEX_FILE).exists());
    assert!(secrets.is_empty());
}

#[tokio::test]
async fn test_reading_sends_only_reading_commands() {
    // GIVEN a fake mailbox
    let server = FakeImap::start().await;
    let client = test_client();
    let account = server.account();
    let query = MailQuery {
        from: Some("zoe".into()),
        subject: Some("Invoice".into()),
        since: chrono::NaiveDate::from_ymd_opt(2026, 9, 1),
        before: chrono::NaiveDate::from_ymd_opt(2026, 11, 1),
        unread_only: true,
        ..MailQuery::default()
    };
    // WHEN every read tool runs once
    client.check(&account, GOOD_PASSWORD).await.unwrap();
    client.list_folders(&account, GOOD_PASSWORD).await.unwrap();
    client
        .search(&account, GOOD_PASSWORD, &query)
        .await
        .unwrap();
    client
        .read(&account, GOOD_PASSWORD, "INBOX", 7)
        .await
        .unwrap();
    // THEN the server received no writing command and no plain SELECT, and
    // every fetch peeked
    let commands = server.commands();
    for verb in verbs(&commands) {
        let bare = verb.trim_start_matches("UID ");
        assert!(
            !WRITING_COMMANDS.contains(&bare),
            "{verb} reached the server: {commands:?}"
        );
    }
    for fetch in commands.iter().filter(|c| c.contains("FETCH")) {
        assert!(fetch.contains("BODY.PEEK["), "{fetch}");
    }
    assert!(verbs(&commands).contains(&"EXAMINE".to_owned()));
}

#[tokio::test]
async fn test_list_folders_answers_every_folder() {
    // GIVEN a fake mailbox with an inbox and a folder of folders
    let server = FakeImap::start().await;
    // WHEN its folders are listed
    let folders = test_client()
        .list_folders(&server.account(), GOOD_PASSWORD)
        .await
        .unwrap();
    // THEN both come back, the second marked as not openable
    let names: Vec<_> = folders.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, ["INBOX", "Archive"]);
    assert!(!folders[0].no_select);
    assert!(folders[1].no_select);
    assert_eq!(folders[0].delimiter.as_deref(), Some("/"));
}

#[tokio::test]
async fn test_search_answers_summaries_newest_first() {
    // GIVEN a fake inbox with a read and an unread message
    let server = FakeImap::start().await;
    // WHEN every message is searched
    let found = test_client()
        .search(&server.account(), GOOD_PASSWORD, &MailQuery::default())
        .await
        .unwrap();
    // THEN both come back newest first, decoded, the unread one marked
    assert_eq!(found.iter().map(|m| m.uid).collect::<Vec<_>>(), [7, 3]);
    assert_eq!(found[0].subject.as_deref(), Some("Invoice for Zo\u{eb}"));
    assert_eq!(found[0].from.as_deref(), Some("Zo\u{eb} <zoe@example.org>"));
    assert!(found[0].unread);
    assert!(!found[1].unread);
    assert_eq!(found[1].date.as_deref(), Some("2026-10-01T09:00:00Z"));
}

#[tokio::test]
async fn test_search_for_unread_sends_unseen() {
    // GIVEN a fake inbox
    let server = FakeImap::start().await;
    let query = MailQuery {
        unread_only: true,
        ..MailQuery::default()
    };
    // WHEN only unread messages are searched
    let found = test_client()
        .search(&server.account(), GOOD_PASSWORD, &query)
        .await
        .unwrap();
    // THEN the criteria carry UNSEEN and only the unread message comes back
    assert!(server.commands().iter().any(|c| c == "UID SEARCH UNSEEN"));
    assert_eq!(found.iter().map(|m| m.uid).collect::<Vec<_>>(), [7]);
}

#[tokio::test]
async fn test_read_decodes_the_whole_message() {
    // GIVEN a fake inbox holding a message with an attachment
    let server = FakeImap::start().await;
    // WHEN it is read
    let message = test_client()
        .read(&server.account(), GOOD_PASSWORD, "INBOX", 7)
        .await
        .unwrap();
    // THEN sender, recipients, date, subject, text and attachment names are
    // answered, never the attachment's content
    assert_eq!(message.from.as_deref(), Some("Zo\u{eb} <zoe@example.org>"));
    assert_eq!(message.to, ["ana@example.org"]);
    assert_eq!(message.cc, ["Carl <carl@example.org>"]);
    assert_eq!(message.date.as_deref(), Some("2026-10-02T08:30:00+02:00"));
    assert_eq!(message.subject.as_deref(), Some("Invoice for Zo\u{eb}"));
    assert_eq!(message.body.trim(), "The invoice is attached.");
    assert!(!message.body_truncated);
    assert_eq!(message.attachments, ["invoice.pdf"]);
    assert!(!message.body.contains("JVBERi0"));
}

#[tokio::test]
async fn test_read_of_a_missing_message_or_folder_is_typed() {
    // GIVEN a fake inbox
    let server = FakeImap::start().await;
    let client = test_client();
    let account = server.account();
    // WHEN a missing message, then a missing folder, are read
    let missing = client
        .read(&account, GOOD_PASSWORD, "INBOX", 99)
        .await
        .unwrap_err();
    let folder = client
        .read(&account, GOOD_PASSWORD, "Nowhere", 7)
        .await
        .unwrap_err();
    // THEN each failure carries its own code
    assert_eq!(missing.code(), "message_not_found");
    assert_eq!(folder.code(), "folder_refused");
}
