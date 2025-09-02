use std::collections::BTreeSet;
use std::time::Instant;

use super::SystemLabelId as _;
use super::folder_banner::{AutoDeleteBanner, AutoDeleteState, SpamOrTrash};
use crate::actions::{
    AllConversationActions, AllListActions, ConversationActionSheet, MovableSystemFolderAction,
};
use crate::datatypes::LocalConversationId;
use crate::datatypes::{
    AttachmentMetadata, CustomLabel, ExclusiveLocation, LocalMessageId, MessageRecipients,
    MessageSenders, MobileAction,
};
use crate::models::{
    Attachment, Conversation, ConversationLabel, MailSettings, Message, MessageLabel,
};
use crate::{AppError, MailContextResult, MailUserContext};
use futures::try_join;
use itertools::Itertools;
use proton_core_api::services::proton::LabelId;
use proton_core_api::session::Session;
use proton_core_common::datatypes::{LocalLabelId, UnixTimestamp};
use proton_core_common::models::{Label, LabelError, ModelExtension, ModelIdExtension as _, User};
use proton_core_common::services::NetworkMonitorService;
use proton_mail_api::services::proton::common::ConversationId;
use sqlite_watcher::watcher::TableObserver;
use stash::orm::Model;
use stash::params;
use stash::stash::{Stash, StashError, Tether, WatcherHandle};
use tracing::{debug, warn};

/// Contextual representation of a [`Conversation`] when it is opened for display
/// in a [`Label`].
///
/// The data contained in the [`ConversationLabel`] is superimposed over the
/// data in the [`Conversation`] to produce the correct information that needs
/// to be displayed to the client.
#[derive(Debug, Eq, PartialEq, Clone)]
pub struct ContextualConversation {
    /// Local id of the conversation.
    pub local_id: LocalConversationId,

    /// Remote id of the conversation.
    pub remote_id: Option<ConversationId>,

    /// Attachment metadata associated with this conversation.
    pub attachments_metadata: Vec<AttachmentMetadata>,

    /// List of custom labels.
    pub custom_labels: Vec<CustomLabel>,

    /// Whether a snooze reminder should be displayed.
    pub display_snooze_reminder: bool,

    /// Order in the list this conversation should be displayed.
    pub display_order: u64,

    /// Exclusive location of the [`Conversation`] (e.g. Inbox, Archive, Outbox
    /// etc.). This field is auto-calculated, and not stored in the database.
    /// When the model is read from database, this field should be calculated,
    /// and always be [`Some`]. If it is [`None`], it means either that the
    /// model is not fully initialized or there is very nasty bug. Failed
    /// initialization is logged as an error, but flow is not impacted due to
    /// the fact that this is not a critical field.
    pub exclusive_location: Option<ExclusiveLocation>,

    /// Time at which this conversation expires.
    pub expiration_time: UnixTimestamp,

    /// Whether this conversation is starred.
    pub is_starred: bool,

    /// Number of attachments on the conversation.
    pub num_attachments: u64,

    /// Number of messages in this context.
    pub num_messages: u64,

    /// Number of unread messages in this context.
    pub num_unread: u64,

    /// Number of messages in this conversation.
    pub total_messages: u64,

    /// Number of unread messages in this conversation.
    pub total_unread: u64,

    /// Address of the recipients of the messages contained within.
    pub recipients: MessageRecipients,

    /// Address of all the senders in the messages.
    pub senders: MessageSenders,

    /// Total size of all the messages.
    pub size: u64,

    /// Conversation subject.
    pub subject: String,

    /// Time of reception of the last message in this conversation.
    pub time: UnixTimestamp,

    /// Time of snooze of the conversation - It may not be snoozed at the moment.
    pub snooze_time: UnixTimestamp,

    /// When this conversation is snoozed until - when present it is snoozed at the moment.
    pub snoozed_until: Option<UnixTimestamp>,

    /// Whether the conversation has messages downloaded.
    pub has_messages: bool,
}

impl ContextualConversation {
    /// Create a new instance for a `conversation` and the `local_label_id` where
    /// the contextual information should be applied.
    ///
    /// If the `local_label_id` is not present in the `conversation`, `None` is
    /// returned. This means that the conversation is not present in this label.
    pub fn new(conversation: Conversation, local_label_id: LocalLabelId) -> Option<Self> {
        let label = conversation.label(local_label_id)?.clone();
        let is_starred = conversation.is_starred();
        let attachments_metadata = conversation.get_attachment_metadata();

        Some(Self {
            local_id: conversation.id(),
            remote_id: conversation.remote_id,
            attachments_metadata,
            custom_labels: conversation.custom_labels,
            display_order: conversation.display_order,
            display_snooze_reminder: conversation.display_snooze_reminder,
            exclusive_location: conversation.exclusive_location,
            expiration_time: label.context_expiration_time,
            is_starred,
            num_attachments: label.context_num_attachments,
            num_messages: label.context_num_messages,
            total_messages: conversation.num_messages,
            num_unread: label.context_num_unread,
            total_unread: conversation.num_unread,
            recipients: conversation.recipients,
            senders: conversation.senders,
            size: label.context_size,
            subject: conversation.subject,
            time: label.context_time,
            snooze_time: label.context_snooze_time,
            snoozed_until: conversation.snoozed_until,
            has_messages: conversation.has_messages,
        })
    }

    /// Load a conversation with `local_conversation_id` and the
    /// `local_label_id` where  the contextual information should be applied.
    ///
    /// If the `local_label_id` is not present in the `conversation`, `None` is
    /// returned. This means that the conversation is not present in this label.
    ///
    /// # Errors
    ///
    /// Returns error if conversation could not be loaded from the database.
    pub async fn load(
        local_conversation_id: LocalConversationId,
        local_label_id: LocalLabelId,
        tether: &Tether,
    ) -> Result<Option<Self>, StashError> {
        if let Some(conversation) = Conversation::find_first(
            "WHERE local_id = ? AND deleted = 0",
            params![local_conversation_id],
            tether,
        )
        .await?
        {
            Ok(Self::new(conversation, local_label_id))
        } else {
            Ok(None)
        }
    }

    /// Retrieve all the conversations which are the label with `local_label_id`.
    ///
    /// # Errors
    ///
    /// Returns error if the query fails.
    pub async fn in_label(
        local_label_id: LocalLabelId,
        tether: &Tether,
    ) -> Result<Vec<Self>, StashError> {
        Ok(Conversation::in_label(local_label_id, tether)
            .await?
            .into_iter()
            .filter_map(|c| Self::new(c, local_label_id))
            .collect())
    }

    /// Open a conversation in the context of a label.
    ///
    /// It acts as a wrapper around [`Self::conversation_and_messages`] and
    /// promotes the fact that the conversation is opened by a user in the context of a label.
    /// If thats not the case, use [`Self::conversation_and_messages`] instead.
    ///
    /// Note: This function will also mark the conversation as read if it has a snooze reminder,
    /// as a part of the snooze reminder logic.
    pub async fn open_conversation(
        local_conversation_id: LocalConversationId,
        local_label_id: LocalLabelId,
        ctx: &MailUserContext,
    ) -> Result<Option<ContextualConversationAndMessages>, AppError> {
        let stash = ctx.user_stash();
        let api = ctx.session();
        if let Some(conv_and_messages) = Self::conversation_and_messages(
            ctx.network_monitor_service(),
            local_conversation_id,
            local_label_id,
            stash,
            api,
        )
        .await?
        {
            if conv_and_messages.conversation.display_snooze_reminder {
                let queue = ctx.action_queue();
                if let Err(e) = Conversation::action_mark_read(
                    queue,
                    local_label_id,
                    vec![local_conversation_id],
                )
                .await
                {
                    tracing::error!("Failed to mark reminded conversation as read: {:?}", e);
                }
            }

            Ok(Some(conv_and_messages))
        } else {
            Ok(None)
        }
    }

    /// Retrieve the conversation with `local_conversation_id` in the
    /// context of `local_label_id` and its respective messages.
    ///
    /// This function also retrieve the messages from the server if
    /// they have never been synced before.
    ///
    /// # Error
    ///
    /// Returns error if the query failed, syncing the data failed or
    /// the conversation has no messages.
    #[tracing::instrument(skip(stash, api, network_monitor_service))]
    pub async fn conversation_and_messages(
        network_monitor_service: &NetworkMonitorService,
        local_conversation_id: LocalConversationId,
        local_label_id: LocalLabelId,
        stash: &Stash,
        api: &Session,
    ) -> Result<Option<ContextualConversationAndMessages>, AppError> {
        let t = Instant::now();
        let mut conn = stash.connection();
        let label = Label::find_by_id(local_label_id, &conn)
            .await?
            .ok_or(AppError::LabelNotFound(local_label_id))?;
        let conversation = match Conversation::sync_conversation_messages(
            network_monitor_service,
            local_conversation_id,
            &mut conn,
            api,
        )
        .await
        {
            Ok(conversation) => conversation,
            Err(AppError::ConversationNotFound(_)) => {
                return Ok(None);
            }
            Err(AppError::ConversationDoesNotExistOnServer(remote_id)) => {
                warn!("Conversation {remote_id:?} does not exist on the server");
                return Ok(None);
            }
            Err(e) => return Err(e),
        };
        let Some(conversation) = ContextualConversation::new(conversation, local_label_id) else {
            warn!(
                "Conversation synced, but could not be converted to contextual in the current label"
            );
            return Ok(None);
        };
        let messages = Message::in_conversation(local_conversation_id, &conn).await?;
        tracing::info!("Conversation has {:02} messages", messages.len());
        let id_to_open =
            Conversation::message_id_to_open(local_conversation_id, &label, &messages)?;

        debug!(
            "Obtained messages and conversations for {local_conversation_id} in {:?}",
            t.elapsed()
        );

        Ok(Some(ContextualConversationAndMessages {
            conversation,
            messages,
            message_id_to_open: id_to_open,
        }))
    }

    /// Watch a conversation with `local_conversation_id` in the context of
    /// `local_label_id`.
    ///
    /// A message is sent if the conversation or the conversation messages
    /// are updated.
    ///
    /// # Errors
    ///
    /// Returns error if the queries failed.
    pub fn watch(stash: &Stash) -> Result<WatcherHandle, StashError> {
        stash.subscribe_to(|sender| Box::new(ContextualConversationWatcher { sender }))
    }

    /// Get the available actions from bottom bar for given conversations
    ///
    #[tracing::instrument(skip_all, fields(label_id=current_label_id.as_u64()))]
    pub async fn all_available_list_actions_for_conversations(
        current_label_id: LocalLabelId,
        conversation_ids: Vec<LocalConversationId>,
        tether: &Tether,
    ) -> Result<AllListActions, AppError> {
        debug!("{conversation_ids:?}");
        let current_label_fut = async {
            Label::resolve_remote_label_id(current_label_id, tether)
                .await
                .map_err(AppError::from)
        };
        let conversations_fut = async {
            Conversation::find_by_ids(conversation_ids.to_vec(), tether)
                .await
                .map_err(AppError::from)
                .map(|convs| {
                    convs
                        .into_iter()
                        .filter_map(|conv| ContextualConversation::new(conv, current_label_id))
                        .collect_vec()
                })
        };

        let (inbox, archive, trash, spam, bottom_bar_actions, current_label, conversations) = try_join!(
            MovableSystemFolderAction::inbox(tether),
            MovableSystemFolderAction::archive(tether),
            MovableSystemFolderAction::trash(tether),
            MovableSystemFolderAction::spam(tether),
            MobileAction::list_toolbar_actions(tether),
            current_label_fut,
            conversations_fut
        )?;

        // Calculate state flags for the new builder
        let any_unread = conversations.iter().any(|c| c.num_unread > 0);
        let any_read = conversations.iter().any(|c| c.num_unread == 0);
        let any_starred = conversations.iter().any(|c| c.is_starred);
        let all_starred = conversations.iter().all(|c| c.is_starred);

        // Use the new unified from_context approach
        let actions = AllListActions::from_context(
            true, // is_conversation = true for conversations
            current_label,
            any_unread,
            any_read,
            any_starred,
            all_starred,
            &bottom_bar_actions,
            inbox,
            archive,
            trash,
            spam,
        );
        debug!("All available bottombar actions: {actions:?}");
        Ok(actions)
    }

    /// Get the available conversation actions for a single conversation (similar to Message::all_available_message_actions_for_message)
    ///
    /// # Errors
    ///
    /// Returns error if the database request fail or conversation is not found.
    ///
    #[tracing::instrument(skip_all, fields(label_id=%current_label_id, conversation_id=conversation_id.as_u64()))]
    pub async fn all_available_conversation_actions_for_conversation(
        current_label_id: LocalLabelId,
        conversation_id: LocalConversationId,
        tether: &Tether,
    ) -> Result<AllConversationActions, AppError> {
        debug!("Getting conversation actions for conversation: {conversation_id:?}");

        // Load the conversation to get its state
        let conversation = Self::load(conversation_id, current_label_id, tether).await?;
        if conversation.is_none() {
            warn!("Conversation not found: {conversation_id:?}");
            // Return empty actions for missing conversation
            return Ok(AllConversationActions {
                visible_list_actions: vec![],
                hidden_list_actions: vec![],
            });
        }
        let conversation = conversation.unwrap();

        let (inbox, archive, trash, spam, conversation_toolbar_actions) = try_join!(
            MovableSystemFolderAction::inbox(tether),
            MovableSystemFolderAction::archive(tether),
            MovableSystemFolderAction::trash(tether),
            MovableSystemFolderAction::spam(tether),
            MobileAction::conversation_toolbar_actions(tether)
        )?;
        let current_label = Label::resolve_remote_label_id(current_label_id, tether).await?;

        // Calculate state flags for the builder
        let any_unread = conversation.num_unread > 0;
        let any_read = conversation.num_unread == 0;
        let any_starred = conversation.is_starred;
        let all_starred = conversation.is_starred; // Single conversation, so any == all

        // Use the unified builder-based approach (AllConversationActions = AllListActions)
        let actions = AllListActions::from_context(
            true, // is_conversation = true
            current_label,
            any_unread,
            any_read,
            any_starred,
            all_starred,
            &conversation_toolbar_actions,
            inbox,
            archive,
            trash,
            spam,
        );

        debug!("all available conversation actions for conversation: {actions:?}");
        Ok(actions)
    }

    /// Get the available actions to populate the conversation action sheet.
    ///
    /// Conversation sheet contains context aware set of actions for given conversation.
    /// It is split up into different categories to be easy to display in the UI.
    ///
    /// # Errors
    ///
    /// Returns error if the database request fail.
    ///
    #[tracing::instrument(skip_all, fields(label_id=%current_label_id, conversation_id=conversation_id.as_u64()))]
    pub async fn all_available_conversation_actions_for_action_sheet(
        current_label_id: LocalLabelId,
        conversation_id: LocalConversationId,
        tether: &Tether,
    ) -> Result<ConversationActionSheet, AppError> {
        let actions = Self::all_available_conversation_actions_for_conversation(
            current_label_id,
            conversation_id,
            tether,
        )
        .await?;

        Ok(actions.into())
    }

    /// Gets the banner for folder autodelete.
    ///
    /// This can be called on any folder, it will only return the banner when it's in the
    /// correct folders.
    pub async fn auto_delete_banner(
        local_label_id: LocalLabelId,
        ctx: &MailUserContext,
    ) -> MailContextResult<Option<AutoDeleteBanner>> {
        let tether = &ctx.user_stash().connection();
        let user = ctx.user().await?;
        let user: &User = &user;
        let trash = Label::remote_id_counterpart(LabelId::trash(), tether)
            .await?
            .ok_or(LabelError::CouldNotResolveLocalLabel(LabelId::inbox()))?;
        let spam = Label::remote_id_counterpart(LabelId::spam(), tether)
            .await?
            .ok_or(LabelError::CouldNotResolveLocalLabel(LabelId::spam()))?;
        let folder = if trash == local_label_id {
            SpamOrTrash::Trash
        } else if spam == local_label_id {
            SpamOrTrash::Spam
        } else {
            return Ok(None);
        };
        let settings = MailSettings::get_or_default(tether).await;
        let state = if user.is_paying_for_mail() {
            match settings.auto_delete_spam_and_trash_days {
                None | Some(0) => AutoDeleteState::AutoDeleteDisabled,
                Some(_) => AutoDeleteState::AutoDeleteEnabled,
            }
        } else {
            AutoDeleteState::AutoDeleteUpsell
        };
        Ok(Some(AutoDeleteBanner { state, folder }))
    }
}

/// Result of calling [`ContextualConversation::conversation_and_messages`];
pub struct ContextualConversationAndMessages {
    /// The conversation
    pub conversation: ContextualConversation,

    /// The conversation's messages.
    pub messages: Vec<Message>,

    /// The id of message to display first.
    pub message_id_to_open: LocalMessageId,
}

pub struct ContextualConversationWatcher {
    sender: flume::Sender<()>,
}

impl TableObserver for ContextualConversationWatcher {
    fn tables(&self) -> Vec<String> {
        vec![
            Conversation::table_name().to_string(),
            ConversationLabel::table_name().to_string(),
            Message::table_name().to_string(),
            MessageLabel::table_name().to_string(),
            Label::table_name().to_string(),
            // This is needed for pgp attachments
            Attachment::table_name().to_string(),
        ]
    }

    fn on_tables_changed(&self, _changed_tables: &BTreeSet<String>) {
        self.sender
            .send(())
            .inspect_err(|e| {
                tracing::error!(
                    "Failed to send notification for ContextualConversationWatcher: {}",
                    e
                )
            })
            .ok();
    }
}
