//! Functions for working with [`Message`]s.
//!
//! The functions presented here can operate in one of two scopes: either on a
//! [`Mailbox`], or on a [`MailSession`]. The difference is that operations that
//! rely on the context of a mailbox/label view are performed on a mailbox, and
//! operations that are more global in nature are performed on a session. The
//! scope of the methods might change over time, but their primary association
//! of working with messages, and hence their placement in this module, won't.
//!

use crate::mail::datatypes::{Message, MessageSearchOptions};
use crate::mail::{MailSession, MailSessionError, Mailbox, MailboxError};
use itertools::Itertools;
use proton_mail_common::models::Message as RealMessage;
use stash::orm::Model;
use std::sync::Arc;

use super::datatypes::MessageAvailableAction;

/// Return the decrypted body of the specified message.
///
/// If the message body has never been fetched before, it will be retrieved from
/// the servers.
///
/// # Parameters
///
/// * `mailbox` - The mailbox to use for the request.
/// * `id`      - The local ID of the message to retrieve.
///
/// # Errors
///
/// Returns an error if the network request, the database query, reading/writing
/// the body to the cache, or decrypting the body fails.
///
#[uniffi::export]
pub async fn body(mailbox: Arc<Mailbox>, id: u64) -> Result<String, MailboxError> {
    RealMessage::load(id.into(), mailbox.stash())
        .await?
        .ok_or(MailboxError::MessageNotFound(id))
        // TODO: This might need to return a DecryptedMessageBody instead, but it's
        // TODO: not clear how to do that with the new cache functionality.
        // TODO: It might also be necessary to call Message.message_body() first.
        .map(|message| message.body)
}

/// Filter or search messages which match the specified options.
///
/// Note that search results are inserted into the database.
///
/// # Parameters
///
/// * `session` - The session to use for the request.
/// * `options` - The search options to use.
///
/// # Errors
///
/// Returns an error if the network request or database query fails.
///
#[uniffi::export]
pub async fn search_for_messages(
    session: Arc<MailSession>,
    options: MessageSearchOptions,
) -> Result<Vec<Message>, MailSessionError> {
    Ok(
        RealMessage::search(options.into(), session.api(), session.stash())
            .await?
            .into_iter()
            .map(Into::into)
            .collect(),
    )
}

/// Returns available actions for message
/// Any action returned here should impact current state of the message
/// and also should be available for the user to perform.
/// There is no need for any additional calculations before executing them.
///
/// # Parameters
///
/// * `session` - The session to use for the request.
/// * `id`      - The local ID of the message to retrieve.
///
/// # Errors
///
/// Returns an error if the database query fails.
///
#[uniffi::export]
pub async fn available_actions_for_message(
    session: Arc<MailSession>,
    id: u64,
) -> Result<Vec<MessageAvailableAction>, MailboxError> {
    if let Some(message) = RealMessage::load(id, session.stash()).await? {
        let actions = message
            .available_actions(session.stash())
            .await?
            .into_iter()
            .map_into()
            .collect();

        Ok(actions)
    } else {
        Ok(vec![])
    }
}
