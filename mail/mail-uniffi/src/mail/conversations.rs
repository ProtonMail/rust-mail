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
use crate::errors::{MailErrorKind, ProtonMailError, VoidProtonMailResult};
use crate::mail::datatypes::{
    AllBottomBarMessageActions, Conversation, ConversationAvailableActions,
    ConversationSearchOptions, LabelAsAction, Message, MoveAction,
};
use crate::mail::{MailUserSession, Mailbox};
use crate::PaginatorFilter;
use crate::{uniffi_async, watch_channel, LiveQueryCallback, WatchHandle};
use itertools::Itertools;
use proton_api_core::session::CoreSession;
use proton_core_common::datatypes::LocalId as RealLocalId;
use proton_mail_common::datatypes::{ContextualConversation, ContextualConversationAndMessages};
use proton_mail_common::errors::{MailErrorDetails as RealMailErrorDetails, MailErrorReason};
use proton_mail_common::models::{
    watch_available_move_to_actions as real_watch_available_move_to_actions,
    PaginatorFilter as RealPaginatorFilter,
};
use proton_mail_common::models::{Conversation as RealConversation, Label as RealLabel};
use stash::orm::Model;
use std::sync::Arc;

use super::messages::WatchedLabelAs;

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
) -> VoidProtonMailResult {
    let user_context = session.ctx();
    uniffi_async(async move {
        RealConversation::action_apply_label(
            user_context.session(),
            user_context.queue(),
            label_id.into(),
            ids.into_iter().map(Into::into).collect(),
        )
        .await
        .map(|_| ())
        .map_err(RealMailErrorDetails::from)
    })
    .await
    .map_err(|details| MailErrorKind::UserActionError.with(details))
    .into()
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
    conversation_ids: Vec<Id>,
) -> VoidProtonMailResult {
    let label_id = mailbox.mbox().label_id();
    let user_context = mailbox.mbox().user_context();
    uniffi_async(async move {
        RealConversation::action_mark_deleted(
            user_context.session(),
            user_context.queue(),
            label_id,
            conversation_ids.into_iter().map(Into::into),
        )
        .await
        .map(|_| ())
        .map_err(RealMailErrorDetails::from)
    })
    .await
    .map_err(|details| MailErrorKind::UserActionError.with(details))
    .into()
}

/// Returns available actions for conversations.
/// Any action returned here should reflect the display needs.
///
/// # Parameters
///
/// * `session` - The session to use for the request.
/// * `view`    - The local ID of the label which conversations are viewed in.
/// * `ids`     - The local IDs of the conversations to calcualte available actions for.
///
/// # Errors
///
/// Returns an error if the database query fails.
///
#[proton_uniffi_macros::export_result]
pub async fn available_actions_for_conversations(
    mailbox: Arc<Mailbox>,
    ids: Vec<Id>,
) -> Result<ConversationAvailableActions, ProtonMailError> {
    uniffi_async(async move {
        let view = mailbox.mbox().label_id();
        let view = RealLabel::load(view, mailbox.stash())
            .await?
            .ok_or_else(|| RealMailErrorDetails::from(MailErrorReason::UnknownLabel))?;
        let actions = RealConversation::available_actions(
            view,
            ids.into_iter().map_into().collect(),
            mailbox.stash(),
        )
        .await?;

        Result::<_, RealMailErrorDetails>::Ok(ConversationAvailableActions::from(actions))
    })
    .await
    .map_err(|details| MailErrorKind::UserActionError.with(details))
}

/// Returns available label_as actions for conversations.
/// Any action returned here should reflect the display needs.
///
/// # Parameters
///
/// * `session` - The session to use for the request.
/// * `ids`     - The local IDs of the conversations to calcualte available actions for.
///
/// # Errors
///
/// Returns an error if the database query fails.
///
#[proton_uniffi_macros::export_result]
pub async fn available_label_as_actions_for_conversations(
    mailbox: Arc<Mailbox>,
    ids: Vec<Id>,
) -> Result<Vec<LabelAsAction>, ProtonMailError> {
    uniffi_async(async move {
        let actions = RealConversation::available_label_as_actions(
            ids.into_iter().map_into().collect(),
            mailbox.stash(),
        )
        .await?
        .into_iter()
        .map_into()
        .collect_vec();

        Result::<_, RealMailErrorDetails>::Ok(actions)
    })
    .await
    .map_err(|details| MailErrorKind::UserActionError.with(details))
}

/// Watches label_as actions for conversations.
/// Any action returned here should reflect the display needs.
///
/// # Parameters
///
/// * `session`  - The session to use for the request.
/// * `ids`      - The local IDs of the conversations to calcualte available actions for.
/// * `callback` - The callback to use for updates.
///
/// # Errors
///
/// Returns an error if the database query fails.
///
#[proton_uniffi_macros::export_result]
pub async fn watch_available_label_as_actions_for_conversations(
    mailbox: Arc<Mailbox>,
    ids: Vec<Id>,
    callback: Box<dyn LiveQueryCallback>,
) -> Result<WatchedLabelAs, ProtonMailError> {
    uniffi_async(async move {
        let (tx, rx) = flume::unbounded();
        let handle = watch_channel(rx, callback).await;

        let actions = RealConversation::watch_available_label_as_actions(
            ids.into_iter().map_into().collect(),
            mailbox.stash(),
            tx,
        )
        .await?
        .into_iter()
        .map_into()
        .collect_vec();

        Result::<_, RealMailErrorDetails>::Ok(WatchedLabelAs { actions, handle })
    })
    .await
    .map_err(|details| MailErrorKind::UserActionError.with(details))
}

// Returns available move_to actions for conversations.
/// Any action returned here should reflect the display needs.
///
/// # Parameters
///
/// * `session` - The session to use for the request.
/// * `view`    - The local ID of the label which conversations are viewed in.
/// * `ids`     - The local IDs of the conversations to calcualte available actions for.
///
/// # Errors
///
/// Returns an error if the database query fails.
///
#[proton_uniffi_macros::export_result]
pub async fn available_move_to_actions_for_conversations(
    mailbox: Arc<Mailbox>,
    ids: Vec<Id>,
) -> Result<Vec<MoveAction>, ProtonMailError> {
    uniffi_async(async move {
        let view = mailbox.mbox().label_id();
        let view = RealLabel::load(view, mailbox.stash())
            .await?
            .ok_or_else(|| RealMailErrorDetails::from(MailErrorReason::UnknownLabel))?;
        let actions = RealConversation::available_move_to_actions(
            view,
            ids.into_iter().map_into().collect(),
            mailbox.stash(),
        )
        .await?
        .into_iter()
        .map_into()
        .collect_vec();

        Result::<_, RealMailErrorDetails>::Ok(actions)
    })
    .await
    .map_err(|details| MailErrorKind::UserActionError.with(details))
}

/// Returns available actions for conversation bottom bar.
///
/// # Parameters
///
/// * `mailbox`          - The current Mailbox.
/// * `conversation_ids` - The local IDs of the conversations to calculate available actions for.
///
/// # Errors
///
/// Returns an error if the database query fails.
///
#[proton_uniffi_macros::export_result]
pub async fn all_available_bottom_bar_actions_for_conversations(
    mailbox: Arc<Mailbox>,
    conversation_ids: Vec<Id>,
) -> Result<AllBottomBarMessageActions, ProtonMailError> {
    uniffi_async(async move {
        let actions = RealConversation::all_available_bottom_bar_actions_for_conversations(
            mailbox.label_id().into(),
            conversation_ids.into_iter().map_into().collect(),
            mailbox.stash(),
        )
        .await?
        .into();

        Result::<_, RealMailErrorDetails>::Ok(actions)
    })
    .await
    .map_err(|details| MailErrorKind::UserActionError.with(details))
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
#[proton_uniffi_macros::export_result]
pub async fn conversation(
    mailbox: Arc<Mailbox>,
    id: Id,
) -> Result<Option<ConversationAndMessages>, ProtonMailError> {
    get_conversation(mailbox, id)
        .await
        .map_err(|details| MailErrorKind::UserActionError.with(details))
        .map_err(Into::into)
}

async fn get_conversation(
    mailbox: Arc<Mailbox>,
    id: Id,
) -> Result<Option<ConversationAndMessages>, RealMailErrorDetails> {
    let conn = mailbox.stash().connection();
    let session = mailbox.mbox().user_context().session().clone();
    uniffi_async(async move {
        Result::<_, RealMailErrorDetails>::Ok(
            ContextualConversation::conversation_and_messages(
                RealLocalId::from(id),
                mailbox.mbox().label_id(),
                &conn,
                session.api(),
            )
            .await?
            .map(Into::into),
        )
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
#[proton_uniffi_macros::export_result]
pub async fn conversations_for_label(
    session: Arc<MailUserSession>,
    label_id: Id,
) -> Result<Vec<Conversation>, ProtonMailError> {
    let stash = session.user_stash().clone();
    uniffi_async(async move {
        Result::<_, RealMailErrorDetails>::Ok(
            ContextualConversation::in_label(RealLocalId::from(label_id), &stash)
                .await?
                .into_iter()
                .map(Into::into)
                .collect(),
        )
    })
    .await
    .map_err(|details| MailErrorKind::UserActionError.with(details))
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
#[proton_uniffi_macros::export_result]
pub async fn load_conversation(
    session: Arc<MailUserSession>,
    id: Id,
    label_id: Id,
) -> Result<Option<Conversation>, ProtonMailError> {
    let stash = session.user_stash().clone();
    uniffi_async(async move {
        let Some(conversation) = RealConversation::load(id.into(), &stash).await? else {
            return Ok(None);
        };

        Result::<_, RealMailErrorDetails>::Ok(
            ContextualConversation::new(conversation, label_id.into()).map(Into::into),
        )
    })
    .await
    .map_err(|details| MailErrorKind::UserActionError.with(details))
}

/// Mark the given conversations as read.
///
/// # Parameters
///
/// * `mailbox` - The mailbox to use for the request.
/// * `ids`     - The local IDs of the conversations to mark as read.
///
/// # Errors
///
/// Returns an error if the database query fails.
///
#[uniffi::export]
pub async fn mark_conversations_as_read(
    mailbox: Arc<Mailbox>,
    ids: Vec<Id>,
) -> VoidProtonMailResult {
    uniffi_async(async move {
        let user_context = mailbox.mbox().user_context();
        RealConversation::action_mark_read(
            user_context.session(),
            user_context.queue(),
            mailbox.label_id().into(),
            ids.into_iter().map(Into::into).collect(),
        )
        .await
        .map(|_| ())
        .map_err(RealMailErrorDetails::from)
    })
    .await
    .map_err(|details| MailErrorKind::UserActionError.with(details))
    .into()
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
) -> VoidProtonMailResult {
    uniffi_async(async move {
        let user_context = mailbox.mbox().user_context();
        RealConversation::action_mark_unread(
            user_context.session(),
            user_context.queue(),
            mailbox.label_id().into(),
            ids.into_iter().map(Into::into).collect(),
        )
        .await
        .map(|_| ())
        .map_err(RealMailErrorDetails::from)
    })
    .await
    .map_err(|details| MailErrorKind::UserActionError.with(details))
    .into()
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
) -> VoidProtonMailResult {
    uniffi_async(async move {
        let user_context = mailbox.mbox().user_context();
        RealConversation::action_move(
            user_context.session(),
            user_context.queue(),
            mailbox.label_id().into(),
            label_id.into(),
            ids.into_iter().map(Into::into).collect(),
        )
        .await
        .map(|_| ())
        .map_err(RealMailErrorDetails::from)
    })
    .await
    .map_err(|details| MailErrorKind::UserActionError.with(details))
    .into()
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
/// * `filter`   - The filter options for pagination.
/// * `callback` - The callback to use for updates. When the specified
///                conversations change, the callback will be invoked.
///
/// # Errors
///
/// Returns an error if the database query fails.
///
#[allow(clippy::missing_panics_doc)]
#[proton_uniffi_macros::export_result]
pub async fn paginate_conversations_for_label(
    session: Arc<MailUserSession>,
    label_id: Id,
    filter: PaginatorFilter,
    callback: Box<dyn LiveQueryCallback>,
) -> Result<Arc<ConversationPaginator>, ProtonMailError> {
    let context = session.ctx();
    let (msg_sender, msg_receiver) = flume::unbounded();
    uniffi_async(async move {
        let real_paginator = RealConversation::paginate_in_label(
            &context,
            RealLocalId::from(label_id),
            50,
            RealPaginatorFilter::from(filter),
            true,
            Some(msg_sender),
        )
        .await?;
        Result::<_, RealMailErrorDetails>::Ok(Arc::new(ConversationPaginator {
            real_paginator,
            handle: watch_channel(msg_receiver, callback).await,
            label_id,
        }))
    })
    .await
    .map_err(|details| MailErrorKind::UserActionError.with(details))
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
) -> VoidProtonMailResult {
    let user_context = session.ctx();
    uniffi_async(async move {
        RealConversation::action_remove_label(
            user_context.session(),
            user_context.queue(),
            label_id.into(),
            ids.into_iter().map(Into::into).collect(),
        )
        .await
        .map(|_| ())
        .map_err(RealMailErrorDetails::from)
    })
    .await
    .map_err(|details| MailErrorKind::UserActionError.with(details))
    .into()
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
#[proton_uniffi_macros::export_result]
pub async fn search_for_conversations(
    session: Arc<MailUserSession>,
    local_label_id: Id,
    options: ConversationSearchOptions,
) -> Result<Vec<Conversation>, ProtonMailError> {
    let stash = session.user_stash().clone();
    uniffi_async(async move {
        Result::<_, RealMailErrorDetails>::Ok(
            RealConversation::search(
                options.into_api_options(&stash).await?,
                session.ctx().session().api(),
                &stash,
            )
            .await?
            .into_iter()
            .filter_map(|c| ContextualConversation::new(c, local_label_id.into()))
            .map_into()
            .collect(),
        )
    })
    .await
    .map_err(|details| MailErrorKind::UserActionError.with(details))
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
) -> VoidProtonMailResult {
    let user_context = session.ctx();
    uniffi_async(async move {
        RealConversation::action_star(
            user_context.session(),
            user_context.queue(),
            ids.into_iter().map(Into::into).collect(),
        )
        .await
        .map(|_| ())
        .map_err(RealMailErrorDetails::from)
    })
    .await
    .map_err(|details| MailErrorKind::UserActionError.with(details))
    .into()
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
) -> VoidProtonMailResult {
    let user_context = session.ctx();
    uniffi_async(async move {
        RealConversation::action_unstar(
            user_context.session(),
            user_context.queue(),
            ids.into_iter().map(Into::into).collect(),
        )
        .await
        .map(|_| ())
        .map_err(RealMailErrorDetails::from)
    })
    .await
    .map_err(|details| MailErrorKind::UserActionError.with(details))
    .into()
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
#[proton_uniffi_macros::export_result]
pub async fn watch_conversation(
    mailbox: Arc<Mailbox>,
    id: Id,
    callback: Box<dyn LiveQueryCallback>,
) -> Result<Option<WatchedConversation>, ProtonMailError> {
    uniffi_async(async move {
        let Some(conversation_messages) = get_conversation(Arc::clone(&mailbox), id).await? else {
            return Ok(None);
        };

        let receiver = ContextualConversation::watch_conversation_and_messages(
            RealLocalId::from(id),
            mailbox.stash(),
        )
        .await?;

        let watcher = watch_channel(receiver, callback).await;

        Result::<_, RealMailErrorDetails>::Ok(Some(WatchedConversation {
            conversation: conversation_messages.conversation,
            messages: conversation_messages.messages,
            message_id_to_open: conversation_messages.message_id_to_open,
            handle: watcher,
        }))
    })
    .await
    .map_err(|details| MailErrorKind::UserActionError.with(details))
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
#[proton_uniffi_macros::export_result]
pub async fn watch_conversations_for_label(
    session: Arc<MailUserSession>,
    label_id: Id,
    callback: Box<dyn LiveQueryCallback>,
) -> Result<WatchedConversations, ProtonMailError> {
    uniffi_async(async move {
        let (conversations, receiver) = ContextualConversation::watch_in_label(
            RealLocalId::from(label_id),
            session.user_stash(),
        )
        .await?;
        let watcher = watch_channel(receiver, callback).await;
        Result::<_, RealMailErrorDetails>::Ok(WatchedConversations {
            conversations: conversations.into_iter().map(Into::into).collect(),
            handle: watcher,
        })
    })
    .await
    .map_err(|details| MailErrorKind::UserActionError.with(details))
}

/// Action to change labels on a batch of conversations.
///
/// All given conversations will get the selected labels.
/// All given conversations will keep the partially selected labels.
/// All given conversations will lose any other labels.
///
/// # Parameters
///
/// * `mailbox`                      - The current mailbox.
/// * `conversation_ids`             - List of ids of the conversations to label.
/// * `selected_label_ids`           - List of ids of the Labels to set.
/// * `partially_selected_label_ids` - List of ids of the Labels to keep as is.
/// * `must_archive`                 - If true, the given conversations must be archived.
///
/// # Errors
///
/// Returns an error if the action can not be applied.
///
#[proton_uniffi_macros::export_result]
pub async fn label_conversations_as(
    mailbox: Arc<Mailbox>,
    conversation_ids: Vec<Id>,
    selected_label_ids: Vec<Id>,
    partially_selected_label_ids: Vec<Id>,
    must_archive: bool,
) -> Result<bool, ProtonMailError> {
    let user_context = mailbox.mbox().user_context();
    let source_label_id = mailbox.label_id();
    uniffi_async(async move {
        Result::<_, RealMailErrorDetails>::Ok(
            RealConversation::action_label_as(
                user_context.session(),
                user_context.queue(),
                source_label_id.into(),
                conversation_ids.into_iter().map_into().collect(),
                selected_label_ids.into_iter().map_into().collect(),
                partially_selected_label_ids
                    .into_iter()
                    .map_into()
                    .collect(),
                must_archive,
            )
            .await?,
        )
    })
    .await
    .map_err(|details| MailErrorKind::UserActionError.with(details))
}

// watches available move_to actions for conversations or messages.
/// Any action returned here should reflect the display needs.
///
/// # Parameters
///
/// * `session` - The session to use for the request.
/// * `view`    - The local ID of the label which conversations are viewed in.
/// * `ids`     - The local IDs of the conversations to calcualte available actions for.
///
/// # Errors
///
/// Returns an error if the database query fails.
///
#[proton_uniffi_macros::export_result]
pub async fn watch_available_move_to_actions(
    mailbox: Arc<Mailbox>,
    callback: Box<dyn LiveQueryCallback>,
) -> Result<Arc<WatchHandle>, ProtonMailError> {
    uniffi_async(async move {
        let (tx, rx) = flume::unbounded();
        let handle = watch_channel(rx, callback).await;
        real_watch_available_move_to_actions(tx, mailbox.stash()).await?;
        Result::<_, RealMailErrorDetails>::Ok(handle)
    })
    .await
    .map_err(|details| MailErrorKind::UserActionError.with(details))
}
