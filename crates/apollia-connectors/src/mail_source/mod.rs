//! A mailbox as the source of a batch: one item per message.
//!
//! [`MailSource`] implements the batch source contract of `apollia-core` on a
//! mailbox reached over IMAP ([`ImapMailbox`]) or on an Outlook account
//! ([`OutlookMailbox`]). Both yield the same item: its origin names the
//! message for good (the folder with `UIDVALIDITY` and the uid over IMAP,
//! the folder with the Graph id for Outlook), and its fields are the sender,
//! the recipients, the date, the subject, the body as text and the names of
//! the attachments ([`FIELDS`]).
//!
//! A source is opened on a [`MailFilter`]: a folder, a period, unread
//! messages only. The count it announces is the server's own: the `STATUS`
//! of the folder over IMAP, the count Graph gives for the filter on Outlook.
//! Messages are then read page by page, as the batch asks for them.
//!
//! The source only reads. It has no writer, so a batch cannot write a result
//! back into a mailbox; over IMAP a folder is opened with `EXAMINE` and a
//! message fetched with `BODY.PEEK`, and Outlook is read with `GET` alone.
//!
//! The contract is synchronous while both mailboxes are read over the
//! network, so the source holds the handle of the Tokio runtime and blocks on
//! it. It is driven from a blocking task, as the batch engine does, never
//! from a task of the runtime itself.

mod imap;
mod outlook;

#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, VecDeque};
use std::io;
use std::path::PathBuf;

use apollia_core::batch_source::{BatchSource, OriginRef, SourceError, SourceItem, SourceItems};
use chrono::NaiveDate;
use tokio::runtime::Handle;

use crate::imap::FolderUids;

pub use imap::ImapMailbox;
pub use outlook::OutlookMailbox;

/// The kind under which a mailbox source is registered.
pub const MAIL_KIND: &str = "mail";

/// Option naming the folder; the inbox when absent.
pub const OPTION_FOLDER: &str = "folder";
/// Option keeping the messages received on or after a day, `YYYY-MM-DD`.
pub const OPTION_SINCE: &str = "since";
/// Option keeping the messages received before a day, `YYYY-MM-DD`.
pub const OPTION_BEFORE: &str = "before";
/// Option keeping the unread messages only, `true` or `false`.
pub const OPTION_UNREAD_ONLY: &str = "unread_only";

/// Field holding the sender.
pub const FIELD_FROM: &str = "from";
/// Field holding the recipients, direct then copied, comma separated.
pub const FIELD_TO: &str = "to";
/// Field holding the date, RFC 3339.
pub const FIELD_DATE: &str = "date";
/// Field holding the subject.
pub const FIELD_SUBJECT: &str = "subject";
/// Field holding the body as text.
pub const FIELD_BODY: &str = "body";
/// Field holding the names of the attachments, comma separated.
pub const FIELD_ATTACHMENTS: &str = "attachments";

/// The fields of a message, in display order.
pub const FIELDS: [&str; 6] = [
    FIELD_FROM,
    FIELD_TO,
    FIELD_DATE,
    FIELD_SUBJECT,
    FIELD_BODY,
    FIELD_ATTACHMENTS,
];

/// Which messages of a mailbox a source yields.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MailFilter {
    /// Folder read; the inbox when absent.
    pub folder: Option<String>,
    /// Received on or after this day.
    pub since: Option<NaiveDate>,
    /// Received before this day.
    pub before: Option<NaiveDate>,
    /// Only messages not yet read.
    pub unread_only: bool,
}

impl MailFilter {
    /// Reads the filter from the options of a source ([`OPTION_FOLDER`],
    /// [`OPTION_SINCE`], [`OPTION_BEFORE`], [`OPTION_UNREAD_ONLY`]).
    ///
    /// # Errors
    ///
    /// The reason, when a day is not written `YYYY-MM-DD` or the unread
    /// option is neither `true` nor `false`.
    pub fn from_options(options: &BTreeMap<String, String>) -> Result<Self, String> {
        let text = |key: &str| {
            options
                .get(key)
                .map(|value| value.trim())
                .filter(|value| !value.is_empty())
        };
        let day = |key: &str| {
            text(key)
                .map(|raw| {
                    NaiveDate::parse_from_str(raw, "%Y-%m-%d")
                        .map_err(|_| format!("the option '{key}' is not a day (YYYY-MM-DD): {raw}"))
                })
                .transpose()
        };
        let unread_only = match text(OPTION_UNREAD_ONLY) {
            None | Some("false") => false,
            Some("true") => true,
            Some(other) => {
                return Err(format!(
                    "the option '{OPTION_UNREAD_ONLY}' is neither true nor false: {other}"
                ))
            }
        };
        Ok(Self {
            folder: text(OPTION_FOLDER).map(str::to_owned),
            since: day(OPTION_SINCE)?,
            before: day(OPTION_BEFORE)?,
            unread_only,
        })
    }
}

/// The mailbox a source reads.
#[derive(Clone)]
pub enum Mailbox {
    /// A mailbox reached over IMAP.
    Imap(ImapMailbox),
    /// An Outlook account.
    Outlook(OutlookMailbox),
}

impl Mailbox {
    async fn estimated_count(&self, filter: &MailFilter) -> Result<u64, String> {
        match self {
            Self::Imap(mailbox) => mailbox.estimated_count(filter).await,
            Self::Outlook(mailbox) => mailbox.estimated_count(filter).await,
        }
    }

    async fn page(&self, filter: &MailFilter, cursor: Cursor) -> Result<Page, String> {
        match self {
            Self::Imap(mailbox) => mailbox.page(filter, cursor).await,
            Self::Outlook(mailbox) => mailbox.page(filter, cursor).await,
        }
    }
}

/// Where the reading of a mailbox stands.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Cursor {
    /// Nothing was read yet.
    Start,
    /// IMAP: the identifiers still to fetch, newest first, and the validity
    /// they were listed under.
    Imap(FolderUids),
    /// Outlook: the link of the next page.
    Outlook(String),
    /// Every message was read.
    End,
}

/// The messages of one page and where the next one starts.
struct Page {
    items: Vec<SourceItem>,
    next: Cursor,
}

/// A mailbox read as the items of a batch.
pub struct MailSource {
    location: String,
    fields: Vec<String>,
    mailbox: Mailbox,
    filter: MailFilter,
    runtime: Handle,
}

impl MailSource {
    /// The source reading `mailbox` through `filter`. `location` names the
    /// connection in the failures the source reports; `runtime` is the Tokio
    /// runtime the network calls run on.
    #[must_use]
    pub fn new(
        location: impl Into<String>,
        mailbox: Mailbox,
        filter: MailFilter,
        runtime: Handle,
    ) -> Self {
        Self {
            location: location.into(),
            fields: FIELDS.iter().map(|field| (*field).to_owned()).collect(),
            mailbox,
            filter,
            runtime,
        }
    }

    fn failure(&self, reason: String) -> SourceError {
        tracing::event!(
            tracing::Level::WARN,
            location = %self.location,
            error = %reason,
            "connectors.mail_source.read_failed"
        );
        SourceError::Io {
            path: PathBuf::from(&self.location),
            source: io::Error::other(reason),
        }
    }
}

impl BatchSource for MailSource {
    fn fields(&self) -> &[String] {
        &self.fields
    }

    fn items(&self) -> Result<SourceItems<'_>, SourceError> {
        let estimated_count = self
            .runtime
            .block_on(self.mailbox.estimated_count(&self.filter))
            .map_err(|reason| self.failure(reason))?;
        Ok(SourceItems {
            estimated_count,
            items: Box::new(Messages {
                source: self,
                cursor: Cursor::Start,
                page: VecDeque::new(),
            }),
        })
    }
}

/// The messages of a source, fetched one page at a time.
struct Messages<'a> {
    source: &'a MailSource,
    cursor: Cursor,
    page: VecDeque<SourceItem>,
}

impl Iterator for Messages<'_> {
    type Item = Result<SourceItem, SourceError>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(item) = self.page.pop_front() {
                return Some(Ok(item));
            }
            if self.cursor == Cursor::End {
                return None;
            }
            let cursor = std::mem::replace(&mut self.cursor, Cursor::End);
            let source = self.source;
            match source
                .runtime
                .block_on(source.mailbox.page(&source.filter, cursor))
            {
                Ok(page) => {
                    self.page = page.items.into();
                    self.cursor = page.next;
                }
                // The cursor stays at the end: a failed page ends the sequence.
                Err(reason) => return Some(Err(source.failure(reason))),
            }
        }
    }
}

/// One message, whichever mailbox it was read from.
struct Message {
    origin: String,
    from: Option<String>,
    recipients: Vec<String>,
    date: Option<String>,
    subject: Option<String>,
    body: String,
    attachments: Vec<String>,
}

impl Message {
    fn into_item(self) -> SourceItem {
        let fields = [
            (FIELD_FROM, self.from.unwrap_or_default()),
            (FIELD_TO, self.recipients.join(", ")),
            (FIELD_DATE, self.date.unwrap_or_default()),
            (FIELD_SUBJECT, self.subject.unwrap_or_default()),
            (FIELD_BODY, self.body),
            (FIELD_ATTACHMENTS, self.attachments.join(", ")),
        ];
        SourceItem {
            origin: OriginRef::new(self.origin),
            fields: fields
                .into_iter()
                .map(|(name, value)| (name.to_owned(), value))
                .collect(),
        }
    }
}
