//! Functions for working with [`Conversation`]s.
//!
//! The functions presented here can operate in one of two scopes: either on a
//! [`Mailbox`], or on a [`MailSession`]. The difference is that operations that
//! rely on the context of a mailbox/label view are performed on a mailbox, and
//! operations that are more global in nature are performed on a session. The
//! scope of the methods might change over time, but their primary association
//! of working with conversations, and hence their placement in this module,
//! won't.
//!

use crate::core::datatypes::Id;
use crate::mail::datatypes::{
    Conversation, ConversationAvailableAction, ConversationSearchOptions, Message,
};
use crate::mail::{MailSessionError, MailUserSession, Mailbox, MailboxError};
use crate::{uniffi_async, watch, LiveQueryCallback, WatchHandle};
use indoc::formatdoc;
use itertools::Itertools;
use proton_api_core::session::CoreSession;
use proton_core_common::datatypes::LocalId as RealLocalId;
use proton_mail_common::datatypes::ContextualConversation;
use proton_mail_common::models::{
    Conversation as RealConversation, Label as RealLabel, Message as RealMessage,
};
use stash::orm::Model;
use stash::params;
use std::sync::Arc;

/// Label the given conversations with the given label id.
///
/// # Parameters
///
/// * `session`  - The session to use for the request.
/// * `label_id` - The local ID of the label to apply.
/// * `ids`      - The local IDs of the conversations to apply the label to.
///
/// # Errors
///
/// Returns an error if the database query fails.
///
#[uniffi::export]
pub async fn apply_label_to_conversations(
    session: Arc<MailUserSession>,
    label_id: Id,
    ids: Vec<Id>,
) -> Result<(), MailboxError> {
    let conn = session.user_stash().connection();
    uniffi_async(async move {
        Ok(
            RealConversation::apply_label(label_id.into(), ids.into_iter().map(Into::into), &conn)
                .await?,
        )
    })
    .await
}

/// Delete the given conversations.
///
/// # Parameters
///
/// * `mailbox` - The mailbox to use for the request.
/// * `ids`     - The local IDs of the conversations to delete.
///
/// # Errors
///
/// Returns an error if the database query fails.
///
#[uniffi::export]
pub async fn delete_conversations(mailbox: Arc<Mailbox>, ids: Vec<Id>) -> Result<(), MailboxError> {
    let conn = mailbox.stash().connection();
    uniffi_async(async move {
        RealConversation::delete_multiple_from_label(
            ids.into_iter().map(Into::into).collect(),
            mailbox.label_id().into(),
            &conn,
        )
        .await?;
        Ok(())
    })
    .await
}

/// Returns available actions for conversation.
/// Any action returned here should impact current state of the conversation
/// and also should be available for the user to perform.
/// There is no need for any additional calculations before executing them.
///
/// # Parameters
///
/// * `session` - The session to use for the request.
/// * `id`      - The local ID of the conversation to retrieve.
///
/// # Errors
///
/// Returns an error if the database query fails.
///
#[uniffi::export]
pub async fn available_actions_for_conversation(
    session: Arc<MailUserSession>,
    id: Id,
) -> Result<Vec<ConversationAvailableAction>, MailboxError> {
    let conn = session.user_stash().connection();
    uniffi_async(async move {
        if let Some(conversation) = RealConversation::load(id.into(), &conn).await? {
            let actions = conversation
                .available_actions(session.user_stash())
                .await?
                .into_iter()
                .map_into()
                .collect();

            Ok(actions)
        } else {
            Ok(vec![])
        }
    })
    .await
}

/// Get a specified conversation.
///
/// # Parameters
///
/// * `mailbox` - The mailbox to use for the request.
/// * `id`      - The local ID of the conversation to get.
///
/// # Errors
///
/// Returns an error if the database query fails.
///
#[allow(clippy::missing_panics_doc)]
#[uniffi::export]
pub async fn conversation(
    mailbox: Arc<Mailbox>,
    id: Id,
) -> Result<Option<Conversation>, MailboxError> {
    let conn = mailbox.stash().connection();
    uniffi_async(async move {
        Ok(ContextualConversation::new(
            RealConversation::load(id.into(), &conn).await?.unwrap(),
            mailbox.label_id().into(),
        )
        .map(Into::into))
    })
    .await
}

/// Get conversations for the given label.
///
/// # Parameters
///
/// * `session`  - The session to use for the request.
/// * `label_id` - The local ID of the label to get conversations for.
///
/// # Errors
///
/// Returns an error if the database query fails.
///
#[allow(clippy::missing_panics_doc)]
#[uniffi::export]
pub async fn conversations_for_label(
    session: Arc<MailUserSession>,
    label_id: Id,
) -> Result<Vec<Conversation>, MailboxError> {
    let stash = session.user_stash().clone();
    uniffi_async(async move {
        Ok(RealConversation::find(
            formatdoc!(
                "
                JOIN conversation_labels
                    ON conversations.local_id = conversation_labels.local_conversation_id
                WHERE
                    conversation_labels.local_label_id = ?
                "
            ),
            params![RealLocalId::from(label_id)],
            &stash,
            None,
        )
        .await?
        .into_iter()
        .map(|c| {
            ContextualConversation::new(c, label_id.into())
                .unwrap()
                .into()
        })
        .collect())
    })
    .await
}

/// Retrieve a conversation by local ID.
///
/// Notably, this retrieves a local conversation that has been saved in the
/// database. It does not use the network.
///
/// # Parameters
///
/// * `session`         - The session to use for the request.
/// * `id`              - The local ID of the conversation to retrieve.
/// * `local_label_id`  - Local label id of the label context in which to
///                       display the conversation.
///
/// # Errors
///
/// Returns an error if the database query fails.
///
#[uniffi::export]
pub async fn load_conversation(
    session: Arc<MailUserSession>,
    id: Id,
    label_id: Id,
) -> Result<Option<Conversation>, MailboxError> {
    let stash = session.user_stash().clone();
    uniffi_async(async move {
        let Some(conversation) = RealConversation::load(id.into(), &stash).await? else {
            return Ok(None);
        };

        Ok(ContextualConversation::new(conversation, label_id.into()).map(Into::into))
    })
    .await
}

/// Mark the given conversations as read.
///
/// # Parameters
///
/// * `session` - The session to use for the request.
/// * `ids`     - The local IDs of the conversations to mark as read.
///
/// # Errors
///
/// Returns an error if the database query fails.
///
#[uniffi::export]
pub async fn mark_conversations_as_read(
    session: Arc<MailUserSession>,
    ids: Vec<Id>,
) -> Result<(), MailboxError> {
    let tether = session.user_stash().connection();
    uniffi_async(async move {
        Ok(RealConversation::mark_multiple_as_read(
            ids.into_iter().map(Into::into).collect(),
            &tether,
        )
        .await?)
    })
    .await
}

/// Mark the given conversations as unread.
///
/// # Parameters
///
/// * `session` - The session to use for the request.
/// * `ids`     - The local IDs of the conversations to mark as unread.
///
/// # Errors
///
/// Returns an error if the database query fails.
///
#[uniffi::export]
pub async fn mark_conversations_as_unread(
    session: Arc<MailUserSession>,
    ids: Vec<Id>,
) -> Result<(), MailboxError> {
    let conn = session.user_stash().connection();
    uniffi_async(async move {
        Ok(RealConversation::mark_multiple_as_unread(
            ids.into_iter().map(Into::into).collect(),
            &conn,
        )
        .await?)
    })
    .await
}

/// Move the given conversations from the current mailbox.
///
/// Move the conversations with the specified IDs from the current mailbox to
/// the label with specified label ID. If the current mailbox is not a folder,
/// the conversation will not be moved.
///
/// # Parameters
///
/// * `mailbox` - The mailbox to use for the request.
/// * `label_id` - The local ID of the label to move to.
/// * `ids`      - The local IDs of the conversations to move.
///
/// # Errors
///
/// Returns an error if the database query fails.
///
#[uniffi::export]
pub async fn move_conversations(
    mailbox: Arc<Mailbox>,
    label_id: Id,
    ids: Vec<Id>,
) -> Result<(), MailboxError> {
    let conn = mailbox.stash().connection();
    uniffi_async(async move {
        RealConversation::move_conversations(
            mailbox.label_id().into(),
            label_id.into(),
            ids.into_iter().map(Into::into).collect(),
            &conn,
        )
        .await?;
        Ok(())
    })
    .await
}

/// Unlabel the given conversations with the given label id.
///
/// # Parameters
///
/// * `session`  - The session to use for the request.
/// * `label_id` - The local ID of the label to remove.
/// * `ids`      - The local IDs of the conversations to remove the label from.
///
/// # Errors
///
/// Returns an error if the database query fails.
///
#[uniffi::export]
pub async fn remove_label_from_conversations(
    session: Arc<MailUserSession>,
    label_id: Id,
    ids: Vec<Id>,
) -> Result<(), MailboxError> {
    let conn = session.user_stash().connection();
    uniffi_async(async move {
        Ok(
            RealConversation::remove_label(label_id.into(), ids.into_iter().map(Into::into), &conn)
                .await?,
        )
    })
    .await
}

/// Filter or search conversations which match the specified options.
///
/// Note that search results are inserted into the database.
///
/// # Parameters
///
/// * `session`         - The session to use for the request.
/// * `local_label_id`  - Local label id of the label context in which to
///                       display the results.
/// * `options`         - The search options to use.
///
/// # Errors
///
/// Returns an error if the network request or database query fails.
///
#[uniffi::export]
pub async fn search_for_conversations(
    session: Arc<MailUserSession>,
    local_label_id: Id,
    options: ConversationSearchOptions,
) -> Result<Vec<Conversation>, MailSessionError> {
    let stash = session.user_stash().clone();
    uniffi_async(async move {
        Ok(RealConversation::search(
            options.into_api_options(&stash).await?,
            session.ctx().session().api(),
            &stash,
        )
        .await?
        .into_iter()
        .filter_map(|c| ContextualConversation::new(c, local_label_id.into()))
        .map(Into::into)
        .collect())
    })
    .await
}

/// Star the given conversations.
///
/// # Parameters
///
/// * `session` - The session to use for the request.
/// * `ids`     - The local IDs of the conversations to mark as starred.
///
/// # Errors
///
/// Returns an error if the database query fails.
///
#[uniffi::export]
pub async fn star_conversations(
    session: Arc<MailUserSession>,
    ids: Vec<Id>,
) -> Result<(), MailboxError> {
    let stash = session.user_stash().clone();
    uniffi_async(async move {
        Ok(
            RealConversation::star_multiple(ids.into_iter().map(Into::into).collect(), &stash)
                .await?,
        )
    })
    .await
}

/// Unstar the given conversations.
///
/// # Parameters
///
/// * `session` - The session to use for the request.
/// * `ids`     - The local IDs of the conversations to mark as unstarred.
///
/// # Errors
///
/// Returns an error if the database query fails.
///
#[uniffi::export]
pub async fn unstar_conversations(
    session: Arc<MailUserSession>,
    ids: Vec<Id>,
) -> Result<(), MailboxError> {
    let stash = session.user_stash().clone();
    uniffi_async(async move {
        Ok(
            RealConversation::unstar_multiple(ids.into_iter().map(Into::into).collect(), &stash)
                .await?,
        )
    })
    .await
}

/// Data for a watched conversation.
#[derive(uniffi::Record)]
pub struct WatchedConversation {
    /// The conversation.
    pub conversation: Conversation,

    /// The messages in the conversation.
    pub messages: Vec<Message>,

    /// The Id of the message to open.
    pub message_id_to_open: Option<Id>,

    /// The handle to stop watching the conversation.
    pub conversation_handle: Arc<WatchHandle>,

    /// The handle to stop watching the conversation's messages.
    pub messages_handle: Arc<WatchHandle>,
}

/// Watch the given conversation.
///
/// Watches the specified conversation for changes. When the conversation's
/// messages change, the callback will be invoked.
///
/// # Parameters
///
/// * `mailbox`  - The mailbox to use for the request.
/// * `id`       - The local ID of the conversation to watch.
/// * `callback` - The callback to use for updates. When the specified
///                conversation's messages change, the callback will be
///                invoked.
///
/// # Errors
///
/// Returns an error if the database query fails.
///
#[allow(clippy::missing_panics_doc)]
#[uniffi::export]
pub async fn watch_conversation(
    mailbox: Arc<Mailbox>,
    id: Id,
    callback: Box<dyn LiveQueryCallback>,
) -> Result<WatchedConversation, MailboxError> {
    let stash = mailbox.stash().clone();
    uniffi_async(async move {
        let callback = Arc::new(callback);
        let (conversations, conversation_handle) = watch::<_, _, RealConversation>(
            "WHERE local_id = ?",
            params![RealLocalId::from(id)],
            move |r| r.local_id == Some(id.into()),
            |r| r.local_id.expect("local_id should never be None"),
            &stash,
            Arc::clone(&callback),
        )
        .await?;
        let (messages, messages_handle) = watch::<_, _, RealMessage>(
            "WHERE local_conversation_id = ? LIMIT 1",
            params![RealLocalId::from(id)],
            move |r| r.local_conversation_id == Some(id.into()),
            |r| r.local_id.expect("local_id should never be None"),
            &stash,
            callback,
        )
        .await?;
        let label_id = mailbox.label_id();
        let label = RealLabel::load(label_id.into(), &stash)
            .await?
            .ok_or(MailboxError::LabelNotFound(label_id))?;
        let message_id_to_open =
            RealConversation::first_unread_message(&label, messages.as_slice()).map(|i| i.as_u64());
        Ok(WatchedConversation {
            conversation: ContextualConversation::new(
                conversations.into_iter().next().unwrap(),
                mailbox.label_id().into(),
            )
            .unwrap()
            .into(),
            messages: messages.into_iter().map(Into::into).collect(),
            message_id_to_open: message_id_to_open.map(Into::into),
            conversation_handle,
            messages_handle,
        })
    })
    .await
}

/// Data for watched conversations.
#[derive(uniffi::Record)]
pub struct WatchedConversations {
    /// The conversations.
    pub conversations: Vec<Conversation>,

    /// The handle to stop watching the conversations.
    pub handle: Arc<WatchHandle>,
}

/// Watch conversations for the given label.
///
/// Watches conversations with the specified label for changes. When the
/// conversations change, the callback will be invoked.
///
/// # Parameters
///
/// * `session`  - The session to use for the request.
/// * `label_id` - The local ID of the label to watch.
/// * `callback` - The callback to use for updates. When the specified
///                conversations change, the callback will be invoked.
///
/// # Errors
///
/// Returns an error if the database query fails.
///
#[allow(clippy::missing_panics_doc)]
#[uniffi::export]
pub async fn watch_conversations_for_label(
    session: Arc<MailUserSession>,
    label_id: Id,
    callback: Box<dyn LiveQueryCallback>,
) -> Result<WatchedConversations, MailboxError> {
    let stash = session.user_stash().clone();
    uniffi_async(async move {
        let (conversations, handle) = watch::<_, _, RealConversation>(
            formatdoc!(
                "
                JOIN conversation_labels
                    ON conversations.local_id = conversation_labels.local_conversation_id
                WHERE
                    conversation_labels.local_label_id = ?
                "
            ),
            params![RealLocalId::from(label_id)],
            move |r| {
                r.labels
                    .iter()
                    .any(|l| l.local_label_id == Some(label_id.into()))
            },
            |r| r.local_id.expect("local_id should never be None"),
            &stash,
            Arc::new(callback),
        )
        .await?;
        Ok(WatchedConversations {
            conversations: conversations
                .into_iter()
                .map(|c| {
                    ContextualConversation::new(c, label_id.into())
                        .unwrap()
                        .into()
                })
                .collect(),
            handle,
        })
    })
    .await
}
