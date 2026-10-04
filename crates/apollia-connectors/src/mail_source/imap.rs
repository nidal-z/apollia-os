//! The messages of a mailbox reached over IMAP.

use apollia_core::batch_source::SourceItem;

use super::{Cursor, MailFilter, Message, Page};
use crate::imap::{FolderUids, ImapAccount, ImapClient, ImapError, MailMessage, MailQuery};

/// Messages fetched per page.
const PAGE_SIZE: usize = 25;

/// A mailbox reached over IMAP, with what opens it.
#[derive(Clone)]
pub struct ImapMailbox {
    client: ImapClient,
    account: ImapAccount,
    password: String,
    /// Messages fetched per page; [`PAGE_SIZE`] outside tests.
    pub(super) page_size: usize,
}

impl std::fmt::Debug for ImapMailbox {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImapMailbox")
            .field("account", &self.account.email)
            .finish_non_exhaustive()
    }
}

impl ImapMailbox {
    /// The mailbox of `account`, read with `client` under `password`.
    #[must_use]
    pub fn new(client: ImapClient, account: ImapAccount, password: String) -> Self {
        Self {
            client,
            account,
            password,
            page_size: PAGE_SIZE,
        }
    }

    fn query(filter: &MailFilter) -> MailQuery {
        MailQuery {
            folder: filter.folder.clone(),
            since: filter.since,
            before: filter.before,
            unread_only: filter.unread_only,
            ..MailQuery::default()
        }
    }

    /// The count `STATUS` gives for the folder: its unread messages when the
    /// filter keeps those only, every message otherwise. A period narrows the
    /// messages read, not this count.
    pub(super) async fn estimated_count(&self, filter: &MailFilter) -> Result<u64, String> {
        let query = Self::query(filter);
        let count = self
            .client
            .folder_count(&self.account, &self.password, query.folder())
            .await
            .map_err(reason)?;
        Ok(u64::from(if filter.unread_only {
            count.unread
        } else {
            count.messages
        }))
    }

    pub(super) async fn page(&self, filter: &MailFilter, cursor: Cursor) -> Result<Page, String> {
        let query = Self::query(filter);
        let mut listed = match cursor {
            Cursor::Start => self
                .client
                .matching_uids(&self.account, &self.password, &query)
                .await
                .map_err(reason)?,
            Cursor::Imap(listed) => listed,
            Cursor::Outlook(_) | Cursor::End => {
                return Ok(Page {
                    items: Vec::new(),
                    next: Cursor::End,
                })
            }
        };
        let uid_validity = listed.uid_validity;
        let rest = listed
            .uids
            .split_off(listed.uids.len().min(self.page_size.max(1)));
        let messages = self
            .client
            .read_many(&self.account, &self.password, query.folder(), &listed)
            .await
            .map_err(reason)?;
        Ok(Page {
            items: messages
                .into_iter()
                .map(|message| message_item(uid_validity, message))
                .collect(),
            next: if rest.is_empty() {
                Cursor::End
            } else {
                Cursor::Imap(FolderUids {
                    uid_validity,
                    uids: rest,
                })
            },
        })
    }
}

/// The origin of a message read over IMAP: `imap:<UIDVALIDITY>:<uid>:<folder>`.
/// The folder comes last because its name may hold any character.
pub(super) fn origin(uid_validity: u32, uid: u32, folder: &str) -> String {
    format!("imap:{uid_validity}:{uid}:{folder}")
}

fn message_item(uid_validity: u32, message: MailMessage) -> SourceItem {
    Message {
        origin: origin(uid_validity, message.uid, &message.folder),
        from: message.from,
        recipients: message.to.into_iter().chain(message.cc).collect(),
        date: message.date,
        subject: message.subject,
        body: message.body,
        attachments: message.attachments,
    }
    .into_item()
}

fn reason(error: ImapError) -> String {
    error.to_string()
}
