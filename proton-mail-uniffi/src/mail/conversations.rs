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

use crate::core::datatypes::RemoteId;
use crate::mail::datatypes::{
    Conversation, ConversationAvailableAction, ConversationSearchOptions, Message,
};
use crate::mail::{MailSession, MailSessionError, Mailbox, MailboxError};
use crate::{LiveQueryCallback, WatchHandle};
use itertools::Itertools;
use proton_core_common::datatypes::{LocalId as RealLocalId, RemoteId as RealRemoteId};
use proton_core_common::models::ModelExtension;
use proton_mail_common::datatypes::ContextualConversation;
use proton_mail_common::models::{Conversation as RealConversation, Message as RealMessage};
use stash::orm::{Model, ResultsetChange};
use stash::params;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::spawn as spawn_async;
use tracing::{debug, warn};

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
    session: Arc<MailSession>,
    label_id: u64,
    ids: Vec<u64>,
) -> Result<(), MailboxError> {
    Ok(RealConversation::apply_label_to_multiple(
        label_id.into(),
        ids.into_iter().map(Into::into).collect(),
        &session.stash().connection(),
    )
    .await?)
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
pub async fn delete_conversations(
    mailbox: Arc<Mailbox>,
    ids: Vec<u64>,
) -> Result<(), MailboxError> {
    RealConversation::delete_multiple(
        ids.into_iter().map(Into::into).collect(),
        mailbox.label_id().into(),
        &mailbox.stash().connection(),
    )
    .await?;
    Ok(())
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
    session: Arc<MailSession>,
    id: u64,
) -> Result<Vec<ConversationAvailableAction>, MailboxError> {
    if let Some(conversation) = RealConversation::load(id.into(), session.stash()).await? {
        let actions = conversation
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
    session: Arc<MailSession>,
    id: u64,
    label_id: u64,
) -> Result<Option<Conversation>, MailboxError> {
    let Some(conversation) = RealConversation::load(id.into(), session.stash()).await? else {
        return Ok(None);
    };

    Ok(ContextualConversation::new(conversation, label_id.into()).map(Into::into))
}

/// Retrieve a conversation by remote ID.
///
/// Notably, this retrieves a local conversation that has been saved in the
/// database. It does not use the network.
///
/// # Parameters
///
/// * `session`         - The session to use for the request.
/// * `id`              - The remote ID of the conversation to retrieve.
/// * `local_label_id`  - Local label id of the label context in which to
///                       display the conversation.
///
/// # Errors
///
/// Returns an error if the database query fails.
///
#[uniffi::export]
pub async fn load_remote_conversation(
    session: Arc<MailSession>,
    id: RemoteId,
    local_label_id: u64,
) -> Result<Option<Conversation>, MailboxError> {
    let Some(conversation) =
        RealConversation::find_by_id(RealRemoteId::from(id), session.stash()).await?
    else {
        return Ok(None);
    };

    Ok(ContextualConversation::new(conversation, local_label_id.into()).map(Into::into))
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
pub async fn mark_converstions_as_read(
    session: Arc<MailSession>,
    ids: Vec<u64>,
) -> Result<(), MailboxError> {
    Ok(RealConversation::mark_multiple_as_read(
        ids.into_iter().map(Into::into).collect(),
        &session.stash().connection(),
    )
    .await?)
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
    session: Arc<MailSession>,
    ids: Vec<u64>,
) -> Result<(), MailboxError> {
    Ok(RealConversation::mark_multiple_as_unread(
        ids.into_iter().map(Into::into).collect(),
        &session.stash().connection(),
    )
    .await?)
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
    label_id: u64,
    ids: Vec<u64>,
) -> Result<(), MailboxError> {
    RealConversation::move_conversations(
        mailbox.label_id().into(),
        label_id.into(),
        ids.into_iter().map(Into::into).collect(),
        &mailbox.stash().connection(),
    )
    .await?;
    Ok(())
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
    session: Arc<MailSession>,
    label_id: u64,
    ids: Vec<u64>,
) -> Result<(), MailboxError> {
    Ok(RealConversation::remove_label_from_multiple(
        label_id.into(),
        ids.into_iter().map(Into::into).collect(),
        &session.stash().connection(),
    )
    .await?)
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
    session: Arc<MailSession>,
    local_label_id: u64,
    options: ConversationSearchOptions,
) -> Result<Vec<Conversation>, MailSessionError> {
    Ok(
        RealConversation::search(options.into(), session.api(), session.stash())
            .await?
            .into_iter()
            .filter_map(|c| ContextualConversation::new(c, local_label_id.into()))
            .map(Into::into)
            .collect(),
    )
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
    session: Arc<MailSession>,
    ids: Vec<u64>,
) -> Result<(), MailboxError> {
    Ok(
        RealConversation::star_multiple(ids.into_iter().map(Into::into).collect(), session.stash())
            .await?,
    )
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
    session: Arc<MailSession>,
    ids: Vec<u64>,
) -> Result<(), MailboxError> {
    Ok(RealConversation::unstar_multiple(
        ids.into_iter().map(Into::into).collect(),
        session.stash(),
    )
    .await?)
}

/// Messages and watch handle for watched messages.
#[derive(uniffi::Record)]
pub struct WatchedConversation {
    /// The messages in the conversation.
    messages: Vec<Message>,

    /// The handle to stop watching the conversation.
    handle: Arc<WatchHandle>,
}

/// Watch the given conversation.
///
/// Watches the specified conversation for changes. When the conversation's
/// messages change, the callback will be invoked.
///
/// # Parameters
///
/// * `session`  - The session to use for the request.
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
    session: Arc<MailSession>,
    id: u64,
    callback: Box<dyn LiveQueryCallback>,
) -> Result<WatchedConversation, MailboxError> {
    let (sender, receiver) = flume::unbounded::<ResultsetChange<RealMessage, RealLocalId>>();
    let results = RealMessage::find(
        "WHERE local_conversation_id = ?",
        params![id],
        session.stash(),
        Some(sender),
    )
    .await?;
    // Unwrapping is safe here, as we will always have the local ID
    let mut ids = results
        .iter()
        .map(|m| m.local_id.unwrap())
        .collect::<Vec<_>>();
    let stop_flag = Arc::new(AtomicBool::new(false));
    let stop_flag_clone = Arc::clone(&stop_flag);

    spawn_async(async move {
        while let Ok(change) = receiver.recv_async().await {
            if stop_flag_clone.load(Ordering::SeqCst) {
                debug!("Stop flag set, stopping watch");
                break;
            }
            match change {
                ResultsetChange::Inserted(message) => {
                    if message.local_conversation_id == Some(id.into()) {
                        debug!("Received new message for watched conversation ({id})");
                        // Unwrapping is safe here, as we will always have the local ID
                        ids.push(message.local_id.unwrap());
                        callback.on_update();
                    } else {
                        debug!(
                            "Received new message for different conversation ({} instead of {id})",
                            message.local_conversation_id.unwrap()
                        );
                    }
                }
                ResultsetChange::Updated(message) => {
                    if message.local_conversation_id == Some(id.into()) {
                        debug!("Received updated message for watched conversation ({id})");
                        callback.on_update();
                    } else {
                        debug!("Received updated message for different conversation ({} instead of {id})", message.local_conversation_id.unwrap());
                    }
                }
                ResultsetChange::Deleted(local_message_id) => {
                    if ids.contains(&local_message_id) {
                        debug!("Received deleted message for watched conversation ({id})");
                        callback.on_update();
                    } else {
                        debug!("Received deleted message for different conversation (unknown instead of {id})");
                    }
                }
                _ => {
                    warn!("Received unknown change type");
                }
            };
        }
    });

    Ok(WatchedConversation {
        messages: results.into_iter().map(Into::into).collect(),
        handle: Arc::new(WatchHandle { stop_flag }),
    })
}
