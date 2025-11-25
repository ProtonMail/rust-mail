#[cfg(test)]
#[path = "../tests/models/conversations.rs"]
mod conversations;

use super::network::split_request;
use crate::actions::conversations::label_as::UndoLabelAsConversations;
use crate::actions::conversations::r#move::UndoMoveToConversations;
use crate::actions::conversations::{LabelAs, Snooze, UndoLabelAsArchiveConversations};
use crate::actions::conversations::{MarkRead, MarkUnread, Move, Unsnooze};
use crate::actions::{
    ActionMoveData, ConversationOrMessage, LabelAsAction, LabelAsData, LabelAsOutput, LabelPair,
    MoveAction, Undo, filter_responses,
};
use crate::datatypes::{
    AttachmentMetadata, ConversationLabelsCount, CustomLabel, Disposition, ExclusiveLocation,
    LocalMessageId, MessageAttachmentInfos, MessageLabelsCount, MessageRecipients, MessageSenders,
    ReadFilter, SystemLabelId,
};
use crate::datatypes::{LocalConversationId, RollbackItemType};
use crate::models::*;
use crate::snooze::SnoozeOptions;
use crate::{AppError, actions::conversations::Delete};
use crate::{MailContextError, find_in_query};
use anyhow::Context;
use chrono::Local;
use futures::future;
use indoc::{formatdoc, indoc};
use itertools::Itertools;
#[cfg(feature = "action_rebase")]
use proton_action_queue::action::ActionGroup;
use proton_action_queue::action::MetadataBuilder;

use derivative::Derivative;
use proton_action_queue::queue::{ActionError as QueueActionError, Queue, QueuedActionOutput};
use proton_action_queue::rebase::RebaseChangeSet;
use proton_core_api::consts::Mail;
use proton_core_api::service::ApiServiceError;
use proton_core_api::services::proton::{LabelId, ProtonIdMarker};
use proton_core_api::session::Session;
use proton_core_common::datatypes::{
    InitializationKey, LabelType, LocalLabelId, SystemLabel, UnixTimestamp, WeekStart,
};
use proton_core_common::models::{
    InitializationError, InitializationWatcher, InitializedComponent, Label, ModelExtension,
    ModelIdExtension, User,
};
use proton_core_common::services::NetworkMonitorService;
use proton_core_common::utils::MapVec as _;
use proton_mail_api::services::proton::ProtonMail;
use proton_mail_api::services::proton::common::ConversationId;
use proton_mail_api::services::proton::requests::GetConversationsOptions;
use proton_mail_api::services::proton::response_data::{
    AttachmentMetadata as ApiAttachmentMetadata, Conversation as ApiConversation,
    ConversationLabel as ApiConversationLabel, MessageMetadata as ApiMessageMetadata,
    OperationResult,
};
use serde::{Deserialize, Serialize};
use sqlite_watcher::watcher::TableObserver;
use stash::exports::{Connection, ToSql};
use stash::exports::{SqliteError, Transaction};
use stash::macros::Model;
use stash::orm::Model;
use stash::orm::ModelHooks;
use stash::params;
use stash::rusqlite::{OptionalExtension, params_from_iter};
use stash::stash::{Bond, RunTransaction, Stash, StashError, Tether, WatcherHandle};
use stash::utils::{ConnectionExt, IterMapToSql, MapToSql as _, placeholders, placeholders_n};
use std::collections::{BTreeSet, HashMap};
use std::future::Future;
use std::ops::{AddAssign, Deref, DerefMut};
use std::sync::Arc;
use tracing::{debug, error, info, warn};

#[derive(Clone, Debug, Eq, Model, PartialEq)]
#[TableName("conversations")]
#[ModelHooks]
pub struct Conversation {
    #[IdField(autoincrement)]
    pub local_id: Option<LocalConversationId>,

    #[DbField]
    pub remote_id: Option<ConversationId>,

    #[DbField]
    pub attachment_info: MessageAttachmentInfos,

    pub attachments_metadata: Vec<AttachmentMetadata>,

    #[DbField]
    pub deleted: bool,

    #[DbField]
    pub display_snooze_reminder: bool,

    /// This field is only present for currently snoozed conversations.
    /// It is determined by the `ConversationLabel` pointing to Snoozed label of the conversation.
    /// It is not present (None) for conversations that are not snoozed or already reminded.
    /// This is important for the client to know when to display the time of reminder.
    pub snoozed_until: Option<UnixTimestamp>,

    pub locations: Vec<ExclusiveLocation>,

    #[DbField]
    pub expiration_time: UnixTimestamp,

    pub labels: Vec<ConversationLabel>,

    #[DbField]
    pub num_attachments: u64,

    #[DbField]
    pub num_messages: u64,

    #[DbField]
    pub num_unread: u64,

    #[DbField]
    pub display_order: u64,

    #[DbField]
    pub recipients: MessageRecipients,

    #[DbField]
    pub senders: MessageSenders,

    #[DbField]
    pub size: u64,

    #[DbField]
    pub subject: String,

    /// Whether this conversation is fully known.
    ///
    /// When in message view mode we need to be able to create messages
    /// without their conversation counterpart. We create an unknown conversation
    /// entry.
    ///
    /// As it is expensive to sync the conversation, we need to defer this until
    /// we either retrieve the conversation from the server or one of the
    /// events creates it for us.
    #[DbField]
    pub is_known: bool,

    pub custom_labels: Vec<CustomLabel>,

    /// Whether the conversation has synced its messages.
    #[DbField]
    pub has_messages: bool,
}

#[cfg(feature = "test-utils")]
impl Conversation {
    pub fn test_default() -> Self {
        Self {
            local_id: None,
            remote_id: None,
            attachment_info: Default::default(),
            attachments_metadata: vec![],
            deleted: false,
            display_snooze_reminder: false,
            snoozed_until: None,
            locations: vec![],
            expiration_time: UnixTimestamp::new(0),
            labels: vec![],
            num_attachments: 0,
            num_messages: 0,
            num_unread: 0,
            display_order: 0,
            recipients: Default::default(),
            senders: Default::default(),
            size: 0,
            subject: "".to_string(),
            is_known: false,
            custom_labels: vec![],
            has_messages: false,
        }
    }
}

impl ModelIdExtension for Conversation {
    type RemoteId = ConversationId;

    fn remote_id(&self) -> Option<&Self::RemoteId> {
        self.remote_id.as_ref()
    }
}

type LabelAsResult = Result<QueuedActionOutput<LabelAs>, QueueActionError<LabelAs>>;

impl Conversation {
    pub fn label(&self, local_id: LocalLabelId) -> Option<&ConversationLabel> {
        self.labels
            .iter()
            .find(|&label| label.local_label_id == Some(local_id))
    }

    pub async fn action_star(queue: &Queue, ids: Vec<LocalConversationId>) -> LabelAsResult {
        let tether = queue.stash().connection().await?;

        let label_id = Label::remote_id_counterpart(LabelId::starred(), &tether)
            .await?
            .expect("Star system label not found");

        Self::action_apply_label(queue, label_id, ids).await
    }

    pub async fn action_unstar(queue: &Queue, ids: Vec<LocalConversationId>) -> LabelAsResult {
        let tether = queue.stash().connection().await?;

        let label_id = Label::remote_id_counterpart(LabelId::starred(), &tether)
            .await?
            .expect("Star system label not found");

        Self::action_remove_label(queue, label_id, ids).await
    }

    /// Mark multiple conversations as read.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed.
    ///
    pub async fn action_mark_read(
        queue: &Queue,
        label_id: LocalLabelId,
        conversation_ids: Vec<LocalConversationId>,
    ) -> Result<QueuedActionOutput<MarkRead>, QueueActionError<MarkRead>> {
        let action = MarkRead::new(label_id, conversation_ids);
        queue.queue_action(action).await
    }

    /// Mark multiple conversations as unread.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed.
    ///
    pub async fn action_mark_unread(
        queue: &Queue,
        label_id: LocalLabelId,
        conversation_ids: Vec<LocalConversationId>,
    ) -> Result<QueuedActionOutput<MarkUnread>, QueueActionError<MarkUnread>> {
        let action = MarkUnread::new(label_id, conversation_ids);
        queue.queue_action(action).await
    }

    /// Delete multiple conversations.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed.
    ///
    pub async fn action_delete(
        queue: &Queue,
        label_id: LocalLabelId,
        conversation_ids: Vec<LocalConversationId>,
    ) -> Result<QueuedActionOutput<Delete>, QueueActionError<Delete>> {
        let action = Delete::new(label_id, conversation_ids);
        queue.queue_action(action).await
    }

    /// Move multiple conversations.
    ///
    /// # Errors
    ///
    /// Returns an error if the action failed.
    ///
    pub async fn action_move(
        tether: &Tether,
        queue: &Queue,
        destination_id: LocalLabelId,
        target_ids: Vec<LocalConversationId>,
    ) -> Result<Option<Undo>, MailContextError> {
        if let Some(action) = ActionMoveData::new(tether, destination_id, target_ids).await? {
            let action = Move(action);
            let QueuedActionOutput { local, id } = queue.queue_action(action).await?;
            Ok(Some(Undo::ConversationsMoveTo(UndoMoveToConversations {
                action: local,
                id,
            })))
        } else {
            Ok(None)
        }
    }

    /// Soft delete multiple conversations.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed.
    ///
    pub async fn action_mark_deleted(
        queue: &Queue,
        label_id: LocalLabelId,
        conversation_ids: impl IntoIterator<Item = LocalConversationId>,
    ) -> Result<QueuedActionOutput<Delete>, QueueActionError<Delete>> {
        let action = Delete::new(label_id, conversation_ids);
        queue.queue_action(action).await
    }

    pub async fn action_remove_label(
        queue: &Queue,
        label: LocalLabelId,
        ids: Vec<LocalConversationId>,
    ) -> LabelAsResult {
        let action = LabelAs(LabelAsData::new_remove(
            ids.into_iter().map(|id| LabelPair { label, id }).collect(),
        ));
        queue.queue_action(action).await
    }

    pub async fn action_apply_label(
        queue: &Queue,
        label: LocalLabelId,
        ids: Vec<LocalConversationId>,
    ) -> LabelAsResult {
        let action = LabelAs(LabelAsData::new_add(
            ids.into_iter().map(|id| LabelPair { label, id }).collect(),
        ));
        queue.queue_action(action).await
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
    pub async fn action_label_as(
        tether: &Tether,
        queue: &Queue,
        source_label_id: LocalLabelId,
        conversation_ids: Vec<LocalConversationId>,
        selected_label_ids: Vec<LocalLabelId>,
        partially_selected_label_ids: Vec<LocalLabelId>,
        must_archive: bool,
    ) -> Result<LabelAsOutput, AppError> {
        let all_labels = Label::local_ids_by_kind(LabelType::Label, tether).await?;
        let cartesian = ConversationLabel::find_by_conversations_and_labels(
            &conversation_ids,
            &all_labels,
            tether,
        )
        .await?;

        let label_as_action = {
            let action = LabelAs(LabelAsData::new(
                cartesian
                    .into_iter()
                    .filter_map(|x| {
                        Some(LabelPair {
                            label: x.local_label_id?,
                            id: x.local_conversation_id?,
                        })
                    })
                    .collect(),
                source_label_id,
                conversation_ids.clone(),
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
            ActionMoveData::new(tether, archive, conversation_ids)
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
                    undo: Some(Undo::ConversationsLabelAs(UndoLabelAsConversations {
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

                let undo = Some(Undo::ConversationsMoveTo(UndoMoveToConversations {
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
                let undo = Some(Undo::ConversationsLabelAs(UndoLabelAsConversations {
                    action: label_as_action,
                    id: queued_label_as.id,
                    must_archive: Some(UndoLabelAsArchiveConversations {
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

    pub async fn action_snooze(
        queue: &Queue,
        label_id: LocalLabelId,
        conversation_ids: Vec<LocalConversationId>,
        snooze_time: UnixTimestamp,
    ) -> Result<QueuedActionOutput<Snooze>, QueueActionError<Snooze>> {
        let action = Snooze::new(label_id, conversation_ids, snooze_time);
        queue.queue_action(action).await
    }

    pub async fn action_unsnooze(
        queue: &Queue,
        label_id: LocalLabelId,
        conversation_ids: Vec<LocalConversationId>,
    ) -> Result<QueuedActionOutput<Unsnooze>, QueueActionError<Unsnooze>> {
        let action = Unsnooze::new(label_id, conversation_ids);
        queue.queue_action(action).await
    }

    /// Find a group of Conversations by their IDs.
    ///
    /// # Errors
    ///
    /// When database request fail.
    ///
    pub(crate) async fn find_by_ids(
        conversation_ids: impl IntoIterator<Item = LocalConversationId>,
        tether: &Tether,
    ) -> Result<Vec<Self>, StashError> {
        let (query, params) =
            find_in_query!("WHERE deleted = 0 AND local_id IN ({})", conversation_ids);
        Conversation::find(query, params, tether).await
    }

    /// Create a new unknown conversation where we only know the `remote_id`.
    ///
    /// See [`Conversation::is_known`] for more details.
    pub fn unknown(remote_id: ConversationId) -> Self {
        Self {
            local_id: None,
            remote_id: Some(remote_id),
            attachment_info: Default::default(),
            attachments_metadata: vec![],
            deleted: false,
            display_snooze_reminder: false,
            snoozed_until: None,
            locations: vec![],
            expiration_time: 0.into(),
            labels: vec![],
            num_attachments: 0,
            num_messages: 0,
            num_unread: 0,
            display_order: 0,
            recipients: Default::default(),
            senders: Default::default(),
            size: 0,
            subject: "".to_string(),
            is_known: false,
            custom_labels: vec![],
            has_messages: false,
        }
    }

    /// Save a non existing conversation to the database.
    ///
    /// This method is complementary way to store conversation. It only will proceed
    /// with conversations that are not yet present in database. This functionality
    /// is required due to multiprocess nature of mail application and the possibility to
    /// view mailboxes without interfering with processes triggered by the user.
    ///
    /// If the conversation is not known, it will replace existing conversation with API data.
    ///
    /// Conversation is updated only if the current open label context state is different. This way
    /// new messages in conversations are visible earlier and we prevent unnecessary updates if not
    /// relevant to the current location.
    ///
    /// # Errors
    ///
    /// Returns an error if the local conversation id is not set or the query failed.
    ///
    #[tracing::instrument(skip(self, rebase_change_set, bond), fields(remote_id = ?self.remote_id))]
    pub async fn create_or_get_local(
        &mut self,
        current_label_id: &LabelId,
        rebase_change_set: &mut RebaseChangeSet,
        bond: &Bond<'_>,
    ) -> Result<(), StashError> {
        if let Some(remote_id) = self.remote_id.clone()
            && let Some(existing) = Self::find_by_remote_id(remote_id, bond).await?
        {
            if existing.is_known {
                let should_skip = match (
                    self.labels
                        .iter()
                        .find(|l| l.remote_label_id.as_ref() == Some(current_label_id)),
                    existing
                        .labels
                        .iter()
                        .find(|l| l.remote_label_id.as_ref() == Some(current_label_id)),
                ) {
                    (Some(this_label), Some(other_label)) => {
                        tracing::debug!(
                            "Both API and remote have the same {current_label_id:?}. Checking if stats are equal"
                        );
                        this_label.are_stats_equal(other_label)
                    }
                    (Some(_), None) | (None, Some(_)) => false,
                    (None, None) => {
                        tracing::debug!("Both API and remote have {current_label_id:?}");
                        true
                    }
                };

                if should_skip {
                    *self = existing;
                    debug!(
                        local_id=?self.local_id,
                        "Skipping saving conversation, we already have it in the local DB"
                    );
                    return Ok(());
                }
                self.local_id = existing.local_id;
                tracing::debug!("Updating known conversation with API data");
            } else {
                // Otherwise, update the unknown conversation with API data
                self.local_id = existing.local_id;
                tracing::debug!("Updating unknown conversation with API data");
            }
        } else {
            tracing::debug!("Saving new conversation from API");
        }
        tracing::debug!(deleted=?self.deleted, local_id=?self.local_id, "Saving conversation");

        <Self as Model>::save(self, bond).await?;
        rebase_change_set.add(self.id());
        Ok(())
    }

    /// Label multiple conversations.
    ///
    /// # Errors
    ///
    /// Returns an error if the data could not be written to the database.
    ///
    pub async fn apply_label_async(
        label_id: LocalLabelId,
        ids: impl IntoIterator<Item = LocalConversationId>,
        bond: &Bond<'_>,
    ) -> Result<Vec<LocalMessageId>, StashError> {
        let ids = Vec::from_iter(ids);
        bond.sync_bridge(move |tx| Self::apply_label(label_id, ids, tx))
            .await
    }

    /// Label multiple conversations.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed.
    ///
    pub async fn apply_label_to_multiple_remote<PM: ProtonMail>(
        label_id: LabelId,
        ids: Vec<ConversationId>,
        spam_action: Option<bool>,
        api: &PM,
    ) -> Result<Vec<OperationResult<ConversationId>>, ApiServiceError> {
        info!("Applying {label_id:?} to {ids:?}",);
        let request = |ids: Vec<ConversationId>| {
            let label_id = label_id.clone();
            async {
                api.put_conversations_label(ids, label_id, spam_action)
                    .await
                    .map(|r| r.responses)
            }
        };
        Conversation::split_request(ids, request).await
    }

    /// TODO: Document this method.
    ///
    /// # Errors
    ///
    /// Returns an error if the data could not be written to the database.
    ///
    pub async fn create_or_update_conversations(
        conversations: Vec<Conversation>,
        bond: &Bond<'_>,
    ) -> Result<Vec<LocalConversationId>, AppError> {
        let mut ids = Vec::with_capacity(conversations.len());

        for mut conv in conversations {
            Self::save(&mut conv, bond).await?;
            ids.push(conv.id());
        }

        Ok(ids)
    }

    /// Mark conversations as deleted.
    ///
    /// Note that this is a soft delete. Conversations are only
    /// really deleted when the event loop sends the delete event.
    ///
    /// Finally, only the messages in the active label will be marked as deleted
    /// unless the label is AllMail which will mark all messages in all labels as deleted.
    /// moreover the conversation will be removed from all labels as well as deleted field will
    /// be set to true.
    ///
    /// # Errors
    ///
    /// Returns an error if the data could not be written to the database.
    ///
    pub async fn mark_deleted(
        label_id: LocalLabelId,
        ids: impl IntoIterator<Item = LocalConversationId>,
        bond: &Bond<'_>,
    ) -> Result<(), AppError> {
        let all_mail_id = SystemLabel::AllMail.local_id(bond).await?;
        let is_all_mail = all_mail_id
            .filter(|all_mail_id| *all_mail_id == label_id)
            .is_some();

        if is_all_mail {
            Self::mark_deleted_all_mail(ids, bond).await?;
        } else {
            Self::mark_deleted_current_label(label_id, ids, bond).await?;
        }

        Ok(())
    }

    /// Mark conversations as deleted for `AllMail` label.
    /// More information can be found in [`Conversation::mark_deleted`].
    ///
    /// # Errors
    ///
    /// Returns an error if the data could not be written to the database.
    ///
    async fn mark_deleted_all_mail(
        ids: impl IntoIterator<Item = LocalConversationId>,
        bond: &Bond<'_>,
    ) -> Result<(), AppError> {
        for id in ids {
            info!("Marking {id:?} as deleted in all mail");
            let Some(mut conversation) = Conversation::find_by_id(id, bond).await? else {
                continue;
            };

            conversation.deleted = true;
            conversation.num_unread = 0;
            conversation.num_messages = 0;
            conversation.num_attachments = 0;
            conversation.size = 0;
            conversation.save(bond).await?;

            let mut messages = Message::find(
                formatdoc! {"
                WHERE local_conversation_id=? AND deleted = 0
               "},
                params![id],
                bond,
            )
            .await?;

            for message in &mut messages {
                message.deleted = true;
                message.save(bond).await?
            }

            if !messages.is_empty() {
                let stats =
                    Message::update_message_counters_after_soft_delete(messages.into_iter(), bond)
                        .await?;
                conversation
                    .remove_conversation_from_all_labels(stats, bond)
                    .await?;
            }
        }

        Ok(())
    }

    /// Updates all labels counters after soft delete of conversation in active view `AllMail`.
    ///
    /// # Errors
    ///
    /// Will return an error if the data could not be written to the database.
    ///
    async fn remove_conversation_from_all_labels(
        &self,
        all_stats: HashMap<LocalLabelId, MessageLabelStats>,
        bond: &Bond<'_>,
    ) -> Result<(), AppError> {
        let conv_labels = ConversationLabel::find(
            "WHERE local_conversation_id=? AND deleted=0",
            params![self.id()],
            bond,
        )
        .await?;

        for mut conv_label in conv_labels {
            let label_id = conv_label.local_label_id.unwrap();
            let stats = all_stats.get(&label_id);

            let mut conv_counter = ConversationCounters::find_by_id(label_id, bond)
                .await?
                .ok_or_else(|| AppError::LabelNotFound(label_id))?;
            conv_counter.total = conv_counter.total.saturating_sub(1);

            if stats.filter(|s| s.unread_count > 0).is_some() {
                conv_counter.unread = conv_counter.unread.saturating_sub(1);
            }

            conv_counter.save(bond).await?;

            conv_label.deleted = true;
            conv_label.save(bond).await?;
        }

        Ok(())
    }

    /// Mark conversations as deleted in active label.
    /// More information can be found in [`Conversation::mark_deleted`].
    ///
    /// # Errors
    ///
    /// Returns an error if the data could not be written to the database.
    ///
    async fn mark_deleted_current_label(
        label_id: LocalLabelId,
        ids: impl IntoIterator<Item = LocalConversationId>,
        bond: &Bond<'_>,
    ) -> Result<(), AppError> {
        for id in ids {
            info!("Marking {id:?} as deleted in {label_id:?}");
            let Some(mut conversation) = Conversation::find_first(
                "WHERE local_id=? AND deleted=0 AND is_known=1",
                params![id],
                bond,
            )
            .await?
            else {
                continue;
            };

            let mut messages = Message::find(
                formatdoc! {"
                WHERE local_conversation_id=? AND deleted = 0 AND local_id IN (
                    SELECT local_message_id FROM message_labels WHERE local_label_id = ?
                )
               "},
                params![id, label_id],
                bond,
            )
            .await?;

            for message in &mut messages {
                message.deleted = true;
                message.save(bond).await?
            }

            if !messages.is_empty() {
                let all_stats =
                    Message::update_message_counters_after_soft_delete(messages.into_iter(), bond)
                        .await?;

                let stats = all_stats.get(&label_id);

                conversation.mark_delete_update_stats(stats, bond).await?;

                conversation
                    .remove_conversation_from_label(label_id, stats, bond)
                    .await?;
            }
        }

        Ok(())
    }

    /// Updates active label counters after soft delete of conversation.
    ///
    /// # Errors
    ///
    /// Will return an error if the data could not be written to the database.
    ///
    pub async fn remove_conversation_from_label(
        &mut self,
        label_id: LocalLabelId,
        stats: Option<&MessageLabelStats>,
        bond: &Bond<'_>,
    ) -> Result<(), AppError> {
        let conv_label = ConversationLabel::find_first(
            "WHERE local_conversation_id=? AND deleted=0 AND local_label_id=?",
            params![self.id(), label_id],
            bond,
        )
        .await?;

        if let Some(mut conv_label) = conv_label {
            let mut conv_counter = ConversationCounters::find_by_id(label_id, bond)
                .await?
                .ok_or_else(|| AppError::LabelNotFound(label_id))?;
            conv_counter.total = conv_counter.total.saturating_sub(1);

            if stats.filter(|s| s.unread_count > 0).is_some() {
                conv_counter.unread = conv_counter.unread.saturating_sub(1);
            }

            conv_counter.save(bond).await?;
            conv_label.deleted = true;
            conv_label.save(bond).await?;
        }

        Ok(())
    }

    /// Mark conversations as undeleted.
    ///
    /// Only the messages in the active label will be marked as undeleted
    /// unless the label is AllMail which will mark all messages in all labels as undeleted.
    /// moreover the conversation will be assigned to all labels as well as deleted field will
    /// be set to false.
    ///
    /// # Errors
    ///
    /// Returns an error if the data could not be written to the database.
    ///
    pub async fn mark_undeleted(
        label_id: LocalLabelId,
        ids: impl IntoIterator<Item = LocalConversationId>,
        bond: &Bond<'_>,
    ) -> Result<(), AppError> {
        let all_mail_id = SystemLabel::AllMail.local_id(bond).await?;
        let is_all_mail = all_mail_id
            .filter(|all_mail_id| *all_mail_id == label_id)
            .is_some();

        if is_all_mail {
            Self::mark_undeleted_all_mail(ids, bond).await?;
        } else {
            Self::mark_undeleted_current_label(label_id, ids, bond).await?;
        }

        Ok(())
    }

    /// Mark conversations as undeleted for `AllMail` label.
    /// More information can be found in [`Conversation::mark_undeleted`].
    ///
    /// # Errors
    ///
    /// Returns an error if the data could not be written to the database.
    ///
    async fn mark_undeleted_all_mail(
        ids: impl IntoIterator<Item = LocalConversationId>,
        bond: &Bond<'_>,
    ) -> Result<(), AppError> {
        for id in ids {
            info!("Unmarking {id:?} as deleted in all mail",);
            let Some(mut conversation) = Conversation::find_by_id(id, bond).await? else {
                continue;
            };

            let mut messages = Message::find(
                formatdoc! {"
                WHERE local_conversation_id=? AND deleted = 1
               "},
                params![id],
                bond,
            )
            .await?;

            let mut count = 0;
            let mut unread_count = 0;
            let mut attachment_count = 0;
            let mut size = 0;

            for message in &mut messages {
                message.deleted = false;
                count += 1;
                unread_count += message.unread as u64;
                attachment_count += message.num_attachments as u64;
                size += message.size;

                message.save(bond).await?
            }

            conversation.deleted = false;
            conversation.num_messages += count;
            conversation.num_unread += unread_count;
            conversation.num_attachments += attachment_count;
            conversation.size += size;

            conversation.save(bond).await?;

            if !messages.is_empty() {
                let stats = Message::update_message_counters_after_soft_undelete(
                    messages.into_iter(),
                    bond,
                )
                .await?;
                conversation
                    .add_conversation_to_all_labels(stats, bond)
                    .await?;
            }
        }

        Ok(())
    }

    /// Updates all labels counters after undelete of conversation in active view `AllMail`.
    ///
    /// # Errors
    ///
    /// Will return an error if the data could not be written to the database.
    ///
    async fn add_conversation_to_all_labels(
        &self,
        all_stats: HashMap<LocalLabelId, MessageLabelStats>,
        bond: &Bond<'_>,
    ) -> Result<(), AppError> {
        let conv_labels = ConversationLabel::find(
            "WHERE local_conversation_id=? AND deleted=1",
            params![self.id()],
            bond,
        )
        .await?;

        for mut conv_label in conv_labels {
            let label_id = conv_label.local_label_id.unwrap();
            let mut conv_counter = ConversationCounters::find_by_id(label_id, bond)
                .await?
                .ok_or_else(|| AppError::LabelNotFound(label_id))?;
            let stats = all_stats.get(&label_id);

            conv_counter.total += 1;

            if stats.filter(|s| s.unread_count > 0).is_some() {
                conv_counter.unread += 1;
            }

            conv_counter.save(bond).await?;

            conv_label.deleted = false;
            conv_label.save(bond).await?;
        }

        Ok(())
    }

    /// Mark conversations as undeleted in active label.
    /// More information can be found in [`Conversation::mark_undeleted`].
    ///
    /// # Errors
    ///
    /// Returns an error if the data could not be written to the database.
    ///
    async fn mark_undeleted_current_label(
        label_id: LocalLabelId,
        ids: impl IntoIterator<Item = LocalConversationId>,
        bond: &Bond<'_>,
    ) -> Result<(), AppError> {
        for id in ids {
            info!("Unmarking {id:?} as deleted in {label_id:?}",);
            let Some(mut conversation) =
                Conversation::find_first("WHERE local_id=? AND is_known=1", params![id], bond)
                    .await?
            else {
                continue;
            };

            let mut messages = Message::find(
                formatdoc! {"
                WHERE local_conversation_id=? AND deleted = 1 AND local_id IN (
                    SELECT local_message_id FROM message_labels WHERE local_label_id = ?
                )
               "},
                params![id, label_id],
                bond,
            )
            .await?;

            for message in &mut messages {
                message.deleted = false;
                message.save(bond).await?
            }

            if !messages.is_empty() {
                let all_stats = Message::update_message_counters_after_soft_undelete(
                    messages.into_iter(),
                    bond,
                )
                .await?;
                let stats = all_stats.get(&label_id);

                conversation
                    .add_conversation_to_label(label_id, stats, bond)
                    .await?;

                conversation.mark_undelete_update_stats(stats, bond).await?;
            }
        }

        Ok(())
    }

    pub async fn is_deleted(id: LocalConversationId, tether: &Tether) -> Result<bool, StashError> {
        Ok(tether
            .query_value_opt(
                format!(
                    "SELECT deleted FROM {} WHERE local_id=?",
                    Self::table_name()
                ),
                params![id],
            )
            .await?
            .unwrap_or(true))
    }

    /// Updates active label counters after undelete of conversation.
    ///
    /// # Errors
    ///
    /// Will return an error if the data could not be written to the database.
    ///
    pub async fn add_conversation_to_label(
        &mut self,
        label_id: LocalLabelId,
        stats: Option<&MessageLabelStats>,
        bond: &Bond<'_>,
    ) -> Result<(), AppError> {
        let conv_label = ConversationLabel::find_first(
            "WHERE local_conversation_id=? AND deleted=1 AND local_label_id=?",
            params![self.id(), label_id],
            bond,
        )
        .await?;

        if let Some(mut conv_label) = conv_label {
            let mut conv_counter = ConversationCounters::find_by_id(label_id, bond)
                .await?
                .ok_or_else(|| AppError::LabelNotFound(label_id))?;
            conv_counter.total += 1;

            if stats.filter(|s| s.unread_count > 0).is_some() {
                conv_counter.unread += 1;
            }

            conv_counter.save(bond).await?;

            conv_label.deleted = false;
            conv_label.save(bond).await?;
        }

        Ok(())
    }
    /// Updates conversation counters after delete of conversation.
    ///
    /// # Errors
    ///
    /// Will return an error if the data could not be written to the database.
    ///
    pub async fn mark_delete_update_stats(
        &mut self,
        stats: Option<&MessageLabelStats>,
        bond: &Bond<'_>,
    ) -> Result<(), AppError> {
        let undeleted_messages = Message::count(
            "WHERE local_conversation_id=? AND deleted=0",
            params![self.local_id],
            bond,
        )
        .await?;

        if undeleted_messages == 0 {
            self.deleted = true;
        }

        if let Some(stats) = stats {
            self.num_messages = self.num_messages.saturating_sub(stats.count);
            self.num_unread = self.num_unread.saturating_sub(stats.unread_count);
            self.num_attachments = self.num_attachments.saturating_sub(stats.attachment_count);
            self.size = self.size.saturating_sub(stats.size);
        }

        self.save(bond).await?;

        Ok(())
    }

    /// Updates conversation counters after undelete of conversation.
    ///
    /// # Errors
    ///
    /// Will return an error if the data could not be written to the database.
    ///
    pub async fn mark_undelete_update_stats(
        &mut self,
        stats: Option<&MessageLabelStats>,
        bond: &Bond<'_>,
    ) -> Result<(), AppError> {
        if let Some(stats) = stats {
            self.num_messages += stats.count;
            self.num_unread += stats.unread_count;
            self.num_attachments += stats.attachment_count;
            self.size += stats.size;
            self.deleted = false;
            self.save(bond).await?;
        }

        Ok(())
    }

    /// Delete multiple conversations.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed.
    ///
    pub async fn delete_multiple_remote<PM: ProtonMail>(
        ids: Vec<ConversationId>,
        label_id: LabelId,
        api: &PM,
    ) -> Result<Vec<OperationResult<ConversationId>>, ApiServiceError> {
        info!("Deleting {ids:?} in {label_id:?}");
        let request = |ids: Vec<ConversationId>| {
            let label_id = label_id.clone();
            async {
                api.put_conversations_delete(ids, label_id)
                    .await
                    .map(|r| r.responses)
            }
        };
        Conversation::split_request(ids, request).await
    }

    /// Get the conversation counts.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed.
    ///
    pub async fn fetch_counts<PM: ProtonMail>(
        api: &PM,
    ) -> Result<Vec<ConversationLabelsCount>, ApiServiceError> {
        api.get_conversations_count()
            .await
            .map(|r| r.counts.map_vec())
    }

    /// Retrieve in the first order the first unread message that should be displayed to the user
    /// from the conversation's `messages`. If none was found it will pick last message in the view.
    ///
    /// The returned message will depend on the `label` where the conversation
    /// is returned.
    ///
    /// # Errors
    ///
    /// When unable to pick the message for the conversation in the current view.
    ///
    pub fn message_id_to_open(
        local_id: LocalConversationId,
        label: &Label,
        messages: &[Message],
    ) -> Result<LocalMessageId, AppError> {
        if messages.is_empty() {
            return Err(AppError::ConversationHasNoMessages(local_id));
        }
        // If we fail to find any message, return the last message in the list.
        Ok(Self::first_unread_message(label, messages).unwrap_or(messages.last().unwrap().id()))
    }

    /// Retrieve in the first order the first unread message that should be displayed to the user
    /// from the conversation's `messages`. If none was found it will pick last message in the view.
    ///
    /// The returned message will depend on the `label` where the conversation
    /// is returned.
    ///
    pub fn first_unread_message(label: &Label, messages: &[Message]) -> Option<LocalMessageId> {
        if messages.is_empty() {
            return None;
        }

        fn first_consecutive_unread_msg(
            label_id: Option<&LabelId>,
            messages: &[Message],
            filter: impl Fn(&Message) -> bool,
        ) -> Option<LocalMessageId> {
            let mut last_unread = None;

            for msg in messages.iter().rev() {
                if msg.unread && filter(msg) {
                    last_unread.clone_from(&msg.local_id);
                } else if last_unread.is_some() {
                    break;
                }
            }

            last_unread.or_else(|| {
                messages
                    .iter()
                    .rev()
                    .find(|m| {
                        filter(m) && label_id.is_none_or(|label_id| m.label_ids.contains(label_id))
                    })
                    .and_then(|m| m.local_id)
            })
        }

        let view_is_starred_label_or_folder = label.label_type == LabelType::Label
            || label.label_type == LabelType::Folder
            || label.remote_id == Some(LabelId::starred());
        // If this is not a custom label or a folder we don't want to match against
        // label id.
        let label_id = if label.label_type != LabelType::System {
            Some(label.remote_id.as_ref()?)
        } else {
            None
        };

        if view_is_starred_label_or_folder {
            first_consecutive_unread_msg(label_id, messages, |msg| !msg.flags.is_draft())
        } else {
            first_consecutive_unread_msg(label_id, messages, |msg| {
                !(msg.flags.is_draft() || msg.flags.is_sent_auto())
            })
        }
    }

    /// TODO: Document this method.
    #[inline]
    #[must_use]
    pub fn is_starred(&self) -> bool {
        self.labels
            .iter()
            .any(|l| l.remote_label_id == Some(LabelId::starred()))
    }

    /// Load all models::Label for `self` models::ConversationLabel list.
    ///
    /// # Errors
    ///
    /// Database error.
    ///
    pub fn load_labels(&self, conn: &Connection) -> Result<Vec<Label>, StashError> {
        let ids = self
            .labels
            .iter()
            .filter_map(|label| label.local_label_id)
            .collect_vec();

        let placeholders = placeholders(&ids);
        let labels = Label::find_sync(
            format!(
                "WHERE local_id IN ({placeholders}) ORDER BY label_type DESC, display_order ASC",
            ),
            params_from_iter(ids),
            conn,
        )?;

        Ok(labels)
    }

    /// Mark multiple conversations as read.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed.
    ///
    pub async fn mark_multiple_as_read_remote<PM: ProtonMail>(
        ids: Vec<ConversationId>,
        api: &PM,
    ) -> Result<Vec<OperationResult<ConversationId>>, ApiServiceError> {
        info!("Marking {ids:?} as read");
        let request = |ids: Vec<ConversationId>| async {
            api.put_conversations_read(ids).await.map(|r| r.responses)
        };
        Conversation::split_request(ids, request).await
    }

    /// Mark multiple conversations as unread.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed.
    ///
    pub async fn mark_multiple_as_unread_remote(
        ids: Vec<ConversationId>,
        label_id: LabelId,
        api: &impl ProtonMail,
    ) -> Result<Vec<OperationResult<ConversationId>>, ApiServiceError> {
        info!("Marking {ids:?} as unread");
        let request = |ids: Vec<ConversationId>| async {
            api.put_conversations_unread(ids, label_id.clone())
                .await
                .map(|r| r.responses)
        };
        Conversation::split_request(ids, request).await
    }

    /// Mark multiple conversations as unread.
    /// For each conversation only the last read message gets marked as unread
    pub fn mark_unread(
        local_label_id: LocalLabelId,
        conversation_ids: impl IntoIterator<Item = LocalConversationId>,
        tx: &Transaction<'_>,
    ) -> Result<Vec<LocalMessageId>, StashError> {
        let mut modified = Vec::new();
        for conversation_id in conversation_ids {
            info!("Marking {conversation_id:?} as unread");
            let Some(mut conversation) = Conversation::load_by_id_sync(conversation_id, tx)? else {
                warn!("Conversation with id {conversation_id} does not exist!");
                continue;
            };

            // if conversation already has an unread message in this label context, skip.
            if let Some(conv_label) = conversation
                .labels
                .iter()
                .find(|l| l.local_label_id == Some(local_label_id))
                && conv_label.context_num_unread != 0
            {
                continue;
            }

            // Find all messages that need to be marked as read.
            let message = Message::find_first_sync(
                "
                JOIN message_labels AS ml ON messages.local_id = ml.local_message_id AND local_label_id=?
                WHERE local_conversation_id=?
                AND unread=0
                ORDER BY time DESC",
                (local_label_id, conversation_id),
                tx,
            )
            ?;

            let Some(mut message) = message else {
                let total_conversation_message_count =
                    Message::count_sync("WHERE local_conversation_id=?", (conversation_id,), tx)?;
                if total_conversation_message_count == 0 {
                    // These conversations where asked to be marked as read, but had
                    // no messages. Either the messages were already mark as read or
                    // there was no metadata. For these we need to set the unread
                    // count to 1 and update the current label count. We let the
                    // event loop take care of the rest.

                    let should_update_counter = if let Some(conv_label) = conversation
                        .labels
                        .iter_mut()
                        .find(|l| l.local_label_id.unwrap() == local_label_id)
                    {
                        // we only want to update the counter if it's the first time we have an
                        // unread message.
                        let should_update = conv_label.context_num_unread == 0;
                        conv_label.context_num_unread += 1;
                        should_update
                    } else {
                        false
                    };

                    conversation.num_unread += 1;
                    conversation.save_sync(tx)?;

                    if should_update_counter
                        && let Some(mut counter) =
                            ConversationCounters::load_by_id_sync(local_label_id, tx)?
                    {
                        counter.unread += 1;
                        counter.save_sync(tx)?;
                    }
                }
                continue;
            };

            // Update the message
            message.unread = true;
            message.save_sync(tx)?;
            modified.push(message.id());

            // Update the label counts

            let label_ids = tx.query_rows_col::<LocalLabelId>(
                "
                SELECT local_label_id
                FROM message_labels
                WHERE local_message_id=?",
                (message.id(),),
            )?;

            for label_id in label_ids {
                if let Some(mut counter) = MessageCounters::load_by_id_sync(label_id, tx)? {
                    // Always update the message count
                    counter.unread += 1;
                    counter.save_sync(tx)?;
                }

                if let Some(mut counter) = ConversationCounters::load_by_id_sync(label_id, tx)? {
                    if let Some(conv_label) = conversation
                        .labels
                        .iter_mut()
                        .find(|l| l.local_label_id.unwrap() == label_id)
                    {
                        // Only update conversation unread count if it is the first time we are marking
                        // the message as read.
                        if conv_label.context_num_unread == 0 {
                            counter.unread += 1;
                            counter.save_sync(tx)?;
                        }
                        conv_label.context_num_unread += 1;
                    } else {
                        debug!("No conv_label in convs");
                    }
                } else {
                    debug!("conv not labeled");
                }
            }

            // update conversation
            conversation.num_unread += 1;
            conversation.save_sync(tx)?;
        }
        Ok(modified)
    }

    pub async fn mark_unread_async(
        local_label_id: LocalLabelId,
        conversation_ids: impl IntoIterator<Item = LocalConversationId>,
        tx: &Bond<'_>,
    ) -> Result<Vec<LocalMessageId>, StashError> {
        let ids = Vec::from_iter(conversation_ids);
        tx.sync_bridge(move |tx| Self::mark_unread(local_label_id, ids, tx))
            .await
    }

    /// Unlabel multiple conversations.
    ///
    /// # Errors
    ///
    /// Returns an error if the data could not be written to the database.
    ///
    #[tracing::instrument(skip_all)]
    pub async fn remove_label_async(
        label_id: LocalLabelId,
        ids: impl IntoIterator<Item = LocalConversationId>,
        bond: &Bond<'_>,
    ) -> Result<Vec<LocalMessageId>, StashError> {
        let ids = Vec::from_iter(ids);
        bond.sync_bridge(move |tx| Self::remove_label(label_id, ids, tx))
            .await
    }

    /// Unlabel multiple conversations.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed.
    ///
    pub async fn remove_label_from_multiple_remote<PM: ProtonMail>(
        label_id: LabelId,
        ids: Vec<ConversationId>,
        api: &PM,
    ) -> Result<Vec<OperationResult<ConversationId>>, ApiServiceError> {
        info!("Removing {label_id:?} from {ids:?}");
        let request = |ids: Vec<ConversationId>| {
            let label_id = label_id.clone();
            async {
                api.put_conversations_unlabel(ids, label_id)
                    .await
                    .map(|r| r.responses)
            }
        };
        Conversation::split_request(ids, request).await
    }

    pub async fn snooze(
        local_label_id: LocalLabelId,
        ids: &[LocalConversationId],
        snooze_until: UnixTimestamp,
        bond: &Bond<'_>,
    ) -> Result<(), AppError> {
        Self::validate_snooze_location(local_label_id, bond).await?;

        if snooze_until <= UnixTimestamp::now() {
            return Err(AppError::SnoozeTimeInThePast);
        }

        Self::snooze_unchecked(ids, snooze_until, bond).await
    }

    pub async fn snooze_unchecked(
        ids: &[LocalConversationId],
        snooze_until: UnixTimestamp,
        bond: &Bond<'_>,
    ) -> Result<(), AppError> {
        let local_inbox_id = SystemLabel::Inbox
            .local_id(bond)
            .await?
            .expect("Inbox should be set");

        let local_snoozed_id = SystemLabel::Snoozed
            .local_id(bond)
            .await?
            .expect("Snoozed should be set");

        for id in ids {
            let message_ids =
                Message::update_snooze_time_with_conv_id(*id, local_inbox_id, snooze_until, bond)
                    .await?;
            Self::remove_label_async(local_inbox_id, [*id], bond).await?;

            if message_ids.is_empty() {
                // we don't have any messages available to us, apply the label and manually modify the time.
                // apply_label will create an initial state for the `ConversationLabel`
                Self::apply_label_async(local_snoozed_id, [*id], bond).await?;
                if let Some(mut label) = ConversationLabel::find_by_conversation_and_label_id(
                    *id,
                    local_snoozed_id,
                    bond,
                )
                .await?
                {
                    label.context_snooze_time = snooze_until;
                    label.save(bond).await?;
                }
            } else {
                // Apply snooze label directly to the know messages.
                Message::apply_label_async(local_snoozed_id, message_ids, bond).await?;
            }
        }

        Ok(())
    }

    pub async fn unsnooze(
        local_label_id: LocalLabelId,
        ids: &[LocalConversationId],
        bond: &Bond<'_>,
    ) -> Result<(), AppError> {
        Self::validate_snooze_location(local_label_id, bond).await?;

        let local_snoozed_id = SystemLabel::Snoozed
            .local_id(bond)
            .await?
            .expect("Snoozed should be set");
        let local_inbox_id = SystemLabel::Inbox
            .local_id(bond)
            .await?
            .expect("Inbox should be set");

        for id in ids {
            let message_ids = Message::update_snooze_time_with_conv_id(
                *id,
                local_snoozed_id,
                UnixTimestamp::new(0),
                bond,
            )
            .await?;

            Self::remove_label_async(local_snoozed_id, [*id], bond).await?;
            if message_ids.is_empty() {
                Self::apply_label_async(local_inbox_id, [*id], bond).await?;
            } else {
                Message::apply_label_async(local_inbox_id, message_ids, bond).await?;
            }
        }

        Ok(())
    }

    async fn validate_snooze_location(
        local_label_id: LocalLabelId,
        bond: &Bond<'_>,
    ) -> Result<(), AppError> {
        let label = Label::find_by_id(local_label_id, bond)
            .await?
            .ok_or(AppError::LabelNotFound(local_label_id))?;

        if !label.is_snooze_location() {
            return Err(AppError::InvalidSnoozeLocation(label.name.clone()));
        }

        Ok(())
    }

    pub async fn set_display_snooze_reminder(
        ids: &[LocalConversationId],
        bond: &Bond<'_>,
    ) -> Result<(), AppError> {
        let placeholders = placeholders(ids);
        let params = ids.to_sql();
        bond.execute(
            format!(
                "UPDATE {} SET display_snooze_reminder = 1 WHERE {} IN ({placeholders})",
                Conversation::table_name(),
                Conversation::id_field_name()
            ),
            params,
        )
        .await?;

        Ok(())
    }

    pub async fn context_snooze_time(
        id: LocalConversationId,
        local_label_id: LocalLabelId,
        tether: &Tether,
    ) -> Result<Option<UnixTimestamp>, StashError> {
        let snooze_time =
            ConversationLabel::find_by_conversation_and_label(id, local_label_id, tether)
                .await?
                .map(|l| l.context_snooze_time);
        Ok(snooze_time)
    }

    /// Sync only conversations metadata
    ///
    pub async fn sync_metadata<PM: ProtonMail>(
        ids: Vec<ConversationId>,
        api: &PM,
        mut tx: impl RunTransaction,
    ) -> Result<Vec<Self>, AppError> {
        let remote_convs = api
            .get_conversations(GetConversationsOptions {
                ids: ids.into_iter().map_into().collect(),
                ..Default::default()
            })
            .await?
            .conversations;
        let mut local_convs = Vec::with_capacity(remote_convs.len());

        tx.run_tx(async |tx| {
            for conv in remote_convs {
                let mut conv = Self::from(conv);
                conv.save(tx).await?;
                local_convs.push(conv);
            }
            Ok(())
        })
        .await?;

        Ok(local_convs)
    }

    /// Undelete multiple conversations.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed.
    ///
    pub async fn undelete_multiple_remote<PM: ProtonMail>(
        ids: Vec<ConversationId>,
        label_id: LabelId,
        api: &PM,
    ) -> Result<Vec<OperationResult<ConversationId>>, ApiServiceError> {
        let request = |ids: Vec<ConversationId>| {
            let label_id = label_id.clone();
            async {
                api.put_conversations_delete(ids, label_id)
                    .await
                    .map(|r| r.responses)
            }
        };
        Conversation::split_request(ids, request).await
    }

    pub async fn available_snooze_actions(
        local_ids: Vec<LocalConversationId>,
        user: &User,
        week_start: WeekStart,
        tether: &Tether,
    ) -> Result<SnoozeOptions, AppError> {
        if local_ids.is_empty() {
            return Err(AppError::EmptyListOfConversations);
        }

        let conversations = Conversation::find_by_ids(local_ids, tether).await?;
        let is_snoozed = conversations.iter().any(|c| c.snoozed_until.is_some());
        let today = Local::now();
        let Some(snooze_options) = SnoozeOptions::new(today, week_start, user, is_snoozed) else {
            // This should never happen, but we handle it just in case.
            return Err(AppError::CouldNotCalculateSnoozeOptions);
        };

        Ok(snooze_options)
    }

    /// Get the available `label as` actions for conversations
    ///
    /// # Errors
    ///
    /// Returns error if the database request fail.
    ///
    #[tracing::instrument(skip_all)]
    pub async fn available_label_as_actions(
        local_ids: Vec<LocalConversationId>,
        tether: &Tether,
    ) -> Result<Vec<LabelAsAction>, AppError> {
        if local_ids.is_empty() {
            return Err(AppError::EmptyListOfConversations);
        }
        debug!("{local_ids:?}");

        let all_label_as = Label::find_by_kind(LabelType::Label, tether).await?;
        let conversations =
            <Conversation as ModelExtension>::find_by_ids(local_ids, tether).await?;
        let all_label_as_actions = conversations.iter().flat_map(|conversation| {
            LabelAsAction::vec(all_label_as.iter(), |label| {
                conversation
                    .custom_labels
                    .iter()
                    .map(|label| Some(label.local_id))
                    .contains(&label.local_id)
            })
        });

        let res = LabelAsAction::finalize(all_label_as_actions);

        debug!("Available label_as actions for conversations: {res:?}");
        Ok(res)
    }

    /// Watches `label as` actions for conversations
    ///
    /// # Errors
    ///
    /// Returns error if the database request fail.
    ///
    #[tracing::instrument(skip_all)]
    pub async fn watch_available_label_as_actions(
        local_ids: Vec<LocalConversationId>,
        tether: &Tether,
    ) -> Result<(Vec<LabelAsAction>, WatcherHandle), AppError> {
        let res = Self::available_label_as_actions(local_ids, tether).await?;
        let handle =
            tether.subscribe_to(|sender| Box::new(ConversationActionWatcher { sender }))?;
        debug!("watch available label_as actions for conversations: {res:?}");
        Ok((res, handle))
    }

    /// Get the available move actions for conversations
    ///
    /// # Errors
    ///
    /// Returns error if the database request fail.
    ///
    #[tracing::instrument(skip_all, fields(label_id=view.id().as_u64()))]
    pub async fn available_move_to_actions(
        view: Label,
        local_ids: Vec<LocalConversationId>,
        tether: &Tether,
    ) -> Result<Vec<MoveAction>, AppError> {
        if local_ids.is_empty() {
            return Err(AppError::EmptyListOfConversations);
        }

        debug!("{local_ids:?}");

        let all_system = Label::find_by_kind(LabelType::System, tether).await?;
        let all_system_excluding_view = all_system
            .iter()
            .filter(|label| label.local_id != view.local_id);
        let all_custom_folders = Label::find_by_kind(LabelType::Folder, tether).await?;
        let conversations = Conversation::find(
            format!(
                "WHERE local_id IN ({})",
                local_ids.iter().map(ToString::to_string).join(",")
            ),
            vec![],
            tether,
        )
        .await?;

        conversations.iter().try_for_each(|conversation| {
            let is_conversation_in_view = conversation
                .labels
                .iter()
                .map(|conv_label| conv_label.local_label_id)
                .any(|local_id| local_id == view.local_id);

            if is_conversation_in_view {
                Ok(())
            } else {
                Err(AppError::ConversationDoesNotHaveLabel(
                    conversation.id(),
                    view.name.clone(),
                ))
            }
        })?;

        let all_move_to_actions = MoveAction::vec(
            all_system_excluding_view
                .clone()
                .chain(all_custom_folders.iter()),
        );

        let res = MoveAction::finalize(all_move_to_actions, tether).await?;
        debug!("available move_to actions: {res:?}");
        Ok(res)
    }

    /// Count all local messages from this conversation
    pub async fn message_count(
        local_id: LocalConversationId,
        tether: &Tether,
    ) -> Result<u64, StashError> {
        Message::count(
            "WHERE local_conversation_id == ?",
            params![local_id],
            tether,
        )
        .await
    }

    pub async fn local_message_count_with_remote_id(
        local_id: LocalConversationId,
        tether: &Tether,
    ) -> Result<u64, StashError> {
        Message::count(
            "WHERE local_conversation_id == ? AND remote_id IS NOT NULL",
            params![local_id],
            tether,
        )
        .await
    }

    /// Finds all the messages from this conversation
    pub async fn load_messages(&self, tether: &Tether) -> Result<Vec<Message>, StashError> {
        Message::find(
            "WHERE local_conversation_id == ? ORDER BY time ASC, display_order ASC",
            params![self.id()],
            tether,
        )
        .await
    }

    /// Finds all the conversations that have expired and deletes them and all of its
    /// messages.
    pub async fn delete_expired(tether: &mut Tether) -> Result<usize, AppError> {
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

        let len = ids.len();

        if len != 0 {
            let label_id = SystemLabel::AllMail
                .local_id(tether)
                .await?
                .ok_or_else(|| StashError::IdNotSet)?;
            tether
                .tx(async |tx| Self::mark_deleted(label_id, ids, tx).await)
                .await?;
        }

        Ok(len)
    }

    #[cfg(test)]
    // TODO: Figure out how we want to do this in the future.
    ///
    /// Intended for testing only
    /// (local_attachment_id, local_message_id)
    /// Sets a conversation to be deleted in `expire_in` ms
    pub async fn set_expiration_time_in(
        id: LocalConversationId,
        expire_in: i64,
        tether: &mut Tether,
    ) -> Result<(), StashError> {
        let affected = tether
            .tx(async |tx| {
                tx.execute(
                    r"
            UPDATE
                conversations
            SET
                expiration_time = (STRFTIME('%s', 'NOW') + ?)
            WHERE
                local_id = ?
            ",
                    params![expire_in, id],
                )
                .await
            })
            .await?;
        if affected != 1 {
            Err(StashError::Custom(anyhow::anyhow!("No conversation found")))
        } else {
            Ok(())
        }
    }

    /// Shared implementation to apply a label for messages and conversation.
    ///
    /// # Params
    ///
    /// * `local_label_id`         - Local label id of the [`Label`].
    /// * `local_conversation_id`  - Local conversation id to which the label
    ///   should be applied.
    /// * `local_message_ids`      - Local ids of the messages which belong to
    ///   `local_conversation_id` where the label
    ///   should be applied.
    pub fn label_impl(
        label_id: LocalLabelId,
        conversation_id: LocalConversationId,
        message_ids: &[LocalMessageId],
        tx: &Transaction<'_>,
    ) -> Result<(), StashError> {
        if message_ids.is_empty() {
            return Ok(());
        }

        let (has_label, is_unread) = if let Some(num_unread) = tx
            .query_row_col::<u64>(
                "SELECT context_num_unread FROM conversation_labels
                            WHERE local_conversation_id=? AND local_label_id=?",
                (conversation_id, label_id),
            )
            .optional()?
        {
            (true, num_unread != 0)
        } else {
            (false, false)
        };

        let stats =
            ConversationMessageLabelStats::with(conversation_id, label_id, message_ids, tx)?;

        // Update conversation labels.
        let mut conversation_label = if let Some(mut label) = ConversationLabel::find_first_sync(
            "WHERE local_conversation_id=? AND local_label_id=?",
            (conversation_id, label_id),
            tx,
        )? {
            label.context_time = label.context_time.max(stats.time);
            label.context_snooze_time = label.context_snooze_time.max(stats.snooze_time);
            label
                .context_expiration_time
                .merge_self(stats.expiration_time);
            label.context_size += stats.size;
            label.context_num_unread += stats.unread;
            label.context_num_attachments += stats.num_attachments as u64;
            label.context_num_messages += stats.count;
            label
        } else {
            let remote_label_id = if let Some(label) = Label::load_by_id_sync(label_id, tx)? {
                label.remote_id
            } else {
                None
            };
            ConversationLabel {
                local_id: None,
                local_conversation_id: Some(conversation_id),
                local_label_id: Some(label_id),
                remote_label_id,
                context_expiration_time: stats.expiration_time,
                context_num_attachments: stats.num_attachments as u64,
                context_num_messages: stats.count,
                context_num_unread: stats.unread,
                context_size: stats.size,
                context_snooze_time: stats.snooze_time,
                context_time: stats.time,
                deleted: false,
            }
        };

        conversation_label.save_sync(tx)?;

        // Update message label counts.
        let Some(mut conv_counters) = ConversationCounters::load_by_id_sync(label_id, tx)? else {
            error!("Could not find label counters");
            return Err(StashError::ExecutionError(SqliteError::QueryReturnedNoRows));
        };

        let Some(mut msg_counters) = MessageCounters::load_by_id_sync(label_id, tx)? else {
            error!("Could not find label counters");
            return Err(StashError::ExecutionError(SqliteError::QueryReturnedNoRows));
        };

        msg_counters.unread += stats.unread;
        msg_counters.total += stats.count;

        let should_increment_count = !has_label;
        let should_increment_unread = !is_unread && stats.unread != 0;

        conv_counters.total += should_increment_count as u64;
        conv_counters.unread += should_increment_unread as u64;

        conv_counters.save_sync(tx)?;
        msg_counters.save_sync(tx)?;

        Ok(())
    }

    #[tracing::instrument(skip(tx, session, network_monitor_service, queue))]
    #[cfg_attr(not(feature = "action_rebase"), allow(unused_variables))]
    pub async fn sync_conversation_messages_from_push_notification(
        network_monitor_service: &NetworkMonitorService,
        local_conversation_id: LocalConversationId,
        tx: &mut impl RunTransaction,
        session: &Session,
        queue: &Queue,
    ) -> Result<Conversation, AppError> {
        let Some(conversation) = Self::find_by_id(local_conversation_id, tx.tether()).await? else {
            return Err(AppError::ConversationNotFound(local_conversation_id));
        };

        let Some(ref rid) = conversation.remote_id else {
            return Err(AppError::ConversationHasNoRemoteId(local_conversation_id));
        };

        if network_monitor_service.is_os_offline() {
            debug!("No connection, skipping sync");
            return Err(AppError::API(ApiServiceError::NetworkError(
                "No connection".to_owned(),
            )));
        }

        info!("Syncing {rid:?}'s messages");
        let mut conversation_response = match session
            .get_conversation(rid.clone())
            .await
            .inspect_err(|e| {
                error!("failed to download conversation messages: {e:?}");
            }) {
            Ok(r) => r,
            Err(ApiServiceError::UnprocessableEntity(s, Some(api_error))) => {
                return if api_error.code == Mail::ConversationDoesNotExist as u32 {
                    Err(AppError::ConversationDoesNotExistOnServer(rid.clone()))
                } else {
                    Err(AppError::from(ApiServiceError::UnprocessableEntity(
                        s,
                        Some(api_error),
                    )))
                };
            }
            Err(e) => return Err(AppError::from(e)),
        };

        tx.run_tx::<_, _>(async move |tx| {
            let mut rebase_change_set = RebaseChangeSet::default();
            let had_messages = conversation.has_messages;
            let should_sync_conv = conversation
                .to_api_conversation()
                .map(|mut v| {
                    let sort_conv_labels_fn =
                        |l1: &ApiConversationLabel, l2: &ApiConversationLabel| l1.id.cmp(&l2.id);
                    let sort_attachment_metadata =
                        |l1: &ApiAttachmentMetadata, l2: &ApiAttachmentMetadata| l1.id.cmp(&l2.id);
                    conversation_response
                        .conversation
                        .labels
                        .sort_unstable_by(sort_conv_labels_fn);
                    v.labels.sort_unstable_by(sort_conv_labels_fn);
                    v.attachments_metadata
                        .sort_unstable_by(sort_attachment_metadata);
                    conversation_response
                        .conversation
                        .attachments_metadata
                        .sort_unstable_by(sort_attachment_metadata);

                    v != conversation_response.conversation
                })
                .unwrap_or(true);

            let new_conversation = if should_sync_conv {
                let mut new_conversation: Conversation = conversation_response.conversation.into();
                new_conversation.local_id = conversation.local_id;
                new_conversation.has_messages = true;
                new_conversation.is_known = true;
                debug!("Updating conversation");
                new_conversation.save(tx).await?;
                rebase_change_set.add(new_conversation.id());

                new_conversation
            } else {
                conversation
            };

            let message_metadata: Vec<ApiMessageMetadata> = conversation_response.messages;
            if had_messages {
                info!("Messages were synced before");
            } else {
                info!("Never synced conversation messages before");
            }
            // We need to always update the conversation and messages as it possible
            // that some message state we have locally does not match the label context
            // data downloaded from the newer conversation, which can lead to other
            // action not behaving accordingly.
            // Note that this does overwrite local state with new state, meaning local
            // changes can temporarily be lost until the respective actions
            // are executed on the server.
            // This has been deemed more acceptable than the user complaining that their
            // conversations can't  be marked as read after marking all
            // conversations as read.
            let ids = Message::create_or_update_messages_from_metadata(message_metadata, None, tx)
                .await
                .map_err(|e| {
                    error!("Failed to write message metadata: {e:?}");
                    e
                })?;
            rebase_change_set.add_many(ids);

            #[cfg(feature = "action_rebase")]
            if let Err(e) = queue
                .rebase_in(ActionGroup::default(), &rebase_change_set, tx)
                .await
            {
                tracing::error!("Failed to rebase changes: {e}");
            }

            Ok(new_conversation)
        })
        .await
        .map_err(AppError::Other)
    }

    #[tracing::instrument(skip(tx, session, network_monitor_service, queue))]
    pub async fn sync_conversation_messages(
        network_monitor_service: &NetworkMonitorService,
        local_conversation_id: LocalConversationId,
        tx: &mut impl RunTransaction,
        session: &Session,
        extra_sync_allowed: bool,
        queue: &Queue,
    ) -> Result<(), AppError> {
        let Some(mut conversation) = Self::find_by_id(local_conversation_id, tx.tether()).await?
        else {
            return Err(AppError::ConversationNotFound(local_conversation_id));
        };

        let total_message_count =
            Conversation::local_message_count_with_remote_id(local_conversation_id, tx.tether())
                .await?;
        let should_sync_all_messages =
            extra_sync_allowed && total_message_count != conversation.num_messages;
        if !conversation.has_messages {
            let Some(ref rid) = conversation.remote_id else {
                return Err(AppError::ConversationHasNoRemoteId(local_conversation_id));
            };
            info!("Syncing {rid:?}'s messages");

            if network_monitor_service.check_now().await.is_offline() {
                debug!("No connection, skipping sync");
                return Err(AppError::API(ApiServiceError::NetworkError(
                    "No connection".to_owned(),
                )));
            }

            let conversation_response = match session
                .get_conversation(rid.clone())
                .await
                .inspect_err(|e| {
                    error!("failed to download conversation messages: {e:?}");
                }) {
                Ok(r) => r,
                Err(ApiServiceError::UnprocessableEntity(s, Some(api_error))) => {
                    return if api_error.code == Mail::ConversationDoesNotExist as u32 {
                        Err(AppError::ConversationDoesNotExistOnServer(rid.clone()))
                    } else {
                        Err(AppError::from(ApiServiceError::UnprocessableEntity(
                            s,
                            Some(api_error),
                        )))
                    };
                }
                Err(e) => return Err(AppError::from(e)),
            };

            tx.run_tx::<_, _>(async move |tx| {
                let mut rebase_change_set = RebaseChangeSet::default();

                let message_metadata: Vec<ApiMessageMetadata> = conversation_response.messages;

                let ids =
                    Message::create_or_update_messages_from_metadata(message_metadata, None, tx)
                        .await
                        .map_err(|e| {
                            error!("Failed to write message metadata: {e:?}");
                            e
                        })?;

                rebase_change_set.add_many(ids);

                if conversation.is_known {
                    debug!("Conversation was known");
                    conversation.has_messages = true;
                    conversation.save(tx).await.map_err(|e| {
                        error!("Failed to write conversation: {e:?}");
                        e
                    })?;
                } else {
                    debug!("Conversation was not known");
                    let mut new_conversation: Conversation =
                        conversation_response.conversation.into();

                    new_conversation.local_id = conversation.local_id;
                    new_conversation.has_messages = true;

                    new_conversation.save(tx).await.map_err(|e| {
                        error!("Failed to write conversation: {e:?}");
                        e
                    })?;
                    rebase_change_set.add(new_conversation.id());
                }

                #[cfg(feature = "action_rebase")]
                {
                    if let Err(e) = queue
                        .rebase_in(ActionGroup::default(), &rebase_change_set, tx)
                        .await
                    {
                        tracing::error!("Failed to rebase changes: {e}");
                    }
                }

                Ok(())
            })
            .await
            .map_err(AppError::Other)?;
        } else if should_sync_all_messages {
            info!("Message state mismatch, syncing conversation from server");
            Self::sync_conversation_messages_from_push_notification(
                network_monitor_service,
                local_conversation_id,
                tx,
                session,
                queue,
            )
            .await?;
            return Ok(());
        } else {
            info!("Conversation messages already synced")
        }

        Ok(())
    }

    /// Retrieve all the conversation which are in a given label.
    ///
    /// # Params
    ///
    /// * `local_label_id` - Label where to search in
    /// * `interface`      - Connection to the database
    /// * `queue`          - Optional subscriber for changes.
    ///
    /// # Errors
    ///
    /// Returns error if the query fails.
    pub async fn in_label(
        local_label_id: LocalLabelId,
        tether: &Tether,
    ) -> Result<Vec<Self>, StashError> {
        Conversation::find(
            indoc!(
                "
                JOIN conversation_labels
                    ON conversations.local_id = conversation_labels.local_conversation_id
                WHERE
                    conversation_labels.local_label_id = ?
                AND
                    conversation_labels.deleted = 0
                ORDER BY
                    conversation_labels.context_time DESC,
                    conversations.display_order DESC
                "
            ),
            params![local_label_id],
            tether,
        )
        .await
    }

    /// This fn should be called for conversation endpoints.
    /// Repeatedly calls `endpoint` in batches of 1 in parallel.
    async fn split_request<F, Fut, T>(
        ids: impl IntoIterator<Item = T>,
        endpoint: F,
    ) -> Result<Vec<OperationResult<T>>, ApiServiceError>
    where
        // TODO: Change me for an AsyncFn
        F: Fn(Vec<T>) -> Fut,
        Fut: Future<Output = Result<Vec<OperationResult<T>>, ApiServiceError>>,
        T: ProtonIdMarker,
    {
        split_request(ids, 1, endpoint).await
    }

    /// Get the possible next display order.
    ///
    /// Finds the maximum display order value in all conversations and adds 1
    /// to the existing value.
    ///
    /// # Errors
    ///
    /// Returns error if the query failed.
    ///
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

    /// Only get Disposition::Attachment attachments
    pub fn get_attachment_metadata(&self) -> Vec<AttachmentMetadata> {
        self.attachments_metadata
            .iter()
            .filter(|mdata| matches!(mdata.disposition, Disposition::Attachment))
            .cloned()
            .collect()
    }

    /// Only get Disposition::Inline attachments
    #[allow(dead_code)] // Will get used later on
    fn get_inline_attachment_metadata(&self) -> Vec<AttachmentMetadata> {
        self.attachments_metadata
            .iter()
            .filter(|mdata| matches!(mdata.disposition, Disposition::Inline))
            .cloned()
            .collect()
    }

    /// Queries `ConversationLabel` database and finds if there is a label with given `LocalId` in it.
    pub async fn has_label(
        &self,
        label_local_id: LocalLabelId,
        tether: &Tether,
    ) -> Result<bool, StashError> {
        let local_conversation_id = self.id();

        // Find the first matching label
        let label = ConversationLabel::find_first(
            "WHERE local_conversation_id = ? AND local_label_id = ? AND deleted = 0",
            params![local_conversation_id, label_local_id],
            tether,
        )
        .await?;

        Ok(label.is_some())
    }

    /// Update a conversation with `local_conversation_id`'s remote id.
    ///
    /// # Error
    ///
    /// Return error if the query failed.
    pub(crate) async fn update_remote_id(
        local_conversation_id: LocalConversationId,
        conversation_id: ConversationId,
        bond: &Bond<'_>,
    ) -> Result<usize, StashError> {
        bond.execute(
            format!(
                "UPDATE {} SET remote_id=? WHERE local_id=?",
                Self::table_name()
            ),
            params![conversation_id, local_conversation_id],
        )
        .await
    }

    pub async fn update_subject(
        id: LocalConversationId,
        subject: String,
        bond: &Bond<'_>,
    ) -> Result<usize, StashError> {
        bond.execute(
            format!(
                "UPDATE {} SET subject=? WHERE local_id=?",
                Self::table_name()
            ),
            params![subject, id],
        )
        .await
    }

    pub fn to_api_conversation(&self) -> Option<ApiConversation> {
        self.remote_id.clone().map(|id| ApiConversation {
            id,
            attachment_info: self
                .attachment_info
                .value
                .iter()
                .map(|(k, v)| (k.clone(), v.clone().into()))
                .collect(),
            attachments_metadata: self
                .attachments_metadata
                .iter()
                .filter_map(|v| v.to_api_attachment_metadata())
                .collect(),
            display_snoozed_reminder: self.display_snooze_reminder,
            expiration_time: self.expiration_time.as_u64(),
            labels: self
                .labels
                .iter()
                .filter_map(|v| v.to_api_conversation_label())
                .collect(),
            num_attachments: self.num_attachments,
            num_messages: self.num_messages,
            num_unread: self.num_unread,
            order: self.display_order,
            recipients: self.recipients.iter().cloned().map(Into::into).collect(),
            senders: self.senders.value.iter().cloned().map(Into::into).collect(),
            size: self.size,
            subject: self.subject.clone(),
            context_time: None,
        })
    }

    #[cfg(feature = "test-utils")]
    pub fn sort_labels(&mut self) {
        self.labels
            .sort_by(|l1, l2| l1.local_label_id.cmp(&l2.local_label_id));
    }
}

impl ConversationOrMessage for Conversation {
    const ROLLBACK_ITEM_TYPE: RollbackItemType = RollbackItemType::Conversation;

    fn apply_label(
        label_id: LocalLabelId,
        ids: impl IntoIterator<Item = Self::IdType>,
        tx: &Transaction<'_>,
    ) -> Result<Vec<LocalMessageId>, StashError> {
        let mut modified = Vec::new();
        for conv_id in ids {
            info!("Applying {label_id:?} to {conv_id:?}");
            let message_ids = tx.query_rows_col::<LocalMessageId>(
                indoc::indoc! {"
                    WITH conv_msgs AS (
                        SELECT local_id, ? AS label_id
                        FROM messages
                        WHERE local_conversation_id=?
                    )
                    INSERT OR IGNORE INTO
                        message_labels (local_message_id, local_label_id)
                    SELECT * FROM conv_msgs
                    RETURNING local_message_id
                    "},
                (label_id, conv_id),
            )?;

            modified.extend(message_ids.iter().copied());

            if !message_ids.is_empty() {
                Conversation::label_impl(label_id, conv_id, &message_ids, tx)?;
                continue;
            }

            // Fallback without message metadata. We should grab the highest time values from
            // all the remaining labels assigned to this conversation. All conversations
            // messages will always have the All Mail label assigned.
            if ConversationLabel::count_sync(
                "WHERE local_conversation_id=? AND local_label_id=?",
                (conv_id, label_id),
                tx,
            )? != 0
            {
                // conv already labeled
                continue;
            }
            let label = Label::load_by_id_exact_sync(label_id, tx)?;

            let mut new_label = ConversationLabel {
                local_id: None,
                local_conversation_id: Some(conv_id),
                local_label_id: Some(label_id),
                remote_label_id: label.remote_id.clone(),
                context_expiration_time: UnixTimestamp::new(0).into(),
                context_num_attachments: 0,
                context_num_messages: 0,
                context_num_unread: 0,
                context_size: 0,
                context_snooze_time: 0.into(),
                context_time: 0.into(),
                deleted: false,
            };
            let conversation_labels =
                ConversationLabel::find_sync("WHERE local_conversation_id=?", (conv_id,), tx)?;
            for conversation_label in conversation_labels {
                new_label.context_expiration_time =
                    if new_label.context_expiration_time.as_u64() == 0 {
                        conversation_label.context_expiration_time
                    } else {
                        conversation_label
                            .context_expiration_time
                            .min(new_label.context_expiration_time)
                    };
                new_label.context_num_attachments = conversation_label
                    .context_num_attachments
                    .max(new_label.context_num_attachments);
                new_label.context_num_messages = conversation_label
                    .context_num_messages
                    .max(new_label.context_num_messages);
                new_label.context_num_unread = conversation_label
                    .context_num_unread
                    .max(new_label.context_num_unread);
                new_label.context_size =
                    conversation_label.context_size.max(new_label.context_size);
                new_label.context_snooze_time = conversation_label
                    .context_snooze_time
                    .max(new_label.context_snooze_time);
                new_label.context_time =
                    conversation_label.context_time.max(new_label.context_time);
            }

            new_label.save_sync(tx)?;

            let mut counters = ConversationCounters::load_by_id_exact_sync(label_id, tx)?;
            counters.total += 1;
            counters.save_sync(tx)?;
        }

        Ok(modified)
    }

    fn remove_label(
        label_id: LocalLabelId,
        ids: impl IntoIterator<Item = Self::IdType>,
        tx: &Transaction<'_>,
    ) -> Result<Vec<LocalMessageId>, StashError> {
        let mut ids = ids.into_iter().peekable();
        if ids.peek().is_none() {
            return Ok(vec![]);
        }

        let mut conv_counter = ConversationCounters::load_by_id_exact_sync(label_id, tx)?;

        let mut modified_messages = Vec::new();

        for id in ids {
            info!("Removing {label_id:?} from {id:?}",);
            // Remove label from messages
            let message_ids = tx.query_rows_col::<LocalMessageId>(
                indoc! {"
                    DELETE FROM message_labels
                    WHERE local_message_id IN (
                        SELECT local_id
                        FROM messages
                        WHERE local_conversation_id=?1
                    ) AND message_labels.local_label_id=?2
                    RETURNING local_message_id
                    "},
                (id, label_id),
            )?;

            modified_messages.extend(message_ids.iter().copied());

            // We can only do this part if we have conversation metadata.
            if !message_ids.is_empty() {
                // PERF: SELECT COUNT unread instead
                let num_unread = Message::find_sync(
                    format!("WHERE local_id IN ({})", placeholders(&message_ids),),
                    params_from_iter(&message_ids),
                    tx,
                )?
                .into_iter()
                .fold(0_u64, |mut value, message| {
                    if message.unread {
                        value += 1;
                    }
                    value
                });

                if let Some(mut msg_counter) = MessageCounters::load_by_id_sync(label_id, tx)? {
                    msg_counter.total = msg_counter.total.saturating_sub(message_ids.len() as u64);
                    msg_counter.unread = msg_counter.unread.saturating_sub(num_unread);
                    msg_counter.save_sync(tx)?;
                }
            }

            // Remove conversation label
            if let Some(num_unread) = tx
                .query_row_col::<u64>(
                    indoc! {"
                    DELETE FROM conversation_labels
                    WHERE local_conversation_id=? AND local_label_id=?
                    RETURNING context_num_unread
                    "},
                    (id, label_id),
                )
                .optional()?
            {
                if num_unread > 0 {
                    conv_counter.unread = conv_counter.unread.saturating_sub(1);
                }
                conv_counter.total = conv_counter.total.saturating_sub(1);
            }
        }

        conv_counter.save_sync(tx)?;
        Ok(modified_messages)
    }

    async fn api_apply_label(
        api: &impl ProtonMail,
        ids: Vec<Self::RemoteId>,
        label_id: LabelId,
    ) -> Result<Vec<Self::RemoteId>, ApiServiceError> {
        info!("Applying {label_id:?} to {ids:?}");
        let label_id = &label_id;
        let request = |ids: Vec<ConversationId>| async move {
            api.put_conversations_label(ids.clone(), label_id.clone(), None)
                .await
                .map(|v| v.responses)
        };
        Conversation::split_request(ids, request)
            .await
            .map(filter_responses)
    }

    async fn api_remove_label(
        api: &impl ProtonMail,
        ids: Vec<Self::RemoteId>,
        label_id: LabelId,
    ) -> Result<Vec<Self::RemoteId>, ApiServiceError> {
        info!("Removing {label_id:?} form {ids:?}");
        let label_id = &label_id;
        let request = |ids: Vec<ConversationId>| async move {
            api.put_conversations_unlabel(ids.clone(), label_id.clone())
                .await
                .map(|v| v.responses)
        };
        Conversation::split_request(ids, request)
            .await
            .map(filter_responses)
    }

    fn get_exclusive_locations(&self) -> Vec<LocalLabelId> {
        self.locations.iter().map(|x| x.local_id()).collect()
    }

    fn mark_read(
        ids: impl IntoIterator<Item = LocalConversationId>,
        bond: &Transaction<'_>,
    ) -> Result<Vec<LocalMessageId>, StashError> {
        let mut read_messages = vec![];
        let mut conversation_label_counts = HashMap::new();
        let mut message_label_counts = HashMap::new();

        for conversation_id in ids {
            info!("Marking {conversation_id:?} as read");
            let mut conversation = Conversation::load_by_id_exact_sync(conversation_id, bond)?;
            // If conversation has no unread messages, we need to check if it has a snooze reminder.
            if conversation.num_unread == 0 {
                if conversation.display_snooze_reminder {
                    conversation.display_snooze_reminder = false;
                    conversation.save_sync(bond)?;
                }

                continue;
            }

            // Otherwise, update conversation unread count.
            conversation.num_unread = 0;
            conversation.display_snooze_reminder = false;

            for conversation_label in conversation
                .labels
                .iter_mut()
                .filter(|l| l.context_num_unread != 0)
            {
                conversation_label.context_num_unread = 0;
                conversation_label_counts
                    .entry(conversation_label.local_label_id.unwrap())
                    .and_modify(|x| *x += 1)
                    .or_insert(1);
            }

            conversation.save_sync(bond)?;

            // Update messages
            let messages = Message::find_sync(
                "WHERE local_conversation_id=? AND unread<>0",
                (conversation_id,),
                bond,
            )?;

            let mut stmt = bond.prepare_cached(
                "SELECT local_label_id FROM message_labels WHERE local_message_id=?",
            )?;
            for mut message in messages {
                let local_message_id = message.id();
                message.unread = false;
                message.save_sync(bond)?;
                read_messages.push(local_message_id);

                let label_ids = stmt
                    .query_map((local_message_id,), |r| r.get(0))?
                    .collect::<Result<Vec<LocalLabelId>, _>>()?;

                for label_id in label_ids {
                    message_label_counts
                        .entry(label_id)
                        .and_modify(|x| *x += 1)
                        .or_insert(1);
                }
            }
        }

        // update message label counters
        for (label_id, count) in &mut message_label_counts {
            if let Some(mut counters) = MessageCounters::load_by_id_sync(*label_id, bond)? {
                counters.unread = counters.unread.saturating_sub(*count);
                counters.save_sync(bond)?;
            }
        }
        // Update conversation counters
        for (label_id, count) in &mut conversation_label_counts {
            if let Some(mut conv_counter) = ConversationCounters::load_by_id_sync(*label_id, bond)?
            {
                conv_counter.unread = conv_counter.unread.saturating_sub(*count);
                conv_counter.save_sync(bond)?;
            }
        }

        Ok(read_messages)
    }

    fn grouped_labels_and_messages_query(placeholders: usize) -> String {
        formatdoc! {"
            SELECT
                local_label_id,
                GROUP_CONCAT(local_conversation_id)
            FROM conversation_labels
            WHERE local_conversation_id IN ({})
            GROUP BY local_label_id
            ",
            placeholders_n(placeholders)
        }
    }
}

impl ModelHooks for Conversation {
    fn after_save(&mut self, tx: &Transaction<'_>) -> Result<(), StashError> {
        // Remove any labels that are no longer associated with this conversation.

        if !self.labels.is_empty() {
            let mut params: Vec<&dyn ToSql> = vec![&self.local_id];

            for l in &self.labels {
                if let Some(l) = &l.remote_label_id {
                    params.push(l);
                }
            }

            tx.execute(
                &formatdoc!(
                    "
                DELETE FROM
                    conversation_labels
                WHERE
                    local_conversation_id = ?
                    AND remote_label_id NOT IN ({})
                ",
                    stash::utils::placeholders_n(params.len() - 1),
                ),
                params_from_iter(params),
            )?;
        } else {
            tx.execute(
                "
                DELETE FROM
                    conversation_labels
                WHERE
                    local_conversation_id = ?
                ",
                (self.local_id,),
            )?;
        }

        // Remove any attachments that are no longer associated with this conversation.
        if !self.attachments_metadata.is_empty() {
            let local_ids = Attachment::create_or_update_from_conversation_metadata(self, tx)?;
            for &id in &local_ids {
                tx.execute(
                    "INSERT OR IGNORE INTO conversation_attachments VALUES (?,?)",
                    (self.id(), id),
                )?;
            }

            let placeholders = placeholders(&local_ids);
            let params = params_from_iter([self.local_id].bridge_sql_extend_iter(local_ids));
            tx.execute(
                &formatdoc!(
                    "
                DELETE FROM
                    conversation_attachments
                WHERE
                    local_conversation_id = ?
                    AND local_attachment_id NOT IN ({placeholders})
                ",
                ),
                params,
            )?;
        } else {
            tx.execute(
                "
                DELETE FROM
                    conversation_attachments
                WHERE
                    local_conversation_id = ?
                ",
                (self.local_id,),
            )?;
        }

        for label in &mut self.labels {
            label.local_conversation_id = self.local_id;
            label.save_sync(tx).inspect_err(|e| {
                error!(
                    "Failed to save conversation label ({}): {e}",
                    label.remote_label_id.as_deref().unwrap_or("?"),
                )
            })?;
        }

        self.snoozed_until = self
            .labels
            .iter()
            .find(|l| l.remote_label_id == Some(LabelId::snoozed()))
            .map(|l| l.context_snooze_time);

        // If exclusive location is not set, we try to calculate it now.
        if self.locations.is_empty() && !self.labels.is_empty() {
            let label_ids = self
                .labels
                .iter()
                .filter_map(|label| label.remote_label_id.clone())
                .map_into()
                .collect_vec();

            self.locations = ExclusiveLocation::from_label_ids_many_sync(&label_ids, tx)?;
        }

        Ok(())
    }

    fn after_load(&mut self, conn: &Connection) -> Result<(), StashError> {
        self.labels = ConversationLabel::find_sync(
            "WHERE local_conversation_id = ?",
            (self.local_id,),
            conn,
        )?;

        self.snoozed_until = self
            .labels
            .iter()
            .find(|l| l.remote_label_id == Some(LabelId::snoozed()))
            .map(|l| l.context_snooze_time);

        let labels = self.load_labels(conn)?;
        self.locations = ExclusiveLocation::from_labels_many(&labels);
        self.attachments_metadata =
            Attachment::load_conversation_attachment_metadata(self.id(), conn)?;
        self.custom_labels = labels
            .into_iter()
            .filter(|l| l.label_type == LabelType::Label)
            .map(CustomLabel::from)
            .collect();

        // Example... not good to do this here, though, as the total number comes
        // from the API.
        // self.num_messages = stash.query::<_, QueryResultU64>(
        //     "SELECT COUNT(*) FROM messages WHERE local_conversation_id = ?",
        //     params![self.local_id],
        // ).await?.into_iter().next().unwrap().value;

        Ok(())
    }

    fn before_save(&mut self, bond: &Transaction<'_>) -> Result<(), StashError> {
        if let Some(remote_id) = &self.remote_id
            && let Some(existing) = Self::find_by_remote_id_sync(remote_id, bond)?
        {
            self.local_id = existing.local_id;
            // We want to preserve this to prevent unnecessary resyncing of conversations
            // messages if we update something.
            self.has_messages = self.has_messages || existing.has_messages;
        }
        Ok(())
    }
}

impl From<ApiConversation> for Conversation {
    fn from(value: ApiConversation) -> Self {
        Self {
            local_id: None,
            remote_id: Some(value.id),
            attachment_info: MessageAttachmentInfos {
                value: value
                    .attachment_info
                    .into_iter()
                    .map(|(k, v)| (k, v.into()))
                    .collect(),
            },
            attachments_metadata: value
                .attachments_metadata
                .into_iter()
                .map(AttachmentMetadata::from)
                .collect(),
            deleted: false,
            display_snooze_reminder: value.display_snoozed_reminder,
            snoozed_until: None,
            expiration_time: value.expiration_time.into(),
            locations: vec![],
            labels: value.labels.map_vec(),
            num_attachments: value.num_attachments,
            num_messages: value.num_messages,
            num_unread: value.num_unread,
            display_order: value.order,
            recipients: MessageRecipients {
                value: value.recipients.map_vec(),
            },
            senders: MessageSenders {
                value: value.senders.map_vec(),
            },
            custom_labels: vec![],
            size: value.size,
            subject: value.subject,
            is_known: true,
            has_messages: false,
        }
    }
}

pub struct ConversationActionWatcher {
    sender: flume::Sender<()>,
}

impl TableObserver for ConversationActionWatcher {
    fn tables(&self) -> Vec<String> {
        vec![
            Conversation::table_name().to_string(),
            ConversationLabel::table_name().to_string(),
        ]
    }

    fn on_tables_changed(&self, _changed_tables: &BTreeSet<String>) {
        self.sender
            .send(())
            .inspect_err(|e| {
                tracing::error!(
                    "Failed to send notification for ConversationWatcher: {:?}",
                    e
                )
            })
            .ok();
    }
}

/// Contextual label metadata associated with a Conversation.
///
/// When a conversation is opened in the context of label, the
/// [`ConversationLabel`] information is superimposed over the [`Conversation`]
/// for that context.
///
#[derive(Clone, Model, Derivative)]
#[derivative(Debug, Eq, PartialEq, Ord, PartialOrd)]
#[TableName("conversation_labels")]
#[ModelHooks]
pub struct ConversationLabel {
    // NOTE: This id is essentially useless. Stash does not support composite primary keys
    // so we do not assign it a special value. The real primary key is
    // (local_conversation_id + local_label_id).
    #[IdField(autoincrement)]
    #[derivative(
        Debug = "ignore",
        PartialEq = "ignore",
        Hash = "ignore",
        PartialOrd = "ignore",
        Ord = "ignore"
    )]
    pub local_id: Option<u64>,

    #[DbField]
    pub local_conversation_id: Option<LocalConversationId>,

    #[DbField]
    pub local_label_id: Option<LocalLabelId>,

    #[DbField]
    pub remote_label_id: Option<LabelId>,

    #[DbField]
    pub context_expiration_time: ContextExpirationTime,

    #[DbField]
    pub context_num_attachments: u64,

    #[DbField]
    pub context_num_messages: u64,

    #[DbField]
    pub context_num_unread: u64,

    #[DbField]
    pub context_size: u64,

    #[DbField]
    pub context_snooze_time: UnixTimestamp,

    #[DbField]
    pub context_time: UnixTimestamp,

    #[DbField]
    pub deleted: bool,
}

#[derive(Debug, Copy, Clone, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct ContextExpirationTime(UnixTimestamp);

impl Default for ContextExpirationTime {
    fn default() -> Self {
        Self(UnixTimestamp::new(0))
    }
}

impl ContextExpirationTime {
    pub fn merge(&mut self, other: UnixTimestamp) {
        self.0 = if self.0.as_u64() == 0 {
            other
        } else {
            self.0.min(other)
        }
    }

    pub fn merge_self(&mut self, other: Self) {
        self.0 = if self.0.as_u64() == 0 {
            other.0
        } else {
            self.0.min(other.0)
        }
    }
}

impl Deref for ContextExpirationTime {
    type Target = UnixTimestamp;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for ContextExpirationTime {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
impl stash::exports::ToSql for ContextExpirationTime {
    fn to_sql(&self) -> Result<stash::exports::ToSqlOutput<'_>, stash::exports::SqliteError> {
        self.0.to_sql()
    }
}

impl stash::exports::FromSql for ContextExpirationTime {
    fn column_result(value: stash::exports::ValueRef<'_>) -> stash::exports::FromSqlResult<Self> {
        u64::column_result(value).map(|v| Self(UnixTimestamp::new(v)))
    }
}

impl From<UnixTimestamp> for ContextExpirationTime {
    fn from(timestamp: UnixTimestamp) -> Self {
        Self(timestamp)
    }
}

impl From<ContextExpirationTime> for UnixTimestamp {
    fn from(timestamp: ContextExpirationTime) -> Self {
        timestamp.0
    }
}

impl From<u64> for ContextExpirationTime {
    fn from(timestamp: u64) -> Self {
        Self(UnixTimestamp::new(timestamp))
    }
}

impl ModelHooks for ConversationLabel {
    fn before_save(&mut self, tx: &Transaction<'_>) -> stash::stash::StashResult<()> {
        let local_conversation_id = self
            .local_conversation_id
            .context("Missing local conversation id")?;

        let remote_label_id = self
            .remote_label_id
            .as_ref()
            .context("Missing remote label id")?;

        let local_label = Label::find_by_remote_id_sync(remote_label_id, tx)?
            .with_context(|| format!("Can't find label with the remote id {remote_label_id}"))?;

        self.local_label_id = local_label.local_id;

        if let Some(label) = ConversationLabel::find_first_sync(
            "WHERE local_label_id=? AND local_conversation_id=?",
            (local_label.id(), local_conversation_id),
            tx,
        )? {
            self.local_id = label.local_id;
        }

        Ok(())
    }
}

#[cfg(feature = "test-utils")]
impl ConversationLabel {
    pub fn test_default() -> Self {
        Self {
            local_id: None,
            local_conversation_id: None,
            local_label_id: None,
            remote_label_id: None,
            context_expiration_time: UnixTimestamp::new(0).into(),
            context_num_attachments: 0,
            context_num_messages: 0,
            context_num_unread: 0,
            context_size: 0,
            context_snooze_time: UnixTimestamp::new(0),
            context_time: UnixTimestamp::new(0),
            deleted: false,
        }
    }
}

impl ConversationLabel {
    /// Get all local label ids for a given `conversation_id`.
    ///
    /// # Errors
    ///
    /// Returns error if the query failed.
    pub async fn labels_ids_for_conversation(
        conversation_id: LocalConversationId,
        tether: &Tether,
    ) -> Result<Vec<LocalLabelId>, StashError> {
        let query = format!(
            "SELECT local_label_id FROM {} WHERE local_conversation_id = ?",
            Self::table_name()
        );

        tether
            .query_values::<_, LocalLabelId>(&query, params![conversation_id])
            .await
    }

    /// Get all local label with given label IDs.
    ///
    /// # Errors
    ///
    /// Returns an error if the query failed.
    ///
    pub async fn find_by_label_ids(
        label_ids: impl IntoIterator<Item = LocalLabelId>,
        tether: &Tether,
    ) -> Result<Vec<Self>, StashError> {
        ConversationLabel::find(
            format!(
                "WHERE local_label_id IN ({})",
                label_ids.into_iter().join(", ")
            ),
            vec![],
            tether,
        )
        .await
    }

    pub async fn find_by_conversation_and_label_id(
        conversation_id: LocalConversationId,
        label_id: LocalLabelId,
        bond: &Bond<'_>,
    ) -> Result<Option<Self>, StashError> {
        Self::find_first(
            "WHERE local_conversation_id = ? AND local_label_id = ?",
            params![conversation_id, label_id],
            bond,
        )
        .await
    }

    /// Adjust the stats of the conversation label when
    /// a message is marked as deleted.
    ///
    /// ## Errors
    ///
    /// Returns error if the query fails.
    ///
    pub async fn mark_delete_update_stats(
        &mut self,
        stats: Option<&MessageLabelStats>,
        bond: &Bond<'_>,
    ) -> Result<(), AppError> {
        if let Some(stats) = stats {
            let mut conv_counter =
                ConversationCounters::find_by_id(self.local_label_id.unwrap(), bond)
                    .await?
                    .ok_or_else(|| AppError::LabelNotFound(self.local_label_id.unwrap()))?;
            let existing_message_count = self.context_num_messages;
            let existing_unread_count = self.context_num_unread;

            self.context_num_messages = self.context_num_messages.saturating_sub(stats.count);
            self.context_num_unread = self.context_num_unread.saturating_sub(stats.unread_count);
            self.context_num_attachments = self
                .context_num_attachments
                .saturating_sub(stats.attachment_count);
            self.context_size = self.context_size.saturating_sub(stats.size);
            self.deleted = self.context_num_messages == 0;
            self.save(bond).await?;

            let mut counter_updated = false;
            if existing_message_count != 0 && self.context_num_messages == 0 {
                conv_counter.total = conv_counter.total.saturating_sub(1);
                counter_updated = true;
            }

            if existing_unread_count != 0 && self.context_num_unread == 0 {
                conv_counter.unread = conv_counter.unread.saturating_sub(1);
                counter_updated = true;
            }

            if counter_updated {
                conv_counter.save(bond).await?;
            }
        }

        Ok(())
    }

    /// Adjust the stats of the conversation label when
    /// a message is marked as undeleted.
    ///
    /// ## Errors
    ///
    /// Returns error if the query fails.
    ///
    pub async fn mark_undelete_update_stats(
        &mut self,
        stats: Option<&MessageLabelStats>,
        bond: &Bond<'_>,
    ) -> Result<(), AppError> {
        if let Some(stats) = stats {
            let mut conv_counter =
                ConversationCounters::find_by_id(self.local_label_id.unwrap(), bond)
                    .await?
                    .ok_or_else(|| AppError::LabelNotFound(self.local_label_id.unwrap()))?;
            let existing_message_count = self.context_num_messages;
            let existing_unread_count = self.context_num_unread;

            self.context_num_messages += stats.count;
            self.context_num_unread += stats.unread_count;
            self.context_num_attachments += stats.attachment_count;
            self.context_size += stats.size;
            self.deleted = self.context_num_messages == 0;
            self.save(bond).await?;

            let mut counter_updated = false;
            if existing_message_count == 0 && self.context_num_messages != 0 {
                conv_counter.total = conv_counter.total.saturating_add(1);
                counter_updated = true;
            }

            if existing_unread_count == 0 && self.context_num_unread != 0 {
                conv_counter.unread = conv_counter.unread.saturating_add(1);
                counter_updated = true;
            }

            if counter_updated {
                conv_counter.save(bond).await?;
            }
        }

        Ok(())
    }

    pub(crate) async fn find_by_conversation_and_label(
        conversation_id: LocalConversationId,
        label_id: LocalLabelId,
        tether: &Tether,
    ) -> Result<Option<Self>, StashError> {
        tether
            .sync_query(move |conn| {
                Self::find_by_conversation_and_label_sync(conversation_id, label_id, conn)
            })
            .await
    }

    pub(crate) fn find_by_conversation_and_label_sync(
        conversation_id: LocalConversationId,
        label_id: LocalLabelId,
        conn: &Connection,
    ) -> Result<Option<Self>, StashError> {
        Self::find_first_sync(
            "WHERE local_conversation_id = ? AND local_label_id = ?",
            (conversation_id, label_id),
            conn,
        )
    }

    pub(crate) async fn find_by_conversations_and_labels(
        messages: &[LocalConversationId],
        labels: &[LocalLabelId],
        tether: &Tether,
    ) -> Result<Vec<Self>, StashError> {
        Self::find(
            formatdoc! { "
                WHERE local_conversation_id IN ({})
                AND local_label_id IN ({})",
                placeholders(messages),
                placeholders(labels)
            },
            messages.to_sql_extend(labels),
            tether,
        )
        .await
    }

    pub fn to_api_conversation_label(&self) -> Option<ApiConversationLabel> {
        self.remote_label_id.clone().map(|id| ApiConversationLabel {
            id,
            context_expiration_time: self.context_expiration_time.as_u64(),
            context_num_attachments: self.context_num_attachments,
            context_num_messages: self.context_num_messages,
            context_num_unread: self.context_num_unread,
            context_size: self.context_size,
            context_snooze_time: self.context_snooze_time.as_u64(),
            context_time: self.context_time.as_u64(),
        })
    }

    pub(crate) fn are_stats_equal(&self, other: &Self) -> bool {
        self.context_time == other.context_time
            && self.context_expiration_time == other.context_expiration_time
            && self.context_num_attachments == other.context_num_attachments
            && self.context_num_messages == other.context_num_messages
            && self.context_num_unread == other.context_num_unread
            && self.context_size == other.context_size
            && self.context_snooze_time == other.context_snooze_time
    }
}

impl AddAssign<ConversationMessageLabelStats> for ConversationLabel {
    fn add_assign(&mut self, rhs: ConversationMessageLabelStats) {
        self.context_size += rhs.size;
        self.context_time = self.context_time.max(rhs.time);
        self.context_expiration_time.merge_self(rhs.expiration_time);
        self.context_num_messages += rhs.count;
        self.context_num_unread += rhs.unread;
        self.context_num_attachments += rhs.num_attachments as u64;
        self.context_snooze_time = self.context_snooze_time.max(rhs.snooze_time);
    }
}

impl From<ConversationMessageLabelStats> for ConversationLabel {
    fn from(value: ConversationMessageLabelStats) -> Self {
        Self {
            local_id: None,
            local_conversation_id: None,
            local_label_id: None,
            remote_label_id: None,
            context_expiration_time: value.expiration_time,
            context_num_attachments: value.num_attachments as u64,
            context_num_messages: value.count,
            context_num_unread: value.unread,
            context_size: value.size,
            context_snooze_time: value.snooze_time,
            context_time: value.time,
            deleted: false,
        }
    }
}

impl From<ApiConversationLabel> for ConversationLabel {
    fn from(value: ApiConversationLabel) -> Self {
        Self {
            local_id: None,
            local_conversation_id: None,
            local_label_id: None,
            remote_label_id: Some(value.id),
            context_expiration_time: value.context_expiration_time.into(),
            context_num_attachments: value.context_num_attachments,
            context_num_messages: value.context_num_messages,
            context_num_unread: value.context_num_unread,
            context_size: value.context_size,
            context_snooze_time: value.context_snooze_time.into(),
            context_time: value.context_time.into(),
            deleted: false,
        }
    }
}

/// Calculates the combined information for a list of message that belong to a given
/// conversation and a given label.
#[derive(Clone, Default)]
pub struct ConversationMessageLabelStats {
    pub size: u64,
    pub time: UnixTimestamp,
    pub expiration_time: ContextExpirationTime,
    // How many messages exist
    pub count: u64,
    pub unread: u64,
    pub num_attachments: u32,
    pub snooze_time: UnixTimestamp,
}

impl ConversationMessageLabelStats {
    /// Get stats about for a conversation with `conversation_id` with the
    /// given `message_ids` for a label with `label_id`.
    fn with(
        conversation_id: LocalConversationId,
        label_id: LocalLabelId,
        message_ids: &[LocalMessageId],
        tether: &Connection,
    ) -> Result<Self, StashError> {
        let params = (label_id, conversation_id).to_sql_extend(message_ids);
        let query = formatdoc! {"
                JOIN message_labels AS ML ON
                    ML.local_message_id = messages.local_id AND
                    ML.local_label_id = ?
                WHERE
                    messages.local_conversation_id = ? AND
                    messages.local_id IN ({})
            ",
            placeholders(message_ids)
        };

        let messages = Message::find_sync(query, params_from_iter(params), tether)?;

        if messages.is_empty() {
            return Err(StashError::QueryReturnedNoRows);
        }

        Ok(Self::from_messages(&messages))
    }

    pub fn from_messages(messages: &[Message]) -> Self {
        assert_ne!(messages.len(), 0);
        let mut stats = Self {
            time: 0.into(),
            expiration_time: 0.into(),
            snooze_time: 0.into(),
            ..Default::default()
        };

        for message in messages {
            stats.size += message.size;
            stats.time = stats.time.max(message.time);
            if message.expiration_time.as_u64() > 0 && message.expiration_time != message.time {
                stats.expiration_time.merge(message.expiration_time);
            }
            stats.count += 1;
            if message.unread {
                stats.unread += 1
            }
            stats.num_attachments += message.num_attachments;
            stats.snooze_time = stats.snooze_time.max(message.snooze_time);
        }

        stats
    }
}

impl From<&ApiMessageMetadata> for ConversationMessageLabelStats {
    fn from(metadata: &ApiMessageMetadata) -> Self {
        Self {
            size: metadata.size,
            time: metadata.time.into(),
            expiration_time: metadata.expiration_time.into(),
            count: 1,
            unread: if metadata.unread { 1 } else { 0 },
            num_attachments: metadata.num_attachments,
            snooze_time: metadata.snooze_time.into(),
        }
    }
}

impl AddAssign<&ApiMessageMetadata> for ConversationMessageLabelStats {
    fn add_assign(&mut self, metadata: &ApiMessageMetadata) {
        self.size += metadata.size;
        self.time = self.time.max(metadata.time.into());
        self.expiration_time = self.expiration_time.max(metadata.expiration_time.into());
        self.count += 1;
        if metadata.unread {
            self.unread += 1;
        }
        self.num_attachments += metadata.num_attachments;
        self.snooze_time = self.snooze_time.max(metadata.snooze_time.into());
    }
}

/// Conversation counters that are related to particular label
/// Allow the user to see how many conversations there are assigned to the label,
/// both unread count and total count.
#[derive(Clone, Debug, Eq, Model, PartialEq)]
#[TableName("conversation_counters")]
pub struct ConversationCounters {
    #[IdField]
    pub local_label_id: LocalLabelId,

    #[DbField]
    pub total: u64,

    #[DbField]
    pub unread: u64,
}

impl ConversationCounters {
    /// Constructor - note: [`ConversationCounters`] does not implement [`Default`] trait
    ///
    pub fn new(local_label_id: LocalLabelId) -> Self {
        Self {
            local_label_id,
            total: Default::default(),
            unread: Default::default(),
        }
    }

    /// Get all conversation counters linked to labels with given kind
    ///
    /// # Errors
    ///
    /// Returns an error if the data could not be read from the database.
    pub async fn find_by_kind(kind: LabelType, tether: &Tether) -> Result<Vec<Self>, StashError> {
        Self::find(
            "INNER JOIN labels ON labels.local_id = local_label_id WHERE label_type = ? ORDER BY labels.display_order ASC",
            params![kind],
            tether,
        ).await
    }

    /// Returns counters, first unread then total
    pub fn counters(&self) -> (u64, u64) {
        (self.unread, self.total)
    }

    /// Returns number of conversations based on the filter.
    /// Can be either:
    /// * Total number
    /// * Unread number
    /// * Read number
    pub fn total(&self, unread: ReadFilter) -> u64 {
        match unread {
            ReadFilter::All => self.total,
            ReadFilter::Unread => self.unread,
            ReadFilter::Read => self.total.saturating_sub(self.unread),
        }
    }

    /// Returns [`ConversationCounts`] datastructure that contains label's Remote ID
    /// instead of the Local ID.
    pub async fn conversation_count(
        &self,
        tether: &Tether,
    ) -> Result<ConversationLabelsCount, AppError> {
        let remote_id = Label::resolve_remote_label_id(self.local_label_id, tether).await?;

        Ok(ConversationLabelsCount {
            label_id: remote_id,
            total: self.total,
            unread: self.unread,
        })
    }

    /// Watch conversation counter for changes.
    ///
    /// When a change occurs a message is produced in the returned receiver.
    ///
    /// # Errors
    ///
    /// Returns error if the query failed
    ///
    pub async fn watch(stash: &Stash) -> Result<WatcherHandle, StashError> {
        stash
            .subscribe_to(|sender| Box::new(ConversationCounterWatcher { sender }))
            .await
    }
}

/// Used to initialize counters by syncing it with the Backend
pub struct StoreLabelCounters(Vec<ConversationLabelsCount>, Vec<MessageLabelsCount>);

impl StoreLabelCounters {
    pub const INIT_KEY: InitializationKey = InitializationKey::new("label_counters");

    /// It initializes counters by syncing with the Backend.
    /// In case of successful initialization, it marks it in the [`InitializedComponents`].
    ///
    /// This function is idempotent. If successfully initialized in the past.
    ///
    pub async fn initialize(
        watcher: Arc<InitializationWatcher>,
        api: &impl ProtonMail,
        stash: &Stash,
    ) -> Result<(), InitializationError<AppError>> {
        InitializedComponent::initialize(
            watcher,
            Self::INIT_KEY,
            &[Label::INIT_KEY],
            stash.connection().await?,
            async || Ok(Self::fetch(api).await?),
            |tx, this| Ok(this.store(tx)?),
        )
        .await
    }

    pub async fn fetch(api: &impl ProtonMail) -> Result<Self, ApiServiceError> {
        let (a, b) =
            future::try_join(Conversation::fetch_counts(api), Message::fetch_counts(api)).await?;
        Ok(Self(a, b))
    }

    pub fn store(self, tx: &Transaction<'_>) -> Result<(), StashError> {
        let Self(convs_count, msgs_count) = self;
        ConversationLabelsCount::create_or_update_conversation_counts_sync(convs_count, tx)?;
        MessageLabelsCount::create_or_update_message_counts_sync(msgs_count, tx)?;
        Ok(())
    }
}

pub struct ConversationCounterWatcher {
    sender: flume::Sender<()>,
}

impl TableObserver for ConversationCounterWatcher {
    fn tables(&self) -> Vec<String> {
        vec![ConversationCounters::table_name().to_string()]
    }

    fn on_tables_changed(&self, _tables: &BTreeSet<String>) {
        self.sender
            .send(())
            .inspect_err(|e| {
                tracing::error!("Failed to send notification for ConversationCounterWatcher: {e:?}")
            })
            .ok();
    }
}

#[cfg(any(test, feature = "test-utils"))]
impl Conversation {
    /// Synchronize the first `count` conversations of the label with `label_id`.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed or the data could not be
    /// written to the database.
    ///
    pub async fn sync_first_conversation_page<PM: ProtonMail>(
        label_id: LabelId,
        count: usize,
        api: &PM,
        tether: &mut Tether,
    ) -> Result<(), AppError> {
        use crate::datatypes::labels::ScrollOrderField;
        use proton_mail_api::MAX_PAGE_ELEMENT_COUNT;

        let order_field = ScrollOrderField::for_label(&label_id);

        let response = api
            .get_conversations(GetConversationsOptions {
                label_id: Some(label_id),
                page: 0,
                page_size: count.min(MAX_PAGE_ELEMENT_COUNT) as u64,
                desc: Some(true),
                sort: order_field.as_api_sort(),
                ..Default::default()
            })
            .await?;

        debug!(
            "Fetched {} conversations TOTAL={}",
            response.conversations.len(),
            response.total
        );
        tether
            .tx(async |tx| {
                Self::create_or_update_conversations(
                    response
                        .conversations
                        .into_iter()
                        .map(Conversation::from)
                        .collect(),
                    tx,
                )
                .await
            })
            .await?;
        Ok(())
    }

    /// Search for conversations.
    ///
    /// This function accepts search options and calls the API to find any
    /// conversations that fit the criteria. It operates globally and is not
    /// based on a particular mailbox; this restriction can be applied via the
    /// options.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed or the data could not be
    /// written to the database. Can also return an error if a found
    /// conversation cannot be loaded, although this would indicate a
    /// significant problem.
    ///
    pub async fn search(
        options: GetConversationsOptions,
        api: &Session,
        tether: &mut Tether,
    ) -> Result<Vec<Conversation>, MailContextError> {
        // Fetch all the conversations from the API
        let conversations = api.get_conversations(options).await?.conversations;

        Self::sync_dependencies(&conversations, api, tether).await?;

        let mut conversations = conversations
            .into_iter()
            .map(Conversation::from)
            .collect_vec();
        tether
            .tx(async |tx| Self::create_or_update_conversations(conversations.clone(), tx).await)
            .await?;
        conversations.sort_unstable_by(|x, y| x.display_order.cmp(&y.display_order).reverse());

        Ok(conversations)
    }

    /// Given a list of conversations check if there are any missing dependencies like undownloaded
    /// labels.
    ///
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed or the data could not be
    /// written to the database.
    ///
    async fn sync_dependencies(
        conversations: &[ApiConversation],
        api: &Session,
        tether: &mut Tether,
    ) -> Result<(), MailContextError> {
        use crate::datatypes::dependencies::MessageOrConversationDependencyFetcher;

        let mut fetcher = MessageOrConversationDependencyFetcher::new();

        for conversation in conversations {
            fetcher.check_api_conversation(conversation, tether).await?
        }

        fetcher.fetch_and_store(api, tether).await?;

        Ok(())
    }
}
