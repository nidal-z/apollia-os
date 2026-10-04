//! Tests of the mailbox source: over IMAP against the fake server, on Outlook
//! against a simulated Graph.

use std::collections::BTreeMap;
use std::sync::Arc;

use apollia_core::batch_source::{BatchSource, SourceError, SourceItem, WriteSession};
use chrono::NaiveDate;
use tokio::runtime::Handle;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::outlook::TokenSource;
use super::*;
use crate::imap::fake_server::{test_client, FakeImap, GOOD_PASSWORD, UID_VALIDITY};
use crate::microsoft::mail::OutlookMailClient;
use crate::HttpClient;

/// What a batch reads from a source: its estimate, then every item.
type Reading = Result<(u64, Vec<Result<SourceItem, SourceError>>), SourceError>;

/// Reads `mailbox` through `filter` the way the batch engine does, from a
/// blocking task.
async fn read(mailbox: Mailbox, filter: MailFilter) -> Reading {
    let runtime = Handle::current();
    tokio::task::spawn_blocking(move || {
        let source = MailSource::new("mail:test", mailbox, filter, runtime);
        let items = source.items()?;
        Ok((items.estimated_count, items.items.collect()))
    })
    .await
    .unwrap()
}

fn imap_mailbox(server: &FakeImap, password: &str) -> ImapMailbox {
    ImapMailbox::new(test_client(), server.account(), password.to_owned())
}

fn outlook_mailbox(server: &MockServer) -> Mailbox {
    let client = OutlookMailClient::with_base_url(HttpClient::new("test").unwrap(), &server.uri());
    let token: TokenSource = Arc::new(|| Box::pin(async { Ok("token".to_owned()) }));
    Mailbox::Outlook(OutlookMailbox::with_token_source(client, token))
}

fn field<'a>(item: &'a SourceItem, name: &str) -> &'a str {
    item.fields.get(name).map_or("", String::as_str)
}

#[tokio::test(flavor = "multi_thread")]
async fn test_imap_yields_one_item_per_message_page_by_page_without_writing() {
    // GIVEN a fake inbox of two messages, read one message per page
    let server = FakeImap::start().await;
    let mut mailbox = imap_mailbox(&server, GOOD_PASSWORD);
    mailbox.page_size = 1;

    // WHEN the source is read
    let (estimated, items) = read(Mailbox::Imap(mailbox), MailFilter::default())
        .await
        .unwrap();
    let items: Vec<SourceItem> = items.into_iter().map(Result::unwrap).collect();

    // THEN the count is the folder's, both messages come newest first, each
    //      named by its folder, UIDVALIDITY and uid, with its six fields
    assert_eq!(estimated, 2);
    assert_eq!(items.len(), 2);
    assert_eq!(
        items[0].origin.as_str(),
        format!("imap:{UID_VALIDITY}:7:INBOX")
    );
    assert_eq!(
        items[1].origin.as_str(),
        format!("imap:{UID_VALIDITY}:3:INBOX")
    );
    assert_eq!(
        items[0]
            .fields
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["attachments", "body", "date", "from", "subject", "to"]
    );
    assert_eq!(field(&items[0], FIELD_FROM), "Zo\u{eb} <zoe@example.org>");
    assert_eq!(
        field(&items[0], FIELD_TO),
        "ana@example.org, Carl <carl@example.org>"
    );
    assert_eq!(field(&items[0], FIELD_DATE), "2026-10-02T08:30:00+02:00");
    assert_eq!(field(&items[0], FIELD_SUBJECT), "Invoice for Zo\u{eb}");
    assert!(field(&items[0], FIELD_BODY).contains("The invoice is attached."));
    assert_eq!(field(&items[0], FIELD_ATTACHMENTS), "invoice.pdf");
    assert_eq!(field(&items[1], FIELD_ATTACHMENTS), "");
    // AND the folder was counted by STATUS, read in two fetches, and never
    //     opened for writing
    let commands = server.commands();
    assert!(commands.contains(&"STATUS \"INBOX\" (MESSAGES UNSEEN)".to_owned()));
    let fetches = commands.iter().filter(|c| c.starts_with("UID FETCH"));
    assert_eq!(fetches.count(), 2);
    assert!(commands.iter().all(|c| {
        let verb = c.to_ascii_uppercase();
        !["SELECT", "STORE", "EXPUNGE", "COPY", "MOVE", "APPEND"]
            .iter()
            .any(|w| verb.starts_with(w) || verb.starts_with(&format!("UID {w}")))
    }));
}

#[tokio::test(flavor = "multi_thread")]
async fn test_imap_filters_by_folder_period_and_unread() {
    // GIVEN a fake inbox with one unread message, and a filter on the inbox,
    //       on a period and on the unread messages
    let server = FakeImap::start().await;
    let filter = MailFilter {
        folder: Some("INBOX".into()),
        since: NaiveDate::from_ymd_opt(2026, 10, 1),
        before: NaiveDate::from_ymd_opt(2026, 10, 3),
        unread_only: true,
    };

    // WHEN the source is read
    let (estimated, items) = read(Mailbox::Imap(imap_mailbox(&server, GOOD_PASSWORD)), filter)
        .await
        .unwrap();

    // THEN the count is the folder's unread count, the unread message alone
    //      comes back, and the search carried the period and the unread state
    assert_eq!(estimated, 1);
    assert_eq!(items.len(), 1);
    assert_eq!(
        items[0].as_ref().unwrap().origin.as_str(),
        format!("imap:{UID_VALIDITY}:7:INBOX")
    );
    assert!(server
        .commands()
        .contains(&"UID SEARCH SINCE 1-Oct-2026 BEFORE 3-Oct-2026 UNSEEN".to_owned()));
}

#[tokio::test(flavor = "multi_thread")]
async fn test_imap_failures_are_reported_by_the_source() {
    // GIVEN a fake mailbox, a wrong password, and a folder it does not hold
    let server = FakeImap::start().await;
    let absent = MailFilter {
        folder: Some("Nowhere".into()),
        ..MailFilter::default()
    };

    // WHEN the source is read with each
    let refused = read(
        Mailbox::Imap(imap_mailbox(&server, "wrong")),
        MailFilter::default(),
    )
    .await;
    let missing = read(Mailbox::Imap(imap_mailbox(&server, GOOD_PASSWORD)), absent).await;

    // THEN both fail as a source failure naming the connection, never the
    //      password
    for outcome in [refused, missing] {
        match outcome {
            Err(SourceError::Io { path, source }) => {
                assert_eq!(path.to_str(), Some("mail:test"));
                assert!(!source.to_string().contains("wrong"));
            }
            other => panic!("expected a source failure, got {:?}", other.map(|o| o.0)),
        }
    }
}

fn graph_message(id: &str, content_type: &str, content: &str) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "subject": format!("Subject {id}"),
        "from": { "emailAddress": { "name": "Bob", "address": "bob@example.org" } },
        "toRecipients": [{ "emailAddress": { "address": "ana@example.org" } }],
        "ccRecipients": [{ "emailAddress": { "name": "Carl", "address": "carl@example.org" } }],
        "receivedDateTime": "2026-10-02T06:30:00Z",
        "body": { "contentType": content_type, "content": content },
        "attachments": [{ "name": "invoice.pdf" }, { "name": "notes.txt" }]
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn test_outlook_yields_one_item_per_message_across_pages() {
    // GIVEN a simulated Graph whose inbox holds three messages on two pages
    let server = MockServer::start().await;
    let folder = "/mailFolders/inbox/messages";
    Mock::given(method("GET"))
        .and(path(folder))
        .and(query_param("$top", "1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "@odata.count": 3,
            "value": [graph_message("A1", "text", "one")]
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(folder))
        .and(query_param("$top", "50"))
        .and(query_param("$count", "true"))
        .and(query_param("$orderby", "receivedDateTime desc"))
        .and(query_param("$expand", "attachments($select=name)"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "@odata.count": 3,
            "@odata.nextLink": format!("{}{folder}?$skiptoken=p2", server.uri()),
            "value": [
                graph_message("A1", "html", "<p>Hello <b>Ana</b></p>"),
                graph_message("A2", "text", "two")
            ]
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(folder))
        .and(query_param("$skiptoken", "p2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "value": [graph_message("A3", "text", "three")]
        })))
        .mount(&server)
        .await;

    // WHEN the source is read
    let (estimated, items) = read(outlook_mailbox(&server), MailFilter::default())
        .await
        .unwrap();
    let items: Vec<SourceItem> = items.into_iter().map(Result::unwrap).collect();

    // THEN the count is Graph's, the three messages come in order, each named
    //      by its Graph id and folder, the HTML body turned into text
    assert_eq!(estimated, 3);
    let origins: Vec<&str> = items.iter().map(|i| i.origin.as_str()).collect();
    assert_eq!(
        origins,
        ["outlook:A1:inbox", "outlook:A2:inbox", "outlook:A3:inbox"]
    );
    assert_eq!(field(&items[0], FIELD_FROM), "Bob <bob@example.org>");
    assert_eq!(
        field(&items[0], FIELD_TO),
        "ana@example.org, Carl <carl@example.org>"
    );
    assert_eq!(field(&items[0], FIELD_DATE), "2026-10-02T06:30:00Z");
    assert_eq!(field(&items[0], FIELD_SUBJECT), "Subject A1");
    assert_eq!(field(&items[0], FIELD_BODY).trim(), "Hello Ana");
    assert_eq!(
        field(&items[0], FIELD_ATTACHMENTS),
        "invoice.pdf, notes.txt"
    );
    assert_eq!(field(&items[2], FIELD_BODY), "three");
    // AND nothing but GET was sent
    let requests = server.received_requests().await.unwrap();
    assert!(requests.iter().all(|r| r.method.as_str() == "GET"));
}

#[tokio::test(flavor = "multi_thread")]
async fn test_outlook_filters_by_folder_period_and_unread() {
    // GIVEN a simulated Graph answering only the archive folder filtered on
    //       a period and on the unread messages
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/mailFolders/archive/messages"))
        .and(query_param(
            "$filter",
            "receivedDateTime ge 2026-09-01T00:00:00Z and receivedDateTime lt \
             2026-10-02T00:00:00Z and isRead eq false",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "@odata.count": 1,
            "value": [graph_message("B1", "text", "late")]
        })))
        .mount(&server)
        .await;
    let filter = MailFilter {
        folder: Some("archive".into()),
        since: NaiveDate::from_ymd_opt(2026, 9, 1),
        before: NaiveDate::from_ymd_opt(2026, 10, 2),
        unread_only: true,
    };

    // WHEN the source is read
    let (estimated, items) = read(outlook_mailbox(&server), filter).await.unwrap();

    // THEN the filtered count and the filtered message come back
    assert_eq!(estimated, 1);
    assert_eq!(items.len(), 1);
    assert_eq!(
        items[0].as_ref().unwrap().origin.as_str(),
        "outlook:B1:archive"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_outlook_never_follows_a_page_link_to_another_server() {
    // GIVEN a simulated Graph whose first page links to another server
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/mailFolders/inbox/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "@odata.count": 2,
            "@odata.nextLink": "https://elsewhere.example/messages?$skiptoken=p2",
            "value": [graph_message("A1", "text", "one")]
        })))
        .mount(&server)
        .await;

    // WHEN the source is read
    let (_, items) = read(outlook_mailbox(&server), MailFilter::default())
        .await
        .unwrap();

    // THEN the first message comes, the link is refused as a failure, and
    //      the sequence ends there
    assert_eq!(items.len(), 2);
    assert!(items[0].is_ok());
    assert!(matches!(items[1], Err(SourceError::Io { .. })));
}

#[tokio::test(flavor = "multi_thread")]
async fn test_a_mailbox_source_cannot_be_written_to() {
    // GIVEN a source on an IMAP mailbox and one on an Outlook account
    let imap = FakeImap::start().await;
    let graph = MockServer::start().await;
    let runtime = Handle::current();
    let mut sources = [
        MailSource::new(
            "imap:ana@example.org",
            Mailbox::Imap(imap_mailbox(&imap, GOOD_PASSWORD)),
            MailFilter::default(),
            runtime.clone(),
        ),
        MailSource::new(
            "outlook:ana@example.org",
            outlook_mailbox(&graph),
            MailFilter::default(),
            runtime,
        ),
    ];

    for source in &mut sources {
        // WHEN its writer is asked for, and a write session opened on it
        let has_writer = source.writer().is_some();
        let session = WriteSession::open(source, chrono::Utc::now());

        // THEN it has none, and the session is refused as read-only
        assert!(!has_writer);
        assert!(matches!(session, Err(SourceError::NotWritable)));
    }
}

#[test]
fn test_the_filter_is_read_from_the_options() {
    // GIVEN the options of a source: all four, none, and two malformed ones
    let options = |pairs: &[(&str, &str)]| -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    };
    let full = options(&[
        (OPTION_FOLDER, "Clients"),
        (OPTION_SINCE, "2026-09-01"),
        (OPTION_BEFORE, "2026-10-02"),
        (OPTION_UNREAD_ONLY, "true"),
    ]);

    // WHEN each is read
    let filter = MailFilter::from_options(&full).unwrap();
    let empty = MailFilter::from_options(&BTreeMap::new()).unwrap();
    let bad_day = MailFilter::from_options(&options(&[(OPTION_SINCE, "yesterday")]));
    let bad_flag = MailFilter::from_options(&options(&[(OPTION_UNREAD_ONLY, "yes")]));

    // THEN the four filters are set, none by default, and a malformed option
    //      is refused with its name
    assert_eq!(
        filter,
        MailFilter {
            folder: Some("Clients".into()),
            since: NaiveDate::from_ymd_opt(2026, 9, 1),
            before: NaiveDate::from_ymd_opt(2026, 10, 2),
            unread_only: true,
        }
    );
    assert_eq!(empty, MailFilter::default());
    assert!(bad_day.unwrap_err().contains(OPTION_SINCE));
    assert!(bad_flag.unwrap_err().contains(OPTION_UNREAD_ONLY));
}
