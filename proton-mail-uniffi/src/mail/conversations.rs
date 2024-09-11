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
use crate::core::paginator::ConversationPaginator;
use crate::mail::datatypes::{
    Conversation, ConversationAvailableAction, ConversationSearchOptions, Message,
};
use crate::mail::{MailSessionError, MailUserSession, Mailbox, MailboxError};
use crate::{uniffi_async, watch_channel, LiveQueryCallback, WatchHandle};
use indoc::formatdoc;
use itertools::Itertools;
use proton_api_core::session::CoreSession;
use proton_core_common::datatypes::LocalId as RealLocalId;
use proton_mail_common::datatypes::{ContextualConversation, ContextualConversationAndMessages};
use proton_mail_common::models::Conversation as RealConversation;
use stash::orm::Model;
use stash::paginator::{Paginator as RealPaginator, Param};
use std::num::NonZeroU32;
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
///
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
/// * `mailbox`  - The mailbox to use for the request.
/// * `id`       - The local ID of the conversation to get.
///
/// This function syncs the conversation's messages from the server at least
/// once.
///
/// # Errors
///
/// Returns an error if the database query fails or the server request failed.
///
#[allow(clippy::missing_panics_doc)]
#[uniffi::export]
pub async fn conversation(
    mailbox: Arc<Mailbox>,
    id: Id,
) -> Result<Option<ConversationAndMessages>, MailboxError> {
    let conn = mailbox.stash().connection();
    let session = mailbox.mbox().user_context().session().clone();
    uniffi_async(async move {
        Ok(ContextualConversation::conversation_and_messages(
            RealLocalId::from(id),
            mailbox.mbox().label_id(),
            &conn,
            session.api(),
        )
        .await?
        .map(Into::into))
    })
    .await
}

/// Results of [`conversation()`]
#[derive(uniffi::Record)]
pub struct ConversationAndMessages {
    /// Conversation.
    pub conversation: Conversation,
    /// ID of the message that should be displayed first.
    pub message_id_to_open: Id,
    /// Messages which belong to the conversation.
    pub messages: Vec<Message>,
}

impl From<ContextualConversationAndMessages> for ConversationAndMessages {
    fn from(value: ContextualConversationAndMessages) -> Self {
        Self {
            conversation: value.conversation.into(),
            message_id_to_open: value.message_id_to_open.into(),
            messages: value.messages.into_iter().map(Into::into).collect(),
        }
    }
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
        Ok(
            ContextualConversation::in_label(RealLocalId::from(label_id), &stash)
                .await?
                .into_iter()
                .map(Into::into)
                .collect(),
        )
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
        Ok(RealConversation::mark_read(ids.into_iter().map(Into::into), &tether).await?)
    })
    .await
}

/// Mark the given conversations as unread.
///
/// # Parameters
///
/// * `mailbox` - The mailbox to use for the request.
/// * `ids`     - The local IDs of the conversations to mark as unread.
///
/// # Errors
///
/// Returns an error if the database query fails.
///
#[uniffi::export]
pub async fn mark_conversations_as_unread(
    mailbox: Arc<Mailbox>,
    ids: Vec<Id>,
) -> Result<(), MailboxError> {
    let conn = mailbox.stash().connection();
    let label_id = mailbox.mbox().label_id();
    uniffi_async(async move {
        Ok(RealConversation::mark_unread(label_id, ids.into_iter().map(Into::into), &conn).await?)
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

/// Paginate conversations for the given label.
///
/// Gets a paginator for conversations belonging to the specified label, which
/// allows navigation through the conversations by page/window, and watches for
/// changes. When the conversations change, the callback will be invoked.
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
pub async fn paginate_conversations_for_label(
    session: Arc<MailUserSession>,
    label_id: Id,
    callback: Box<dyn LiveQueryCallback>,
) -> Result<ConversationPaginator, MailboxError> {
    let stash = session.user_stash().clone();
    let (msg_sender, msg_receiver) = flume::unbounded();
    uniffi_async(async move {
        #[allow(clippy::cast_possible_wrap)]
        let real_paginator = RealPaginator::new(
            formatdoc!(
                "
                JOIN conversation_labels
                    ON conversations.local_id = conversation_labels.local_conversation_id
                WHERE
                    conversation_labels.local_label_id = ?
                ORDER BY
                    conversation_labels.context_time DESC,
                    conversations.display_order DESC
                "
            ),
            vec![Param::Integer(label_id.as_u64() as i64)],
            &stash,
            NonZeroU32::new(50).unwrap(),
            Some(msg_sender),
        )
        .await?;
        Ok(ConversationPaginator {
            real_paginator,
            handle: watch_channel(msg_receiver, callback),
            label_id,
        })
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
        .map_into()
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
    pub message_id_to_open: Id,

    /// The handle to stop watching the conversation and messages;
    pub handle: Arc<WatchHandle>,
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
) -> Result<Option<WatchedConversation>, MailboxError> {
    uniffi_async(async move {
        let Some(conversation_messages) = conversation(Arc::clone(&mailbox), id).await? else {
            return Ok(None);
        };

        let receiver = ContextualConversation::watch_conversation_and_messages(
            RealLocalId::from(id),
            mailbox.stash(),
        )
        .await?;

        let watcher = watch_channel(receiver, callback);

        Ok(Some(WatchedConversation {
            conversation: conversation_messages.conversation,
            messages: conversation_messages.messages,
            message_id_to_open: conversation_messages.message_id_to_open,
            handle: watcher,
        }))
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
    uniffi_async(async move {
        let (conversations, receiver) = ContextualConversation::watch_in_label(
            RealLocalId::from(label_id),
            session.user_stash(),
        )
        .await?;
        let watcher = watch_channel(receiver, callback);
        Ok(WatchedConversations {
            conversations: conversations.into_iter().map(Into::into).collect(),
            handle: watcher,
        })
    })
    .await
}
