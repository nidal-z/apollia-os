//! What the read tools ask a mailbox and what they answer: the search query,
//! a folder, a message summary and a whole message.
//!
//! Messages are decoded by `mail-parser`: headers with their encoded words,
//! the text body (the HTML part turned into text when there is no plain one),
//! and the names of the attachments, never their content.

use chrono::NaiveDate;
use mail_parser::{Address, MessageParser, MimeHeaders};
use serde::{Deserialize, Serialize};

use super::error::ImapError;

/// Folder searched when the query names none.
pub const DEFAULT_FOLDER: &str = "INBOX";

/// Messages answered by a search when the query sets no limit.
pub const DEFAULT_SEARCH_LIMIT: u32 = 20;

/// Most messages a search answers, whatever the query asks.
pub const MAX_SEARCH_LIMIT: u32 = 100;

/// Longest text body answered, in characters; the rest is cut.
pub const MAX_BODY_CHARS: usize = 100_000;

/// A folder of the mailbox.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MailFolder {
    /// Name to give back to a search or a read.
    pub name: String,
    /// Separator of the folder hierarchy, when the server has one.
    pub delimiter: Option<String>,
    /// The folder cannot be opened, it only holds other folders.
    pub no_select: bool,
}

/// What a search looks for. Every field is optional; together they narrow.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MailQuery {
    /// Folder to search; the inbox when absent.
    #[serde(default)]
    pub folder: Option<String>,
    /// Text the sender contains.
    #[serde(default)]
    pub from: Option<String>,
    /// Text the subject contains.
    #[serde(default)]
    pub subject: Option<String>,
    /// Received on or after this day.
    #[serde(default)]
    pub since: Option<NaiveDate>,
    /// Received before this day.
    #[serde(default)]
    pub before: Option<NaiveDate>,
    /// Only messages not yet read.
    #[serde(default)]
    pub unread_only: bool,
    /// Most messages answered, newest first; see [`DEFAULT_SEARCH_LIMIT`].
    #[serde(default)]
    pub limit: Option<u32>,
}

impl MailQuery {
    /// The folder the query searches.
    #[must_use]
    pub fn folder(&self) -> &str {
        self.folder
            .as_deref()
            .map(str::trim)
            .filter(|f| !f.is_empty())
            .unwrap_or(DEFAULT_FOLDER)
    }

    /// How many messages the query answers at most.
    #[must_use]
    pub fn limit(&self) -> usize {
        let limit = self
            .limit
            .unwrap_or(DEFAULT_SEARCH_LIMIT)
            .clamp(1, MAX_SEARCH_LIMIT);
        usize::try_from(limit).unwrap_or(usize::MAX)
    }

    /// The criteria of the IMAP `SEARCH` command for this query.
    ///
    /// # Errors
    /// [`ImapError::InvalidQuery`] when a text carries a line break, which
    /// would end the command early.
    pub fn criteria(&self) -> Result<String, ImapError> {
        let mut parts = Vec::new();
        let mut utf8 = false;
        for (key, value) in [("FROM", &self.from), ("SUBJECT", &self.subject)] {
            if let Some(value) = value.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
                utf8 |= !value.is_ascii();
                parts.push(format!("{key} {}", quote(value)?));
            }
        }
        if let Some(day) = self.since {
            parts.push(format!("SINCE {}", imap_date(day)));
        }
        if let Some(day) = self.before {
            parts.push(format!("BEFORE {}", imap_date(day)));
        }
        if self.unread_only {
            parts.push("UNSEEN".to_owned());
        }
        if parts.is_empty() {
            parts.push("ALL".to_owned());
        }
        let criteria = parts.join(" ");
        Ok(if utf8 {
            format!("CHARSET UTF-8 {criteria}")
        } else {
            criteria
        })
    }
}

/// How many messages a folder holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FolderCount {
    /// Every message of the folder.
    pub messages: u32,
    /// Those not yet read.
    pub unread: u32,
}

/// The messages of a folder a query matches, by their identifiers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderUids {
    /// Validity of the identifiers: a message is named for good by this
    /// value, its folder and its uid together.
    pub uid_validity: u32,
    /// Identifiers of the messages, newest first.
    pub uids: Vec<u32>,
}

/// A message as a search answers it: enough to choose which one to read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MailSummary {
    /// Identifier of the message in its folder, to give back to a read.
    pub uid: u32,
    /// Sender.
    pub from: Option<String>,
    /// Subject.
    pub subject: Option<String>,
    /// Date the sender wrote, RFC 3339.
    pub date: Option<String>,
    /// Not yet read.
    pub unread: bool,
}

/// A whole message, as the read tool answers it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MailMessage {
    /// Identifier of the message in its folder.
    pub uid: u32,
    /// Folder the message was read from.
    pub folder: String,
    /// Sender.
    pub from: Option<String>,
    /// Direct recipients.
    pub to: Vec<String>,
    /// Copied recipients.
    pub cc: Vec<String>,
    /// Date the sender wrote, RFC 3339.
    pub date: Option<String>,
    /// Subject.
    pub subject: Option<String>,
    /// Body as text.
    pub body: String,
    /// The body was longer than [`MAX_BODY_CHARS`] and was cut.
    pub body_truncated: bool,
    /// Names of the attached files. Their content is never read.
    pub attachments: Vec<String>,
}

/// Summarise a message from its raw header block.
pub(crate) fn summary_from_header(uid: u32, header: &[u8], unread: bool) -> MailSummary {
    let parsed = MessageParser::default().parse_headers(header);
    MailSummary {
        uid,
        from: parsed.as_ref().and_then(|m| first_address(m.from())),
        subject: parsed.as_ref().and_then(|m| m.subject().map(str::to_owned)),
        date: parsed
            .as_ref()
            .and_then(|m| m.date().map(mail_parser::DateTime::to_rfc3339)),
        unread,
    }
}

/// Decode a whole raw message.
///
/// # Errors
/// [`ImapError::Protocol`] when the bytes are not a message.
pub(crate) fn message_from_raw(
    uid: u32,
    folder: &str,
    raw: &[u8],
) -> Result<MailMessage, ImapError> {
    let parsed = MessageParser::default()
        .parse(raw)
        .ok_or_else(|| ImapError::Protocol {
            detail: format!("message {uid} could not be decoded"),
        })?;
    let text = parsed
        .body_text(0)
        .map(|t| t.into_owned())
        .unwrap_or_default();
    let body_truncated = text.chars().count() > MAX_BODY_CHARS;
    let body = if body_truncated {
        text.chars().take(MAX_BODY_CHARS).collect()
    } else {
        text
    };
    Ok(MailMessage {
        uid,
        folder: folder.to_owned(),
        from: first_address(parsed.from()),
        to: all_addresses(parsed.to()),
        cc: all_addresses(parsed.cc()),
        date: parsed.date().map(mail_parser::DateTime::to_rfc3339),
        subject: parsed.subject().map(str::to_owned),
        body,
        body_truncated,
        attachments: parsed
            .attachments()
            .filter_map(|part| part.attachment_name().map(str::to_owned))
            .collect(),
    })
}

fn first_address(address: Option<&Address<'_>>) -> Option<String> {
    all_addresses(address).into_iter().next()
}

fn all_addresses(address: Option<&Address<'_>>) -> Vec<String> {
    let Some(address) = address else {
        return Vec::new();
    };
    address
        .iter()
        .filter_map(|addr| match (addr.name(), addr.address()) {
            (Some(name), Some(email)) => Some(format!("{name} <{email}>")),
            (None, Some(email)) => Some(email.to_owned()),
            (Some(name), None) => Some(name.to_owned()),
            (None, None) => None,
        })
        .collect()
}

/// An IMAP quoted string. A line break would end the command, so it is
/// refused rather than escaped.
fn quote(value: &str) -> Result<String, ImapError> {
    if value.contains(['\r', '\n', '\0']) {
        return Err(ImapError::InvalidQuery {
            detail: "a search text cannot hold a line break".to_owned(),
        });
    }
    Ok(format!(
        "\"{}\"",
        value.replace('\\', "\\\\").replace('"', "\\\"")
    ))
}

/// A day as IMAP writes it: `2-Oct-2026`.
fn imap_date(day: NaiveDate) -> String {
    day.format("%-d-%b-%Y").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_query_searches_everything_in_the_inbox() {
        // GIVEN a query naming nothing
        let query = MailQuery::default();
        // WHEN its criteria, folder and limit are read
        // THEN everything in the inbox is searched, twenty at most
        assert_eq!(query.criteria().unwrap(), "ALL");
        assert_eq!(query.folder(), "INBOX");
        assert_eq!(query.limit(), 20);
    }

    #[test]
    fn test_every_field_becomes_a_criterion() {
        // GIVEN a query on sender, subject, period and unread
        let query = MailQuery {
            from: Some("bob".into()),
            subject: Some("say \"hi\"".into()),
            since: NaiveDate::from_ymd_opt(2026, 9, 1),
            before: NaiveDate::from_ymd_opt(2026, 10, 2),
            unread_only: true,
            ..MailQuery::default()
        };
        // WHEN its criteria are read
        let criteria = query.criteria().unwrap();
        // THEN each field is there, the quotes escaped and the days in IMAP form
        assert_eq!(
            criteria,
            "FROM \"bob\" SUBJECT \"say \\\"hi\\\"\" SINCE 1-Sep-2026 BEFORE 2-Oct-2026 UNSEEN"
        );
    }

    #[test]
    fn test_non_ascii_text_declares_its_charset() {
        // GIVEN a subject with an accent
        let query = MailQuery {
            subject: Some("Zo\u{eb}".into()),
            ..MailQuery::default()
        };
        // WHEN its criteria are read
        // THEN the charset is declared before them
        assert_eq!(
            query.criteria().unwrap(),
            "CHARSET UTF-8 SUBJECT \"Zo\u{eb}\""
        );
    }

    #[test]
    fn test_line_break_in_a_text_is_refused() {
        // GIVEN a sender text carrying a line break and a second command
        let query = MailQuery {
            from: Some("bob\r\nA1 STORE 1 +FLAGS (\\Deleted)".into()),
            ..MailQuery::default()
        };
        // WHEN its criteria are read
        // THEN they are refused, so no second command can be smuggled in
        assert_eq!(query.criteria().unwrap_err().code(), "invalid_query");
    }

    #[test]
    fn test_limit_is_bounded() {
        // GIVEN queries asking for none and for too many messages
        let none = MailQuery {
            limit: Some(0),
            ..MailQuery::default()
        };
        let many = MailQuery {
            limit: Some(10_000),
            ..MailQuery::default()
        };
        // WHEN their limits are read
        // THEN each is brought within one and the maximum
        assert_eq!(none.limit(), 1);
        assert_eq!(many.limit(), 100);
    }

    #[test]
    fn test_long_body_is_cut_and_marked() {
        // GIVEN a message whose body is longer than the cap
        let raw = format!(
            "From: a@example.org\r\nSubject: long\r\n\r\n{}",
            "x".repeat(MAX_BODY_CHARS + 10)
        );
        // WHEN it is decoded
        let message = message_from_raw(1, "INBOX", raw.as_bytes()).unwrap();
        // THEN the body is cut to the cap and says so
        assert!(message.body_truncated);
        assert_eq!(message.body.chars().count(), MAX_BODY_CHARS);
    }
}
