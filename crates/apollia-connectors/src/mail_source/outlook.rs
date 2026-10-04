//! The messages of an Outlook account.

use std::sync::Arc;

use apollia_auth::{AccountId, MicrosoftScope};
use futures::future::BoxFuture;
use mail_parser::decoders::html::html_to_text;

use apollia_core::batch_source::SourceItem;

use super::{Cursor, MailFilter, Message, Page};
use crate::error::ConnectorError;
use crate::microsoft::mail::{
    FolderFilter, FolderMessage, FolderPageRequest, OutlookMailClient, Recipient, FOLDER_PAGE_SIZE,
};
use crate::microsoft::MicrosoftConnector;

/// Answers the token of the account a mailbox reads, fresh at each call.
pub(super) type TokenSource =
    Arc<dyn Fn() -> BoxFuture<'static, Result<String, ConnectorError>> + Send + Sync>;

/// An Outlook account, read through the mail client of the Microsoft
/// connector.
#[derive(Clone)]
pub struct OutlookMailbox {
    client: OutlookMailClient,
    token: TokenSource,
}

impl OutlookMailbox {
    /// The mailbox of `account`, read through `connector` with the scope that
    /// reads mail and no other.
    #[must_use]
    pub fn new(connector: Arc<MicrosoftConnector>, account: AccountId) -> Self {
        let client = connector.mail().clone();
        let token: TokenSource = Arc::new(move || {
            let connector = connector.clone();
            let account = account.clone();
            Box::pin(async move {
                connector
                    .bearer(&account, &[MicrosoftScope::MailRead])
                    .await
            })
        });
        Self { client, token }
    }

    /// The mailbox `client` reads with the tokens of `token`, for the tests
    /// that read a simulated server.
    #[cfg(test)]
    pub(super) fn with_token_source(client: OutlookMailClient, token: TokenSource) -> Self {
        Self { client, token }
    }

    fn folder_filter(filter: &MailFilter) -> FolderFilter {
        FolderFilter {
            folder: filter.folder.clone(),
            since: filter.since,
            before: filter.before,
            unread_only: filter.unread_only,
        }
    }

    /// The count Graph gives for the filter, asked with a page of one message.
    pub(super) async fn estimated_count(&self, filter: &MailFilter) -> Result<u64, String> {
        let bearer = (self.token)().await.map_err(reason)?;
        let page = self
            .client
            .folder_page(
                &Self::folder_filter(filter),
                FolderPageRequest::First { top: 1 },
                &bearer,
                &*self.token,
            )
            .await
            .map_err(reason)?;
        Ok(page
            .total
            .unwrap_or_else(|| u64::try_from(page.messages.len()).unwrap_or(u64::MAX)))
    }

    pub(super) async fn page(&self, filter: &MailFilter, cursor: Cursor) -> Result<Page, String> {
        let request = match &cursor {
            Cursor::Start => FolderPageRequest::First {
                top: FOLDER_PAGE_SIZE,
            },
            Cursor::Outlook(link) => FolderPageRequest::Next(link),
            Cursor::Imap(_) | Cursor::End => {
                return Ok(Page {
                    items: Vec::new(),
                    next: Cursor::End,
                })
            }
        };
        let folder_filter = Self::folder_filter(filter);
        let bearer = (self.token)().await.map_err(reason)?;
        let page = self
            .client
            .folder_page(&folder_filter, request, &bearer, &*self.token)
            .await
            .map_err(reason)?;
        Ok(Page {
            items: page
                .messages
                .into_iter()
                .map(|message| message_item(folder_filter.folder(), message))
                .collect(),
            next: page.next_link.map_or(Cursor::End, Cursor::Outlook),
        })
    }
}

/// The origin of an Outlook message: `outlook:<Graph id>:<folder>`. A Graph
/// id holds no colon; the folder comes last.
pub(super) fn origin(id: &str, folder: &str) -> String {
    format!("outlook:{id}:{folder}")
}

fn message_item(folder: &str, message: FolderMessage) -> SourceItem {
    let body = message.body.map_or_else(String::new, |body| {
        if body.content_type.eq_ignore_ascii_case("html") {
            html_to_text(&body.content)
        } else {
            body.content
        }
    });
    Message {
        origin: origin(&message.id, folder),
        from: message.from.as_ref().map(address),
        recipients: message
            .to_recipients
            .iter()
            .chain(&message.cc_recipients)
            .map(address)
            .collect(),
        date: message.received_date_time,
        subject: message.subject,
        body,
        attachments: message
            .attachments
            .into_iter()
            .filter_map(|attachment| attachment.name)
            .collect(),
    }
    .into_item()
}

/// A recipient as the IMAP side writes one: `Name <address>`, or the address.
fn address(recipient: &Recipient) -> String {
    let email = &recipient.email_address;
    match email.name.as_deref().filter(|name| !name.is_empty()) {
        Some(name) => format!("{name} <{}>", email.address),
        None => email.address.clone(),
    }
}

fn reason(error: ConnectorError) -> String {
    error.to_string()
}
