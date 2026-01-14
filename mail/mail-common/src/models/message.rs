#[cfg(test)]
#[path = "../tests/models/messages.rs"]
mod messages;

mod message_body;
mod message_mime_type;
pub use self::message_body::*;
pub use self::message_mime_type::*;
use crate::actions::messages::DeleteAllMessagesInLabel;
use crate::actions::messages::Ham;
use crate::actions::messages::Read;
use crate::actions::messages::ReportPhishing;
use crate::actions::messages::Unread;
use crate::actions::messages::{Delete, UndoLabelAsArchiveMessages};
use crate::actions::messages::{LabelAs, UndoLabelAsMessages};
use crate::actions::messages::{Move, UndoMoveToMessages};
use crate::actions::{
    ActionMoveData, AllListActions, AllMessageActions, LabelAsData, LabelAsOutput, LabelPair,
    MovableSystemFolderAction, Undo,
};
use crate::datatypes::ConversationViewOptions;
use crate::datatypes::MimeType;
use crate::models::*;
use crate::{MailContextError, find_in_query};
use futures::try_join;
use indoc::{formatdoc, indoc};
use proton_action_queue::action::ActionGroup;
use proton_action_queue::action::MetadataBuilder;
use proton_action_queue::enqueue;
use proton_action_queue::queue::MultiActionError;
use proton_action_queue::queue::{ActionError as QueueActionError, Queue, QueuedActionOutput};
use proton_core_api::session::Session;
use proton_core_common::utils::MapVec as _;
use proton_sqlite3::rusqlite::Transaction;
use proton_sqlite3::rusqlite::params_from_iter;
use sqlite_watcher::watcher::TableObserver;
use stash::exports::Connection;
use stash::orm::DbRecord;
use stash::rusqlite::OptionalExtension;
use stash::utils::{ConnectionExt, MapToSql, placeholders, placeholders_n};

use crate::MailContextResult;
use crate::actions::{
    ConversationOrMessage, LabelAsAction, MessageActionSheet, MoveAction, filter_responses,
};
use crate::datatypes::{
    AttachmentMetadata, CustomLabel, Disposition, EncryptedMessageBody, ExclusiveLocation,
    LocalMessageId, MessageFlags, MessageLabelsCount, MessageRecipients, MessageSender,
    MobileAction, ParsedHeaders, ReadFilter, RollbackItemType, SystemLabelId,
};
use crate::datatypes::{LocalConversationId, ParsedHeaderValue};
use crate::decrypted_message::ThemeOpts;
use crate::mailbox::decrypted_message::DecryptedMessageBody;
use crate::{AppError, MailUserContext};
use anyhow::{Context, anyhow};
use itertools::Itertools;
use proton_action_queue::rebase::RebaseChangeSet;
use proton_core_api::service::ApiServiceError;
use proton_core_api::services::proton::{AddressId, LabelId};
use proton_core_api::services::proton::{PrivateEmail, PrivateString};
use proton_core_common::RebasableQueue;
use proton_core_common::datatypes::{
    LabelType, LocalAddressId, LocalLabelId, SystemLabel, UnixTimestamp,
};
use proton_core_common::event_loop::events::Action;
use proton_core_common::models::{Address, Label, LabelError, ModelExtension, ModelIdExtension};
use proton_crypto_inbox::proton_crypto;
use proton_mail_api::MAX_PAGE_ELEMENT_COUNT;
use proton_mail_api::services::proton::ProtonMail;
use proton_mail_api::services::proton::common::{ConversationId, ExternalId, MessageId};
use proton_mail_api::services::proton::prelude::{
    MessageMetadata, MessageReplyTo as ApiMessageReplyTo,
};
use proton_mail_api::services::proton::requests::GetMessagesOptions;
use proton_mail_api::services::proton::response_data::{
    Message as ApiMessage, MessageBody as ApiMessageBody, MessageMetadata as ApiMessageMetadata,
    OperationResult,
};
use proton_mail_api::services::proton::responses::GetMessagesResponse;
use proton_mail_common_derive::ScrollerEq;
use stash::exports::ToSql;
use stash::macros::{DbRecord, Model};
use stash::orm::{Model, ModelHooks};
use stash::params;
use stash::stash::{Bond, RunTransaction, Stash, StashError, Tether, WatcherHandle};
use std::collections::HashSet;
use std::collections::hash_map::Entry as HmEntry;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::future::Future;
use std::sync::Arc;
use tracing::{debug, error, info, trace, warn};

#[derive(Clone, Debug, Eq, Model, PartialEq, ScrollerEq)]
#[TableName("messages")]
#[ModelHooks]
pub struct Message {
    #[IdField(autoincrement)]
    pub local_id: Option<LocalMessageId>,

    #[DbField]
    pub remote_id: Option<MessageId>,

    #[DbField]
    pub local_conversation_id: Option<LocalConversationId>,

    #[DbField]
    #[scroller_eq(skip)]
    pub remote_conversation_id: Option<ConversationId>,

    #[DbField]
    pub local_address_id: LocalAddressId,

    #[DbField]
    #[scroller_eq(skip)]
    pub remote_address_id: AddressId,

    pub attachments_metadata: Vec<AttachmentMetadata>,

    #[DbField]
    pub cc_list: MessageRecipients,

    #[DbField]
    pub bcc_list: MessageRecipients,

    #[DbField]
    pub deleted: bool,

    #[scroller_eq(skip)]
    pub location: Option<ExclusiveLocation>,

    /// The unix timestamp at which this message is set to expire at.
    /// 0 means that it will not expire.
    #[DbField]
    pub expiration_time: UnixTimestamp,

    #[DbField]
    pub external_id: Option<ExternalId>,

    #[DbField]
    pub flags: MessageFlags,

    #[DbField]
    pub is_forwarded: bool,

    #[DbField]
    pub is_replied: bool,

    #[DbField]
    pub is_replied_all: bool,

    /// You shouldn't add or remove labels from this field as some things will not get updated.
    /// If you want to modify this use [`Message::apply_label`]
    pub label_ids: Vec<LabelId>,

    #[DbField]
    pub num_attachments: u32,

    #[DbField]
    pub display_order: u64,

    #[DbField]
    pub sender: MessageSender,

    #[DbField]
    #[scroller_eq(skip)]
    pub size: u64,

    #[DbField]
    pub snooze_time: UnixTimestamp,

    #[DbField]
    pub subject: String,

    #[DbField]
    pub time: UnixTimestamp,

    #[DbField]
    pub to_list: MessageRecipients,

    #[DbField]
    pub unread: bool,

    pub custom_labels: Vec<CustomLabel>,
}

impl ModelIdExtension for Message {
    type RemoteId = MessageId;

    fn remote_id(&self) -> Option<&Self::RemoteId> {
        self.remote_id.as_ref()
    }
}

type LabelAsResult = Result<QueuedActionOutput<LabelAs>, QueueActionError<LabelAs>>;

impl Message {
    /// Open a message in the either context of a label or a conversation.
    ///
    /// It acts as a wrapper around [`Self::load`] and promotes the fact that the message is opened by a user in the context of a label.
    /// If thats not the case, use [`Self::load`] instead.
    ///
    /// Note: This function will also mark the message as read if it has a snooze reminder,
    /// as a part of the snooze reminder logic.
    pub async fn open_message(
        local_message_id: LocalMessageId,
        ctx: &MailUserContext,
    ) -> Result<Option<Message>, AppError> {
        let tether = ctx.user_stash().connection().await?;
        if let Some(message) = Message::load(local_message_id, &tether).await? {
            if message.display_snooze_reminder() {
                let queue = ctx.action_queue();
                if let Err(e) = Message::action_mark_read(queue, vec![message.id()]).await {
                    tracing::error!("Failed to mark reminded message as read: {:?}", e);
                }
            }
            Ok(Some(message))
        } else {
            Ok(None)
        }
    }

    pub async fn action_star(queue: &Queue, ids: Vec<LocalMessageId>) -> LabelAsResult {
        let tether = queue.stash().connection().await?;

        let label_id = Label::remote_id_counterpart(LabelId::starred(), &tether)
            .await?
            .expect("Star system label not found");

        Self::action_apply_label(queue, label_id, ids).await
    }

    pub async fn action_unstar(queue: &Queue, ids: Vec<LocalMessageId>) -> LabelAsResult {
        let tether = queue.stash().connection().await?;

        let label_id = Label::remote_id_counterpart(LabelId::starred(), &tether)
            .await?
            .expect("Star system label not found");

        Self::action_remove_label(queue, label_id, ids).await
    }

    pub async fn action_remove_label(
        queue: &Queue,
        label: LocalLabelId,
        ids: Vec<LocalMessageId>,
    ) -> LabelAsResult {
        let action = LabelAs(LabelAsData::new_remove(
            ids.into_iter().map(|id| LabelPair { label, id }).collect(),
        ));
        queue.queue_action(action).await
    }

    pub async fn action_apply_label(
        queue: &Queue,
        label: LocalLabelId,
        ids: Vec<LocalMessageId>,
    ) -> LabelAsResult {
        let action = LabelAs(LabelAsData::new_add(
            ids.into_iter().map(|id| LabelPair { label, id }).collect(),
        ));
        queue.queue_action(action).await
    }

    pub async fn action_mark_read(
        queue: &Queue,
        message_ids: Vec<LocalMessageId>,
    ) -> Result<QueuedActionOutput<Read>, QueueActionError<Read>> {
        let action = Read::new(message_ids);
        queue.queue_action(action).await
    }

    pub async fn action_mark_unread(
        queue: &Queue,
        message_ids: Vec<LocalMessageId>,
    ) -> Result<QueuedActionOutput<Unread>, QueueActionError<Unread>> {
        let action = Unread::new(message_ids);
        queue.queue_action(action).await
    }

    pub async fn action_delete(
        queue: &Queue,
        label_id: LocalLabelId,
        message_ids: Vec<LocalMessageId>,
    ) -> Result<QueuedActionOutput<Delete>, QueueActionError<Delete>> {
        let action = Delete::new(label_id, message_ids);
        queue.queue_action(action).await
    }

    pub async fn action_move(
        tether: &Tether,
        queue: &Queue,
        destination_id: LocalLabelId,
        target_ids: Vec<LocalMessageId>,
    ) -> Result<Option<Undo>, MailContextError> {
        if let Some(action) = ActionMoveData::new(tether, destination_id, target_ids).await? {
            let action = Move(action);
            let QueuedActionOutput { local, id } = queue.queue_action(action).await?;
            Ok(Some(Undo::MessagesMoveTo(UndoMoveToMessages {
                action: local,
                id,
            })))
        } else {
            Ok(None)
        }
    }

    pub async fn mark_multiple_as_read(
        ids: impl IntoIterator<Item = LocalMessageId>,
        bond: &Bond<'_>,
    ) -> Result<(), StashError> {
        for id in ids {
            if let Some(mut message) = Message::load(id, bond).await? {
                message.unread = false;
                message.save(bond).await?;
            }
        }
        Ok(())
    }

    pub async fn action_ham(
        queue: &Queue,
        message_ids: Vec<LocalMessageId>,
    ) -> Result<(), MultiActionError> {
        let tether = &queue.stash().connection().await?;
        let inbox = Label::resolve_local_label_id(LabelId::inbox(), tether)
            .await
            .context("inbox doesn't exist?")?;

        let move_action = ActionMoveData::new(tether, inbox, message_ids.iter().copied())
            .await?
            .context("No input")?;

        let _id = enqueue!(queue, [Move(move_action), Ham::new(message_ids)])?;

        Ok(())
    }

    pub async fn action_report_phishing(
        queue: &Queue,
        message_id: LocalMessageId,
        tether: &Tether,
    ) -> anyhow::Result<()> {
        let spam = Label::resolve_local_label_id(LabelId::spam(), tether).await?;

        let move_action = ActionMoveData::new(tether, spam, [message_id])
            .await?
            .context("No input")?;
        let _id = enqueue!(queue, [Move(move_action), ReportPhishing::new(message_id)])?;

        Ok(())
    }

    pub async fn action_label_as(
        tether: &Tether,
        queue: &Queue,
        source_label_id: LocalLabelId,
        message_ids: Vec<LocalMessageId>,
        selected_label_ids: Vec<LocalLabelId>,
        partially_selected_label_ids: Vec<LocalLabelId>,
        must_archive: bool,
    ) -> Result<LabelAsOutput, AppError> {
        let all_labels = Label::local_ids_by_kind(LabelType::Label, tether).await?;
        let cartesian =
            MessageLabel::find_by_conversations_and_labels(&message_ids, &all_labels, tether)
                .await?;

        let label_as_action = {
            let action = LabelAs(LabelAsData::new(
                cartesian
                    .into_iter()
                    .map(|x| LabelPair {
                        label: x.local_label_id,
                        id: x.local_message_id,
                    })
                    .collect(),
                source_label_id,
                message_ids.clone(),
                &selected_label_ids,
                &partially_selected_label_ids,
                &all_labels,
            ));
            if action.0.is_empty() {
                None
            } else {
                Some(action)
            }
        };

        let move_action = if must_archive {
            let archive = Label::resolve_local_label_id(LabelId::archive(), tether).await?;
            ActionMoveData::new(tether, archive, message_ids)
                .await?
                .map(Move)
        } else {
            None
        };

        // There are 4 possibilities:
        // - No labels   && no archive -> do nothing,  no undo
        // - some labels && no archive -> queue label, undo
        // - no labels   && archive    -> just move,   undo
        // - some labels && archive    -> queue both,  undo

        let output = match (label_as_action, move_action) {
            (None, None) => {
                warn!("No labels && no archive -> noop");
                LabelAsOutput {
                    input_label_is_empty: false,
                    undo: None,
                }
            }
            (Some(label_as_action), None) => {
                debug!("some labels && no archive -> queue label, undo");

                let action_output = queue
                    .queue_action(label_as_action.clone())
                    .await
                    .context("Error labeling locally")?;

                LabelAsOutput {
                    input_label_is_empty: action_output.local,
                    undo: Some(Undo::MessagesLabelAs(UndoLabelAsMessages {
                        action: label_as_action,
                        id: action_output.id,
                        must_archive: None,
                    })),
                }
            }
            (None, Some(move_action)) => {
                debug!("no labels && archive -> just move, undo");
                let QueuedActionOutput { local, id } = queue
                    .queue_action(move_action)
                    .await
                    .context("Error queuing move to archive")?;

                let undo = Some(Undo::MessagesMoveTo(UndoMoveToMessages {
                    action: local,
                    id,
                }));

                LabelAsOutput {
                    input_label_is_empty: false,
                    undo,
                }
            }
            (Some(label_as_action), Some(move_action)) => {
                debug!("some labels && archive -> queue both, undo");
                let queued_label_as = queue
                    .queue_action(label_as_action.clone())
                    .await
                    .context("Error queuing move to archive")?;

                let meta = MetadataBuilder::new()
                    .with_dependency(queued_label_as.id)
                    .build();

                let queued_move = queue
                    .queue_action_with_metadata(move_action, meta)
                    .await
                    .context("Error queuing with move to archive dependency")?;
                let undo = Some(Undo::MessagesLabelAs(UndoLabelAsMessages {
                    action: label_as_action,
                    id: queued_label_as.id,
                    must_archive: Some(UndoLabelAsArchiveMessages {
                        id: queued_move.id,
                        action: queued_move.local,
                    }),
                }));

                LabelAsOutput {
                    input_label_is_empty: queued_label_as.local,
                    undo,
                }
            }
        };

        Ok(output)
    }

    pub(crate) async fn find_by_ids(
        message_ids: impl IntoIterator<Item = LocalMessageId>,
        tether: &Tether,
    ) -> Result<Vec<Self>, StashError> {
        let (query, params) = find_in_query!("WHERE deleted = 0 AND local_id IN ({})", message_ids);
        Message::find(query, params, tether).await
    }

    #[tracing::instrument(skip_all, fields(label_id=current_label_id.as_u64()))]
    pub async fn all_available_list_actions_for_messages(
        current_label_id: LocalLabelId,
        message_ids: Vec<LocalMessageId>,
        tether: &Tether,
    ) -> Result<AllListActions, AppError> {
        debug!("{message_ids:?}");

        let inbox = MovableSystemFolderAction::inbox(tether).await?;
        let archive = MovableSystemFolderAction::archive(tether).await?;
        let trash = MovableSystemFolderAction::trash(tether).await?;
        let spam = MovableSystemFolderAction::spam(tether).await?;
        let list_actions = MobileAction::list_toolbar_actions(tether).await?;
        let current_label = Label::resolve_remote_label_id(current_label_id, tether).await?;
        let messages = Self::find_by_ids(message_ids.to_vec(), tether).await?;

        let any_unread = messages.iter().any(|m| m.unread);
        let any_read = messages.iter().any(|m| !m.unread);
        let any_starred = messages.iter().any(|m| m.is_starred());
        let all_starred = messages.iter().all(|m| m.is_starred());

        let actions = AllListActions::from_context(
            false, // is_conversation = false for messages
            current_label,
            any_unread,
            any_read,
            any_starred,
            all_starred,
            &list_actions,
            inbox,
            archive,
            trash,
            spam,
        );

        debug!("all available bottom bar actions for messages: {actions:?}");

        Ok(actions)
    }

    #[tracing::instrument(skip_all, fields(message_id=message_id.as_u64()))]
    pub async fn all_available_message_actions_for_message(
        current_label_id: LocalLabelId,
        message_id: LocalMessageId,
        theme: ThemeOpts,
        tether: &Tether,
    ) -> Result<AllMessageActions, AppError> {
        debug!("Getting message actions for message: {message_id:?}");

        let message = Self::load(message_id, tether).await?;

        if message.is_none() {
            warn!("Message not found: {message_id:?}");

            return Ok(AllMessageActions {
                visible_message_actions: vec![],
                hidden_message_actions: vec![],
            });
        }
        let message = message.unwrap();

        let (inbox, archive, trash, spam, message_toolbar_actions) = try_join!(
            MovableSystemFolderAction::inbox(tether),
            MovableSystemFolderAction::archive(tether),
            MovableSystemFolderAction::trash(tether),
            MovableSystemFolderAction::spam(tether),
            MobileAction::message_toolbar_actions(tether)
        )?;
        let current_label = Label::resolve_remote_label_id(current_label_id, tether).await?;

        let actions = AllMessageActions::from_context(
            current_label,
            message.unread,
            message.is_starred(),
            message.can_reply(),
            message.can_reply() && (message.to_list.len() + message.cc_list.len() > 1),
            Some(theme),
            &message_toolbar_actions,
            inbox,
            archive,
            trash,
            spam,
        );

        debug!("all available message actions for message: {actions:?}");
        Ok(actions)
    }

    pub async fn get_sender_address(
        id: LocalMessageId,
        tether: &Tether,
    ) -> Result<Option<MessageSender>, StashError> {
        tether
            .query_value_opt(
                formatdoc!(
                    "SELECT sender FROM {} WHERE local_id = ?",
                    Self::table_name()
                ),
                params![id],
            )
            .await
    }

    /// Return the boolean value indicating if the message sender is blocked.
    ///
    /// When message is not present in database, it will return `None`.
    /// Otherwise, it will return `Some(bool)` where `true` means the sender is blocked
    /// and `false` means the sender is not blocked.
    ///
    pub async fn is_sender_blocked(
        id: LocalMessageId,
        tether: &Tether,
    ) -> Result<Option<bool>, StashError> {
        let message_sender = Self::get_sender_address(id, tether).await?;
        let Some(message_sender) = message_sender else {
            return Ok(None);
        };
        let incoming_default =
            IncomingDefault::by_email(message_sender.address.into_clear_text_string(), tether)
                .await?
                .map(|i| i.location);
        let is_blocked = incoming_default == Some(IncomingDefaultLocation::Blocked);

        Ok(Some(is_blocked))
    }

    /// Save a non existing message to the database.
    ///
    /// This method is complementary way to store message. It only will proceed
    /// with messages that are not yet present in database. This functionality
    /// is required due to multiprocess nature of mail application and the possibility to
    /// view mailboxes without interfering with processes triggered by the user.
    ///
    /// Method also gives back existing message if it was not saved.
    ///
    pub async fn create_or_get_local(
        &mut self,
        rebase_change_set: &mut RebaseChangeSet,
        rebase_feature_enabled: bool,
        bond: &Bond<'_>,
    ) -> Result<(), StashError> {
        if !rebase_feature_enabled
            && let Some(remote_id) = self.remote_id.clone()
            && let Some(existing) = Self::find_by_remote_id(remote_id, bond).await?
        {
            *self = existing;

            tracing::trace!(
                remote_id = ?self.remote_id,
                "Skipping saving message, we already have it in the local DB"
            );

            return Ok(());
        }

        self.save(bond).await?;
        rebase_change_set.add(self.id());
        Ok(())
    }

    fn set_coversation_before_save(&mut self, tx: &Transaction<'_>) -> Result<(), StashError> {
        if self.local_conversation_id.is_none()
            && let Some(remote_conversation_id) = &self.remote_conversation_id
        {
            if let Some(conversation) =
                Conversation::find_by_remote_id_sync(remote_conversation_id, tx)?
            {
                self.local_conversation_id = conversation.local_id;
            } else {
                // Create an unknown entry.
                let mut conversation = Conversation::unknown(remote_conversation_id.clone());
                conversation.save_sync(tx)?;
                self.local_conversation_id = conversation.local_id;
            }
        }

        Ok(())
    }

    pub async fn create_or_update_messages_from_metadata_vec(
        metadata: Vec<ApiMessageMetadata>,
        event_action: Option<Action>,
        bond: &Bond<'_>,
    ) -> Result<Vec<Message>, AppError> {
        let mut messages = Vec::with_capacity(metadata.len());

        for metadata in metadata {
            if Self::sync_decision(&metadata, event_action, bond).await?
                == MessageSyncDecision::Skip
            {
                continue;
            }
            let mut message = Message::from_api_metadata(metadata, bond).await?;
            Self::save(&mut message, bond).await?;
            messages.push(message);
        }

        Ok(messages)
    }

    pub async fn create_or_update_messages_from_metadata(
        metadata: Vec<ApiMessageMetadata>,
        event_action: Option<Action>,
        bond: &Bond<'_>,
    ) -> Result<Vec<LocalMessageId>, AppError> {
        Ok(
            Self::create_or_update_messages_from_metadata_vec(metadata, event_action, bond)
                .await?
                .into_iter()
                .filter_map(|x| x.local_id)
                .collect(),
        )
    }

    pub async fn mark_deleted(ids: Vec<LocalMessageId>, bond: &Bond<'_>) -> Result<(), AppError> {
        info!("Marking {ids:?} as deleted");
        let (query, params) = find_in_query!("WHERE deleted = 0 AND local_id IN ({})", ids);
        let messages = Message::find(query, params, bond).await?;
        let mut messages_by_conversation = HashMap::new();

        for mut message in messages {
            message.deleted = true;
            message.save(bond).await?;
            messages_by_conversation
                .entry(message.local_conversation_id)
                .or_insert_with(Vec::new)
                .push(message);
        }

        for (conversation_id, messages) in messages_by_conversation {
            let all_stats =
                Message::update_message_counters_after_soft_delete(messages, bond).await?;
            let conversation = Conversation::find_first(
                "WHERE local_id=? AND deleted=0 AND is_known=1",
                params![conversation_id],
                bond,
            )
            .await?;

            if let Some(mut conversation) = conversation {
                let label_ids = all_stats.keys().copied().collect::<Vec<_>>();
                let (query, mut params) = find_in_query!(
                    "WHERE local_conversation_id=? AND deleted=0 AND local_label_id IN ({})",
                    label_ids
                );
                params.insert(0, Box::new(conversation.id()) as Box<dyn ToSql + Send>);

                let conv_labels = ConversationLabel::find(query, params, bond).await?;
                let all_mail_stats = SystemLabel::AllMail
                    .local_id(bond)
                    .await?
                    .and_then(|id| all_stats.get(&id));

                conversation
                    .mark_delete_update_stats(all_mail_stats, bond)
                    .await?;

                for mut conv_label in conv_labels {
                    let label_id = &conv_label.local_label_id.unwrap();
                    conv_label
                        .mark_delete_update_stats(all_stats.get(label_id), bond)
                        .await?;
                }
            }
        }

        Ok(())
    }

    pub async fn mark_undeleted(ids: Vec<LocalMessageId>, bond: &Bond<'_>) -> Result<(), AppError> {
        info!("Unmarking {ids:?} as deleted");
        let (query, params) = find_in_query!("WHERE deleted = 1 AND local_id IN ({})", ids);
        let messages = Message::find(query, params, bond).await?;
        let mut messages_by_conversation = HashMap::new();

        for mut message in messages {
            message.deleted = false;
            message.save(bond).await?;
            messages_by_conversation
                .entry(message.local_conversation_id)
                .or_insert_with(Vec::new)
                .push(message);
        }

        for (conversation_id, messages) in messages_by_conversation {
            let all_stats =
                Message::update_message_counters_after_soft_undelete(messages, bond).await?;
            let conversation =
                Conversation::find_first("WHERE local_id=?", params![conversation_id], bond)
                    .await?;

            if let Some(mut conversation) = conversation {
                if conversation.deleted {
                    for (label_id, stats) in all_stats.iter() {
                        conversation
                            .add_conversation_to_label(*label_id, Some(stats), bond)
                            .await?;
                    }
                }

                let label_ids = all_stats.keys().copied().collect::<Vec<_>>();
                let (query, mut params) = find_in_query!(
                    "WHERE local_conversation_id=? AND deleted=0 AND local_label_id IN ({})",
                    label_ids
                );
                params.insert(0, Box::new(conversation.id()) as Box<dyn ToSql + Send>);

                let conv_labels = ConversationLabel::find(query, params, bond).await?;
                let all_mail_stats = SystemLabel::AllMail
                    .local_id(bond)
                    .await?
                    .and_then(|id| all_stats.get(&id));

                conversation
                    .mark_undelete_update_stats(all_mail_stats, bond)
                    .await?;

                for mut conv_label in conv_labels {
                    let label_id = &conv_label.local_label_id.unwrap();

                    conv_label
                        .mark_undelete_update_stats(all_stats.get(label_id), bond)
                        .await?;
                }
            }
        }

        Ok(())
    }

    pub async fn fetch_counts<PM: ProtonMail>(
        api: &PM,
    ) -> Result<Vec<MessageLabelsCount>, ApiServiceError> {
        api.get_messages_count().await.map(|r| r.counts.map_vec())
    }

    pub async fn fetch_metadata<PM: ProtonMail>(
        filter: GetMessagesOptions,
        api: &PM,
    ) -> Result<GetMessagesResponse, ApiServiceError> {
        api.get_messages(filter).await
    }

    pub fn all_message_labels(&self, conn: &Connection) -> Result<Vec<Label>, StashError> {
        let labels = Label::find_sync(
            r#"
            WHERE local_id IN (
                SELECT local_label_id FROM message_labels WHERE local_message_id = ?
            ) ORDER BY label_type DESC, display_order ASC
            "#,
            (self.local_id,),
            conn,
        )?;

        Ok(labels)
    }

    #[inline]
    #[must_use]
    pub fn is_starred(&self) -> bool {
        self.label_ids.iter().any(|l| *l == LabelId::starred())
    }

    pub async fn sync_first_message_page<PM: ProtonMail>(
        label_id: LabelId,
        count: usize,
        api: &PM,
        tether: &mut Tether,
    ) -> Result<(), AppError> {
        let response = api
            .get_messages(GetMessagesOptions {
                desc: Some(true),
                label_id: Some(vec![label_id]),
                page: 0,
                page_size: count.min(MAX_PAGE_ELEMENT_COUNT) as u64,
                ..Default::default()
            })
            .await?;

        debug!(
            "Fetched {} messages TOTAL={}",
            response.messages.len(),
            response.total
        );

        tether
            .tx(async |tx| {
                Self::create_or_update_messages_from_metadata(response.messages, None, tx).await
            })
            .await?;
        Ok(())
    }

    #[tracing::instrument(skip_all, fields(label_id=%current_label_id, message_id=message_id.as_u64()))]
    pub async fn all_available_message_actions_for_action_sheet(
        current_label_id: LocalLabelId,
        message_id: LocalMessageId,
        theme: ThemeOpts,
        tether: &Tether,
    ) -> Result<MessageActionSheet, AppError> {
        let actions = Self::all_available_message_actions_for_message(
            current_label_id,
            message_id,
            theme,
            tether,
        )
        .await?;

        Ok(actions.into())
    }

    #[tracing::instrument(skip_all)]
    pub async fn available_label_as_actions(
        message_ids: Vec<LocalMessageId>,
        tether: &Tether,
    ) -> Result<Vec<LabelAsAction>, AppError> {
        if message_ids.is_empty() {
            return Err(AppError::EmptyListOfMessages);
        }

        debug!("{message_ids:?}");

        let all_label_as = Label::find_by_kind(LabelType::Label, tether).await?;
        let messages = Message::find(
            format!(
                "WHERE local_id IN ({})",
                message_ids.iter().map(ToString::to_string).join(",")
            ),
            vec![],
            tether,
        )
        .await?;

        let all_label_as_actions = messages.into_iter().flat_map(|message| {
            LabelAsAction::vec(all_label_as.iter(), |label| {
                message
                    .custom_labels
                    .iter()
                    .map(|label| Some(label.local_id))
                    .contains(&label.local_id)
            })
        });

        let res = LabelAsAction::finalize(all_label_as_actions);
        debug!("Available label_as actions for messages: {res:?}");
        Ok(res)
    }

    pub async fn watch(stash: &Stash) -> Result<WatcherHandle, StashError> {
        stash
            .subscribe_to(|sender| Box::new(MessageWatcher { sender }))
            .await
    }

    #[tracing::instrument(skip_all)]
    pub async fn watch_available_label_as_actions(
        message_ids: Vec<LocalMessageId>,
        tether: &Tether,
    ) -> Result<(Vec<LabelAsAction>, WatcherHandle), AppError> {
        if message_ids.is_empty() {
            return Err(AppError::EmptyListOfMessages);
        }

        debug!("{message_ids:?}");

        let handle = tether.subscribe_to(|sender| Box::new(MessageWatcher { sender }))?;

        let all_label_as = Label::find_by_kind(LabelType::Label, tether).await?;
        let messages = <Message as ModelExtension>::find_by_ids(message_ids, tether).await?;
        let all_label_as_actions = messages.iter().flat_map(|message| {
            LabelAsAction::vec(all_label_as.iter(), |label| {
                message
                    .custom_labels
                    .iter()
                    .map(|label| Some(label.local_id))
                    .contains(&label.local_id)
            })
        });

        let res = LabelAsAction::finalize(all_label_as_actions);
        debug!("available label_as actions for messages: {res:?}");
        Ok((res, handle))
    }

    #[tracing::instrument(skip_all, fields(label_id=view.id().as_u64()))]
    pub async fn available_move_to_actions(
        view: Label,
        message_ids: Vec<LocalMessageId>,
        tether: &Tether,
    ) -> Result<Vec<MoveAction>, AppError> {
        if message_ids.is_empty() {
            return Err(AppError::EmptyListOfMessages);
        }

        debug!("{message_ids:?}");

        let all_system = Label::find_by_kind(LabelType::System, tether).await?;
        let all_system_excluding_view = all_system
            .iter()
            .filter(|label| label.local_id != view.local_id);
        let all_custom_folders = Label::find_by_kind(LabelType::Folder, tether).await?;
        let all_move_to_actions = MoveAction::vec(
            all_system_excluding_view
                .clone()
                .chain(all_custom_folders.iter()),
        );

        let res = MoveAction::finalize(all_move_to_actions, tether).await?;
        debug!("available label_as actions for messages: {res:?}");
        Ok(res)
    }

    /// Gets the body of a message from a message id.
    ///
    /// This will attempt to fetch the message data from the servers if it has
    /// not yet been downloaded before.
    #[tracing::instrument(skip(user_context))]
    pub async fn message_body(
        user_context: &MailUserContext,
        id: LocalMessageId,
    ) -> MailContextResult<DecryptedMessageBody> {
        let tether = &mut user_context.user_stash().connection().await?;
        let saved_message = Message::load(id, tether)
            .await?
            .ok_or(AppError::MessageMissing(id))?;

        saved_message.fetch_message_body(user_context, tether).await
    }

    #[tracing::instrument(skip(user_context))]
    pub async fn message_body_with_sender(
        user_context: &MailUserContext,
        id: LocalMessageId,
    ) -> MailContextResult<(PrivateEmail, DecryptedMessageBody)> {
        let tether = &mut user_context.user_stash().connection().await?;
        let saved_message = Message::load(id, tether)
            .await?
            .ok_or(AppError::MessageMissing(id))?;

        let sender = saved_message.sender.address.clone();
        let body = saved_message
            .fetch_message_body(user_context, tether)
            .await?;

        Ok((sender, body))
    }

    #[tracing::instrument(skip_all, fields(message_id=%self.id()))]
    pub async fn fetch_message_body(
        &self,
        ctx: &MailUserContext,
        tx: impl RunTransaction,
    ) -> Result<DecryptedMessageBody, MailContextError> {
        self.fetch_message_body_impl(ctx, tx, true, true).await
    }

    #[tracing::instrument(skip_all, fields(message_id=%self.id()))]
    pub async fn prefetch_message_body(
        &self,
        ctx: &MailUserContext,
        tx: impl RunTransaction,
    ) -> Result<DecryptedMessageBody, MailContextError> {
        self.fetch_message_body_impl(ctx, tx, false, false).await
    }

    async fn fetch_message_body_impl(
        &self,
        ctx: &MailUserContext,
        mut tx: impl RunTransaction,
        with_attachment_prefetching: bool,
        with_network_check: bool,
    ) -> Result<DecryptedMessageBody, MailContextError> {
        if let Some(decrypted) = Self::load_decrypted_message_from_cache(
            ctx.as_arc(),
            self.id(),
            &self.remote_address_id,
            tx.tether(),
        )
        .await?
        {
            debug!("Found message body in cache.");
            return Ok(decrypted);
        }
        debug!("Message body not in cache. Fetching...");

        let Some(remote_id) = self.remote_id.clone() else {
            return Err(AppError::MessageHasNoRemoteId(self.id()).into());
        };

        if with_network_check && ctx.network_monitor_service().is_os_offline() {
            debug!("No connection, skipping sync");
            return Err(MailContextError::Api(ApiServiceError::NetworkError(
                "No connection".to_owned(),
            )));
        }

        let (_, encrypted_body) = Self::sync_message_and_body(
            remote_id,
            ctx.session(),
            &mut tx,
            ctx.rebaseable_queue().await,
        )
        .await?;

        trace!("Message successfully downloaded. Decrypting...");

        let decrypted = Self::decrypt_message_body(
            ctx,
            &self.remote_address_id,
            encrypted_body,
            tx.tether(),
            with_attachment_prefetching,
        )
        .await?;

        info!("Message successfully synced.");
        Ok(decrypted)
    }

    pub async fn delete_expired(tether: &mut Tether) -> Result<(), AppError> {
        let ids = Self::find_ids(
            r"
        WHERE
          expiration_time < STRFTIME('%s', 'NOW')
          AND expiration_time != 0
          AND deleted = 0
        ",
            vec![],
            tether,
        )
        .await?;

        if !ids.is_empty() {
            tether
                .tx(async |tx| Self::mark_deleted(ids, tx).await)
                .await?;
        }

        Ok(())
    }

    pub fn mark_read_or_unread(
        mark_read: bool,
        ids: &[LocalMessageId],
        tx: &Transaction<'_>,
    ) -> Result<Vec<LocalMessageId>, StashError> {
        let t0 = std::time::Instant::now();
        struct IdPair {
            local_message_id: LocalMessageId,
            local_conversation_id: LocalConversationId,
        }

        let mut conversation_count_changed = HashMap::new();

        let mut updated: Vec<IdPair> = Vec::with_capacity(ids.len());

        let msgs = Message::find_sync(
            format!(
                "WHERE local_id IN ({placeholders}) AND unread = {unread}",
                placeholders = placeholders(ids),
                unread = if mark_read { 1 } else { 0 }
            ),
            params_from_iter(ids),
            tx,
        )?;

        // update unread flag
        for mut msg in msgs {
            info!(
                "Marking {:?} as {}",
                msg.id(),
                if mark_read { "read" } else { "unread" }
            );
            msg.unread = !mark_read;
            if mark_read {
                // Reset snooze state
                msg.set_display_snooze_reminder(false);
            }
            msg.save_sync(tx)?;
            updated.push(IdPair {
                local_message_id: msg.id(),
                local_conversation_id: msg.local_conversation_id.unwrap(),
            });
            *conversation_count_changed
                .entry(msg.local_conversation_id.expect("Should be set"))
                .or_insert(0) += 1;
        }

        for (conversation_id, count) in conversation_count_changed {
            if let Some(mut conversation) = Conversation::load_by_id_sync(conversation_id, tx)? {
                if mark_read {
                    conversation.display_snooze_reminder = false;
                    conversation.num_unread = conversation.num_unread.saturating_sub(count);
                } else {
                    conversation.num_unread += count;
                }
                conversation.save_sync(tx)?;
            }
        }

        if updated.is_empty() {
            // Nothing was changed.
            return Ok(vec![]);
        }

        // Publish updates for all affected ids.

        // Messages Counters
        for id_pair in &updated {
            let counters = MessageCounter::find_sync(
                indoc! {"
                    WHERE local_label_id IN (
                        SELECT local_label_id FROM message_labels
                        WHERE local_message_id=?
                    )"},
                (id_pair.local_message_id,),
                tx,
            )?;
            for mut counter in counters {
                if mark_read {
                    counter.unread = counter.unread.saturating_sub(1);
                } else {
                    counter.unread += 1;
                }

                counter.save_sync(tx)?
            }
        }

        let mut label_ids: HashMap<LocalLabelId, u64> = HashMap::new();
        // Update conversation labels
        for id_pair in &updated {
            let mut conversation_labels = ConversationLabel::find_sync(
                indoc! {
                "WHERE local_conversation_id=? AND local_label_id IN (
                    SELECT local_label_id FROM message_labels WHERE local_message_id=?
                )"},
                (id_pair.local_conversation_id, id_pair.local_message_id),
                tx,
            )?;
            for conversation_label in &mut conversation_labels {
                if mark_read {
                    conversation_label.context_num_unread =
                        conversation_label.context_num_unread.saturating_sub(1);

                    if conversation_label.context_num_unread == 0 {
                        *label_ids
                            .entry(conversation_label.local_label_id.unwrap())
                            .or_insert(0) += 1;
                    }
                } else {
                    conversation_label.context_num_unread += 1;

                    if conversation_label.context_num_unread == 1 {
                        *label_ids
                            .entry(conversation_label.local_label_id.unwrap())
                            .or_insert(0) += 1;
                    }
                }
                conversation_label.save_sync(tx)?
            }
        }

        for (label_id, count) in label_ids {
            // Update conversation label counts.
            if let Some(mut counters) = ConversationCounter::load_by_id_sync(label_id, tx)? {
                if mark_read {
                    counters.unread = counters.unread.saturating_sub(count);
                } else {
                    counters.unread += count;
                }
                counters.save_sync(tx)?;
            }
        }

        info!(%mark_read, "took {:?}", t0.elapsed());
        Ok(updated.into_iter().map(|x| x.local_message_id).collect())
    }

    pub async fn from_api_data(
        value: ApiMessage,
        tether: &Tether,
    ) -> Result<(Self, MessageBodyMetadata, String), AppError> {
        let remote_address_id = value.metadata.address_id.clone();
        let remote_message_id = value.metadata.id.clone();
        let remote_conversation_id = value.metadata.conversation_id.clone();
        let metadata = Message::from_api_metadata(value.metadata, tether).await?;
        let (body_metadata, body) = MessageBodyMetadata::from_api_message_body(
            value.body,
            remote_message_id,
            remote_conversation_id,
            remote_address_id,
        );

        Ok((metadata, body_metadata, body))
    }

    pub async fn from_api_metadata(
        value: ApiMessageMetadata,
        tether: &Tether,
    ) -> Result<Self, AppError> {
        let exclusive_location =
            ExclusiveLocation::from_label_ids(&value.label_ids, tether).await?;

        Ok(Self {
            local_id: None,
            remote_id: Some(value.id),
            local_conversation_id: None,
            remote_conversation_id: Some(value.conversation_id),
            local_address_id: Address::remote_id_counterpart(value.address_id.clone(), tether)
                .await?
                .ok_or_else(|| {
                    AppError::LocalIdNotFound(
                        "Address".to_owned(),
                        value.address_id.clone().into_inner(),
                    )
                })?,
            remote_address_id: value.address_id,
            attachments_metadata: value
                .attachments_metadata
                .into_iter()
                .map(AttachmentMetadata::from)
                .collect(),
            bcc_list: MessageRecipients {
                value: value.bcc_list.map_vec(),
            },
            cc_list: MessageRecipients {
                value: value.cc_list.map_vec(),
            },
            deleted: false,
            display_order: value.order,
            expiration_time: value.expiration_time.into(),
            external_id: value.external_id,
            flags: value.flags.into(),
            is_forwarded: value.is_forwarded,
            is_replied: value.is_replied,
            is_replied_all: value.is_replied_all,
            location: exclusive_location,
            label_ids: value.label_ids,
            num_attachments: value.num_attachments,
            sender: value.sender.into(),
            size: value.size,
            snooze_time: value.snooze_time.into(),
            subject: value.subject,
            time: value.time.into(),
            to_list: MessageRecipients {
                value: value.to_list.map_vec(),
            },
            unread: value.unread,
            custom_labels: vec![],
        })
    }

    pub async fn in_label(
        local_label_id: LocalLabelId,
        tether: &Tether,
    ) -> Result<Vec<Self>, StashError> {
        Message::find(
            formatdoc!(
                "
                JOIN message_labels
                    ON messages.local_id = message_labels.local_message_id
                WHERE
                    message_labels.local_label_id = ?
                    AND messages.deleted = 0
                ORDER BY messages.time DESC, display_order DESC
                "
            ),
            params![local_label_id],
            tether,
        )
        .await
    }

    pub async fn ids_in_label(
        local_label_id: LocalLabelId,
        tether: &Tether,
    ) -> Result<Vec<LocalMessageId>, StashError> {
        tether
            .query_values::<_, LocalMessageId>(
                indoc!(
                    "
                SELECT local_id
                FROM messages
                JOIN message_labels
                    ON messages.local_id = message_labels.local_message_id
                WHERE
                    message_labels.local_label_id = ?
                    AND messages.deleted = 0
                ORDER BY messages.time DESC, display_order DESC
                "
                ),
                params![local_label_id],
            )
            .await
    }

    pub async fn ids_in_label_with_deleted(
        local_label_id: LocalLabelId,
        tether: &Tether,
    ) -> Result<Vec<LocalMessageId>, StashError> {
        tether
            .query_values::<_, LocalMessageId>(
                indoc!(
                    "
                SELECT local_message_id FROM message_labels
                    WHERE message_labels.local_label_id = ?
                "
                ),
                params![local_label_id],
            )
            .await
    }

    pub async fn in_conversation(
        local_conversation_id: LocalConversationId,
        view_options: ConversationViewOptions,
        tether: &Tether,
    ) -> Result<Vec<Self>, StashError> {
        let template_query = if view_options.is_all() {
            String::new()
        } else {
            let trash_id = SystemLabel::Trash.remote_id();
            formatdoc!(
                "(SELECT DISTINCT message_labels.local_message_id
                    FROM message_labels
                    WHERE message_labels.local_label_id == (SELECT local_id FROM labels WHERE remote_id = {trash_id}))"
            )
        };
        let view_options = match view_options {
            ConversationViewOptions::All => template_query,
            ConversationViewOptions::NonTrashed => {
                format!("AND local_id NOT IN {template_query}")
            }
            ConversationViewOptions::Trashed => {
                format!("AND local_id IN {template_query}")
            }
        };

        Message::find(
            formatdoc!(
                "WHERE local_conversation_id = ? AND messages.deleted = 0
                {view_options}
                ORDER BY time ASC, display_order ASC",
            ),
            params![local_conversation_id],
            tether,
        )
        .await
    }

    async fn split_request<F, Fut>(
        ids: impl IntoIterator<Item = MessageId>,
        endpoint: F,
    ) -> Result<Vec<OperationResult<MessageId>>, ApiServiceError>
    where
        F: Fn(Vec<MessageId>) -> Fut,
        Fut: Future<Output = Result<Vec<OperationResult<MessageId>>, ApiServiceError>>,
    {
        split_request(ids, 150, endpoint).await
    }

    pub async fn update_snooze_time_with_conv_id(
        conversation_id: LocalConversationId,
        label_id: LocalLabelId,
        snooze_time: UnixTimestamp,
        bond: &Bond<'_>,
    ) -> Result<Vec<LocalMessageId>, StashError> {
        bond.query_values::<_, LocalMessageId>(
            indoc! {
            "UPDATE messages SET snooze_time = MAX(time, ?)
                WHERE flags & ? AND local_conversation_id =? AND local_id IN (
                    SELECT local_message_id FROM message_labels WHERE local_label_id =?
                )
                RETURNING local_id"
                },
            params![
                snooze_time,
                MessageFlags::RECEIVED,
                conversation_id,
                label_id
            ],
        )
        .await
    }

    pub async fn update_message_counters_after_soft_delete(
        messages: impl IntoIterator<Item = Message>,
        bond: &Bond<'_>,
    ) -> Result<HashMap<LocalLabelId, MessageLabelStats>, StashError> {
        let label_stats = MessageLabelStats::build(messages, bond).await?;
        for (label_id, stats) in label_stats.iter() {
            if let Some(mut counters) = MessageCounter::find_by_id(*label_id, bond).await? {
                counters.total = counters.total.saturating_sub(stats.count);
                counters.unread = counters.unread.saturating_sub(stats.unread_count);
                counters.save(bond).await?;
            }
        }

        Ok(label_stats)
    }

    pub async fn update_message_counters_after_soft_undelete(
        messages: impl IntoIterator<Item = Message>,
        bond: &Bond<'_>,
    ) -> Result<HashMap<LocalLabelId, MessageLabelStats>, StashError> {
        let label_stats = MessageLabelStats::build(messages, bond).await?;
        for (label_id, stats) in label_stats.iter() {
            if let Some(mut counters) = MessageCounter::find_by_id(*label_id, bond).await? {
                counters.total += stats.count;
                counters.unread += stats.unread_count;
                counters.save(bond).await?;
            }
        }

        Ok(label_stats)
    }

    pub async fn next_display_order(tether: &Tether) -> Result<u64, StashError> {
        Ok(tether
            .query_value::<_, u64>(
                format!(
                    "SELECT IFNULL(MAX(display_order),0) FROM {}",
                    Self::table_name()
                ),
                vec![],
            )
            .await?
            .saturating_add(1))
    }

    pub fn get_attachment_metadata(&self) -> Vec<AttachmentMetadata> {
        self.attachments_metadata
            .iter()
            .filter(|mdata| matches!(mdata.disposition, Disposition::Attachment))
            .cloned()
            .collect()
    }

    pub async fn sync_metadata<PM: ProtonMail>(
        ids: Vec<MessageId>,
        api: &PM,
        mut tx: impl RunTransaction,
    ) -> Result<Vec<Self>, AppError> {
        let remote_msgs = Self::fetch_metadata(
            GetMessagesOptions {
                ids: ids.into_iter().map_into().collect(),
                ..Default::default()
            },
            api,
        )
        .await?
        .messages;
        let mut local_msgs = Vec::with_capacity(remote_msgs.len());

        tx.run_tx(async |tx| {
            for msg in remote_msgs {
                let mut remote_msg = Message::from_api_metadata(msg, tx).await?;

                if !remote_msg.is_local_draft(tx).await? {
                    remote_msg.save(tx).await?;
                }
                local_msgs.push(remote_msg);
            }
            Ok(())
        })
        .await?;

        Ok(local_msgs)
    }

    #[tracing::instrument(skip(ctx, tether, with_attachment_prefetch))]
    pub async fn force_sync_message_and_body(
        ctx: &MailUserContext,
        message_id: MessageId,
        with_attachment_prefetch: bool,
        tether: &mut Tether,
    ) -> MailContextResult<(Message, DecryptedMessageBody)> {
        tracing::info!("Force syncing");

        let (message, encrypted) = Self::sync_message_and_body(
            message_id,
            ctx.session(),
            tether,
            ctx.rebaseable_queue().await,
        )
        .await?;

        let decrypted = Self::decrypt_message_body(
            ctx,
            &message.remote_address_id,
            encrypted,
            tether.tether(),
            with_attachment_prefetch,
        )
        .await?;

        Ok((message, decrypted))
    }

    #[tracing::instrument(skip(api, tx, queue))]
    async fn sync_message_and_body(
        message_id: MessageId,
        api: &Session,
        tx: &mut impl RunTransaction,
        queue: RebasableQueue<'_>,
    ) -> Result<(Message, EncryptedMessageBody), MailContextError> {
        info!("Fetching message");
        let message = api.get_message(message_id).await.map(|v| v.message)?;

        let (mut message, mut body_metadata, body) = Message::from_api_data(message, tx.tether())
            .await
            .inspect_err(|e| {
                error!("Failed to convert message from api: {e:?}");
            })?;

        tx.run_tx(async |tx| {
            message.save(tx).await.inspect_err(|e| {
                error!("Failed to save message metadata: {e:?}");
            })?;

            body_metadata.save(tx).await.inspect_err(|e| {
                error!("Failed to save message body metadata: {e:?}");
            })?;

            let rebase_change_set = RebaseChangeSet::from(message.id());
            if let Err(e) = queue
                .rebase_in(ActionGroup::default(), &rebase_change_set, tx)
                .await
            {
                tracing::error!("Failed to rebase: {e}")
            }

            Ok(())
        })
        .await
        .map_err(MailContextError::Other)?;

        info!("Message saved with {:?}", message.id());

        Ok((
            message,
            EncryptedMessageBody {
                encrypted_body: body,
                metadata: body_metadata,
            },
        ))
    }

    async fn decrypt_message_body(
        ctx: &MailUserContext,
        address_id: &AddressId,
        encrypted_message_body: EncryptedMessageBody,
        tether: &Tether,
        attachment_prefetch: bool,
    ) -> Result<DecryptedMessageBody, MailContextError> {
        let pgp = proton_crypto::new_pgp_provider();
        let address_keys = ctx.unlocked_address_keys(&pgp, tether, address_id).await?;

        encrypted_message_body
            .decrypt_and_store(ctx, address_id, address_keys, pgp, attachment_prefetch)
            .await
    }

    #[tracing::instrument(skip(ctx, tether))]
    pub(crate) async fn load_decrypted_message_from_cache(
        ctx: Arc<MailUserContext>,
        local_id: LocalMessageId,
        address_id: &AddressId,
        tether: &Tether,
    ) -> Result<Option<DecryptedMessageBody>, MailContextError> {
        let Some(metadata) = MessageBodyMetadata::for_message(local_id, tether)
            .await
            .context("Failed to retrieve message body metadata from db")?
        else {
            return Ok(None);
        };

        let Some(msg) = RawMessageBody::load(local_id, tether)
            .await
            .context("Failed to retrieve decrypted message body from db")?
        else {
            return Ok(None);
        };

        Ok(Some(DecryptedMessageBody::from_raw_message_body(
            ctx,
            metadata,
            address_id.clone(),
            msg,
        )))
    }

    #[must_use]
    pub fn is_draft(&self) -> bool {
        self.flags.is_draft()
            && self
                .label_ids
                .iter()
                .any(|l| *l == LabelId::all_drafts() || *l == LabelId::drafts())
    }

    /// Whether this message is a draft and has been modified locally.
    pub async fn is_local_draft(&self, tether: &Tether) -> Result<bool, AppError> {
        let local_id = match self.local_id {
            Some(local_id) => local_id,
            None if self.remote_id.is_some() => {
                let Some(local_id) =
                    Message::remote_id_counterpart(self.remote_id.clone().unwrap(), tether).await?
                else {
                    return Ok(false);
                };
                local_id
            }
            _ => {
                return Err(AppError::Label(LabelError::LabelWithoutIds));
            }
        };

        Ok(DraftMetadata::find_by_message_id(local_id, tether)
            .await?
            .is_some())
    }

    #[tracing::instrument(skip(ctx))]
    pub async fn find_or_fetch_by_remote_id(
        ctx: &MailUserContext,
        remote_id: MessageId,
    ) -> MailContextResult<LocalMessageId> {
        let tether = &mut ctx.user_stash().connection().await?;
        if let Some(message) = Self::find_by_remote_id(remote_id.clone(), tether).await? {
            return Ok(message.id());
        }
        tracing::debug!("Message does not exist, fetching");
        let result = Message::sync_metadata(vec![remote_id], ctx.session(), tether).await?;
        if result.len() != 1 {
            return Err(MailContextError::Other(anyhow!(
                "Failed to sync message from server"
            )));
        }
        tracing::info!("Message metadata sync with {:?}", result[0].id());
        Ok(result[0].id())
    }

    /// Bulk check unread status for messages by remote IDs.
    ///
    /// Returns a Vec<bool> where each boolean corresponds to the unread status
    /// of the message at the same index in the input remote_ids Vec.
    /// For messages that don't exist in the database, returns true (treating them as unread).
    ///
    /// This method is designed to work offline-only and is primarily used for
    /// iOS push notification clearing logic.
    pub async fn bulk_unread_status_by_remote_ids(
        remote_ids: Vec<MessageId>,
        tether: &Tether,
    ) -> Result<Vec<bool>, StashError> {
        if remote_ids.is_empty() {
            return Ok(Vec::new());
        }

        let query = format!(
            "SELECT remote_id, unread FROM messages WHERE remote_id IN ({}) AND deleted = 0",
            placeholders(&remote_ids)
        );

        let remote_ids_for_query = remote_ids.clone();
        let found_statuses: HashMap<MessageId, bool> = tether
            .sync_query(move |conn| {
                let mut stmt = conn.prepare(&query)?;
                let params: Vec<&dyn ToSql> = remote_ids_for_query
                    .iter()
                    .map(|id| id as &dyn ToSql)
                    .collect();
                let rows = stmt.query_map(params.as_slice(), |row| {
                    let remote_id: MessageId = row.get(0)?;
                    let unread: bool = row.get(1)?;
                    Ok((remote_id, unread))
                })?;

                let mut result = HashMap::new();
                for row in rows {
                    let (remote_id, unread) = row?;
                    result.insert(remote_id, unread);
                }
                Ok(result)
            })
            .await?;

        let results = remote_ids
            .iter()
            .map(|id| found_statuses.get(id).copied().unwrap_or(true))
            .collect();

        Ok(results)
    }

    pub async fn set_flags(
        local_id: LocalMessageId,
        flags: MessageFlags,
        bond: &Bond<'_>,
    ) -> Result<(), StashError> {
        bond.execute(
            indoc! {
                "UPDATE messages SET flags = flags | ? WHERE local_id = ?"
            },
            params![flags, local_id],
        )
        .await?;
        Ok(())
    }

    pub async fn unset_flags(
        local_id: LocalMessageId,
        flags: MessageFlags,
        bond: &Bond<'_>,
    ) -> Result<(), StashError> {
        bond.execute(
            indoc! {
                "UPDATE messages SET flags = flags & ~? WHERE local_id = ?"
            },
            params![flags, local_id],
        )
        .await?;
        Ok(())
    }

    pub async fn action_delete_all_in_label(
        queue: &Queue,
        label_id: LocalLabelId,
        tether: &Tether,
    ) -> Result<
        Option<QueuedActionOutput<DeleteAllMessagesInLabel>>,
        QueueActionError<DeleteAllMessagesInLabel>,
    > {
        let action = DeleteAllMessagesInLabel::new(label_id, tether)
            .await
            .map_err(Into::into)
            .map_err(QueueActionError::Action)?;

        if let Some(action) = action {
            Ok(Some(queue.queue_action(action).await?))
        } else {
            Ok(None)
        }
    }

    #[must_use]
    pub fn is_scheduled_for_send(&self) -> bool {
        self.label_ids.contains(&LabelId::all_scheduled()) && self.flags.is_schedule_send()
    }

    #[must_use]
    pub fn is_sent(&self) -> bool {
        (self.label_ids.contains(&LabelId::all_sent()) || self.label_ids.contains(&LabelId::sent()))
            && self.flags.is_sent()
    }

    /// Returns whether this message contains an RSVP invitation.
    ///
    /// Since this function doesn't parse the invitation[1], it's possible it
    /// returns a false-positive - case in point, we'll return `true` for all
    /// mails that contain an attachment called `invite.ics` even if this
    /// attachment isn't really an invitation.
    ///
    /// This is good enough as showing potential "whoopsie, not really an rsvp"
    /// message is a UI-problem.
    ///
    /// [1] loading attachments is asynchronous, while we need for this function
    ///     to be synchronous, because we need to know rsvp-ness when displaying
    ///     an email list (i.e. no time to actually load and parse all the
    ///     attachments)
    pub fn is_rsvp(&self) -> bool {
        self.attachments_metadata
            .iter()
            .any(|att| att.mime_type.is_calendar())
    }

    pub async fn update_ids_and_display_order(
        id: LocalMessageId,
        local_conversation_id: LocalConversationId,
        display_order: u64,
        message_id: MessageId,
        conversation_id: ConversationId,
        tx: &Bond<'_>,
    ) -> Result<(), StashError> {
        tx.execute(
            formatdoc! {"
            UPDATE {} SET
                display_order = ?,
                remote_id =?,
                remote_conversation_id =?,
                local_conversation_id =?
            WHERE local_id = ?
        ", Message::table_name()},
            params![
                display_order,
                message_id,
                conversation_id,
                local_conversation_id,
                id
            ],
        )
        .await?;
        Ok(())
    }

    pub fn can_reply(&self) -> bool {
        !self.label_ids.iter().any(|label_id| {
            *label_id == LabelId::all_scheduled()
                || *label_id == LabelId::outbox()
                || *label_id == LabelId::drafts()
                || *label_id == LabelId::all_drafts()
        })
    }

    pub async fn mark_unread_async(
        ids: impl IntoIterator<Item = LocalMessageId>,
        bond: &Bond<'_>,
    ) -> Result<Vec<LocalMessageId>, StashError> {
        let ids = Vec::from_iter(ids);
        bond.sync_bridge(move |tx| Self::mark_read_or_unread(false, &ids, tx))
            .await
    }

    pub fn display_snooze_reminder(&self) -> bool {
        self.flags.display_snooze_reminder()
    }

    pub fn set_display_snooze_reminder(&mut self, value: bool) {
        self.flags.set(MessageFlags::DISPLAY_SNOOZE_REMINDER, value);
    }

    pub fn snoozed_until(&self) -> Option<UnixTimestamp> {
        self.label_ids
            .iter()
            .find(|&label_id| *label_id == LabelId::snoozed())
            .map(|_| self.snooze_time)
    }

    pub(crate) async fn save_scroller_messages(
        api_messages: Vec<ApiMessageMetadata>,
        rebase_change_set: &mut RebaseChangeSet,
        has_rebase_feature: bool,
        unresoled_label_ids: &HashSet<LabelId>,
        tx: &Bond<'_>,
    ) -> Result<Vec<Message>, MailContextError> {
        let mut messages = Vec::with_capacity(api_messages.len());
        for api_message in api_messages {
            let Some(message) = (if Message::sync_decision(&api_message, None, tx).await?
                == MessageSyncDecision::Skip
            {
                Message::find_by_remote_id(api_message.id.clone(), tx).await?
            } else {
                let mut message = Message::from_api_metadata(api_message, tx).await?;
                message.prune_unresolved_labels(unresoled_label_ids);
                message
                    .create_or_get_local(rebase_change_set, has_rebase_feature, tx)
                    .await?;
                Some(message)
            }) else {
                continue;
            };
            messages.push(message)
        }

        Ok(messages)
    }

    pub(crate) async fn sync_decision(
        metadata: &ApiMessageMetadata,
        event_action: Option<Action>,
        tx: &Bond<'_>,
    ) -> Result<MessageSyncDecision, StashError> {
        // Here the following cases can happen:
        // 1. It's a draft, we don't have it open: Treat it as a normal message, update.
        // 2. It's a draft, we have it open: Skip body and metadata updates because we
        // don't have conflict resolution strategies in place
        // 3. It _was_ a draft, we have it open, now it has been sent: We might have
        // missed updates, let's do a full update.
        // 4. If it's `Action::Update` we need to update the body (except of course if the
        //    draft is open)

        let mut is_stale_draft = false;
        if let Some(draft_metadata) =
            DraftMetadata::find_by_message_with_remote_id(metadata.id.clone(), tx).await?
        {
            // We have a message that has been opened as a draft, but it is possible that
            // another session has sent this draft. Deleting the metadata at this point in
            // time can trigger the composer to display a collection of metadata not found errors
            // that can be very confusing for the user.
            // We let the update progress and the next action that executes for that
            // draft will trigger a failure and clean itself up.
            // It's possible that some messages will never properly clean up this way, but
            // this should happen very often and the associated metadata is not very large
            // with each draft. Correctly solving this requires knowledge of active composer
            // states on the rust side.

            let flags = MessageFlags::from(metadata.flags);
            // if the send action id is still present it means the sending is still ongoing
            // and we should not remove/modify the data as the server response will take care of it.
            if !((flags.is_schedule_send() || flags.is_sent())
                && draft_metadata.send_action_id.is_none())
            {
                // Case 2.
                tracing::info!(
                    "Skipping message update for {} because it's opened locally",
                    metadata.id
                );
                return Ok(MessageSyncDecision::Skip);
            }

            // Case 3.
            // We delete the local message body so that it gets re-requested
            // whenever it gets open again. This is because we're skipping updates.
            // Since we're skipping previous `Action::Update`s, this could be just an
            // `Action::UpdateFlags` and we would have a stale body.
            tracing::debug!(
                "Message {} has draft metadata but was already sent, update will be allowed",
                metadata.id
            );

            is_stale_draft = true;
        }

        // Case 4.
        if (event_action == Some(Action::Update) || is_stale_draft)
            && let Some(local_id) = Message::remote_id_counterpart(metadata.id.clone(), tx).await?
        {
            _ = RawMessageBody::delete(local_id, tx).await;
        }

        Ok(MessageSyncDecision::Apply)
    }

    pub async fn handle_event(
        tx: &Bond<'_>,
        id: &MessageId,
        action: Action,
        message: Option<&MessageMetadata>,
        changeset: &mut RebaseChangeSet,
        unresolved_label_ids: &HashSet<LabelId>,
    ) -> Result<Option<LocalMessageId>, AppError> {
        action
            .log_entry(id, async |remote_id| {
                Message::remote_id_counterpart(remote_id.clone(), tx)
                    .await
                    .unwrap_or_default()
                    .map(|v| v.as_u64())
            })
            .await;

        match action {
            Action::Delete => {
                tx.execute(
                    "DELETE FROM messages WHERE remote_id = ?",
                    params![id.clone()],
                )
                .await?;
                Ok(None)
            }

            Action::Create => {
                let Some(message_metadata) = message else {
                    warn!("Got a message-event without any message, skipping it");
                    return Ok(None);
                };

                if Message::sync_decision(message_metadata, Some(action), tx).await?
                    == MessageSyncDecision::Skip
                {
                    tracing::debug!("Create skipped for {id:?}");
                    return Ok(None);
                }
                let mut message = Message::from_api_metadata(message_metadata.clone(), tx).await?;
                message.prune_unresolved_labels(unresolved_label_ids);
                Message::save(&mut message, tx).await?;

                tracing::info!("Created with {:?}", message.id());
                changeset.add(message.id());
                Ok(Some(message.id()))
            }

            Action::Update | Action::UpdateFlags => {
                let Some(message_metadata) = message else {
                    warn!("Got a message-event without any message, skipping it");
                    return Ok(None);
                };

                if Message::sync_decision(message_metadata, Some(action), tx).await?
                    == MessageSyncDecision::Skip
                {
                    tracing::debug!("Update skipped for {id:?}");
                    return Ok(None);
                }
                let mut message = Message::from_api_metadata(message_metadata.clone(), tx).await?;
                message.prune_unresolved_labels(unresolved_label_ids);
                Message::save(&mut message, tx).await?;
                changeset.add(message.id());
                Ok(None)
            }
        }
    }

    pub fn prune_unresolved_labels(&mut self, label_ids: &HashSet<LabelId>) {
        if label_ids.is_empty() {
            return;
        }
        self.label_ids
            .retain(|label_id| !label_ids.contains(label_id));
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum MessageSyncDecision {
    Apply,
    Skip,
}

impl ConversationOrMessage for Message {
    const ROLLBACK_ITEM_TYPE: RollbackItemType = RollbackItemType::Message;

    fn apply_label(
        local_label_id: LocalLabelId,
        ids: impl IntoIterator<Item = Self::IdType>,
        bond: &Transaction<'_>,
    ) -> Result<Vec<LocalMessageId>, StashError> {
        let mut conversation_messages = BTreeMap::<LocalConversationId, Vec<LocalMessageId>>::new();
        let mut modified = Vec::new();
        for id in ids {
            info!("Applying {local_label_id:?} to {id:?}");
            if bond
                .query_row_col::<u64>(
                    "INSERT OR IGNORE INTO message_labels
                    VALUES (?,?)
                    RETURNING local_message_id",
                    (id, local_label_id),
                )
                .optional()?
                .is_some()
            {
                if let Some(message) = Message::load_by_id_sync(id, bond)? {
                    conversation_messages
                        .entry(message.local_conversation_id.unwrap())
                        .and_modify(|v| v.push(id))
                        .or_insert_with(|| vec![id]);
                }
                modified.push(id);
            } else {
                trace!("{id:?} already labeled {local_label_id:?}");
            }
        }

        for (conversation_id, message_ids) in conversation_messages {
            Conversation::label_impl(local_label_id, conversation_id, &message_ids, bond)?;
        }

        Ok(modified)
    }

    fn remove_label(
        local_label_id: LocalLabelId,
        ids: impl IntoIterator<Item = Self::IdType>,
        tx: &Transaction<'_>,
    ) -> Result<Vec<LocalMessageId>, StashError> {
        let mut ids = ids.into_iter().peekable();
        if ids.peek().is_none() {
            return Ok(vec![]);
        }

        // First let's unlabel all messages.

        // We need to remember how many were unread and modified so we can update
        // the unread counts and the message counter
        let mut unread_msg_count = 0_u64;
        let mut updated_count = 0_u64;

        let ids = ids.collect_vec();
        let conversations = tx.query_rows_col::<LocalConversationId>(
            indoc::formatdoc! {"
                    SELECT DISTINCT m.local_conversation_id
                    FROM messages m
                    WHERE local_id IN ({})
                    ", placeholders(&ids)},
            params_from_iter(&ids),
        )?;

        let mut modified_ids = vec![];
        for id in ids {
            info!("Removing {local_label_id:?} from {id:?}");
            // unlabel the message and return whether it was unlabeled
            if let Some(id) = tx
                .query_row_col::<LocalMessageId>(
                    indoc::indoc! {"
                    DELETE FROM message_labels
                    WHERE local_label_id=?
                      AND local_message_id=?
                    RETURNING local_message_id
                    "},
                    (local_label_id, id),
                )
                .optional()?
            {
                modified_ids.push(id);
                updated_count += 1;
            } else {
                tracing::trace!("Message {id} was not labeled with {local_label_id}");
            };

            let unread = tx.query_row_col::<u64>(
                indoc::formatdoc! {"
                    SELECT unread
                    FROM messages
                    WHERE local_id=?
                "},
                [id],
            )?;

            unread_msg_count += unread;
        }

        // Update message counters
        if updated_count == 0 {
            warn!("No updated messages?");
            return Ok(vec![]);
        }

        let mut msg_counters = MessageCounter::load_by_id_sync(local_label_id, tx)?
            .context("No message counter for label")?;

        msg_counters.unread = msg_counters.unread.saturating_sub(unread_msg_count);
        msg_counters.total = msg_counters.total.saturating_sub(updated_count);
        msg_counters
            .save_sync(tx)
            .context("Error saving counters")?;

        let mut conv_counters = ConversationCounter::load_by_id_sync(local_label_id, tx)?
            .context("No conversation counter for label")?;

        for conversation_id in conversations {
            // We get the stats for the remaining messages (those that have not just been deleted)
            // and update the conversation label accordingly.
            let messages = Message::find_sync(
                indoc! {"
                    JOIN message_labels AS ML
                    ON ML.local_message_id = messages.local_id
                       AND ML.local_label_id = ?
                    WHERE messages.local_conversation_id = ?
                "},
                (local_label_id, conversation_id),
                tx,
            )?;

            let label_stats = if messages.is_empty() {
                // Now we can delete the conversation_label too
                None
            } else {
                Some(ConversationMessageLabelStats::from_messages(&messages))
            };

            let conversation_label = ConversationLabel::find_by_conversation_and_label_sync(
                conversation_id,
                local_label_id,
                tx,
            )?;

            match (label_stats, conversation_label) {
                // If some messages in the conversation still remain in the label
                (Some(stats), Some(mut conversation_label)) => {
                    assert_ne!(
                        stats.count, 0,
                        "Entered unreachable code: At least one message must still exist here."
                    );

                    conversation_label.context_num_messages = stats.count;
                    conversation_label.context_time = stats.time;
                    conversation_label.context_snooze_time = stats.snooze_time;
                    conversation_label.context_expiration_time = stats.expiration_time;
                    conversation_label.context_size = stats.size;
                    conversation_label.context_num_attachments = stats.num_attachments as u64;
                    conversation_label.context_num_unread = stats.unread;

                    // If it had at least 1 unread message that has been removed &&
                    // there aren't any more in the conversation we can decrease the counter
                    // because the last unread message(s) for this conversation have been removed
                    if unread_msg_count != 0 && conversation_label.context_num_unread == 0 {
                        conv_counters.unread = conv_counters.unread.saturating_sub(1);
                    }

                    if conversation_label.context_num_messages == 0 {
                        conv_counters.total = conv_counters.total.saturating_sub(1);
                        conversation_label.delete_sync(tx)?;
                    } else {
                        conversation_label.save_sync(tx)?;
                    }
                }
                // If no more messages remain we can unlabel the conversation
                _ => {
                    if tx
                        .query_row_col::<u64>(
                            indoc::indoc! {"
                            DELETE FROM conversation_labels
                            WHERE local_label_id=?
                              AND local_conversation_id=?
                            RETURNING local_conversation_id
                            "},
                            (local_label_id, conversation_id),
                        )
                        .optional()?
                        .is_some()
                    {
                        trace!("Deleting conversation label");

                        conv_counters.total = conv_counters.total.saturating_sub(1);
                        // See previous match arm for an explanation of the logic
                        if unread_msg_count != 0 {
                            assert_ne!(unread_msg_count, 0);
                            conv_counters.unread = conv_counters.unread.saturating_sub(1);
                        }
                    } else {
                        tracing::trace!("Conversation {conversation_id} was not unlabeled");
                        continue;
                    }
                }
            }
        }

        conv_counters
            .save_sync(tx)
            .context("Error saving counters")?;

        Ok(modified_ids)
    }

    async fn api_apply_label(
        api: &impl ProtonMail,
        ids: Vec<Self::RemoteId>,
        label_id: LabelId,
    ) -> Result<Vec<Self::RemoteId>, ApiServiceError> {
        info!("Applying {label_id:?} to {ids:?}");
        let label_id = &label_id;
        let request = |ids: Vec<MessageId>| async move {
            api.put_messages_label(ids.clone(), label_id.clone(), None)
                .await
                .map(|v| v.responses)
        };
        Message::split_request(ids, request)
            .await
            .map(filter_responses)
    }

    async fn api_remove_label(
        api: &impl ProtonMail,
        ids: Vec<Self::RemoteId>,
        label_id: LabelId,
    ) -> Result<Vec<Self::RemoteId>, ApiServiceError> {
        info!("Removing {label_id:?} from {ids:?}");
        let label_id = &label_id;
        let request = |ids: Vec<MessageId>| async move {
            api.put_messages_unlabel(ids.clone(), label_id.clone())
                .await
                .map(|v| v.responses)
        };
        Message::split_request(ids, request)
            .await
            .map(filter_responses)
    }

    fn get_exclusive_locations(&self) -> Vec<LocalLabelId> {
        self.location
            .as_ref()
            .map_or(vec![], |x| vec![x.local_id()])
    }

    fn mark_read(
        ids: impl IntoIterator<Item = Self::IdType>,
        tx: &Transaction<'_>,
    ) -> Result<Vec<Self::IdType>, StashError> {
        let ids = Vec::from_iter(ids);
        Self::mark_read_or_unread(true, &ids, tx)
    }

    fn grouped_labels_and_messages_query(placeholders: usize) -> String {
        formatdoc! {"
            SELECT
                local_label_id,
                GROUP_CONCAT(local_message_id)
            FROM message_labels
            WHERE local_message_id IN ({})
            GROUP BY local_label_id
            ",
            placeholders_n(placeholders)
        }
    }
}

impl ModelHooks for Message {
    fn after_load(&mut self, conn: &Connection) -> Result<(), StashError> {
        self.attachments_metadata = Attachment::load_message_attachment_metadata(self.id(), conn)?;

        let labels = self.all_message_labels(conn)?;

        self.location = ExclusiveLocation::from_labels(&labels);
        self.label_ids = labels
            .iter()
            .map(|l| l.remote_id.clone().unwrap())
            .collect();

        self.custom_labels = labels
            .into_iter()
            .filter(|l| l.label_type == LabelType::Label)
            .map(CustomLabel::from)
            .collect();

        Ok(())
    }

    fn after_save(&mut self, tx: &Transaction<'_>) -> Result<(), StashError> {
        // Remove any labels that are no longer associated with this message.
        if !self.label_ids.is_empty() {
            let id = [self.id()];
            let params = id.to_sql_extend_iter(&*self.label_ids);
            tx.execute(
                &formatdoc!(
                    "
                DELETE FROM
                    message_labels
                WHERE
                    local_message_id = ?
                    AND local_label_id NOT IN (
                        SELECT local_id FROM labels WHERE remote_id IN ({})
                    )
                ",
                    stash::utils::placeholders(&self.label_ids),
                ),
                params_from_iter(params),
            )?;
        } else {
            tx.execute(
                "
                DELETE FROM
                    message_labels
                WHERE
                    local_message_id = ?
                ",
                (self.local_id,),
            )?;
        }

        // This code appears to be doing nothing other than setting up the relationship between
        // message and its labels. This does not cover conversation label updates as this
        // method is meant to be used in conjunction with the event loop state updates where
        // conversations update their own state.
        for label_id in &mut self.label_ids {
            tx.execute(
                r#"
                INSERT OR IGNORE INTO
                    message_labels (local_message_id, local_label_id)
                VALUES
                    (?, (SELECT local_id FROM labels WHERE remote_id=? LIMIT 1))
                "#,
                (self.local_id, label_id.as_str()),
            )?;
        }

        // Remove any attachments that are no longer associated with this conversation.
        let attachment_ids = if !self.attachments_metadata.is_empty() {
            let local_ids = Attachment::create_or_update_from_message_metadata(self, tx)?;

            for id in &local_ids {
                tx.execute(
                    "INSERT OR IGNORE INTO message_attachments_metadata VALUES (?,?)",
                    (self.id(), *id),
                )?;
            }

            local_ids
        } else {
            vec![]
        };

        let params = params_from_iter(
            (self.local_id, Disposition::Attachment, AttachmentType::Pgp)
                .to_sql_extend_iter(&*attachment_ids),
        );
        tx.execute(
                &formatdoc!("
                    DELETE FROM message_attachments_metadata WHERE
                            local_attachment_id IN (
                                SELECT local_id FROM attachments
                                JOIN message_attachments_metadata ON message_attachments_metadata.local_message_id = ? AND
                                    message_attachments_metadata.local_attachment_id = attachments.local_id
                                WHERE attachments.disposition = ? AND attachments.attachment_type <> ?
                                AND attachments.local_id NOT IN ({})
                            )",
                    stash::utils::placeholders_n(attachment_ids.len()),
                ),
            params)
            ?;

        // If exclusive location is not set, we try to calculate it now.
        if self.location.is_none() && !self.label_ids.is_empty() {
            self.location = ExclusiveLocation::from_label_ids_sync(&self.label_ids, tx)?;
        }

        Ok(())
    }

    fn before_save(&mut self, tx: &Transaction<'_>) -> Result<(), StashError> {
        if let Some(remote_id) = &self.remote_id
            && let Some(existing) = Self::find_by_remote_id_sync(remote_id, tx)?
        {
            self.local_id = existing.local_id;
        }

        self.set_coversation_before_save(tx)?;
        Ok(())
    }
}

pub struct MessageWatcher {
    sender: flume::Sender<()>,
}

impl TableObserver for MessageWatcher {
    fn tables(&self) -> Vec<String> {
        vec![
            Message::table_name().to_string(),
            MessageLabel::table_name().to_string(),
            Label::table_name().to_string(),
            Attachment::table_name().to_string(), // This is needed for pgp attachments
        ]
    }

    fn on_tables_changed(&self, _changed_tables: &BTreeSet<String>) {
        self.sender
            .send(())
            .inspect_err(|e| {
                tracing::error!("Failed to send notification for MessageWatcher: {:?}", e)
            })
            .ok();
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AttachmentData {
    pub data: Vec<u8>,
    pub mime: String,
}

impl AttachmentData {
    pub fn empty() -> Self {
        Self {
            data: Vec::new(),
            mime: "image/*".into(),
        }
    }
}

#[derive(Clone, Debug, Eq, Model, PartialEq)]
#[TableName("message_labels")]
pub struct MessageLabel {
    #[IdField]
    pub local_label_id: LocalLabelId,

    #[DbField]
    pub local_message_id: LocalMessageId,
}

impl MessageLabel {
    async fn find_by_conversations_and_labels(
        messages: &[LocalMessageId],
        labels: &[LocalLabelId],
        tether: &Tether,
    ) -> Result<Vec<Self>, StashError> {
        MessageLabel::load_inner(
            formatdoc! { "
                SELECT * FROM message_labels
                WHERE local_message_id IN ({})
                AND local_label_id IN ({})",
                placeholders(messages),
                placeholders(labels)
            },
            messages.to_sql_extend(labels),
            tether,
        )
        .await
    }
}

#[cfg(feature = "test-utils")]
impl Message {
    #[must_use]
    pub fn test_default() -> Self {
        Self {
            local_address_id: 0.into(),
            remote_address_id: AddressId::new(Default::default()),
            // The rest are by default default.
            flags: Default::default(),
            local_id: Default::default(),
            remote_id: Default::default(),
            local_conversation_id: Default::default(),
            remote_conversation_id: Default::default(),
            attachments_metadata: Default::default(),
            bcc_list: Default::default(),
            cc_list: Default::default(),
            deleted: Default::default(),
            expiration_time: UnixTimestamp::new(0),
            external_id: Default::default(),
            is_forwarded: Default::default(),
            is_replied: Default::default(),
            is_replied_all: Default::default(),
            label_ids: Default::default(),
            location: Default::default(),
            num_attachments: Default::default(),
            display_order: Default::default(),
            sender: Default::default(),
            size: Default::default(),
            snooze_time: UnixTimestamp::new(0),
            subject: Default::default(),
            time: UnixTimestamp::new(0),
            to_list: Default::default(),
            unread: Default::default(),
            custom_labels: Default::default(),
        }
    }
}

/// Metadata associated with the Body of a message.
///
/// Note that this information does not come directly from the API, and so there
/// is no equivalent API struct to convert from. Rather, the metadata is
/// obtained from [`DecryptedMessageBody`].
///
/// For metadata associated with a message see [`MessageMetadata`].
///
#[derive(Clone, Debug, Default, Eq, Model, PartialEq)]
#[TableName("message_bodies")]
#[ModelHooks]
pub struct MessageBodyMetadata {
    #[IdField(optional)]
    pub local_message_id: Option<LocalMessageId>,

    #[DbField]
    pub remote_message_id: Option<MessageId>,

    #[DbField]
    pub header: String,

    /// Raw mime type of the underlying message - usually that's going to be
    /// either text/html or text/plain, but for mime-encrypted messages it's
    /// going to say multipart/mixed, nevermind the mime of the decrypted body.
    ///
    /// Note that most of the time what you're *actually* looking for is
    /// [`MessageBody`]'s mime type as that one accounts for mime-encrypted
    /// messages.
    #[DbField]
    pub mime_type: MimeType,

    #[DbField]
    pub parsed_headers: ParsedHeaders,

    pub attachments: Vec<Attachment>,
    pub reply_to: MessageReplyTo,
    pub reply_tos: Vec<MessageReplyTo>,
}

impl MessageBodyMetadata {
    pub async fn for_message(
        local_message_id: LocalMessageId,
        tether: &Tether,
    ) -> Result<Option<Self>, StashError> {
        // There is no local id on this type so we can't use find_by_id.
        Self::find_first(
            "WHERE local_message_id =?",
            params![local_message_id],
            tether,
        )
        .await
    }

    /// Create a [`MessageBodyMetadata`] from an [`ApiMessageBody`].
    ///
    /// The local and remote ids are required to correctly fill out
    /// all the attachment metadata.
    ///
    /// Returns an instance of [`Self`] and the message body.
    pub fn from_api_message_body(
        api_message_body: ApiMessageBody,
        remote_message_id: MessageId,
        remote_conversation_id: ConversationId,
        remote_address_id: AddressId,
    ) -> (Self, String) {
        let attachments = api_message_body
            .attachments
            .into_iter()
            .map(|a| {
                let mut attachment = Attachment::from(a);
                attachment.remote_message_id = Some(remote_message_id.clone());
                attachment.remote_conversation_id = Some(remote_conversation_id.clone());
                attachment.remote_address_id = Some(remote_address_id.clone());
                attachment
            })
            .collect();

        (
            Self {
                local_message_id: None,
                remote_message_id: Some(remote_message_id),
                header: api_message_body.header,
                mime_type: api_message_body.mime_type.into(),
                parsed_headers: ParsedHeaders {
                    headers: api_message_body.parsed_headers,
                },
                reply_to: api_message_body.reply_to.into(),
                reply_tos: api_message_body.reply_tos.map_vec(),
                attachments,
            },
            api_message_body.body,
        )
    }

    pub async fn update_fields_after_draft_create_or_update(
        &self,
        bond: &Bond<'_>,
    ) -> Result<(), StashError> {
        bond.execute(
            formatdoc! {"
            UPDATE {} SET
                header = ?,
                parsed_headers = ?,
                remote_message_id = ?
            WHERE local_message_id = ?
        ", Self::table_name()},
            params![
                self.header.clone(),
                self.parsed_headers.clone(),
                self.remote_message_id.clone(),
                self.local_message_id.unwrap()
            ],
        )
        .await?;
        Ok(())
    }

    pub fn parsed_header_value(&self, key: &str) -> Option<ParsedHeaderValue> {
        let value = self.parsed_headers.headers.get(key)?;
        match value {
            serde_json::Value::String(s) => Some(ParsedHeaderValue::String(s.clone())),
            serde_json::Value::Array(array) => {
                let mut result = Vec::with_capacity(array.len());
                for (idx, item) in array.iter().enumerate() {
                    if let serde_json::Value::String(str) = item {
                        result.push(str.clone());
                    } else {
                        tracing::warn!(
                            "Header array value {key}[{idx}] of message {:?} has invalid value type",
                            self.remote_message_id
                        );
                    }
                }
                Some(ParsedHeaderValue::Array(result))
            }
            _ => {
                tracing::warn!(
                    "Header value {key} of message {:?} has invalid value type",
                    self.remote_message_id
                );
                None
            }
        }
    }
}

impl ModelHooks for MessageBodyMetadata {
    fn after_save(&mut self, tx: &Transaction<'_>) -> Result<(), StashError> {
        if self.local_message_id.is_none()
            && let Some(remote_id) = &self.remote_message_id
        {
            if let Some(existing) =
                Self::find_first_sync("WHERE remote_message_id=?", (remote_id,), tx)?
            {
                self.local_message_id = existing.local_message_id;
            } else {
                let Some(message) = Message::find_by_remote_id_sync(remote_id, tx)? else {
                    return Err(StashError::Custom(anyhow!(
                        "Failed to find message with remote id {}",
                        self.remote_message_id.as_ref().unwrap()
                    )));
                };
                self.local_message_id = message.local_id;
            }
        }
        // Update all attachment links - When creating drafts we can update
        // and create new ones.
        // PGP attachments should never be deleted.
        tx.execute(
            indoc! {"DELETE FROM message_attachments
                WHERE local_message_id=?1
               AND local_attachment_id NOT IN (
                    SELECT local_attachment_id FROM attachments WHERE local_message_id=?1 AND attachment_type = ?2
               )"},
            (self.local_message_id, AttachmentType::Pgp,),
        )
            ?;

        for attachment in &mut self.attachments {
            attachment.save_sync(tx)?;
            tx
                .execute(
                    "INSERT OR IGNORE INTO message_attachments (local_attachment_id, local_message_id) VALUES (?,?)",
                    (attachment.id(), self.local_message_id,),
                )
                ?;
        }

        self.reply_to.store_reply_to(self.id(), tx)?;
        for reply_to in &self.reply_tos {
            reply_to.store_reply_tos(self.id(), tx)?;
        }
        Ok(())
    }

    fn after_load(&mut self, conn: &Connection) -> Result<(), StashError> {
        self.attachments = Attachment::for_message_sync(self.local_message_id.unwrap(), conn)
            .inspect_err(|e| error!("Failed to load attachments for body metadata: {e:?}"))?;

        self.reply_to = MessageReplyTo::load_reply_to(self.id(), conn)?;
        self.reply_tos = MessageReplyTo::load_reply_tos(self.id(), conn)?;

        Ok(())
    }

    fn before_save(&mut self, bond: &Transaction<'_>) -> Result<(), StashError> {
        if self.local_message_id.is_none()
            && let Some(remote_id) = &self.remote_message_id
            && let Some(message) = Message::find_by_remote_id_sync(remote_id, bond)?
        {
            self.local_message_id = message.local_id;
        }

        Ok(())
    }
}

#[derive(Debug)]
pub struct MessageLabelStats {
    pub unread_count: u64,
    pub count: u64,
    pub attachment_count: u64,
    pub size: u64,
}

impl MessageLabelStats {
    async fn build(
        messages: impl IntoIterator<Item = Message>,
        tether: &Tether,
    ) -> Result<HashMap<LocalLabelId, MessageLabelStats>, StashError> {
        let messages = messages.into_iter();
        let mut label_stats = HashMap::with_capacity(messages.size_hint().1.unwrap_or(4));
        for message in messages {
            let label_ids = tether
                .query_values::<_, LocalLabelId>(
                    "SELECT local_label_id FROM message_labels WHERE local_message_id=?",
                    params![message.id()],
                )
                .await?;
            for label_id in label_ids {
                match label_stats.entry(label_id) {
                    HmEntry::Occupied(mut o) => {
                        let details: &mut MessageLabelStats = o.get_mut();
                        details.count += 1;
                        if message.unread {
                            details.unread_count += 1;
                        }
                        details.attachment_count += message.num_attachments as u64;
                        details.size += message.size;
                    }
                    HmEntry::Vacant(v) => {
                        v.insert(MessageLabelStats {
                            count: 1,
                            unread_count: message.unread as u64,
                            attachment_count: message.num_attachments as u64,
                            size: message.size,
                        });
                    }
                }
            }
        }

        Ok(label_stats)
    }
}

#[derive(Clone, Debug, Eq, Model, PartialEq)]
#[TableName("message_counters")]
pub struct MessageCounter {
    #[IdField]
    pub local_label_id: LocalLabelId,

    #[DbField]
    pub total: u64,

    #[DbField]
    pub unread: u64,
}

impl MessageCounter {
    pub fn new(local_label_id: LocalLabelId) -> Self {
        Self {
            local_label_id,
            total: Default::default(),
            unread: Default::default(),
        }
    }

    pub fn get(&self) -> (u64, u64) {
        (self.unread, self.total)
    }

    pub fn total(&self, unread: ReadFilter) -> u64 {
        match unread {
            ReadFilter::All => self.total,
            ReadFilter::Unread => self.unread,
            ReadFilter::Read => self.total.saturating_sub(self.unread),
        }
    }

    pub async fn watch(stash: &Stash) -> Result<WatcherHandle, StashError> {
        stash
            .subscribe_to(|sender| Box::new(MessageCounterWatcher { sender }))
            .await
    }
}

pub struct MessageCounterWatcher {
    sender: flume::Sender<()>,
}

impl TableObserver for MessageCounterWatcher {
    fn tables(&self) -> Vec<String> {
        vec![MessageCounter::table_name().to_string()]
    }

    fn on_tables_changed(&self, _tables: &BTreeSet<String>) {
        self.sender
            .send(())
            .inspect_err(|e| {
                tracing::error!("Failed to send notification for MessageCounterWatcher: {e:?}")
            })
            .ok();
    }
}

// Note: This is not a model as this represents a link table that links data together.
// We also want to use a more efficient sql query to update than the one provided by the
// Model marcro.
#[derive(Clone, Default, Debug, DbRecord, Eq, PartialEq)]
pub struct MessageReplyTo {
    #[DbField]
    pub address: PrivateEmail,

    #[DbField]
    pub name: PrivateString,

    #[DbField]
    pub bimi_selector: Option<String>,

    #[DbField]
    pub display_sender_image: bool,

    #[DbField]
    pub is_proton: bool,

    #[DbField]
    pub is_simple_login: bool,
}

impl MessageReplyTo {
    fn store_reply_to(
        &self,
        message_id: LocalMessageId,
        tx: &Transaction<'_>,
    ) -> Result<usize, StashError> {
        self.store_impl(message_id, "message_reply_to", tx)
    }

    fn store_reply_tos(
        &self,
        message_id: LocalMessageId,
        tx: &Transaction<'_>,
    ) -> Result<usize, StashError> {
        self.store_impl(message_id, "message_reply_tos", tx)
    }

    fn store_impl(
        &self,
        message_id: LocalMessageId,
        table_name: &str,
        tx: &Transaction<'_>,
    ) -> Result<usize, StashError> {
        tx.execute(
            &formatdoc! {
            "INSERT INTO `{table_name}` (
                local_message_id,
                name,
                address,
                bimi_selector,
                is_proton,
                is_simple_login,
                display_sender_image
            ) VALUES (?,?,?,?,?,?,?)
            ON CONFLICT (local_message_id) DO UPDATE SET
                name=excluded.name,
                address=excluded.address,
                bimi_selector=excluded.bimi_selector,
                is_proton=excluded.is_proton,
                is_simple_login=excluded.is_simple_login,
                display_sender_image=excluded.display_sender_image
            "},
            (
                message_id,
                self.name.clone(),
                self.address.clone(),
                self.bimi_selector.clone(),
                self.is_proton,
                self.is_simple_login,
                self.display_sender_image,
            ),
        )
        .map_err(Into::into)
    }

    fn load_reply_to(
        message_id: LocalMessageId,
        conn: &Connection,
    ) -> Result<MessageReplyTo, StashError> {
        Ok(MessageReplyTo::model_find_first(
            "SELECT * FROM message_reply_to WHERE local_message_id = ?",
            (message_id,),
            conn,
        )?
        .context("Message should always have one reply to field")?)
    }

    fn load_reply_tos(
        message_id: LocalMessageId,
        conn: &Connection,
    ) -> Result<Vec<MessageReplyTo>, StashError> {
        MessageReplyTo::model_find(
            "SELECT * FROM message_reply_tos WHERE local_message_id = ?",
            (message_id,),
            conn,
        )
    }
}

impl From<ApiMessageReplyTo> for MessageReplyTo {
    fn from(value: ApiMessageReplyTo) -> Self {
        Self {
            address: value.address,
            name: value.name,
            bimi_selector: value.bimi_selector,
            is_proton: value.is_proton,
            is_simple_login: value.is_simple_login,
            display_sender_image: value.display_sender_image,
        }
    }
}
