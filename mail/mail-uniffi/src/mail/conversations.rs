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

use crate::core::datatypes::{Id, NonDefaultWeekStart, UnixTimestamp};
use crate::errors::{ActionError, MobileActionsResult, SnoozeError, VoidActionResult};
use crate::mail::datatypes::{
    AllConversationActions, AllListActions, AutoDeleteBanner, Conversation,
    ConversationActionSheet, ConversationSearchOptions, LabelAsAction, LabelAsOutput, Message,
    MobileAction, MoveAction, SnoozeActions, Undo,
};
use crate::mail::mail_scroller::{
    ConversationScroller, ConversationScrollerLiveQueryCallback, ReadFilter,
    spawn_conversation_scroller_watcher,
};
use crate::mail::{MailUserSession, Mailbox};
use crate::{LiveQueryCallback, WatchHandle, uniffi_async, watch_channel};
use itertools::Itertools;
use proton_core_api::session::Session;
use proton_core_common::datatypes::WeekStart as RealWeekStart;
use proton_core_common::models::Label as RealLabel;
use proton_core_common::utils::MapVec;
use proton_mail_common::datatypes::{
    ContextualConversation, ContextualConversationAndMessages, LocalConversationId,
    MobileAction as RealMobileAction,
};
use proton_mail_common::errors::{
    ActionErrorReason as RealActionErrorReason, ProtonMailError as RealProtonMailError,
};
use proton_mail_common::mail_scroller::MailScroller;
use proton_mail_common::models::Conversation as RealConversation;
use stash::orm::Model;
use stash::stash::Stash;
use std::sync::Arc;

use super::messages::WatchedLabelAs;

/// Delete the given conversations.
///
/// # Errors
///
/// Returns an error if the database query fails.
///
#[uniffi_export]
#[returns(VoidActionResult)]
pub async fn delete_conversations(
    mailbox: Arc<Mailbox>,
    conversation_ids: Vec<Id>,
) -> Result<(), ActionError> {
    let label_id = mailbox.mbox().label_id();
    let user_context = mailbox.ctx()?;

    uniffi_async(async move {
        RealConversation::action_mark_deleted(
            user_context.action_queue(),
            label_id,
            conversation_ids.into_iter().map(Into::into),
        )
        .await
        .map(|_| ())
        .map_err(RealProtonMailError::from)
    })
    .await
    .map_err(ActionError::from)
    .into()
}

/// Returns available label_as actions for conversations.
/// Any action returned here should reflect the display needs.
///
/// # Errors
///
/// Returns an error if the database query fails.
///
#[uniffi_export]
pub async fn available_label_as_actions_for_conversations(
    mailbox: Arc<Mailbox>,
    ids: Vec<Id>,
) -> Result<Vec<LabelAsAction>, ActionError> {
    let stash = mailbox.stash()?;
    uniffi_async(async move {
        let tether = stash.connection();
        let actions = RealConversation::available_label_as_actions(ids.map_vec(), &tether)
            .await?
            .map_vec();

        Ok::<_, RealProtonMailError>(actions)
    })
    .await
    .map_err(ActionError::from)
}

/// Watches label_as actions for conversations.
/// Any action returned here should reflect the display needs.
///
/// # Errors
///
/// Returns an error if the database query fails.
///
#[uniffi_export]
pub async fn watch_available_label_as_actions_for_conversations(
    mailbox: Arc<Mailbox>,
    ids: Vec<Id>,
    callback: Box<dyn LiveQueryCallback>,
) -> Result<WatchedLabelAs, ActionError> {
    let ctx = mailbox.ctx()?;
    let stash = mailbox.stash()?;
    uniffi_async(async move {
        let tether = stash.connection();
        let (actions, handle) = RealConversation::watch_available_label_as_actions(
            ids.into_iter().map_into().collect(),
            &tether,
        )
        .await?;
        let actions = actions.into_iter().map_into().collect_vec();
        let handle = watch_channel(&*ctx, handle, callback);

        Result::<_, RealProtonMailError>::Ok(WatchedLabelAs { actions, handle })
    })
    .await
    .map_err(ActionError::from)
}

// Returns available move_to actions for conversations.
/// Any action returned here should reflect the display needs.
///
/// # Errors
///
/// Returns an error if the database query fails.
///
#[uniffi_export]
pub async fn available_move_to_actions_for_conversations(
    mailbox: Arc<Mailbox>,
    ids: Vec<Id>,
) -> Result<Vec<MoveAction>, ActionError> {
    let stash = mailbox.stash()?;
    uniffi_async(async move {
        let view = mailbox.mbox().label_id();
        let tether = stash.connection();
        let view = RealLabel::load(view, &tether)
            .await?
            .ok_or_else(|| RealProtonMailError::reason(RealActionErrorReason::UnknownLabel))?;
        let actions = RealConversation::available_move_to_actions(view, ids.map_vec(), &tether)
            .await?
            .map_vec();

        Ok::<_, RealProtonMailError>(actions)
    })
    .await
    .map_err(ActionError::from)
}

/// Returns available snooze actions for conversation.
///
/// This function will return options depending on current day of the week.
/// If the conversation is already snoozed, it will return the unsnooze option.
///
#[allow(unused_variables)]
#[allow(clippy::needless_pass_by_value)]
#[uniffi_export]
pub async fn available_snooze_actions_for_conversation(
    session: Arc<MailUserSession>,
    week_start: NonDefaultWeekStart,
    ids: Vec<Id>,
) -> Result<SnoozeActions, SnoozeError> {
    let ctx = session.ctx()?;
    let stash = session.user_stash()?;
    uniffi_async(async move {
        let tether = stash.connection();
        let user = ctx.user().await?;
        let settings = ctx.user_settings().await?;
        let week_start = match settings.week_start {
            RealWeekStart::Default => week_start.into(),
            non_default => non_default,
        };
        let snooze_options = RealConversation::available_snooze_actions(
            ids.map_vec(),
            &user,
            week_start.into(),
            &tether,
        )
        .await?;

        Result::<_, RealProtonMailError>::Ok(SnoozeActions::from(snooze_options))
    })
    .await
    .map_err(SnoozeError::from)
}

#[uniffi_export]
pub async fn snooze_conversations(
    session: Arc<MailUserSession>,
    label_id: Id,
    ids: Vec<Id>,
    snooze_time: UnixTimestamp,
) -> Result<(), SnoozeError> {
    let user_context = session.ctx()?;
    uniffi_async(async move {
        RealConversation::action_snooze(
            user_context.action_queue(),
            label_id.into(),
            ids.map_vec(),
            snooze_time.into(),
        )
        .await?;

        Result::<_, RealProtonMailError>::Ok(())
    })
    .await
    .map_err(SnoozeError::from)
}

#[uniffi_export]
pub async fn unsnooze_conversations(
    session: Arc<MailUserSession>,
    label_id: Id,
    ids: Vec<Id>,
) -> Result<(), SnoozeError> {
    let user_context = session.ctx()?;
    uniffi_async(async move {
        RealConversation::action_unsnooze(
            user_context.action_queue(),
            label_id.into(),
            ids.map_vec(),
        )
        .await?;

        Result::<_, RealProtonMailError>::Ok(())
    })
    .await
    .map_err(SnoozeError::from)
}

/// Returns available actions for conversation list toolbar.
///
/// # Errors
///
/// Returns an error if the database query fails.
///
#[uniffi_export]
pub async fn all_available_list_actions_for_conversations(
    mailbox: Arc<Mailbox>,
    conversation_ids: Vec<Id>,
) -> Result<AllListActions, ActionError> {
    let stash = mailbox.stash()?;
    uniffi_async(async move {
        let tether = stash.connection();
        let actions = ContextualConversation::all_available_list_actions_for_conversations(
            mailbox.label_id().into(),
            conversation_ids.map_vec(),
            &tether,
        )
        .await?
        .into();

        Ok::<_, RealProtonMailError>(actions)
    })
    .await
    .map_err(ActionError::from)
}

/// Get the available actions to populate the conversation action sheet.
///
/// Conversation sheet contains context aware set of actions for given conversation.
/// It is split up into different categories to be easy to display in the UI.
///
/// # Errors
///
/// Returns an error if the database query fails.
///
#[uniffi_export]
pub async fn all_available_conversation_actions_for_action_sheet(
    mailbox: Arc<Mailbox>,
    conversation_id: Id,
) -> Result<ConversationActionSheet, ActionError> {
    let stash = mailbox.stash()?;
    let current_label_id = mailbox.label_id();
    uniffi_async(async move {
        let tether = stash.connection();
        let action_sheet =
            ContextualConversation::all_available_conversation_actions_for_action_sheet(
                current_label_id.into(),
                conversation_id.into(),
                &tether,
            )
            .await?;
        Ok::<_, RealProtonMailError>(action_sheet.into())
    })
    .await
    .map_err(ActionError::from)
}

/// Get the available conversation actions for a single conversation.
///
/// Returns all available actions split into visible and hidden categories,
/// matching the toolbar structure used by the UI.
///
/// # Errors
///
/// Returns an error if the database query fails or conversation is not found.
///
#[uniffi_export]
pub async fn all_available_conversation_actions_for_conversation(
    mailbox: Arc<Mailbox>,
    conversation_id: Id,
) -> Result<AllConversationActions, ActionError> {
    let stash = mailbox.stash()?;
    let current_label_id = mailbox.label_id();
    uniffi_async(async move {
        let tether = stash.connection();
        let all_actions =
            ContextualConversation::all_available_conversation_actions_for_conversation(
                current_label_id.into(),
                conversation_id.into(),
                &tether,
            )
            .await?;
        Ok::<_, RealProtonMailError>(all_actions.into())
    })
    .await
    .map_err(ActionError::from)
}

/// Get a specified conversation.
///
/// This function syncs the conversation's messages from the server at least
/// once.
///
/// # Errors
///
/// Returns an error if the database query fails or the server request failed.
///
#[uniffi_export]
pub async fn conversation(
    mailbox: Arc<Mailbox>,
    id: Id,
) -> Result<Option<ConversationAndMessages>, ActionError> {
    let stash = mailbox.stash()?;
    let session = mailbox.session()?;

    get_conversation(mailbox, stash, session, id)
        .await
        .map_err(ActionError::from)
        .map_err(Into::into)
}

async fn get_conversation(
    mailbox: Arc<Mailbox>,
    stash: Stash,
    session: Session,
    id: Id,
) -> Result<Option<ConversationAndMessages>, RealProtonMailError> {
    uniffi_async(async move {
        Ok::<_, RealProtonMailError>(
            ContextualConversation::conversation_and_messages(
                LocalConversationId::from(id),
                mailbox.mbox().label_id(),
                &stash,
                &session,
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
            messages: value.messages.map_vec(),
        }
    }
}

/// Get conversations for the given label.
///
/// # Errors
///
/// Returns an error if the database query fails.
///
#[uniffi_export]
pub async fn conversations_for_label(
    session: Arc<MailUserSession>,
    label_id: Id,
) -> Result<Vec<Conversation>, ActionError> {
    let stash = session.user_stash()?;
    uniffi_async(async move {
        let tether = stash.connection();
        Result::<_, RealProtonMailError>::Ok(
            ContextualConversation::in_label(label_id.into(), &tether)
                .await?
                .map_vec(),
        )
    })
    .await
    .map_err(ActionError::from)
}

/// Retrieve a conversation by local ID.
///
/// Notably, this retrieves a local conversation that has been saved in the
/// database. It does not use the network.
///
/// # Errors
///
/// Returns an error if the database query fails.
///
#[uniffi_export]
pub async fn load_conversation(
    session: Arc<MailUserSession>,
    id: Id,
    label_id: Id,
) -> Result<Option<Conversation>, ActionError> {
    let stash = session.user_stash()?;
    uniffi_async(async move {
        let tether = stash.connection();
        let Some(conversation) = RealConversation::load(id.into(), &tether).await? else {
            return Ok(None);
        };

        Result::<_, RealProtonMailError>::Ok(
            ContextualConversation::new(conversation, label_id.into()).map(Into::into),
        )
    })
    .await
    .map_err(ActionError::from)
}

/// Mark the given conversations as read.
///
/// # Errors
///
/// Returns an error if the database query fails.
///
#[uniffi_export]
#[returns(VoidActionResult)]
pub async fn mark_conversations_as_read(
    mailbox: Arc<Mailbox>,
    ids: Vec<Id>,
) -> Result<(), ActionError> {
    let user_context = mailbox.ctx()?;
    uniffi_async(async move {
        RealConversation::action_mark_read(
            user_context.action_queue(),
            mailbox.label_id().into(),
            ids.map_vec(),
        )
        .await
        .map_err(RealProtonMailError::from)
    })
    .await
    .map_err(ActionError::from)
    .into()
}

/// Mark the given conversations as unread.
///
/// # Errors
///
/// Returns an error if the database query fails.
///
#[uniffi_export]
#[returns(VoidActionResult)]
pub async fn mark_conversations_as_unread(
    mailbox: Arc<Mailbox>,
    ids: Vec<Id>,
) -> Result<(), ActionError> {
    let user_context = mailbox.ctx()?;
    uniffi_async(async move {
        RealConversation::action_mark_unread(
            user_context.action_queue(),
            mailbox.label_id().into(),
            ids.map_vec(),
        )
        .await
        .map_err(RealProtonMailError::from)
    })
    .await
    .map_err(ActionError::from)
    .into()
}

/// Move the given conversations from the current mailbox.
///
/// Move the conversations with the specified IDs from the current mailbox to
/// the label with specified label ID. If the current mailbox is not a folder,
/// the conversation will not be moved.
///
/// # Errors
///
/// Returns an error if the database query fails.
///
#[uniffi_export]
pub async fn move_conversations(
    mailbox: Arc<Mailbox>,
    label_id: Id,
    ids: Vec<Id>,
) -> Result<Option<Arc<Undo>>, ActionError> {
    let ctx = mailbox.ctx()?;
    uniffi_async(async move {
        let tether = ctx.user_stash().connection();
        RealConversation::action_move(&tether, ctx.action_queue(), label_id.into(), ids.map_vec())
            .await
            .map(|undo| undo.map(|undo| Arc::new(undo.into())))
            .map_err(RealProtonMailError::from)
    })
    .await
    .map_err(ActionError::from)
    .into()
}

/// Paginate conversations for the given label.
///
/// Gets a paginator for conversations belonging to the specified label, which
/// allows navigation through the conversations by page/window, and watches for
/// changes. When the conversations change, the callback will be invoked.
///
/// # Errors
///
/// Returns an error if the database query fails.
///
#[uniffi_export]
pub async fn scroll_conversations_for_label(
    session: Arc<MailUserSession>,
    label_id: Id,
    filter: ReadFilter,
    callback: Box<dyn ConversationScrollerLiveQueryCallback>,
) -> Result<Arc<ConversationScroller>, ActionError> {
    let context = session.ctx()?;
    uniffi_async(async move {
        let (scroller, handle) =
            MailScroller::conversations(context.as_weak(), label_id.into(), filter.into(), 50)
                .await?;
        let handle = spawn_conversation_scroller_watcher(&context, handle, callback);

        Result::<_, RealProtonMailError>::Ok(Arc::new(ConversationScroller {
            scroller: Arc::new(scroller),
            handle,
        }))
    })
    .await
    .map_err(ActionError::from)
}

/// Filter or search conversations which match the specified options.
///
/// Note that search results are inserted into the database.
///
/// # Errors
///
/// Returns an error if the network request or database query fails.
///
#[uniffi_export]
pub async fn search_for_conversations(
    session: Arc<MailUserSession>,
    local_label_id: Id,
    options: ConversationSearchOptions,
) -> Result<Vec<Conversation>, ActionError> {
    let user_context = session.ctx()?;
    let stash = session.user_stash()?;
    uniffi_async(async move {
        let mut tether = stash.connection();
        let conversations = RealConversation::search(
            options.into_api_options(&tether).await?,
            user_context.api(),
            &mut tether,
        )
        .await?
        .into_iter()
        .filter_map(|c| ContextualConversation::new(c, local_label_id.into()))
        .map(Into::into)
        .collect();

        Result::<_, RealProtonMailError>::Ok(conversations)
    })
    .await
    .map_err(ActionError::from)
}

/// Star the given conversations.
///
/// # Errors
///
/// Returns an error if the database query fails.
///
#[uniffi_export]
#[returns(VoidActionResult)]
pub async fn star_conversations(
    session: Arc<MailUserSession>,
    ids: Vec<Id>,
) -> Result<(), ActionError> {
    let user_context = session.ctx()?;
    uniffi_async(async move {
        RealConversation::action_star(user_context.action_queue(), ids.map_vec())
            .await
            .map(|_| ())
            .map_err(RealProtonMailError::from)
    })
    .await
    .map_err(ActionError::from)
    .into()
}

/// Unstar the given conversations.
///
/// # Errors
///
/// Returns an error if the database query fails.
///
#[uniffi_export]
#[returns(VoidActionResult)]
pub async fn unstar_conversations(
    session: Arc<MailUserSession>,
    ids: Vec<Id>,
) -> Result<(), ActionError> {
    let user_context = session.ctx()?;
    uniffi_async(async move {
        RealConversation::action_unstar(user_context.action_queue(), ids.map_vec())
            .await
            .map(|_| ())
            .map_err(RealProtonMailError::from)
    })
    .await
    .map_err(ActionError::from)
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
/// # Errors
///
/// Returns an error if the database query fails.
///
#[uniffi_export]
pub async fn watch_conversation(
    mailbox: Arc<Mailbox>,
    id: Id,
    callback: Box<dyn LiveQueryCallback>,
) -> Result<Option<WatchedConversation>, ActionError> {
    let ctx = mailbox.ctx()?;
    let label_id = mailbox.label_id();

    uniffi_async(async move {
        let stash = ctx.user_stash();
        let Some(conv_and_msgs) =
            ContextualConversation::open_conversation(id.into(), label_id.into(), &ctx).await?
        else {
            return Ok(None);
        };

        let receiver = ContextualConversation::watch(&stash)?;
        let watcher = watch_channel(&*ctx, receiver, callback);

        Result::<_, RealProtonMailError>::Ok(Some(WatchedConversation {
            conversation: conv_and_msgs.conversation.into(),
            messages: conv_and_msgs.messages.map_vec(),
            message_id_to_open: conv_and_msgs.message_id_to_open.into(),
            handle: watcher,
        }))
    })
    .await
    .map_err(ActionError::from)
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
/// # Errors
///
/// Returns an error if the database query fails.
///
#[uniffi_export]
pub async fn watch_conversations_for_label(
    session: Arc<MailUserSession>,
    label_id: Id,
    callback: Box<dyn LiveQueryCallback>,
) -> Result<WatchedConversations, ActionError> {
    let user_context = session.ctx()?;
    let stash = session.user_stash()?;
    uniffi_async(async move {
        let tether = stash.connection();
        let conversations = RealConversation::in_label(label_id.into(), &tether).await?;
        let receiver = ContextualConversation::watch(&stash)?;
        let watcher = watch_channel(&*user_context, receiver, callback);
        Result::<_, RealProtonMailError>::Ok(WatchedConversations {
            conversations: conversations
                .into_iter()
                .filter_map(|c| ContextualConversation::new(c, label_id.into()))
                .map(Into::into)
                .collect(),
            handle: watcher,
        })
    })
    .await
    .map_err(ActionError::from)
}

/// Action to change labels on a batch of conversations.
///
/// All given conversations will get the selected labels.
/// All given conversations will keep the partially selected labels.
/// All given conversations will lose any other labels.
///
/// # Errors
///
/// Returns an error if the action can not be applied.
///
#[uniffi_export]
pub async fn label_conversations_as(
    mailbox: Arc<Mailbox>,
    conversation_ids: Vec<Id>,
    selected_label_ids: Vec<Id>,
    partially_selected_label_ids: Vec<Id>,
    must_archive: bool,
) -> Result<LabelAsOutput, ActionError> {
    let ctx = mailbox.ctx()?;
    let source_label_id = mailbox.label_id();
    uniffi_async(async move {
        Result::<_, RealProtonMailError>::Ok(
            RealConversation::action_label_as(
                &ctx.user_stash().connection(),
                ctx.action_queue(),
                source_label_id.into(),
                conversation_ids.map_vec(),
                selected_label_ids.map_vec(),
                partially_selected_label_ids.map_vec(),
                must_archive,
            )
            .await?
            .into(),
        )
    })
    .await
    .map_err(ActionError::from)
}

/// watches available move_to actions for conversations or messages.
/// Any action returned here should reflect the display needs.
///
/// # Errors
///
/// Returns an error if the database query fails.
///
#[uniffi_export]
pub async fn watch_available_move_to_actions(
    mailbox: Arc<Mailbox>,
    callback: Box<dyn LiveQueryCallback>,
) -> Result<Arc<WatchHandle>, ActionError> {
    let ctx = mailbox.ctx()?;
    let stash = mailbox.stash()?;
    uniffi_async(async move {
        let handle = RealLabel::watch(&stash)?;
        let handle = watch_channel(&*ctx, handle, callback);
        Result::<_, RealProtonMailError>::Ok(handle)
    })
    .await
    .map_err(ActionError::from)
}

/// Gets whether or not to display the `AutoDelete` banner.
/// Any action returned here should reflect the display needs.
///
/// # Errors
///
/// Returns an error if the database query fails.
///
#[uniffi_export]
pub async fn get_auto_delete_banner(
    session: Arc<MailUserSession>,
    label_id: Id,
) -> Result<Option<AutoDeleteBanner>, ActionError> {
    let ctx = session.ctx()?;
    uniffi_async(async move {
        let banner = ContextualConversation::auto_delete_banner(label_id.into(), &ctx).await?;
        Ok::<_, RealProtonMailError>(banner.map(Into::into))
    })
    .await
    .map_err(ActionError::from)
}

/// Updates the mobile conversation toolbar actions for the user.
///
/// This function allows updating the actions displayed in the conversation toolbar
/// when viewing conversations on mobile devices.
///
/// # Errors
///
/// Returns an error if the action queue operation fails or if the actions
/// are invalid for the conversation toolbar.
#[uniffi_export]
#[returns(VoidActionResult)]
pub async fn update_mobile_conversation_toolbar_actions(
    session: Arc<MailUserSession>,
    actions: Vec<MobileAction>,
) -> Result<(), ActionError> {
    let ctx = session.ctx()?;

    uniffi_async(async move {
        proton_mail_common::models::MailSettings::action_update_conversation_toolbar(
            ctx.action_queue(),
            actions.map_vec(),
            false,
        )
        .await
        .map_err(RealProtonMailError::from)
    })
    .await
    .map_err(ActionError::from)
}

/// Get the currently configured mobile conversation toolbar actions.
#[uniffi_export]
#[returns(MobileActionsResult)]
pub async fn get_mobile_conversation_toolbar_actions(
    session: Arc<MailUserSession>,
) -> Result<Vec<MobileAction>, ActionError> {
    let ctx = session.ctx()?;

    uniffi_async(async move {
        let tether = ctx.user_stash().connection();
        let actions = RealMobileAction::conversation_toolbar_actions(&tether).await?;
        Result::<_, RealProtonMailError>::Ok(
            actions
                .iter()
                .filter_map(MobileAction::from_real)
                .collect_vec(),
        )
    })
    .await
    .map_err(ActionError::from)
}

/// Get all available mobile conversation toolbar actions.
///
/// Returns the complete set of actions that can be configured for the conversation toolbar.
#[uniffi_export]
#[must_use]
pub fn get_all_mobile_conversation_actions() -> Vec<MobileAction> {
    let actions = RealMobileAction::all_conversation_actions();
    actions
        .iter()
        .filter_map(MobileAction::from_real)
        .collect_vec()
}

/// Set the default mobile conversation toolbar actions for the user.
///
/// This function sets the default actions for the conversation toolbar when viewing conversation on mobile devices.
#[uniffi_export]
#[returns(VoidActionResult)]
pub async fn set_default_mobile_conversation_toolbar_actions(
    session: Arc<MailUserSession>,
) -> Result<(), ActionError> {
    let ctx = session.ctx()?;
    let actions = RealMobileAction::default_chosen_actions();

    uniffi_async(async move {
        proton_mail_common::models::MailSettings::action_update_conversation_toolbar(
            ctx.action_queue(),
            actions.map_vec(),
            true,
        )
        .await
        .map_err(RealProtonMailError::from)
    })
    .await
    .map_err(ActionError::from)
}
