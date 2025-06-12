#[cfg(test)]
#[path = "../tests/models/messages.rs"]
mod messages;

use crate::actions::messages::delete::Delete;
use crate::actions::messages::delete_all::DeleteAllMessagesInLabel;
use crate::actions::messages::ham::Ham;
use crate::actions::messages::label::Label as ActionLabel;
use crate::actions::messages::label_as::LabelAs;
use crate::actions::messages::r#move::Move;
use crate::actions::messages::phishing::ReportPhishing;
use crate::actions::messages::read::Read;
use crate::actions::messages::unlabel::Unlabel;
use crate::actions::messages::unread::Unread;
use crate::actions::{
    AllBottomBarMessageActions, BottomBarActions, GeneralActions, MailActionError,
    MovableSystemFolderAction, filter_responses,
};
use crate::models::*;
use crate::{MailContextError, find_in_query};
use futures::try_join;
use indoc::{formatdoc, indoc};
use proton_action_queue::action::MetadataBuilder;
use proton_action_queue::queue::{ActionError as QueueActionError, Queue, QueuedActionOutput};
use proton_calendar_common::{RsvpError, RsvpEvent, RsvpEventId};
use proton_core_common::utils::MapVec as _;
use sqlite_watcher::watcher::TableObserver;
use stash::utils::{MapToSql, placeholders};
use std::collections::HashSet;
use tokio::fs;

use crate::MailContextResult;
use crate::actions::{
    LabelAsAction, MessageAction, MessageAvailableActions, MoveAction, MoveItemAction, ReplyAction,
};
use crate::datatypes::{
    AttachmentMetadata, CustomLabel, Disposition, EncryptedMessageBody, ExclusiveLocation,
    LocalMessageId, MessageFlags, MessageLabelsCount, MessageRecipients, MessageReplyTos,
    MessageSender, MimeType, MobileActions, ParsedHeaders, ReadFilter, SystemLabelId,
    theme::MailTheme,
};
use crate::decrypted_message::ThemeOpts;
use crate::mailbox::decrypted_message::DecryptedMessageBody;
use crate::{AppError, MailUserContext};
use anyhow::{Context, anyhow};
use itertools::Itertools;
use proton_core_api::service::ApiServiceError;
use proton_core_api::services::proton::{AddressId, LabelId};
use proton_core_api::services::proton::{Proton, ProtonCore};
use proton_core_api::session::{CoreSession, Session};
use proton_core_common::datatypes::{
    LabelType, LocalAddressId, LocalLabelId, SystemLabel, UnixTimestamp,
};
use proton_core_common::models::{Address, Label, LabelError, ModelExtension, ModelIdExtension};
use proton_crypto_inbox::proton_crypto;
use proton_mail_api::MAX_PAGE_ELEMENT_COUNT;
use proton_mail_api::services::proton::ProtonMail;
use proton_mail_api::services::proton::common::{ConversationId, ExternalId, MessageId};
use proton_mail_api::services::proton::requests::GetMessagesOptions;
use proton_mail_api::services::proton::response_data::{
    Message as ApiMessage, MessageBody as ApiMessageBody, MessageMetadata as ApiMessageMetadata,
    MessageMetadata, OperationResult,
};
use proton_mail_api::services::proton::responses::GetMessagesResponse;
use proton_mail_ids::{LocalAttachmentId, LocalConversationId};
use stash::exports::ToSql;
use stash::macros::Model;
use stash::orm::Model;
use stash::params;
use stash::stash::{Bond, RunTransaction, Stash, StashError, Tether, WatcherHandle};
use std::collections::hash_map::Entry as HmEntry;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::future::Future;
use tracing::{debug, error, info, trace, warn};

#[derive(Clone, Debug, Eq, Model, PartialEq)]
#[TableName("messages")]
#[ModelActions(on_load, on_save)]
pub struct Message {
    #[IdField(autoincrement)]
    pub local_id: Option<LocalMessageId>,

    #[DbField]
    pub remote_id: Option<MessageId>,

    #[DbField]
    pub local_conversation_id: Option<LocalConversationId>,

    #[DbField]
    pub remote_conversation_id: Option<ConversationId>,

    #[DbField]
    pub local_address_id: LocalAddressId,

    #[DbField]
    pub remote_address_id: AddressId,

    pub attachments_metadata: Vec<AttachmentMetadata>,

    #[DbField]
    pub cc_list: MessageRecipients,

    #[DbField]
    pub bcc_list: MessageRecipients,

    #[DbField]
    pub deleted: bool,

    /// Exclusive location of the [`Message`] (e.g. Inbox, Archive, Outbox
    /// etc.). This field is auto-calculated, and not stored in the database.
    /// When the model is read from database, this field should be calculated,
    /// and always be [`Some`]. If it is [`None`], it means either that the
    /// model is not fully initialized or there is very nasty bug. Failed
    /// initialization is logged as an error, but flow is not impacted due to
    /// the fact that this is not a critical field.
    pub exclusive_location: Option<ExclusiveLocation>,

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
    pub reply_tos: MessageReplyTos,

    #[DbField]
    pub sender: MessageSender,

    #[DbField]
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

    #[RowIdField]
    pub row_id: Option<u64>,
}

impl ModelIdExtension for Message {
    type RemoteId = MessageId;

    fn remote_id(&self) -> Option<&Self::RemoteId> {
        self.remote_id.as_ref()
    }
}

impl Message {
    /// Label multiple messages.
    ///
    /// # Parameters
    ///
    /// * `queue`       - The action queue.
    /// * `label_id`    - The ID of the label to apply to the messages.
    /// * `message_ids` - The IDs of the messages to label.
    ///
    /// # Errors
    ///
    /// Returns an error if the action failed.
    ///
    pub async fn action_apply_label(
        queue: &Queue,
        label_id: LocalLabelId,
        message_ids: Vec<LocalMessageId>,
    ) -> Result<QueuedActionOutput<ActionLabel>, QueueActionError<ActionLabel>> {
        let action = ActionLabel::new(label_id, message_ids);
        queue.queue_action(action).await
    }

    /// Star multiple messages.
    ///
    /// # Parameters
    ///
    /// * `session`     - The session.
    /// * `queue`       - The action queue.
    /// * `message_ids` - The IDs of the messages to star.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed.
    ///
    pub async fn action_star(
        queue: &Queue,
        message_ids: Vec<LocalMessageId>,
    ) -> Result<QueuedActionOutput<ActionLabel>, QueueActionError<ActionLabel>> {
        let tether = queue.stash().connection();
        let label_id = Label::remote_id_counterpart(LabelId::starred(), &tether)
            .await
            .map_err(|e| QueueActionError::Queue(e.into()))?
            .expect("Star system label not found");
        let action = ActionLabel::new(label_id, message_ids);
        queue.queue_action(action).await
    }

    /// Unstar multiple messages.
    ///
    /// # Parameters
    ///
    /// * `queue`       - The action queue.
    /// * `message_ids` - The IDs of the messages to unstar.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed.
    ///
    pub async fn action_unstar(
        queue: &Queue,
        message_ids: Vec<LocalMessageId>,
    ) -> Result<QueuedActionOutput<Unlabel>, QueueActionError<Unlabel>> {
        let tether = queue.stash().connection();
        let label_id = Label::remote_id_counterpart(LabelId::starred(), &tether)
            .await?
            .expect("Star system label not found");
        let action = Unlabel::new(label_id, message_ids);
        queue.queue_action(action).await
    }

    /// Unlabel multiple messages.
    ///
    /// # Parameters
    ///
    /// * `queue`       - The action queue.
    /// * `label_id`    - The ID of the label to apply to the messages.
    /// * `message_ids` - The IDs of the messages to unlabel.
    ///
    /// # Errors
    ///
    /// Returns an error if the action failed.
    ///
    pub async fn action_remove_label(
        queue: &Queue,
        label_id: LocalLabelId,
        message_ids: Vec<LocalMessageId>,
    ) -> Result<QueuedActionOutput<Unlabel>, QueueActionError<Unlabel>> {
        let action = Unlabel::new(label_id, message_ids);
        queue.queue_action(action).await
    }

    /// Mark multiple messages as read.
    ///
    /// # Parameters
    ///
    /// * `queue`       - The action queue.
    /// * `message_ids` - The IDs of the target messages.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed.
    ///
    pub async fn action_mark_read(
        queue: &Queue,
        message_ids: Vec<LocalMessageId>,
    ) -> Result<(), QueueActionError<Read>> {
        let action = Read::new(message_ids);
        match queue.queue_action(action).await {
            Ok(_) | Err(QueueActionError::Action(MailActionError::NoInput)) => Ok(()),
            Err(other) => Err(other),
        }
    }

    /// Mark multiple messages as unread.
    ///
    /// # Parameters
    ///
    /// * `session`     - The session.
    /// * `queue`       - The action queue.
    /// * `message_ids` - The IDs of the target messages.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed.
    ///
    pub async fn action_mark_unread(
        queue: &Queue,
        message_ids: Vec<LocalMessageId>,
    ) -> Result<(), QueueActionError<Unread>> {
        let action = Unread::new(message_ids);
        match queue.queue_action(action).await {
            Ok(_) | Err(QueueActionError::Action(MailActionError::NoInput)) => Ok(()),
            Err(other) => Err(other),
        }
    }

    /// Mark multiple messages as read.
    ///
    /// # Parameters
    ///
    /// * `queue`       - The action queue.
    /// * `label_id`    - The ID of the label to apply to the messages.
    /// * `message_ids` - The IDs of the target messages.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed.
    ///
    pub async fn action_delete(
        queue: &Queue,
        label_id: LocalLabelId,
        message_ids: Vec<LocalMessageId>,
    ) -> Result<QueuedActionOutput<Delete>, QueueActionError<Delete>> {
        let action = Delete::new(label_id, message_ids);
        queue.queue_action(action).await
    }

    /// Move multiple messages.
    ///
    /// # Parameters
    ///
    /// * `queue`          - The action queue.
    /// * `source_id`      - The ID of the label where the messages are.
    /// * `destination_id` - The ID of the label where the messages go.
    /// * `target_ids`     - The IDs of the messages to move.
    ///
    /// # Errors
    ///
    /// Returns an error if the action failed.
    ///
    pub async fn action_move(
        queue: &Queue,
        source_id: LocalLabelId,
        destination_id: LocalLabelId,
        target_ids: Vec<LocalMessageId>,
    ) -> Result<QueuedActionOutput<Move>, QueueActionError<Move>> {
        let action = Move::new(source_id, destination_id, target_ids);
        queue.queue_action(action).await
    }

    /// Mark multiple messages as read.
    ///
    /// # Parameters
    ///
    /// * `ids`    - The IDs of the messages to mark as read.
    /// * `tether` - The tether to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the data could not be written to the database.
    ///
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

    /// Mark multiple messages as ham (not spam).
    ///
    /// # Parameters
    ///
    /// * `queue`       - The action queue.
    /// * `label_id`    - The ID of the label to apply to the messages.
    /// * `message_ids` - The IDs of the target messages.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed.
    ///
    pub async fn action_ham(
        queue: &Queue,
        message_ids: Vec<LocalMessageId>,
    ) -> Result<QueuedActionOutput<Ham>, QueueActionError<Ham>> {
        let action = Ham::new(message_ids);
        queue.queue_action(action).await
    }

    /// Mark multiple messages as ham (not spam).
    ///
    /// # Parameters
    ///
    /// * `queue`       - The action queue.
    /// * `label_id`    - The ID of the label to apply to the messages.
    /// * `message_ids` - The IDs of the target messages.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed.
    ///
    pub async fn action_report_phishing(
        queue: &Queue,
        message_id: LocalMessageId,
        tether: &Tether,
    ) -> anyhow::Result<()> {
        let spam = Label::remote_id_counterpart(LabelId::spam(), tether)
            .await?
            .ok_or_else(|| LabelError::CouldNotResolveLocalLabel(LabelId::spam()))?;
        let source_label_id = Message::load(message_id, tether)
            .await?
            .context("No message")?
            .exclusive_location
            .context("No exclusive location")?
            .local_id();

        let move_action = Move::new(source_label_id, spam, [message_id]);
        let queued_move = queue.queue_action(move_action).await?;
        let meta = MetadataBuilder::new()
            .with_dependency(queued_move.id)
            .build();

        let action = ReportPhishing::new(message_id);
        queue.queue_action_with_metadata(action, meta).await?;
        Ok(())
    }

    async fn remove_all_labels_except_all_mail(
        ids: &[LocalMessageId],
        bond: &Bond<'_>,
    ) -> Result<(), StashError> {
        let label_ids: Vec<LocalLabelId> = bond
            .query_values(
                formatdoc! {"
                SELECT local_label_id AS value
                FROM message_labels
                WHERE
                    local_message_id in ({})"
                    , placeholders(ids)
                },
                ids.to_sql(),
            )
            .await?;

        let all_mail_id = Label::remote_id_counterpart(LabelId::all_mail(), bond)
            .await?
            .expect("AllMail should be set");

        // It's a good moment to apply all mail label to messages in the case that it slipped by
        if !label_ids.contains(&all_mail_id) {
            Self::apply_label(all_mail_id, ids.iter().cloned(), bond).await?;
        }

        for label_id in label_ids {
            if label_id == all_mail_id {
                continue;
            }

            Self::remove_label(label_id, ids.iter().cloned(), bond).await?;
        }

        Ok(())
    }

    /// Move messages between two labels.
    ///
    /// Note that the logic is the same as [`Conversation::move_conversations`],
    /// so any changes made here should be reflected there.
    #[tracing::instrument(level = tracing::Level::DEBUG, skip_all)]
    pub async fn move_messages(
        source_id: LocalLabelId,
        destination_id: LocalLabelId,
        message_ids: Vec<LocalMessageId>,
        bond: &Bond<'_>,
    ) -> Result<(), AppError> {
        debug_assert_ne!(source_id, destination_id);
        if message_ids.is_empty() {
            debug!("List of ids was empty");
            return Ok(());
        }
        trace!("Moving {n} messages", n = message_ids.len());

        let spam = Label::resolve_local_label_id(LabelId::spam(), bond).await?;
        let trash = Label::resolve_local_label_id(LabelId::trash(), bond).await?;

        if destination_id == trash {
            Message::mark_multiple_as_read(message_ids.iter().cloned(), bond)
                .await
                .context("Failed to mark as read when moving to trash: {e:?}")?;
        }

        let source_label = Label::load(source_id, bond).await?.context(
            "Failed to load source label. This should never happen because we have the local id.",
        )?;

        if [trash, spam].contains(&destination_id) {
            // When moving to trash or spam we delete all labels except all mail.
            trace!("Deleting all labels except AllMail");
            Self::remove_all_labels_except_all_mail(&message_ids, bond).await?;
        } else if source_label.is_movable_folder() {
            trace!("Deleting soruce label {source_id}");
            Message::remove_label(source_id, message_ids.clone(), bond)
                .await
                .context("Failed to remove source label")?;
        } else {
            warn!("Source label {source_id} is not a movable folder, not removing...")
        }

        if [trash, spam].contains(&source_id) {
            Message::apply_remote_label(
                LabelId::almost_all_mail(),
                message_ids.iter().cloned(),
                bond,
            )
            .await
            .context("Failed to add messages to almost_all_mail when moving out of spam/trash")?;
        }

        Message::apply_label(destination_id, message_ids, bond)
            .await
            .context("Failed to apply destination label")?;

        Ok(())
    }

    /// Change Labels of a list of messages and optionally archive them.
    ///
    /// Set Labels from `selected_label_ids` while unsetting all those that are not in
    /// `partially_selected_label_ids`.
    ///
    /// # Parameters
    ///
    /// * `source_label_id`              - Id of the Label containing the messages to label.
    /// * `message_ids`                  - List the ids of the messages to label.
    /// * `selected_label_ids`           - List the ids of the Labels to set.
    /// * `partially_selected_label_ids` - List the ids of the Labels to keep as is.
    /// * `must_archive`                 - If true, the given messages will me move into Archive.
    ///
    /// # Errors
    ///
    /// Returns errors if the operation failed.
    ///
    pub async fn label_as(
        source_label_id: LocalLabelId,
        message_ids: Vec<LocalMessageId>,
        selected_label_ids: &[LocalLabelId],
        partially_selected_label_ids: &[LocalLabelId],
        all_label_ids: &[LocalLabelId],
        must_archive: bool,
        bond: &Bond<'_>,
    ) -> Result<(), AppError> {
        for label_id in all_label_ids {
            if selected_label_ids.contains(label_id) {
                Self::apply_label(*label_id, message_ids.clone(), bond).await?
            } else if !partially_selected_label_ids.contains(label_id) {
                Self::remove_label(*label_id, message_ids.clone(), bond).await?
            }
            // else keep label as is
        }

        if must_archive {
            let archive_id = Label::remote_id_counterpart(LabelId::archive(), bond)
                .await?
                .expect("Archive label must have a RemoteId");
            Self::move_messages(source_label_id, archive_id, message_ids, bond).await?;
        }
        Ok(())
    }

    /// Action to change labels of a group of messages and optionally archive them.
    ///
    /// # Parameters
    ///
    /// * `session`                      - The session.
    /// * `queue`                        - The action queue.
    /// * `message_ids`                  - List the ids of the messages to label.
    /// * `selected_label_ids`           - List the ids of the Labels to set.
    /// * `partially_selected_label_ids` - List the ids of the Labels to keep as is.
    /// * `must_archive`                 - If true, the given messages will me move into Archive.
    ///
    /// # Errors
    ///
    /// Returns an error if the action can not be applied.
    ///
    pub async fn action_label_as(
        queue: &Queue,
        source_label_id: LocalLabelId,
        message_ids: Vec<LocalMessageId>,
        selected_label_ids: Vec<LocalLabelId>,
        partially_selected_label_ids: Vec<LocalLabelId>,
        must_archive: bool,
    ) -> Result<bool, AppError> {
        let action = LabelAs::new(
            source_label_id,
            message_ids,
            selected_label_ids,
            partially_selected_label_ids,
            must_archive,
        );
        let output = queue
            .queue_action(action)
            .await
            .map_err(|e| AppError::Other(anyhow!(e)))?;
        Ok(output.local)
    }

    /// Remotely apply LabelAs action for conversations
    pub(crate) async fn remote_relabel(
        session: &Session,
        added_label_ids: &HashMap<LocalMessageId, HashSet<LocalLabelId>>,
        removed_label_ids: &HashMap<LocalMessageId, HashSet<LocalLabelId>>,
        tether: &Tether,
    ) -> Result<Vec<MessageId>, AppError> {
        /// Gets a hashmap of the remote label id and the local ids.
        async fn group_ids_by_label(
            label_ids: &HashMap<LocalMessageId, HashSet<LocalLabelId>>,
            tether: &Tether,
        ) -> Result<HashMap<LabelId, HashSet<LocalMessageId>>, AppError> {
            let mut map = HashMap::new();
            for (msg_id, local_label_ids) in label_ids {
                let remote_label_ids = Label::local_ids_counterpart(
                    Vec::from_iter(local_label_ids.iter().cloned()),
                    tether,
                )
                .await?;
                for remote_label_id in remote_label_ids {
                    map.entry(remote_label_id)
                        .or_insert_with(HashSet::new)
                        .insert(*msg_id);
                }
            }
            Ok(map)
        }

        let api = session.api();

        let added_by_label = group_ids_by_label(added_label_ids, tether).await?;
        let removed_by_label = group_ids_by_label(removed_label_ids, tether).await?;

        let mut failed_ids: Vec<MessageId> = vec![];
        for (label_id, message_ids) in added_by_label {
            let message_ids =
                Message::local_ids_counterpart(Vec::from_iter(message_ids.clone()), tether).await?;
            let response = api
                .put_messages_label(
                    message_ids.iter().cloned().map_into().collect(),
                    label_id.clone(),
                    None,
                )
                .await;

            match response {
                Ok(res) => failed_ids.extend(filter_responses(res.responses)),
                Err(e) => {
                    error!("Failed to add message to added label: {e:?}");
                    failed_ids.extend(message_ids);
                }
            }
        }
        for (label_id, message_ids) in removed_by_label {
            let message_ids =
                Message::local_ids_counterpart(Vec::from_iter(message_ids.clone()), tether).await?;
            let response = api
                .put_messages_unlabel(
                    message_ids.iter().cloned().map_into().collect(),
                    label_id.clone(),
                )
                .await;

            match response {
                Ok(res) => failed_ids.extend(filter_responses(res.responses)),
                Err(e) => {
                    error!("Failed to add message to added label: {e:?}");
                    failed_ids.extend(message_ids);
                }
            }
        }
        Ok(failed_ids)
    }

    /// Find a group of Messages by their IDs.
    ///
    /// # Parameters
    ///
    /// * `message_ids` - The IDs of the messages to find.
    /// * `interface`   - The database interface.
    ///
    /// # Errors
    ///
    /// When database request fail.
    ///
    pub(crate) async fn find_by_ids(
        message_ids: impl IntoIterator<Item = LocalMessageId>,
        tether: &Tether,
    ) -> Result<Vec<Self>, StashError> {
        let (query, params) = find_in_query!("WHERE deleted = 0 AND local_id IN ({})", message_ids);
        Message::find(query, params, tether).await
    }

    /// Get the available actions from bottom bar for given messages
    ///
    /// # Parameters
    ///
    /// * `current_label_id`  - Id of the current mailbox.
    /// * `message_ids` - List of the messages IDs.
    /// * `interface`   - The database interface.
    ///
    #[tracing::instrument(level = tracing::Level::DEBUG, skip(tether))]
    pub async fn all_available_bottom_bar_actions_for_messages(
        current_label_id: LocalLabelId,
        message_ids: Vec<LocalMessageId>,
        tether: &Tether,
    ) -> Result<AllBottomBarMessageActions, AppError> {
        let messages_fut = async {
            Self::find_by_ids(message_ids.to_vec(), tether)
                .await
                .map_err(AppError::from)
        };

        let current_label_fut = async {
            Label::resolve_remote_label_id(current_label_id, tether)
                .await
                .map_err(AppError::from)
        };

        let (inbox, archive, trash, spam, bottom_bar_actions, current_label, messages) = try_join!(
            MovableSystemFolderAction::inbox(tether),
            MovableSystemFolderAction::archive(tether),
            MovableSystemFolderAction::trash(tether),
            MovableSystemFolderAction::spam(tether),
            MobileActions::bottom_bar_actions(tether),
            current_label_fut,
            messages_fut
        )?;

        let visible_bottom_bar_actions = Self::visible_bottom_bar_actions(
            &current_label,
            &messages,
            &bottom_bar_actions,
            &inbox,
            &archive,
            &trash,
            &spam,
        )?;
        let hidden_bottom_bar_actions = Self::hidden_bottom_bar_actions(
            current_label,
            &messages,
            &visible_bottom_bar_actions,
            &inbox,
            &archive,
            &trash,
            &spam,
        );

        let actions = AllBottomBarMessageActions {
            hidden_bottom_bar_actions,
            visible_bottom_bar_actions,
        };
        debug!("all available bottom bar actions for messages: {actions:?}");
        Ok(actions)
    }

    /// Get actions to display in bottom_bar when selecting messages
    fn visible_bottom_bar_actions(
        current_label: &LabelId,
        messages: &[Self],
        bottom_bar_actions: &[MobileActions],
        inbox: &MovableSystemFolderAction,
        archive: &MovableSystemFolderAction,
        trash: &MovableSystemFolderAction,
        spam: &MovableSystemFolderAction,
    ) -> Result<Vec<BottomBarActions>, AppError> {
        let any_unread = messages.iter().any(|m| m.unread);
        let all_starred = messages.iter().all(|m| m.is_starred());

        let mut result: Vec<_> = bottom_bar_actions
            .iter()
            .filter_map(|a| {
                BottomBarActions::from_mobile_actions(
                    a,
                    any_unread,
                    all_starred,
                    current_label,
                    inbox,
                    archive,
                    trash,
                    spam,
                )
            })
            .collect();
        if result.len() > 5 {
            warn!("Too many actions to put in Bottom Bar, truncating to 5: {result:?}");
            result.truncate(5);
        }
        result.push(BottomBarActions::More);
        Ok(result)
    }

    /// Get actions not displayed in bottom_bar when selecting messages
    fn hidden_bottom_bar_actions(
        current_label: LabelId,
        messages: &[Self],
        visible_actions: &[BottomBarActions],
        inbox: &MovableSystemFolderAction,
        archive: &MovableSystemFolderAction,
        trash: &MovableSystemFolderAction,
        spam: &MovableSystemFolderAction,
    ) -> Vec<BottomBarActions> {
        let any_unread = messages.iter().any(|m| m.unread);
        let any_read = messages.iter().any(|m| !m.unread);
        let any_starred = messages.iter().any(|m| m.is_starred());
        let any_unstarred = messages.iter().any(|m| !m.is_starred());

        BottomBarActions::hidden_bottom_bar_actions(
            current_label,
            any_unread,
            any_read,
            any_unstarred,
            any_starred,
            visible_actions,
            inbox,
            archive,
            trash,
            spam,
        )
    }

    /// Save a message to the database.
    ///
    /// It's imperative that you use this method over [`Model::save()`] to
    /// ensure that local ids are resolved before they can be written
    /// to the database.
    ///
    /// # Parameters
    ///
    /// * `bond` - The database transaction, used for writing changes to storage
    ///
    /// # Errors
    ///
    /// Returns an error if the local conversation id is not set or the query
    /// failed.
    ///
    pub async fn save(&mut self, bond: &Bond<'_>) -> Result<(), StashError> {
        if let Some(remote_id) = self.remote_id.clone() {
            if let Some(existing) = Self::find_by_remote_id(remote_id, bond).await? {
                self.local_id = existing.local_id;
                self.row_id = existing.row_id;
            }
        }

        self.set_coversation_before_save(bond).await?;

        <Self as Model>::save(self, bond).await
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
    /// # Parameters
    ///
    /// * `bond` - The database transaction, used for writing changes to storage
    ///
    /// # Errors
    ///
    /// Returns an error if the local conversation id is not set or the query
    /// failed.
    ///
    pub async fn create_or_get_local(&mut self, bond: &Bond<'_>) -> Result<(), StashError> {
        if let Some(remote_id) = self.remote_id.clone() {
            if let Some(existing) = Self::find_by_remote_id(remote_id, bond).await? {
                *self = existing;

                tracing::debug!(
                    remote_id = ?self.remote_id,
                    "Skipping saving message, we already have it in the local DB"
                );

                return Ok(());
            }
        }

        self.set_coversation_before_save(bond).await?;

        <Self as Model>::save(self, bond).await
    }

    /// Set convarsation ids before saving
    ///
    async fn set_coversation_before_save(&mut self, bond: &Bond<'_>) -> Result<(), StashError> {
        if self.local_conversation_id.is_none() {
            if let Some(remote_conversation_id) = self.remote_conversation_id.clone() {
                if let Some(conversation) =
                    Conversation::find_by_remote_id(remote_conversation_id.clone(), bond).await?
                {
                    self.local_conversation_id = conversation.local_id;
                } else {
                    // Create an unknown entry.
                    let mut conversation = Conversation::unknown(remote_conversation_id);
                    conversation.save(bond).await?;
                    self.local_conversation_id = conversation.local_id;
                }
            }
        }

        Ok(())
    }

    /// Given a vec of message metadatas tries to create them in the database
    ///
    /// # Parameters
    ///
    /// * `metadata`  - The message metadata returned from the API
    /// * `interface` - The database interface, i.e. [`Stash`] or [`Tether`], to
    ///   use for accessing the database.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed, or the data could not be
    /// written to the database.
    ///
    pub async fn create_or_update_messages_from_metadata_vec(
        metadata: Vec<ApiMessageMetadata>,
        bond: &Bond<'_>,
    ) -> Result<Vec<Message>, AppError> {
        let mut ids = Vec::with_capacity(metadata.len());

        for metadata in metadata {
            let mut message = Message::from_api_metadata(metadata, bond).await?;
            Self::save(&mut message, bond).await?;
            ids.push(message);
        }

        Ok(ids)
    }

    /// Given a message metadata tries to create it in the database
    ///
    /// # Parameters
    ///
    /// * `metadata`  - The message metadata returned from the API
    /// * `interface` - The database interface, i.e. [`Stash`] or [`Tether`], to
    ///   use for accessing the database.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed, or the data could not be
    /// written to the database.
    ///
    pub async fn create_or_update_messages_from_metadata(
        metadata: Vec<ApiMessageMetadata>,
        bond: &Bond<'_>,
    ) -> Result<Vec<LocalMessageId>, AppError> {
        Ok(
            Self::create_or_update_messages_from_metadata_vec(metadata, bond)
                .await?
                .into_iter()
                .filter_map(|x| x.local_id)
                .collect(),
        )
    }

    /// Delete multiple messages.
    ///
    /// # Parameters
    ///
    /// * `ids`      - The IDs of the messages to delete.
    /// * `label_id` - TODO: Document this parameter.
    /// * `api`      - The API instance to use.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed.
    ///
    pub async fn delete_multiple_remote<PM: ProtonMail>(
        ids: Vec<MessageId>,
        label_id: LabelId,
        api: &PM,
    ) -> Result<Vec<OperationResult<MessageId>>, ApiServiceError> {
        let request = |ids: Vec<MessageId>| {
            let label_id = label_id.clone();
            async {
                api.put_messages_delete(ids, Some(label_id))
                    .await
                    .map(|r| r.responses)
            }
        };
        Message::split_request(ids, request).await
    }

    /// Mark messages as deleted.
    ///
    /// This is soft delete of messages. It will assign deleted flag to true,
    /// Adjust labels, conversations and conversation labels stats.
    /// Morover if all messages within a conversation were deleted, the conversation
    /// will be deleted as well.
    ///
    /// # Parameters
    ///
    /// * `ids`       - The IDs of the conversations to delete.
    /// * `interface` - The interface to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the data could not be written to the database.
    ///
    pub async fn mark_deleted(ids: Vec<LocalMessageId>, bond: &Bond<'_>) -> Result<(), AppError> {
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

                if conversation.deleted {
                    for (label_id, stats) in all_stats.iter() {
                        conversation
                            .remove_conversation_from_label(*label_id, Some(stats), bond)
                            .await?;
                    }
                }
            }
        }

        Ok(())
    }

    /// Mark messages as undeleted.
    ///
    /// This is soft undelete of messages. It will assign deleted flag to false,
    /// Adjust labels, conversations and conversation labels stats.
    /// Morover if conversation was deleted it will be restored.
    ///
    /// # Parameters
    ///
    /// * `ids`       - The IDs of the messages to undelete.
    /// * `interface` - The interface to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the data could not be written to the database.
    ///
    pub async fn mark_undeleted(ids: Vec<LocalMessageId>, bond: &Bond<'_>) -> Result<(), AppError> {
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

    /// Get the message counts.
    ///
    /// # Parameters
    ///
    /// * `api` - The API instance to use.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed.
    ///
    pub async fn fetch_counts<PM: ProtonMail>(
        api: &PM,
    ) -> Result<Vec<MessageLabelsCount>, ApiServiceError> {
        api.get_messages_count().await.map(|r| r.counts.map_vec())
    }

    /// Get message metadata.
    ///
    /// # Parameters
    ///
    /// * `filter` - The filter to use.
    /// * `api`    - The API instance to use.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed.
    ///
    pub async fn fetch_metadata<PM: ProtonMail>(
        filter: GetMessagesOptions,
        api: &PM,
    ) -> Result<GetMessagesResponse, ApiServiceError> {
        api.get_messages(filter).await
    }

    /// Get all labels for the message.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed, or the data could not be
    /// written to the database.
    ///
    pub async fn all_message_labels(&self, tether: &Tether) -> Result<Vec<Label>, StashError> {
        let labels = Label::find(
            r#"
            WHERE local_id IN (
                SELECT local_label_id FROM message_labels WHERE local_message_id = ?
            ) ORDER BY display_order ASC
            "#,
            params![self.local_id],
            tether,
        )
        .await?;

        Ok(labels)
    }

    /// Extends [`Model::load()`] to pre-load child records.
    ///
    /// # Errors
    ///
    /// See [`Model::load()`].
    ///
    async fn on_load(&mut self, tether: &Tether) -> Result<(), StashError> {
        self.attachments_metadata =
            Attachment::load_message_attachment_metadata(self.id(), tether).await?;

        let labels = self.all_message_labels(tether).await?;

        self.exclusive_location = ExclusiveLocation::from_labels(&labels);
        self.label_ids = labels
            .iter()
            .map(|l| l.remote_id.clone().unwrap())
            .collect();

        self.custom_labels = labels
            .into_iter()
            .filter(|l| l.label_type == LabelType::Label)
            .map(CustomLabel::from)
            .collect();

        // TODO: The message body might need to be loaded in here, but it's not
        // TODO: totally clear how best to do that seeing as the cache feature
        // TODO: requires some additional parameters such as the path. So this can
        // TODO: currently be done as a subsequent manual step.

        Ok(())
    }

    /// Extends [`Model::save()`] to set the contact id for children.
    ///
    /// # Errors
    ///
    /// See [`Model::save()`].
    ///
    pub async fn on_save(&mut self, bond: &Bond<'_>) -> Result<(), StashError> {
        // Remove any labels that are no longer associated with this message.
        if !self.label_ids.is_empty() {
            #[allow(trivial_casts)]
            bond.execute(
                formatdoc!(
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
                [self.id()].to_sql_extend(&*self.label_ids),
            )
            .await?;
        } else {
            bond.execute(
                formatdoc!(
                    "
                DELETE FROM
                    message_labels
                WHERE
                    local_message_id = ?
                ",
                ),
                params![self.local_id],
            )
            .await?;
        }

        // FIXME: this is wrong because we don't update anything.
        // We should use Message::apply_label and Message::remove_label instead.
        for label_id in &mut self.label_ids {
            bond.execute(
                format!(
                    r#"
                INSERT OR IGNORE INTO
                    message_labels (local_message_id, local_label_id)
                VALUES
                    (?, (SELECT local_id FROM {} WHERE remote_id=? LIMIT 1))
                "#,
                    Label::table_name()
                ),
                params![self.local_id, label_id.clone()],
            )
            .await?;
        }

        // Remove any attachments that are no longer associated with this conversation.
        if !self.attachments_metadata.is_empty() {
            let local_ids = Attachment::create_or_update_from_message_metadata(self, bond).await?;

            for id in &local_ids {
                bond.execute(
                    "INSERT OR IGNORE INTO message_attachments VALUES (?,?)",
                    params![self.id(), *id],
                )
                .await?;
            }

            #[allow(trivial_casts)]
            bond.execute(
                formatdoc!("
                    DELETE FROM message_attachments WHERE
                            local_attachment_id IN (
                                SELECT local_id FROM attachments
                                JOIN message_attachments ON message_attachments.local_message_id = ? AND
                                    message_attachments.local_attachment_id = attachments.local_id
                                WHERE attachments.disposition = ?
                                AND attachments.local_id NOT IN ({})

                            )",
                    stash::utils::placeholders_n(local_ids.len()),
                ),
               (self.local_id, Disposition::Attachment).to_sql_extend(&*local_ids)
            )
            .await?;
        } else {
            bond.execute(
                formatdoc!("
                    DELETE FROM message_attachments WHERE
                            local_attachment_id IN (
                                SELECT local_id FROM attachments
                                JOIN message_attachments ON message_attachments.local_message_id = ? AND
                                    message_attachments.local_attachment_id = attachments.local_id
                                WHERE attachments.disposition = ?
                            )"
                ),
                params![self.local_id, Disposition::Attachment],
            )
            .await?;
        }

        // If exclusive location is not set, we try to calculate it now.
        if self.exclusive_location.is_none() && !self.label_ids.is_empty() {
            self.exclusive_location =
                ExclusiveLocation::from_label_ids(&self.label_ids, bond).await?;
        }

        Ok(())
    }

    /// TODO: Document this method.
    #[inline]
    #[must_use]
    pub fn is_starred(&self) -> bool {
        self.label_ids.iter().any(|l| *l == LabelId::starred())
    }

    /// Given a list of message metadata check if there are any missing dependencies like
    /// undownloaded labels or addresses.
    ///
    ///
    /// # Parameters
    ///
    /// * `messages`  - The messages to check.
    /// * `api`       - The API instance to use.
    /// * `stash`     - The stash to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed or the data could not be
    /// written to the database.
    ///
    async fn sync_dependencies_from_metadata(
        messages: &[MessageMetadata],
        api: &Proton,
        tether: &mut Tether,
    ) -> Result<(), AppError> {
        let mut addrs = vec![];
        // First we load the addresses because the addresses need to exist before the messages get
        // loaded.
        for msg in messages {
            if (Address::find_by_remote_id(msg.address_id.to_owned(), tether).await?).is_none() {
                debug!("Address {} not found, syncing...", msg.address_id);
                let addr = api
                    .get_address_by_id(msg.address_id.to_owned())
                    .await?
                    .address;
                addrs.push(Address::from(addr));
            }
        }

        tether
            .tx::<_, _, StashError>(async |tx| {
                for mut addr in addrs {
                    addr.save(tx).await?;
                }
                Ok(())
            })
            .await?;

        let mut missing_labels_ids = vec![];
        for msg in messages {
            for rid in &msg.label_ids {
                if (Label::find_by_remote_id(rid.clone(), tether))
                    .await?
                    .is_none()
                {
                    missing_labels_ids.push(rid.clone());
                }
            }
        }

        if !missing_labels_ids.is_empty() {
            info!(
                "{} label(s) were in a conversations but not locally, synchronizing...",
                missing_labels_ids.len()
            );
            let missing_labels = Label::get_labels_by_ids(api, missing_labels_ids).await?;
            tether
                .tx(async |tx| Label::sync_labels(tx, missing_labels).await)
                .await?;
        }

        Ok(())
    }

    /// Search for messages.
    ///
    /// This function accepts search options and calls the API to find any
    /// messages that fit the criteria. It operates globally and is not based on
    /// a particular mailbox; this restriction can be applied via the options.
    ///
    /// # Parameters
    ///
    /// * `options` - The search options to use.
    /// * `api`     - The API instance to use.
    /// * `stash`   - The stash to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed or the data could not be
    /// written to the database. Can also return an error if the found message
    /// cannot be loaded, although this would indicate a significant problem.
    ///
    pub async fn search(
        options: GetMessagesOptions,
        api: &Proton,
        tether: &mut Tether,
    ) -> Result<Vec<Message>, AppError> {
        let messages = api
            .get_messages(options)
            .await
            .context("Error fetching the messages from the API")?
            .messages
            .into_iter()
            .collect_vec();

        // First we load the addresses because the addresses need to exist before the messages get
        // loaded.
        Self::sync_dependencies_from_metadata(&messages, api, tether).await?;

        let mut messages = tether
            .tx(async |tx| Self::create_or_update_messages_from_metadata_vec(messages, tx).await)
            .await?;

        messages.sort_unstable_by(|x, y| {
            x.time
                .cmp(&y.time)
                .then(x.display_order.cmp(&y.display_order).reverse())
        });

        Ok(messages)
    }

    /// Synchronize the first `count` messages of the label with `label_id`.
    ///
    /// # Parameters
    ///
    /// * `label_id`  - The ID of the label to sync.
    /// * `count`     - TODO: Document this parameter.
    /// * `api`       - The API instance to use.
    /// * `stash`     - The stash to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed or the data could not be
    /// written to the database.
    ///
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
                Self::create_or_update_messages_from_metadata(response.messages, tx).await
            })
            .await?;
        Ok(())
    }

    /// Get the available actions for message excluding move to the current view.
    ///
    /// # Parameters
    ///
    /// * `view` - The label from which conversation is viewed.
    /// * `local_id` - The ID of the message to get the actions for.
    /// * `interface` - The interface to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns error if the database request fail.
    ///
    #[tracing::instrument(level = tracing::Level::DEBUG, skip(tether))]
    pub async fn available_actions(
        view: Label,
        message_id: LocalMessageId,
        theme: ThemeOpts,
        tether: &Tether,
    ) -> Result<MessageAvailableActions, AppError> {
        let Some(message) = Message::find_by_id(message_id, tether).await? else {
            return Err(AppError::MessageMissing(message_id));
        };

        let reply_actions = if message.reply_tos.value.len() > 1 {
            ReplyAction::all()
        } else {
            ReplyAction::single_address()
        };

        let mut message_actions = Vec::new();
        if message.unread {
            message_actions.push(MessageAction::MarkRead);
        } else {
            message_actions.push(MessageAction::MarkUnread);
        }
        if message.is_starred() {
            message_actions.push(MessageAction::Unstar);
        } else {
            message_actions.push(MessageAction::Star);
        }
        message_actions.push(MessageAction::LabelAs);

        let move_actions = MoveItemAction::from_view(view, tether).await?;

        let mut general_actions = vec![
            // Those are geneal default actions available for every message
            GeneralActions::Print,
            GeneralActions::ReportPhishing,
            GeneralActions::SaveAsPdf,
            GeneralActions::ViewHeaders,
            GeneralActions::ViewHtml,
        ];

        // In light theme we do not want to have any actions theme-related
        if theme.current_theme == MailTheme::DarkMode {
            match theme.theme() {
                MailTheme::LightMode => general_actions.push(GeneralActions::ViewMessageInDarkMode),
                MailTheme::DarkMode => general_actions.push(GeneralActions::ViewMessageInLightMode),
            }
        }

        let res = MessageAvailableActions::builder()
            .reply_actions(reply_actions)
            .message_actions(message_actions)
            .move_actions(move_actions)
            .general_actions(general_actions)
            .build();
        debug!("available actions for messages: {res:?}");
        Ok(res)
    }

    /// Get the available `label as` actions for conversations
    ///
    /// # Parameters
    ///
    /// * `local_ids` - The IDs of the conversations to get the actions for.
    /// * `interface` - The interface to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns error if the database request fail.
    ///
    #[tracing::instrument(level = tracing::Level::DEBUG, skip(tether))]
    pub async fn available_label_as_actions(
        message_ids: Vec<LocalMessageId>,
        tether: &Tether,
    ) -> Result<Vec<LabelAsAction>, AppError> {
        if message_ids.is_empty() {
            return Err(AppError::EmptyListOfMessages);
        }

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

    pub fn watch(stash: &Stash) -> Result<WatcherHandle, StashError> {
        stash.subscribe_to(|sender| Box::new(MessageWatcher { sender }))
    }

    /// Watches available `label as` actions for messages
    ///
    /// # Parameters
    ///
    /// * `local_ids` - The IDs of the conversations to get the actions for.
    /// * `interface` - The interface to use for the database connection.
    /// * `sender`    - The sender for the channel to receive updates on.
    ///
    /// # Errors
    ///
    /// Returns error if the database request fail.
    ///
    #[tracing::instrument(level = tracing::Level::DEBUG, skip(tether))]
    pub async fn watch_available_label_as_actions(
        message_ids: Vec<LocalMessageId>,
        tether: &Tether,
    ) -> Result<(Vec<LabelAsAction>, WatcherHandle), AppError> {
        if message_ids.is_empty() {
            return Err(AppError::EmptyListOfMessages);
        }

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

    /// Get the available move actions for messages.
    ///
    /// # Parameters
    ///
    /// * `view` - The label from which conversation is viewed.
    /// * `local_ids` - The IDs of the conversations to get the actions for.
    /// * `interface` - The interface to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns error if the database request fail.
    ///
    #[tracing::instrument(level = tracing::Level::DEBUG, skip(tether))]
    pub async fn available_move_to_actions(
        view: Label,
        message_ids: Vec<LocalMessageId>,
        tether: &Tether,
    ) -> Result<Vec<MoveAction>, AppError> {
        if message_ids.is_empty() {
            return Err(AppError::EmptyListOfMessages);
        }

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
    ///
    /// # Errors
    ///
    /// - if the message failed to download
    /// - if the db query failed
    /// - if the message body could not be written to the cache
    /// - if a message with the given id could not be found
    #[tracing::instrument(level=tracing::Level::DEBUG,skip(user_context))]
    pub async fn message_body(
        user_context: &MailUserContext,
        id: LocalMessageId,
    ) -> MailContextResult<DecryptedMessageBody> {
        let tether = &mut user_context.user_stash().connection();
        let saved_message = Message::load(id, tether)
            .await?
            .ok_or(AppError::MessageMissing(id))?;

        saved_message.fetch_message_body(user_context, tether).await
    }

    #[tracing::instrument(level=tracing::Level::DEBUG,skip(user_context))]
    pub async fn message_body_with_sender(
        user_context: &MailUserContext,
        id: LocalMessageId,
    ) -> MailContextResult<(String, DecryptedMessageBody)> {
        let tether = &mut user_context.user_stash().connection();
        let saved_message = Message::load(id, tether)
            .await?
            .ok_or(AppError::MessageMissing(id))?;

        let sender = saved_message.sender.address.clone();
        let body = saved_message
            .fetch_message_body(user_context, tether)
            .await?;

        Ok((sender, body))
    }

    /// Get the message's body.
    ///
    /// This will attempt to fetch the message data from the servers if it has
    /// not yet been downloaded before.
    ///
    /// # Parameters
    ///
    /// * `cache_path`   - TODO: Document this parameter.
    /// * `address_keys` - The address keys to use for decryption.
    /// * `pgp`          - The PGP provider to use for decryption.
    /// * `api`          - The API instance to use.
    /// * `interface`    - The database interface, i.e. [`Stash`] or [`Tether`],
    ///                    to use for finding the records.
    ///
    /// # Errors
    ///
    /// Returns error if the message failed to download, the db query failed or
    /// the message body could not be written to the cache.
    ///
    #[tracing::instrument(level = tracing::Level::DEBUG, skip_all)]
    pub async fn fetch_message_body(
        &self,
        ctx: &MailUserContext,
        mut tx: impl RunTransaction,
    ) -> Result<DecryptedMessageBody, MailContextError> {
        if let Some(decrypted) =
            Self::load_decrypted_message_from_cache(self.id(), tx.tether()).await?
        {
            debug!("Found message body in cache.");
            return Ok(decrypted);
        }
        trace!("Message body not in cache. Fetching...");

        let Some(remote_id) = self.remote_id.clone() else {
            return Err(AppError::MessageHasNoRemoteId(self.id()).into());
        };

        if ctx.session().status().await.is_offline() {
            debug!("No connection, skipping sync");
            return Err(MailContextError::Api(ApiServiceError::NetworkError(
                "No connection".to_owned(),
            )));
        }

        let (_, encrypted_body) =
            Self::sync_message_and_body(remote_id, ctx.api(), &mut tx).await?;
        trace!("Message successfully downloaded. Decrypting...");

        let decrypted = Self::decrypt_message_body(
            ctx,
            &self.remote_address_id,
            encrypted_body,
            tx.tether(),
            true,
        )
        .await?;
        trace!("Message successfully decrypted. Caching...");

        tx.run_tx(async |tx| {
            Self::store_decrypted_message_body(self.id(), decrypted.body.clone(), tx).await?;

            Ok(())
        })
        .await
        .map_err(MailContextError::Other)?;

        debug!("Message successfully synced.");
        Ok(decrypted)
    }

    /// Finds all messages that have expired and deletes them.
    pub async fn delete_expired(tether: &mut Tether) -> Result<(), AppError> {
        let ids = Self::find_ids(
            r"
        WHERE
          expiration_time < STRFTIME('%s', 'NOW')
          AND expiration_time != 0
        ",
            vec![],
            tether,
        )
        .await?;

        tether
            .tx(async |tx| Self::mark_deleted(ids, tx).await)
            .await?;

        Ok(())
    }

    /// Mark the messages with `ids` as read.
    ///
    /// This method also updates all the label counters and conversation labels
    /// where these messages belong to.
    ///
    /// # Errors
    ///
    /// Returns error if the queries fails.
    pub async fn mark_read(
        ids: impl IntoIterator<Item = LocalMessageId>,
        bond: &Bond<'_>,
    ) -> Result<(), StashError> {
        Self::mark_read_or_unread(true, ids, bond).await
    }

    /// Mark the messages with `ids` as unread.
    ///
    /// This method also updates all the label counters and conversation labels
    /// where these messages belong to.
    ///
    /// # Errors
    ///
    /// Returns error if the queries fails.
    pub async fn mark_unread(
        ids: impl IntoIterator<Item = LocalMessageId>,
        bond: &Bond<'_>,
    ) -> Result<(), StashError> {
        Self::mark_read_or_unread(false, ids, bond).await
    }

    async fn mark_read_or_unread(
        mark_read: bool,
        ids: impl IntoIterator<Item = LocalMessageId>,
        bond: &Bond<'_>,
    ) -> Result<(), StashError> {
        struct IdPair {
            local_message_id: LocalMessageId,
            local_conversation_id: LocalConversationId,
        }

        let ids = ids.into_iter();

        let mut updated: Vec<IdPair> = Vec::with_capacity(ids.size_hint().1.unwrap_or(0));
        let mut conversation_count_changed = HashMap::new();

        // update unread flag
        for id in ids {
            if let Some(mut message) = Message::find_first(
                "WHERE local_id=? AND unread=?",
                params![id, if mark_read { 1 } else { 0 }],
                bond,
            )
            .await?
            {
                message.unread = !mark_read;
                message.save(bond).await?;
                updated.push(IdPair {
                    local_message_id: message.id(),
                    local_conversation_id: message.local_conversation_id.unwrap(),
                });
                *conversation_count_changed
                    .entry(message.local_conversation_id.expect("Should be set"))
                    .or_insert(0) += 1;
            }
        }

        for (conversation_id, count) in conversation_count_changed {
            if let Some(mut conversation) = Conversation::find_by_id(conversation_id, bond).await? {
                if mark_read {
                    conversation.num_unread = conversation.num_unread.saturating_sub(count);
                } else {
                    conversation.num_unread += count;
                }
                conversation.save(bond).await?;
            }
        }

        if updated.is_empty() {
            // Nothing was changed.
            return Ok(());
        }

        // Publish updates for all affected ids.

        // Messages Counters
        for id_pair in &updated {
            let counters = MessageCounters::find(
                indoc! {"
                    WHERE local_label_id IN (
                        SELECT local_label_id FROM message_labels
                        WHERE local_message_id=?
                    )"},
                params![id_pair.local_message_id],
                bond,
            )
            .await?;
            for mut counter in counters {
                if mark_read {
                    counter.unread = counter.unread.saturating_sub(1);
                } else {
                    counter.unread += 1;
                }

                counter.save(bond).await?
            }
        }

        let mut label_ids = BTreeSet::new();
        // Update conversation labels
        for id_pair in &updated {
            let mut conversation_labels = ConversationLabel::find(
                indoc! {
                "WHERE local_conversation_id=? AND local_label_id IN (
                    SELECT local_label_id FROM message_labels WHERE local_message_id=?
                )"},
                params![id_pair.local_conversation_id, id_pair.local_message_id],
                bond,
            )
            .await?;
            for conversation_label in &mut conversation_labels {
                if mark_read {
                    conversation_label.context_num_unread =
                        conversation_label.context_num_unread.saturating_sub(1);

                    if conversation_label.context_num_unread == 0 {
                        label_ids.insert(conversation_label.local_label_id.unwrap());
                    }
                } else {
                    conversation_label.context_num_unread += 1;

                    if conversation_label.context_num_unread == 1 {
                        label_ids.insert(conversation_label.local_label_id.unwrap());
                    }
                }
                conversation_label.save(bond).await?
            }
        }

        for label_id in label_ids {
            // Update conversation label counts.
            if let Some(mut counters) = ConversationCounters::find_by_id(label_id, bond).await? {
                if mark_read {
                    counters.unread = counters.unread.saturating_sub(1);
                } else {
                    counters.unread += 1;
                }
                counters.save(bond).await?;
            }
        }

        Ok(())
    }

    /// Converts an [`ApiMessage`] into its components.
    ///
    /// Returns, in order:
    /// * [`Message`]: Message metadata
    /// * [`MessageBodyMetadata`]: Messge body metadata
    /// * Message body
    ///
    /// # Parameters
    ///
    /// * `value`     - The [`ApiMessage`] to convert.
    /// * `interface` - The database interface, i.e. [`Stash`] or [`Tether`], to
    ///   use for finding the records.
    ///
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

    /// Converts an [`ApiMessageMetadata`] into a [`Message`].
    ///
    /// # Parameters
    ///
    /// * `value`     - The [`ApiMessage`] to convert.
    /// * `interface` - The database interface, i.e. [`Stash`] or [`Tether`], to
    ///   use for finding the records.
    ///
    pub async fn from_api_metadata(
        value: ApiMessageMetadata,
        tether: &Tether,
    ) -> Result<Self, AppError> {
        let label_ids: Vec<LabelId> = value.label_ids.into_iter().map_into().collect();
        let exclusive_location = ExclusiveLocation::from_label_ids(&label_ids, tether).await?;

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
            exclusive_location,
            label_ids,
            num_attachments: value.num_attachments,
            reply_tos: MessageReplyTos {
                value: value.reply_tos.map_vec(),
            },
            sender: value.sender.into(),
            size: value.size,
            snooze_time: value.snooze_time.into(),
            subject: value.subject,
            time: value.time.into(),
            to_list: MessageRecipients {
                value: value.to_list.map_vec(),
            },
            unread: value.unread,
            row_id: None,
            custom_labels: vec![],
        })
    }

    pub async fn apply_remote_label(
        label_id: LabelId,
        ids: impl IntoIterator<Item = LocalMessageId>,
        bond: &Bond<'_>,
    ) -> Result<(), AppError> {
        let local_label_id = Label::resolve_local_label_id(label_id, bond).await?;

        Self::apply_label(local_label_id, ids, bond).await?;
        Ok(())
    }

    /// Apply label with `local_label_id` to the given messages with `ids`.
    ///
    /// This will also update conversation labels and label counters.
    ///
    /// # Errors
    ///
    /// Returns error if the queries fail.
    #[tracing::instrument(level = tracing::Level::DEBUG, skip(ids, bond))]
    pub async fn apply_label(
        local_label_id: LocalLabelId,
        ids: impl IntoIterator<Item = LocalMessageId>,
        bond: &Bond<'_>,
    ) -> Result<(), StashError> {
        let mut conversation_messages = BTreeMap::<LocalConversationId, Vec<LocalMessageId>>::new();

        for id in ids {
            if bond
                .query_value_opt::<LocalConversationId>(
                    indoc::indoc! {
                    "INSERT OR IGNORE INTO message_labels
                    VALUES (?,?)
                    RETURNING local_message_id AS value",
                    },
                    params![id, local_label_id],
                )
                .await?
                .is_some()
            {
                if let Some(message) = Message::find_by_id(id, bond).await? {
                    conversation_messages
                        .entry(message.local_conversation_id.unwrap())
                        .and_modify(|v| v.push(id))
                        .or_insert_with(|| vec![id]);
                }
            } else {
                warn!("{id:?} already labeled {local_label_id:?}");
            }
        }

        for (conversation_id, message_ids) in conversation_messages {
            Conversation::label_impl(local_label_id, conversation_id, &message_ids, bond).await?;
        }

        Ok(())
    }

    /// Remove label with `local_label_id` to the given messages with `ids`.
    ///
    /// This will also update conversation labels and label counters.
    ///
    /// # Errors
    ///
    /// Returns error if the queries fail.
    #[tracing::instrument(level = tracing::Level::DEBUG, skip_all)]
    pub async fn remove_label(
        local_label_id: LocalLabelId,
        ids: impl IntoIterator<Item = LocalMessageId>,
        bond: &Bond<'_>,
    ) -> Result<(), StashError> {
        let mut ids = ids.into_iter().peekable();
        if ids.peek().is_none() {
            if cfg!(debug_assertions) {
                panic!("remove_label for no messages")
            } else {
                return Ok(());
            }
        }

        // First let's unlabel all messages.

        // We need to remember how many were unread and modified so we can update
        // the unread counts and the message counter
        let mut unread_msg_count = 0_u64;
        let mut updated_count = 0_u64;

        let ids = ids.collect_vec();
        let params = ids.to_sql();
        let conversations = bond
            .query_values::<_, LocalConversationId>(
                indoc::formatdoc! {"
                    SELECT DISTINCT m.local_conversation_id AS value
                    FROM messages m
                    WHERE local_id IN ({})
                    ", placeholders(&params)},
                params,
            )
            .await?;

        for id in ids {
            // unlabel the message and return whether it was unlabeled
            if bond
                .query_value_opt::<LocalMessageId>(
                    indoc::indoc! {"
                    DELETE FROM message_labels
                    WHERE local_label_id=?
                      AND local_message_id=?
                    RETURNING local_message_id AS value
                    "},
                    params![local_label_id, id],
                )
                .await?
                .is_none()
            {
                tracing::debug!("Message {id} was not labeled with {local_label_id}");
                //  Notice that we're not updating updated_count
                continue;
            };

            let unread = bond
                .query_value::<_, u64>(
                    indoc::formatdoc! {"
                    SELECT unread as value
                    FROM messages
                    WHERE local_id = ?
                "},
                    params![id],
                )
                .await?;

            unread_msg_count += unread;
            updated_count += 1;
        }

        // Update message counters
        if updated_count == 0 {
            warn!("No updated messages?");
            return Ok(());
        }

        let mut msg_counters = MessageCounters::find_by_id(local_label_id, bond)
            .await?
            .context("No message counter for label")?;

        msg_counters.unread = msg_counters.unread.saturating_sub(unread_msg_count);
        msg_counters.total = msg_counters.total.saturating_sub(updated_count);
        msg_counters
            .save(bond)
            .await
            .context("Error saving counters")?;

        let mut conv_counters = ConversationCounters::find_by_id(local_label_id, bond)
            .await?
            .context("No conversation counter for label")?;

        for conversation_id in conversations {
            // We get the stats for the remaining messages (those that have not just been deleted)
            // and update the conversation label accordingly.
            let messages = Message::find(
                indoc! {"
                    JOIN message_labels AS ML
                    ON ML.local_message_id = messages.local_id
                       AND ML.local_label_id = ?
                    WHERE messages.local_conversation_id = ?
                "},
                params![local_label_id, conversation_id],
                bond,
            )
            .await?;

            let label_stats = if messages.is_empty() {
                // Now we can delete the conversation_label too
                None
            } else {
                Some(ConversationMessageLabelStats::from_messages(&messages))
            };

            let conversation_label = ConversationLabel::find_by_conversation_and_label(
                &conversation_id,
                local_label_id,
                bond,
            )
            .await?;

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
                    conversation_label.save(bond).await?;

                    // If it had at least 1 unread message that has been removed &&
                    // there aren't any more in the conversation we can decrease the counter
                    // because the last unread message(s) for this conversation have been removed
                    if unread_msg_count != 0 && conversation_label.context_num_unread == 0 {
                        conv_counters.unread = conv_counters.unread.saturating_sub(1);
                    }
                }
                // If no more messages remain we can unlabel the conversation
                _ => {
                    if bond
                        .query_value_opt::<u64>(
                            indoc::indoc! {"
                            DELETE FROM conversation_labels
                            WHERE local_label_id=?
                              AND local_conversation_id=?
                            RETURNING local_conversation_id AS value
                            "},
                            params![local_label_id, conversation_id],
                        )
                        .await?
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
                        tracing::warn!("Conversation {conversation_id} was not labeled");
                        continue;
                    }
                }
            }
        }

        conv_counters
            .save(bond)
            .await
            .context("Error saving counters")?;

        Ok(())
    }

    /// Retrieve all the messages which are in a given label.
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

    /// Retrieve all the message ids which are in a given label.
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
    pub async fn ids_in_label(
        local_label_id: LocalLabelId,
        tether: &Tether,
    ) -> Result<Vec<LocalMessageId>, StashError> {
        tether
            .query_values::<_, LocalMessageId>(
                indoc!(
                    "
                SELECT local_id as value
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

    /// Get all messages which belong to the conversation with
    /// `local_conversation_id`.
    ///
    /// # Params
    ///
    /// * `local_conversation_id` - Conversation id to which the messages belong
    ///   to.
    /// * `interface`             - Connection to the database.
    /// * `queue`                 - Optional subscriber for changes.
    ///
    /// # Errors
    ///
    /// Returns error if the query failed
    pub async fn in_conversation(
        local_conversation_id: LocalConversationId,
        tether: &Tether,
    ) -> Result<Vec<Self>, StashError> {
        Message::find(
            "WHERE local_conversation_id = ? AND messages.deleted = 0 ORDER BY time ASC, display_order ASC",
            params![local_conversation_id],
            tether,
        )
        .await
    }

    /// This fn should be called for message endpoints.
    /// Repeatedly calls `endpoint` in batches of 150 in parallel.
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

    /// Update message counters for `messages` after being marked as deleted.
    pub async fn update_message_counters_after_soft_delete(
        messages: impl IntoIterator<Item = Message>,
        bond: &Bond<'_>,
    ) -> Result<HashMap<LocalLabelId, MessageLabelStats>, StashError> {
        let label_stats = MessageLabelStats::build(messages, bond).await?;
        for (label_id, stats) in label_stats.iter() {
            if let Some(mut counters) = MessageCounters::find_by_id(*label_id, bond).await? {
                counters.total = counters.total.saturating_sub(stats.count);
                counters.unread = counters.unread.saturating_sub(stats.unread_count);
                counters.save(bond).await?;
            }
        }

        Ok(label_stats)
    }

    /// Update message counters for `messages` after being unmarked as deleted.
    pub async fn update_message_counters_after_soft_undelete(
        messages: impl IntoIterator<Item = Message>,
        bond: &Bond<'_>,
    ) -> Result<HashMap<LocalLabelId, MessageLabelStats>, StashError> {
        let label_stats = MessageLabelStats::build(messages, bond).await?;
        for (label_id, stats) in label_stats.iter() {
            if let Some(mut counters) = MessageCounters::find_by_id(*label_id, bond).await? {
                counters.total += stats.count;
                counters.unread += stats.unread_count;
                counters.save(bond).await?;
            }
        }

        Ok(label_stats)
    }

    /// Get the possible next display order.
    ///
    /// Finds the maximum display order value in all messages and adds 1
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
                    "SELECT IFNULL(MAX(display_order),0) AS value FROM {}",
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

    /// Sync only messages metadata
    ///
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

    /// Sync the contents of the message and the body from the server for the given `message_id`.
    ///
    /// Note that this function always overrides the data that was previously available.
    ///
    /// # Errors
    ///
    /// - if the message failed to download
    /// - if the db query failed
    /// - if the message body could not be written to the cache
    #[tracing::instrument(level=tracing::Level::DEBUG, skip(ctx))]
    pub async fn force_sync_message_and_body(
        ctx: &MailUserContext,
        message_id: MessageId,
        with_attachment_prefetch: bool,
    ) -> MailContextResult<(Message, DecryptedMessageBody)> {
        let mut tether = ctx.user_stash().connection();

        let (message, encrypted) =
            Self::sync_message_and_body(message_id, ctx.api(), &mut tether).await?;

        let decrypted = Self::decrypt_message_body(
            ctx,
            &message.remote_address_id,
            encrypted,
            &tether,
            with_attachment_prefetch,
        )
        .await?;

        tether
            .tx(async |tx| {
                Self::store_decrypted_message_body(message.id(), decrypted.body.clone(), tx).await
            })
            .await?;
        Ok((message, decrypted))
    }

    /// Sync message and body for mesasge with `message_id`.
    ///
    /// # Errors
    ///
    /// Returns error if the message failed to fetch from the server or update the
    /// metadata on the server.
    #[tracing::instrument(level=tracing::Level::DEBUG, skip(api, tx))]
    async fn sync_message_and_body(
        message_id: MessageId,
        api: &Proton,
        mut tx: impl RunTransaction,
    ) -> Result<(Message, EncryptedMessageBody), MailContextError> {
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

            Ok(())
        })
        .await
        .map_err(MailContextError::Other)?;

        Ok((
            message,
            EncryptedMessageBody {
                encrypted_body: body,
                metadata: body_metadata,
            },
        ))
    }

    /// Decrypt an `encrypted_message_body` with a given `address_id` keys.
    ///
    /// If `attachment_prefetch` is set to `true`, all the attachments will start prefetching
    /// the moment the object is created.
    ///
    /// # Errors
    ///
    /// Returns error if the decryption or loading addresses fails.
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
            .into_decrypted_message(ctx, address_keys, pgp, attachment_prefetch)
            .await
            .map_err(|e| {
                error!("Failed to decrypt message body: {e:?}");
                MailContextError::Crypto
            })
    }

    /// Load a [`DecryptedMessageBody`] for message with `local_id` from the database.
    ///
    /// # Errors
    ///
    /// Returns error if the db query or cache load fails.
    #[tracing::instrument(level=tracing::Level::DEBUG, skip(tether))]
    pub(crate) async fn load_decrypted_message_from_cache(
        local_id: LocalMessageId,
        tether: &Tether,
    ) -> Result<Option<DecryptedMessageBody>, MailContextError> {
        let Some(metadata) = MessageBodyMetadata::for_message(local_id, tether)
            .await
            .inspect_err(|e| error!("Failed to retrieve message body metadata from db: {e:?}"))?
        else {
            return Ok(None);
        };

        let Some(body) = Self::load_decrypted_message_body(local_id, tether).await? else {
            return Ok(None);
        };

        Ok(Some(DecryptedMessageBody::new_without_prefetching(
            body, metadata, None,
        )))
    }

    /// Load the decrypted message body from the cache.
    ///
    /// # Errors
    ///
    /// Returns error if the message failed to load.
    pub(crate) async fn load_decrypted_message_body(
        local_id: LocalMessageId,
        tether: &Tether,
    ) -> Result<Option<String>, StashError> {
        tether
            .query_value::<_, Option<String>>(
                indoc! {
                    "SELECT body as value FROM message_body
                        WHERE message_id = ?"
                },
                params![local_id],
            )
            .await
    }

    pub async fn store_decrypted_message_body(
        local_id: LocalMessageId,
        message: String,
        bond: &Bond<'_>,
    ) -> Result<(), StashError> {
        bond.execute(
            "INSERT OR REPLACE INTO message_body (message_id, body) VALUES (?,?)",
            params![local_id, message],
        )
        .await?;
        Ok(())
    }

    /// Whether this message is a draft.
    ///
    /// A message is considered a draft when it has the AllDrafts label assigned.
    #[must_use]
    pub fn is_draft(&self) -> bool {
        self.flags.is_draft()
            && self
                .label_ids
                .iter()
                .any(|l| *l == LabelId::all_drafts() || *l == LabelId::drafts())
    }

    /// Whether this message is a draft and has been modified locally.
    ///
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

    /// [`RemoteId`] on its own is useless, because all our UniFFI endpoints operate on
    /// local ids. This method translates remote id into local [`Id`].
    ///
    /// It may happen, that the [`RemoteId`] points to the message that does not exist in our
    /// database yet. In that case, Rust SDK will fetch necessary information from API before returning the id.
    ///
    /// # Errors
    ///
    /// Returns an error if the network failed or if the database cannot write/read message.
    ///
    pub async fn find_or_fetch_by_remote_id(
        ctx: &MailUserContext,
        remote_id: MessageId,
    ) -> MailContextResult<LocalMessageId> {
        let tether = &ctx.user_stash().connection();
        if let Some(message) = Self::find_by_remote_id(remote_id.clone(), tether).await? {
            return Ok(message.id());
        }
        let (message, _) = Self::force_sync_message_and_body(ctx, remote_id, false).await?;
        Ok(message.id())
    }

    /// Set the flags without loading the whole model
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

    /// Unset the flags without loading the whole model
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

    /// Delete all messages from a label
    ///
    /// Limited to:
    ///
    /// - drafts
    /// - spam
    /// - trash
    /// - custom labels
    /// - custom folders
    /// # Parameters
    ///
    /// * `queue`       - The action queue.
    /// * `label_id`    - The ID of the label to empty
    ///
    /// # Errors
    ///
    /// Returns an error if the action failed.
    ///
    pub async fn action_delete_all_in_label(
        queue: &Queue,
        label_id: LocalLabelId,
    ) -> Result<
        QueuedActionOutput<DeleteAllMessagesInLabel>,
        QueueActionError<DeleteAllMessagesInLabel>,
    > {
        let action = DeleteAllMessagesInLabel::new(label_id);
        queue.queue_action(action).await
    }

    #[must_use]
    pub fn is_scheduled_for_send(&self) -> bool {
        self.label_ids.contains(&LabelId::all_scheduled()) && self.flags.is_schedule_send()
    }

    #[must_use]
    pub fn is_sent(&self) -> bool {
        self.label_ids.contains(&LabelId::sent()) && self.flags.is_sent()
    }

    /// Returns id of the `invite.ics` attachment, if any.
    ///
    /// See [`Self::is_rsvp()`], [`Self::fetch_rsvp()`].
    pub fn rsvp_attachment_id(&self) -> Option<LocalAttachmentId> {
        self.attachments_metadata.iter().find_map(|att| {
            if att.filename == "invite.ics" {
                att.local_id
            } else {
                None
            }
        })
    }

    /// Returns whether this message is an RSVP invitation.
    ///
    /// Since this function doesn't parse the invitation[1], it's possible it
    /// returns a false-positive - notably, we'll return `true` for all mails
    /// that contain an attachment called `invite.ics` even if this attachment
    /// isn't really a valid invitation.
    ///
    /// This is good enough as showing potential "whoopsie, not really an rsvp"
    /// message is an UI-problem.
    ///
    /// See: [`Self::rsvp_attachment_id()`], [`Self::fetch_rsvp()`].
    ///
    /// [1] loading attachments is asynchronous, while we need for this function
    ///     to be synchronous, because we need to know rsvp-ness when displaying
    ///     an email list (i.e. no time to actually load and parse all the
    ///     attachments)
    pub fn is_rsvp(&self) -> bool {
        self.rsvp_attachment_id().is_some()
    }

    /// Checks if given attachment is an RSVP and, if so, fetches its event from
    /// the calendar and returns it.
    ///
    /// See: [`RsvpEvent::fetch()`].
    ///
    /// TODO (NGC-57) implement support for offline-mode
    #[tracing::instrument(skip(self, ctx, tether))]
    pub async fn fetch_rsvp(
        &self,
        ctx: &MailUserContext,
        rsvp: LocalAttachmentId,
        tether: &mut Tether,
    ) -> MailContextResult<Option<RsvpEvent>> {
        debug!("Fetching RSVP");

        let ics = Attachment::get_attachment(ctx, rsvp).await.map_err(|err| {
            warn!(?err, "Couldn't get the RSVP attachment");
            err
        })?;

        let ics = fs::read(&ics.data_path).await.map_err(|err| {
            warn!(?err, "Couldn't read the RSVP attachment");
            err
        })?;

        let event = match RsvpEventId::from_internal(&ics) {
            Ok(event) => event,

            Err(RsvpError::IcsIsNotRsvpRequest) => {
                return Ok(None);
            }

            Err(err) => {
                warn!(?err, "Couldn't parse the RSVP attachment");
                return Err(err.into());
            }
        };

        info!(?event, "Got RSVP, fetching state from the server");

        let pgp = proton_crypto::new_pgp_provider();

        let keys = ctx
            .unlocked_address_keys(&pgp, tether, &self.remote_address_id)
            .await
            .map_err(|err| {
                warn!(?err, "Couldn't unlock address keys");
                err
            })?;

        match event.fetch(ctx.api(), &pgp, &keys).await {
            Ok(event) => Ok(event),

            Err(err) => {
                warn!(?err, "Couldn't fetch event from the calendar");
                Err(err.into())
            }
        }
    }

    pub async fn update_ids_and_display_order(
        id: LocalMessageId,
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
                remote_conversation_id =?
            WHERE local_id = ?
        ", Message::table_name()},
            params![display_order, message_id, conversation_id, id],
        )
        .await?;
        Ok(())
    }

    pub fn can_reply(&self) -> bool {
        self.label_ids.iter().any(|label_id| {
            !(*label_id == LabelId::all_scheduled()
                || *label_id == LabelId::outbox()
                || *label_id == LabelId::drafts()
                || *label_id == LabelId::all_drafts())
        })
    }

    pub(crate) async fn can_update_from_event_loop(
        message_id: MessageId,
        message_flags: MessageFlags,
        tx: &Bond<'_>,
    ) -> Result<bool, StashError> {
        if DraftMetadata::find_by_message_with_remote_id(message_id.clone(), tx)
            .await?
            .is_some()
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
            tracing::info!(
                "Message {message_id} has draft metadata but was already sent, update will be allowed"
            );
            return Ok(message_flags.is_schedule_send() || message_flags.is_sent());
        }

        Ok(true)
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

#[derive(Debug, Clone)]
pub struct EmbeddedAttachmentInfo {
    pub data: Vec<u8>,
    pub mime: String,
    pub height: Option<String>,
    pub width: Option<String>,
}

#[derive(Clone, Debug, Eq, Model, PartialEq)]
#[TableName("message_labels")]
pub struct MessageLabel {
    #[IdField]
    pub local_label_id: LocalLabelId,

    #[DbField]
    pub local_message_id: LocalMessageId,

    #[RowIdField]
    pub row_id: Option<u64>,
}

impl Default for Message {
    fn default() -> Self {
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
            exclusive_location: Default::default(),
            num_attachments: Default::default(),
            display_order: Default::default(),
            reply_tos: Default::default(),
            sender: Default::default(),
            size: Default::default(),
            snooze_time: UnixTimestamp::new(0),
            subject: Default::default(),
            time: UnixTimestamp::new(0),
            to_list: Default::default(),
            unread: Default::default(),
            custom_labels: Default::default(),
            row_id: Default::default(),
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
#[ModelActions(on_load, on_save)]
pub struct MessageBodyMetadata {
    #[IdField(optional)]
    pub local_message_id: Option<LocalMessageId>,

    #[DbField]
    pub remote_message_id: Option<MessageId>,

    #[DbField]
    pub header: String,

    #[DbField]
    pub mime_type: MimeType,

    #[DbField]
    pub parsed_headers: ParsedHeaders,

    pub attachments: Vec<Attachment>,

    #[RowIdField]
    pub row_id: Option<u64>,
}

impl MessageBodyMetadata {
    /// Save or update the `MessageBodyMetadata` in the database.
    ///
    /// It's imperative to call this function rather than [`Model::save()`] to make sure that
    /// the `MessageBodyMetadata` and it's corresponding `Message` share the same `id`.
    ///
    /// There is currently no way to handle this in stash directly, so we have
    /// to manually perform this check.
    ///
    /// # Parameters
    ///
    /// * `interface` - The database interface, i.e. [`Stash`] or [`Tether`], to
    ///   use for finding the records.
    ///
    /// # Errors
    ///
    /// Returns an error if the query failed.
    ///
    pub async fn save(&mut self, bond: &Bond<'_>) -> Result<(), StashError> {
        if self.local_message_id.is_none() {
            if let Some(remote_id) = self.remote_message_id.clone() {
                let message =
                    Message::find_first("WHERE remote_id = ?", params![remote_id], bond).await?;
                if let Some(message) = message {
                    self.local_message_id = message.local_id;
                }

                // Need get row id or we will create new entry rather
                // than updating.
                if let Some(existing_body_metadata) = Self::find_first(
                    "WHERE local_message_id=?",
                    params![self.local_message_id],
                    bond,
                )
                .await?
                {
                    self.row_id = existing_body_metadata.row_id;
                }
            }
        }

        <Self as Model>::save(self, bond).await
    }

    /// Extends [`Model::load()`] to pre-load attachments.
    ///
    /// # Errors
    ///
    /// See [`Model::load()`].
    ///
    pub async fn on_load(&mut self, tether: &Tether) -> Result<(), StashError> {
        self.attachments = Attachment::for_message(self.local_message_id.unwrap(), tether)
            .await
            .inspect_err(|e| error!("Failed to load attachments for body metadata: {e:?}"))?;

        Ok(())
    }

    /// Extends [`Model::on_save()`] to insert attachment links.
    ///
    /// # Errors
    ///
    /// See [`Model::save()`].
    ///
    pub async fn on_save(&mut self, bond: &Bond<'_>) -> Result<(), StashError> {
        if self.local_message_id.is_none() {
            if let Some(remote_id) = self.remote_message_id.clone() {
                if let Some(existing) = Self::find_first(
                    "WHERE remote_message_id=?",
                    params![remote_id.clone()],
                    bond,
                )
                .await?
                {
                    self.local_message_id = existing.local_message_id;
                    self.row_id = existing.row_id;
                } else {
                    let Some(message) = Message::find_by_remote_id(remote_id, bond).await? else {
                        return Err(StashError::Custom(anyhow!(
                            "Failed to find message with remote id {}",
                            self.remote_message_id.as_ref().unwrap()
                        )));
                    };
                    self.local_message_id = message.local_id;
                }
            }
        }
        // Update all attachment links - When creating drafts we can update
        // and create new ones.
        bond.execute(
            "DELETE FROM message_attachments WHERE local_message_id=?",
            params![self.local_message_id],
        )
        .await?;

        for attachment in &mut self.attachments {
            attachment.save(bond).await?;
            bond
                .execute(
                    "INSERT OR IGNORE INTO message_attachments (local_attachment_id, local_message_id) VALUES (?,?)",
                    params![attachment.id(), self.local_message_id],
                )
                .await?;
        }
        Ok(())
    }

    /// Load a message for the message with `local_message_id`.
    ///
    /// # Errors
    ///
    /// Returns error if the query failed.
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
                attachments,
                row_id: None,
            },
            api_message_body.body,
        )
    }

    /// Update the `header`, `parsed_headers` and `remote_message_id` fields after the
    /// draft has been created or updated on the server.
    ///
    /// # Errors
    ///
    /// Returns error if the query failed.
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
                    "SELECT local_label_id AS value FROM message_labels WHERE local_message_id=?",
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

/// Message counters that are related to particular label
/// Allow the user to see how many message there are assigned to the label,
/// both unread count and total count.
#[derive(Clone, Debug, Eq, Model, PartialEq)]
#[TableName("message_counters")]
pub struct MessageCounters {
    #[IdField]
    pub local_label_id: LocalLabelId,

    #[DbField]
    pub total: u64,

    #[DbField]
    pub unread: u64,

    #[RowIdField]
    pub row_id: Option<u64>,
}

impl MessageCounters {
    /// Constructor - note: [`MessageCounters`] does not implement [`Default`] trait
    ///
    /// # Parameters
    /// * `local_label_id` - local id of the label
    pub fn new(local_label_id: LocalLabelId) -> Self {
        Self {
            local_label_id,
            total: Default::default(),
            unread: Default::default(),
            row_id: Default::default(),
        }
    }

    /// Save message counters to the database.
    ///
    /// It's imperative that you use this method over [`Model::save()`] to ensure
    /// that if the counter already exists it is updated, and not inserted with a conflict.
    ///
    /// # Parameters
    /// * `local_label_id` - local id of the label
    /// * `tx` - transaction used to modify DB
    ///
    /// # Errors
    ///
    /// Returns an error if the query fails.
    pub async fn save(&mut self, bond: &Bond<'_>) -> Result<(), StashError> {
        if self.row_id.is_none() {
            if let Some(existing) = Self::find_by_id(self.local_label_id, bond).await? {
                self.row_id = existing.row_id;
            }
        }
        <Self as Model>::save(self, bond).await
    }

    /// Get all message counters linked to labels with given kind
    ///
    /// # Parameters
    ///
    /// * `kind` - The kind of the label, eg. System, Folder etc.
    /// * `tether` - The tether to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the data could not be read from the database.
    pub async fn find_by_kind(kind: LabelType, tether: &Tether) -> Result<Vec<Self>, StashError> {
        Self::find(
            "INNER JOIN labels ON labels.local_id = local_label_id WHERE label_type = ? ORDER BY labels.display_order ASC",
            params![kind],
            tether
        ).await
    }

    /// Returns counters, first unread then total
    pub fn counters(&self) -> (u64, u64) {
        (self.unread, self.total)
    }

    pub fn total(&self, unread: ReadFilter) -> u64 {
        match unread {
            ReadFilter::All => self.total,
            ReadFilter::Unread => self.unread,
            ReadFilter::Read => self.total.saturating_sub(self.unread),
        }
    }

    /// Returns [`MessageCounts`] datastructure that contains label's Remote ID
    /// instead of the Local ID.
    pub async fn message_count(&self, tether: &Tether) -> Result<MessageLabelsCount, AppError> {
        let remote_id = Label::resolve_remote_label_id(self.local_label_id, tether).await?;

        Ok(MessageLabelsCount {
            label_id: remote_id,
            total: self.total,
            unread: self.unread,
        })
    }

    /// Watch message counter for changes.
    ///
    /// When a change occurs a message is produced in the returned receiver.
    ///
    /// # Errors
    /// Returns error if the query failed
    ///
    pub fn watch(stash: &Stash) -> Result<WatcherHandle, StashError> {
        stash.subscribe_to(|sender| Box::new(MessageCounterWatcher { sender }))
    }
}

pub struct MessageCounterWatcher {
    sender: flume::Sender<()>,
}

impl TableObserver for MessageCounterWatcher {
    fn tables(&self) -> Vec<String> {
        vec![MessageCounters::table_name().to_string()]
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
