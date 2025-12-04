use crate::core::datatypes::{Id, NonDefaultWeekStart, UnixTimestamp};
use crate::errors::{ActionError, MobileActionsResult, SnoozeError, VoidActionResult};
use crate::mail::datatypes::{
    AllConversationActions, AllListActions, AutoDeleteBanner, Conversation,
    ConversationActionSheet, LabelAsAction, LabelAsOutput, Message, MobileAction, MoveAction,
    SnoozeActions, Undo,
};
use crate::mail::mail_scroller::{
    ConversationScroller, ConversationScrollerLiveQueryCallback,
    spawn_conversation_scroller_watcher,
};
use crate::mail::{MailUserSession, Mailbox};
use crate::{LiveQueryCallback, WatchHandle, declare_live_query_tagger, uniffi_async};
use itertools::Itertools;
use proton_core_common::datatypes::{SystemLabel, WeekStart as RealWeekStart};
use proton_core_common::models::Label as RealLabel;
use proton_core_common::utils::MapVec;
use proton_mail_common::MailScroller;
use proton_mail_common::Unexpected;
use proton_mail_common::datatypes::{
    ContextualConversation, ContextualConversationAndMessages, ConversationViewOptions,
    LocalConversationId, MobileAction as RealMobileAction,
    OpenConversationOrigin as RealOpenConversationOrigin,
};
use proton_mail_common::models::Conversation as RealConversation;
use proton_mail_common::{
    ActionErrorReason as RealActionErrorReason, ProtonMailError as RealProtonMailError,
};
use stash::orm::Model;
use std::sync::Arc;

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

#[uniffi_export]
pub async fn available_label_as_actions_for_conversations(
    mailbox: Arc<Mailbox>,
    ids: Vec<Id>,
) -> Result<Vec<LabelAsAction>, ActionError> {
    let stash = mailbox.stash()?;
    uniffi_async(async move {
        let tether = stash.connection().await?;
        let actions = RealConversation::available_label_as_actions(ids.map_vec(), &tether)
            .await?
            .map_vec();

        Ok::<_, RealProtonMailError>(actions)
    })
    .await
    .map_err(ActionError::from)
}

#[uniffi_export]
pub async fn available_move_to_actions_for_conversations(
    mailbox: Arc<Mailbox>,
    ids: Vec<Id>,
) -> Result<Vec<MoveAction>, ActionError> {
    let stash = mailbox.stash()?;
    uniffi_async(async move {
        let view = mailbox.mbox().label_id();
        let tether = stash.connection().await?;
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
        let tether = stash.connection().await?;
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

#[uniffi_export]
pub async fn all_available_list_actions_for_conversations(
    mailbox: Arc<Mailbox>,
    conversation_ids: Vec<Id>,
) -> Result<AllListActions, ActionError> {
    let stash = mailbox.stash()?;
    uniffi_async(async move {
        let tether = stash.connection().await?;
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

#[uniffi_export]
pub async fn all_available_conversation_actions_for_action_sheet(
    mailbox: Arc<Mailbox>,
    conversation_id: Id,
) -> Result<ConversationActionSheet, ActionError> {
    let stash = mailbox.stash()?;
    let current_label_id = mailbox.label_id();
    uniffi_async(async move {
        let tether = stash.connection().await?;
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

#[uniffi_export]
pub async fn all_available_conversation_actions_for_conversation(
    mailbox: Arc<Mailbox>,
    conversation_id: Id,
) -> Result<AllConversationActions, ActionError> {
    let stash = mailbox.stash()?;
    let current_label_id = mailbox.label_id();
    uniffi_async(async move {
        let tether = stash.connection().await?;
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

#[uniffi_export]
pub async fn conversation(
    mailbox: Arc<Mailbox>,
    id: Id,
    show_all: bool,
) -> Result<Option<ConversationAndMessages>, ActionError> {
    let stash = mailbox.stash()?;
    let session = mailbox.session()?;
    let ctx = mailbox
        .ctx()
        .map_err(|_| RealProtonMailError::Unexpected(Unexpected::Internal))?;

    uniffi_async(async move {
        let trash_label_id = SystemLabel::Trash
            .local_id(&stash.connection().await?)
            .await?
            .expect("Trash label ID should be present");
        let view_options = if show_all {
            ConversationViewOptions::All
        } else {
            if mailbox.mbox().label_id() == trash_label_id {
                ConversationViewOptions::Trashed
            } else {
                ConversationViewOptions::NonTrashed
            }
        };
        Ok::<_, RealProtonMailError>(
            ContextualConversation::conversation_and_messages(
                ctx.network_monitor_service(),
                LocalConversationId::from(id),
                mailbox.mbox().label_id(),
                view_options,
                &stash,
                &session,
                ctx.rebaseable_queue().await,
            )
            .await?
            .map(Into::into),
        )
    })
    .await
    .map_err(ActionError::from)
    .map_err(Into::into)
}

#[derive(uniffi::Record)]
pub struct ConversationAndMessages {
    pub conversation: Conversation,
    pub message_id_to_open: Id,
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

/// Retrieve a conversation by local ID.
///
/// Notably, this retrieves a local conversation that has been saved in the
/// database. It does not use the network.
#[uniffi_export]
pub async fn load_conversation(
    session: Arc<MailUserSession>,
    id: Id,
    label_id: Id,
) -> Result<Option<Conversation>, ActionError> {
    let stash = session.user_stash()?;
    uniffi_async(async move {
        let tether = stash.connection().await?;
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
        .map_err(RealProtonMailError::from)?;
        Ok::<_, RealProtonMailError>(())
    })
    .await
    .map_err(ActionError::from)
    .into()
}

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
        .map_err(RealProtonMailError::from)?;
        Ok::<_, RealProtonMailError>(())
    })
    .await
    .map_err(ActionError::from)
    .into()
}

#[uniffi_export]
pub async fn move_conversations(
    mailbox: Arc<Mailbox>,
    label_id: Id,
    ids: Vec<Id>,
) -> Result<Option<Arc<Undo>>, ActionError> {
    let ctx = mailbox.ctx()?;
    uniffi_async(async move {
        let tether = ctx.user_stash().connection().await?;
        RealConversation::action_move(&tether, ctx.action_queue(), label_id.into(), ids.map_vec())
            .await
            .map(|undo| undo.map(|undo| Arc::new(undo.into())))
            .map_err(RealProtonMailError::from)
    })
    .await
    .map_err(ActionError::from)
    .into()
}

#[uniffi_export]
pub async fn scroll_conversations_for_label(
    mailbox: Arc<Mailbox>,
    callback: Box<dyn ConversationScrollerLiveQueryCallback>,
) -> Result<Arc<ConversationScroller>, ActionError> {
    let context = mailbox.ctx()?;

    uniffi_async(async move {
        let label_id = mailbox.label_id();
        let (scroller, handle) =
            MailScroller::conversations(context.as_weak(), label_id.into(), 50).await?;

        let handle = spawn_conversation_scroller_watcher(&context, handle, callback);
        let scroller = ConversationScroller::new(scroller, handle);

        Result::<_, RealProtonMailError>::Ok(Arc::new(scroller))
    })
    .await
    .map_err(ActionError::from)
}

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

#[derive(uniffi::Record)]
pub struct WatchedConversation {
    pub conversation: Conversation,
    pub messages: Vec<Message>,
    pub message_id_to_open: Id,
    pub handle: Arc<WatchHandle>,
}

#[derive(Default, uniffi::Enum)]
pub enum OpenConversationOrigin {
    #[default]
    Default,
    PushNotification,
}

impl From<RealOpenConversationOrigin> for OpenConversationOrigin {
    fn from(origin: RealOpenConversationOrigin) -> Self {
        match origin {
            RealOpenConversationOrigin::Default => OpenConversationOrigin::Default,
            RealOpenConversationOrigin::PushNotification => {
                OpenConversationOrigin::PushNotification
            }
        }
    }
}

impl From<OpenConversationOrigin> for RealOpenConversationOrigin {
    fn from(origin: OpenConversationOrigin) -> Self {
        match origin {
            OpenConversationOrigin::PushNotification => {
                RealOpenConversationOrigin::PushNotification
            }
            OpenConversationOrigin::Default => RealOpenConversationOrigin::Default,
        }
    }
}

declare_live_query_tagger!(WatchConversationMarker);

#[uniffi_export]
pub async fn watch_conversation(
    mailbox: Arc<Mailbox>,
    id: Id,
    origin: OpenConversationOrigin,
    show_all: bool,
    callback: Box<dyn LiveQueryCallback>,
) -> Result<Option<WatchedConversation>, ActionError> {
    let ctx = mailbox.ctx()?;
    let label_id = mailbox.label_id();

    uniffi_async(async move {
        let stash = ctx.user_stash();
        let trash_label_id = SystemLabel::Trash
            .local_id(&stash.connection().await?)
            .await?
            .expect("Trash label ID should be present");
        let view_options = if show_all {
            ConversationViewOptions::All
        } else if label_id == trash_label_id.into() {
            ConversationViewOptions::Trashed
        } else {
            ConversationViewOptions::NonTrashed
        };
        let Some(conv_and_msgs) = ContextualConversation::open_conversation(
            id.into(),
            label_id.into(),
            view_options.into(),
            &ctx,
            origin.into(),
        )
        .await?
        else {
            return Ok(None);
        };

        let receiver = ContextualConversation::watch(&stash).await?;
        let watcher = WatchConversationMarker::watch_channel(&*ctx, receiver, callback);

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

#[derive(uniffi::Record)]
pub struct WatchedConversations {
    pub conversations: Vec<Conversation>,
    pub handle: Arc<WatchHandle>,
}

declare_live_query_tagger!(WatchConversationsForLabelMarker);

#[uniffi_export]
pub async fn watch_conversations_for_label(
    session: Arc<MailUserSession>,
    label_id: Id,
    callback: Box<dyn LiveQueryCallback>,
) -> Result<WatchedConversations, ActionError> {
    let user_context = session.ctx()?;
    let stash = session.user_stash()?;
    uniffi_async(async move {
        let tether = stash.connection().await?;
        let conversations = RealConversation::in_label(label_id.into(), &tether).await?;
        let receiver = ContextualConversation::watch(&stash).await?;
        let watcher =
            WatchConversationsForLabelMarker::watch_channel(&*user_context, receiver, callback);
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
                &ctx.user_stash().connection().await?,
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

declare_live_query_tagger!(WatchAvailableMoveToActionsMarker);

#[uniffi_export]
pub async fn watch_available_move_to_actions(
    mailbox: Arc<Mailbox>,
    callback: Box<dyn LiveQueryCallback>,
) -> Result<Arc<WatchHandle>, ActionError> {
    let ctx = mailbox.ctx()?;
    let stash = mailbox.stash()?;
    uniffi_async(async move {
        let handle = RealLabel::watch(&stash).await?;
        let handle = WatchAvailableMoveToActionsMarker::watch_channel(&*ctx, handle, callback);
        Result::<_, RealProtonMailError>::Ok(handle)
    })
    .await
    .map_err(ActionError::from)
}

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

#[uniffi_export]
#[returns(MobileActionsResult)]
pub async fn get_mobile_conversation_toolbar_actions(
    session: Arc<MailUserSession>,
) -> Result<Vec<MobileAction>, ActionError> {
    let ctx = session.ctx()?;

    uniffi_async(async move {
        let tether = ctx.user_stash().connection().await?;
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

#[uniffi_export]
#[must_use]
pub fn get_all_mobile_conversation_actions() -> Vec<MobileAction> {
    let actions = RealMobileAction::all_conversation_actions();
    actions
        .iter()
        .filter_map(MobileAction::from_real)
        .collect_vec()
}

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
