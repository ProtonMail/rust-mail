#[cfg(test)]
#[path = "../tests/models/conversations.rs"]
mod conversations;

use super::network::split_request;
use crate::actions::conversations::label_as::Handler as LabelAsHandler;
use crate::actions::conversations::LabelAs;
use crate::actions::conversations::{Label as ActionLabel, MarkRead, MarkUnread, Move, Unlabel};
use crate::actions::{
    filter_responses, AllBottomBarMessageActions, BottomBarActions, ConversationAction,
    ConversationAvailableActions, GeneralActions, LabelAsAction, MovableSystemFolderAction,
    MoveAction, MoveItemAction,
};
use crate::datatypes::{
    AttachmentMetadata, ConversationCount, CustomLabel, Disposition, ExclusiveLocation, LabelType,
    MessageAddresses, MessageAttachmentInfos, MobileActions, SystemLabel, SystemLabelId,
};
use crate::find_in_query;
use crate::models::*;
use crate::MailUserContext;
use crate::{actions::conversations::Delete, AppError};
use anyhow::{anyhow, Context};
use indoc::{formatdoc, indoc};
use itertools::Itertools;
use proton_action_queue::queue::{ActionError, ActionOutput, Queue};
use proton_api_core::service::ApiServiceError;
use proton_api_core::services::proton::common::RemoteId as ApiRemoteId;
use proton_api_core::services::proton::Proton;
use proton_api_core::session::{CoreSession, Session};
use proton_api_mail::services::proton::requests::GetConversationsOptions;
use proton_api_mail::services::proton::response_data::{
    Conversation as ApiConversation, ConversationLabel as ApiConversationLabel,
    MessageMetadata as ApiMessageMetadata, OperationResult,
};
use proton_api_mail::services::proton::ProtonMail;
use proton_api_mail::MAX_PAGE_ELEMENT_COUNT;
use proton_core_common::datatypes::{Id, LabelId, LocalId, RemoteId};
use proton_core_common::models::ModelExtension;
use proton_core_common::paginator::{DataSource, Paginator, Param};
use stash::exports::SqliteError;
use stash::exports::ToSql;
use stash::macros::Model;
use stash::orm::{Model, ResultsetChange};
use stash::params;
use stash::stash::{AgnosticInterface, Interface, Stash, StashError, Tether};
use std::collections::hash_map::Entry as HmEntry;
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::num::NonZeroU32;
use tracing::{debug, error, info, warn};
#[derive(Clone, Debug, Default, Eq, Model, PartialEq)]
#[TableName("conversations")]
#[ModelActions(on_load, on_save)]
pub struct Conversation {
    /// The local ID of the record, i.e. the ID assigned by the client
    /// application. This is a restricted-scope unique identifier for the record
    /// within the set of all records of this type, and is important for
    /// relating local records. It has no relationship to the centrally-stored
    /// API ID, and never leaves the local system.
    #[IdField(autoincrement)]
    pub local_id: Option<LocalId>,

    /// The remote ID of the record, i.e. the ID assigned by the API. This is a
    /// globally-consistent unique identifier for the record within the set of
    /// all records of this type, and is important for synchronisation.
    #[DbField]
    pub remote_id: Option<RemoteId>,

    /// TODO: Document this field.
    #[DbField]
    pub attachment_info: MessageAttachmentInfos,

    /// Attachment metadata associated with this conversation.
    pub attachments_metadata: Vec<AttachmentMetadata>,

    /// TODO: Document this field.
    #[DbField]
    pub deleted: bool,

    /// TODO: Document this field.
    #[DbField]
    pub display_snooze_reminder: bool,

    /// Exclusive location of the [`Conversation`] (e.g. Inbox, Archive, Outbox
    /// etc.). This field is auto-calculated, and not stored in the database.
    /// When the model is read from database, this field should be calculated,
    /// and always be [`Some`]. If it is [`None`], it means either that the
    /// model is not fully initialized or there is very nasty bug. Failed
    /// initialization is logged as an error, but flow is not impacted due to
    /// the fact that this is not a critical field.
    pub exclusive_location: Option<ExclusiveLocation>,

    /// TODO: Document this field.
    #[DbField]
    pub expiration_time: u64,

    /// TODO: Document this field.
    pub labels: Vec<ConversationLabel>,

    /// TODO: Document this field
    #[DbField]
    pub num_attachments: u64,

    /// How many messages there are in the conversation.
    #[DbField]
    pub num_messages: u64,

    /// How many unread messages there are in the conversation.
    #[DbField]
    pub num_unread: u64,

    /// TODO: Document this field.
    #[DbField]
    pub display_order: u64,

    #[DbField]
    /// TODO: Document this field.
    pub recipients: MessageAddresses,

    #[DbField]
    /// TODO: Document this field.
    pub senders: MessageAddresses,

    /// TODO: Document this field.
    #[DbField]
    pub size: u64,

    /// TODO: Document this field.
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

    /// List of custom labels.
    pub custom_labels: Vec<CustomLabel>,

    /// Whether the conversation has synced its messages.
    #[DbField]
    pub has_messages: bool,

    #[allow(clippy::doc_markdown)]
    /// The internal row ID of the record in the database. This is assigned by
    /// SQLite, and is used as a consistent identifier for records when
    /// listening for change notifications.
    #[RowIdField]
    pub row_id: Option<u64>,

    /// The database instance that the record is associated with. This is
    /// present for convenience.
    #[StashField]
    pub stash: Option<Stash>,
}

impl Conversation {
    /// Label multiple conversations.
    ///
    /// # Parameters
    ///
    /// * `session`          - The session.
    /// * `queue`            - The action queue.
    /// * `label_id`         - The ID of the label to apply to the conversations.
    /// * `conversation_ids` - The IDs of the conversations to label.
    ///
    /// # Errors
    ///
    /// Returns an error if the action failed.
    ///
    pub async fn action_apply_label(
        queue: &Queue,
        label_id: LocalId,
        conversation_ids: Vec<LocalId>,
    ) -> Result<ActionOutput<ActionLabel>, ActionError<ActionLabel>> {
        let action = ActionLabel::new(label_id, conversation_ids);
        queue.apply_action(action).await
    }

    /// Star multiple conversations.
    ///
    /// # Parameters
    ///
    /// * `queue`            - The action queue.
    /// * `conversation_ids` - The IDs of the conversations to star.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed.
    ///
    pub async fn action_star(
        queue: &proton_action_queue::queue::Queue,
        conversation_ids: Vec<LocalId>,
    ) -> Result<ActionOutput<ActionLabel>, ActionError<ActionLabel>> {
        let label_id = LabelId::starred()
            .counterpart::<crate::models::Label, _>(queue.stash())
            .await
            .map_err(|e| ActionError::Queue(e.into()))?
            .expect("Star system label not found");
        let action = ActionLabel::new(label_id, conversation_ids.into_iter().map_into());
        queue.apply_action(action).await
    }

    /// Unstar multiple conversations.
    ///
    /// # Parameters
    ///
    /// * `queue`            - The action queue.
    /// * `conversation_ids` - The IDs of the conversations to unstar.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed.
    ///
    pub async fn action_unstar(
        queue: &Queue,
        conversation_ids: Vec<LocalId>,
    ) -> Result<ActionOutput<Unlabel>, ActionError<Unlabel>> {
        let label_id = LabelId::starred()
            .counterpart::<crate::models::Label, _>(queue.stash())
            .await?
            .expect("Star system label not found");
        let action = Unlabel::new(label_id, conversation_ids.into_iter().map_into());
        queue.apply_action(action).await
    }

    /// Unlabel multiple conversations.
    ///
    /// # Parameters
    ///
    /// * `queue`            - The action queue.
    /// * `label_id`         - The ID of the label to apply to the conversations.
    /// * `conversation_ids` - The IDs of the conversations to unlabel.
    ///
    /// # Errors
    ///
    /// Returns an error if the action failed.
    ///
    pub async fn action_remove_label(
        queue: &Queue,
        label_id: LocalId,
        conversation_ids: Vec<LocalId>,
    ) -> Result<ActionOutput<Unlabel>, ActionError<Unlabel>> {
        let action = Unlabel::new(label_id, conversation_ids.into_iter().map_into());
        queue.apply_action(action).await
    }

    /// Mark multiple conversations as read.
    ///
    /// # Parameters
    ///
    /// * `session`          - The session.
    /// * `queue`            - The action queue.
    /// * `label_id`         - The ID of the label to apply to the conversations.
    /// * `conversation_ids` - The IDs of the target conversations.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed.
    ///
    pub async fn action_mark_read(
        queue: &Queue,
        label_id: LocalId,
        conversation_ids: Vec<LocalId>,
    ) -> Result<ActionOutput<MarkRead>, ActionError<MarkRead>> {
        let action = MarkRead::new(label_id, conversation_ids);
        queue.apply_action(action).await
    }

    /// Mark multiple conversations as unread.
    ///
    /// # Parameters
    ///
    /// * `queue`            - The action queue.
    /// * `label_id`         - The ID of the label to apply to the conversations.
    /// * `conversation_ids` - The IDs of the target conversations.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed.
    ///
    pub async fn action_mark_unread(
        queue: &Queue,
        label_id: LocalId,
        conversation_ids: Vec<LocalId>,
    ) -> Result<ActionOutput<MarkUnread>, ActionError<MarkUnread>> {
        let action = MarkUnread::new(label_id, conversation_ids);
        queue.apply_action(action).await
    }

    /// Mark multiple conversations as read.
    ///
    /// # Parameters
    ///
    /// * `queue`            - The action queue.
    /// * `label_id`         - The ID of the label to apply to the conversations.
    /// * `conversation_ids` - The IDs of the target conversations.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed.
    ///
    pub async fn action_delete(
        queue: &Queue,
        label_id: LocalId,
        conversation_ids: Vec<LocalId>,
    ) -> Result<ActionOutput<Delete>, ActionError<Delete>> {
        let action = Delete::new(label_id, conversation_ids);
        queue.apply_action(action).await
    }

    /// Move multiple conversations.
    ///
    /// # Parameters
    ///
    /// * `queue`          - The action queue.
    /// * `source_id`      - The ID of the label where the conversations are.
    /// * `destination_id` - The ID of the label where the conversations go.
    /// * `target_ids`     - The IDs of the conversations to move.
    ///
    /// # Errors
    ///
    /// Returns an error if the action failed.
    ///
    pub async fn action_move(
        queue: &Queue,
        source_id: LocalId,
        destination_id: LocalId,
        target_ids: Vec<LocalId>,
    ) -> Result<ActionOutput<Move>, ActionError<Move>> {
        let action = Move::new(source_id, destination_id, target_ids);
        queue.apply_action(action).await
    }

    /// Soft delete multiple conversations.
    ///
    /// # Parameters
    ///
    /// * `queue`            - The action queue.
    /// * `label_id`         - The ID of the current view.
    /// * `conversation_ids` - The IDs of the converstations to delete.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed.
    ///
    pub async fn action_mark_deleted(
        queue: &Queue,
        label_id: LocalId,
        conversation_ids: impl IntoIterator<Item = LocalId>,
    ) -> Result<ActionOutput<Delete>, ActionError<Delete>> {
        let action = Delete::new(label_id, conversation_ids);
        queue.apply_action(action).await
    }

    /// Action to change labels on a batch of conversations.
    ///
    /// All given conversations will get the selected labels.
    /// All given conversations will keep the partially selected labels.
    /// All given conversations will lose any other labels.
    ///
    /// # Parameters
    ///
    /// * `queue`                        - The action queue.
    /// * `source_label_id`              - Id of the currently used label.
    /// * `conversation_ids`             - List of ids of the conversations to label.
    /// * `selected_label_ids`           - List of ids of the Labels to set.
    /// * `partially_selected_label_ids` - List of ids of the Labels to keep as is.
    /// * `must_archive`                 - If true, the given conversations must be archived.
    ///
    /// # Errors
    ///
    /// Returns an error if the action can not be applied.
    ///
    pub async fn action_label_as(
        queue: &Queue,
        source_label_id: LocalId,
        conversation_ids: Vec<LocalId>,
        selected_label_ids: Vec<LocalId>,
        partially_selected_label_ids: Vec<LocalId>,
        must_archive: bool,
    ) -> Result<bool, AppError> {
        let action = LabelAs::new(
            source_label_id,
            conversation_ids,
            selected_label_ids,
            partially_selected_label_ids,
            must_archive,
        );
        let ActionOutput { local, .. } = queue
            .apply_action(action)
            .await
            .map_err(|e| AppError::Other(anyhow!(e)))?;
        Ok(local)
    }

    /// Locally apply LabelAs action for conversations
    pub(crate) async fn label_as<A>(
        source_label_id: LocalId,
        conversation_ids: Vec<LocalId>,
        selected_label_ids: &[LocalId],
        partially_selected_label_ids: &[LocalId],
        must_archive: bool,
        interface: &A,
    ) -> Result<(), AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        for label in Label::find_by_kind(LabelType::Label, interface).await? {
            let label_id = label.local_id.expect("Should be set");
            if selected_label_ids.contains(&label_id) {
                Self::apply_label(label_id, conversation_ids.clone(), interface).await?
            } else if !partially_selected_label_ids.contains(&label_id) {
                Self::remove_label(label_id, conversation_ids.clone(), interface).await?
            }
            // else keep label as is
        }

        if must_archive {
            let archive_id =
                RemoteId::counterpart::<Label, _>(&LabelId::archive().into_inner(), interface)
                    .await?
                    .expect("Archive label must have a RemoteId");
            Self::move_conversations(source_label_id, archive_id, conversation_ids, interface)
                .await?;
        }

        Ok(())
    }

    /// Remotely apply LabelAs action for conversations
    pub(crate) async fn remote_relabel<A>(
        session: &Session,
        added_label_ids: &HashMap<LocalId, HashSet<LocalId>>,
        removed_label_ids: &HashMap<LocalId, HashSet<LocalId>>,
        interface: &A,
    ) -> Result<Vec<RemoteId>, AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        /// Gets a hashmap of the remote label id and the local ids.
        async fn group_ids_by_label(
            label_ids: &HashMap<LocalId, HashSet<LocalId>>,
            interface: &(impl Into<AgnosticInterface> + Interface),
        ) -> Result<HashMap<RemoteId, HashSet<LocalId>>, AppError> {
            let mut map = HashMap::new();
            for (conv_id, local_label_ids) in label_ids {
                let remote_label_ids = LocalId::counterparts::<Label, _>(
                    Vec::from_iter(local_label_ids.iter().cloned()),
                    interface,
                )
                .await?;
                for remote_label_id in remote_label_ids {
                    map.entry(remote_label_id)
                        .or_insert_with(HashSet::new)
                        .insert(*conv_id);
                }
            }
            Ok(map)
        }

        let added_by_label = group_ids_by_label(added_label_ids, interface).await?;
        let removed_by_label = group_ids_by_label(removed_label_ids, interface).await?;

        let api = session.api();

        let mut failed_ids = vec![];
        for (label_id, conversation_ids) in added_by_label {
            let conversation_ids = LocalId::counterparts::<Conversation, _>(
                Vec::from_iter(conversation_ids),
                interface,
            )
            .await?;
            let response = api
                .put_conversations_label(
                    conversation_ids.iter().cloned().map_into().collect(),
                    label_id.clone().into(),
                    None,
                )
                .await;

            match response {
                Ok(res) => failed_ids.extend(filter_responses(res.responses)),
                Err(e) => {
                    error!("{e:?}");
                    failed_ids.extend(conversation_ids);
                }
            };
        }

        for (label_id, conversation_ids) in removed_by_label {
            let conversation_ids = LocalId::counterparts::<Conversation, _>(
                Vec::from_iter(conversation_ids),
                interface,
            )
            .await?;
            let response = api
                .put_conversations_unlabel(
                    conversation_ids.iter().cloned().map_into().collect(),
                    label_id.clone().into(),
                )
                .await;
            match response {
                Ok(res) => failed_ids.extend(filter_responses(res.responses)),
                Err(e) => {
                    error!("{e:?}");
                    failed_ids.extend(conversation_ids);
                }
            };
        }

        Ok(failed_ids)
    }

    /// Revert locally the LabelAs action for conversation.
    pub(crate) async fn undo_label_as<A>(
        local_ids: Vec<LocalId>,
        source_label_id: LocalId,
        mut added_labels: HashMap<LocalId, HashSet<LocalId>>,
        mut removed_labels: HashMap<LocalId, HashSet<LocalId>>,
        mut original_location: HashMap<LocalId, Option<ExclusiveLocation>>,
        must_archive: bool,
        interface: &A,
    ) -> Result<(), AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let archive_id =
            RemoteId::counterpart::<Label, _>(&LabelId::archive().into_inner(), interface)
                .await?
                .expect("Archive label must have a RemoteId");

        for conversation_id in &local_ids {
            LabelAsHandler::revert_one_locally(
                conversation_id,
                added_labels.remove(conversation_id).unwrap_or_default(),
                removed_labels.remove(conversation_id).unwrap_or_default(),
                original_location.remove(conversation_id),
                interface,
            )
            .await?;

            if must_archive {
                Conversation::move_conversations(
                    archive_id,
                    source_label_id,
                    local_ids.clone(),
                    interface,
                )
                .await?;
            }
        }
        Ok(())
    }

    /// Get the available actions from bottom bar for given conversations
    ///
    /// # Parameters
    ///
    /// * `current_label_id`  - Id of the current mailbox.
    /// * `conversations_ids` - List of the conversations IDs.
    /// * `interface`         - The database interface.
    ///
    pub async fn all_available_bottom_bar_actions_for_conversations<A>(
        current_label_id: LocalId,
        conversation_ids: Vec<LocalId>,
        interface: &A,
    ) -> Result<AllBottomBarMessageActions, AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let inbox = MovableSystemFolderAction::inbox(interface).await?;
        let archive = MovableSystemFolderAction::archive(interface).await?;
        let trash = MovableSystemFolderAction::trash(interface).await?;
        let spam = MovableSystemFolderAction::spam(interface).await?;

        let current_label = Label::resolve_remote_label_id(current_label_id, interface).await?;
        let bottom_bar_actions = MobileActions::bottom_bar_actions(interface).await?;
        let conversations = Self::find_by_ids(conversation_ids.to_vec(), interface).await?;
        let visible_bottom_bar_actions = Self::visible_bottom_bar_actions(
            &current_label,
            &conversations,
            &bottom_bar_actions,
            &inbox,
            &archive,
            &trash,
            &spam,
        )?;
        let hidden_bottom_bar_actions = Self::hidden_bottom_bar_actions(
            current_label,
            &conversations,
            &visible_bottom_bar_actions,
            &inbox,
            &archive,
            &trash,
            &spam,
        );

        Ok(AllBottomBarMessageActions {
            hidden_bottom_bar_actions,
            visible_bottom_bar_actions,
        })
    }

    /// Find a group of Conversations by their IDs.
    ///
    /// # Parameters
    ///
    /// * `conversation_ids` - The IDs of the conversations to find.
    /// * `interface`        - The database interface.
    ///
    /// # Errors
    ///
    /// When database request fail.
    ///
    pub(crate) async fn find_by_ids<A>(
        conversation_ids: impl IntoIterator<Item = LocalId>,
        interface: &A,
    ) -> Result<Vec<Self>, StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let (query, params) =
            find_in_query!("WHERE deleted = 0 AND local_id IN ({})", conversation_ids);
        Conversation::find(query, params, interface, None).await
    }

    /// Get actions to display in bottom_bar when selecting messages
    fn visible_bottom_bar_actions(
        current_label: &LabelId,
        conversations: &[Self],
        bottom_bar_actions: &[MobileActions],
        inbox: &MovableSystemFolderAction,
        archive: &MovableSystemFolderAction,
        trash: &MovableSystemFolderAction,
        spam: &MovableSystemFolderAction,
    ) -> Result<Vec<BottomBarActions>, AppError> {
        let any_unread = conversations.iter().any(|c| c.num_unread > 0);
        let all_starred = conversations.iter().all(|c| c.is_starred());

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
        conversations: &[Self],
        visible_actions: &[BottomBarActions],
        inbox: &MovableSystemFolderAction,
        archive: &MovableSystemFolderAction,
        trash: &MovableSystemFolderAction,
        spam: &MovableSystemFolderAction,
    ) -> Vec<BottomBarActions> {
        let any_unread = conversations.iter().any(|m| m.num_unread > 0);
        let any_read = conversations.iter().any(|m| m.num_unread < m.num_messages);
        let any_starred = conversations.iter().any(|m| m.is_starred());
        let any_unstarred = conversations.iter().any(|m| !m.is_starred());

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

    /// Create a new unknown conversation where we only know the `remote_id`.
    ///
    /// See [`Conversation::is_known`] for more details.
    pub fn unknown(remote_id: RemoteId) -> Self {
        Self {
            local_id: None,
            remote_id: Some(remote_id),
            attachment_info: Default::default(),
            attachments_metadata: vec![],
            deleted: false,
            display_snooze_reminder: false,
            exclusive_location: None,
            expiration_time: 0,
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
            row_id: None,
            stash: None,
            has_messages: false,
        }
    }

    /// Save a conversation to the database.
    ///
    /// It's imperative that you use this method over [`Model::save()`] to
    /// ensure that existing conversations are updated.
    ///
    /// # Errors
    ///
    /// Returns an error if the local conversation id is not set or the query
    /// failed.
    ///
    pub async fn save(&mut self) -> Result<(), StashError> {
        let Some(stash) = self.stash.clone() else {
            return Err(StashError::NoStashAvailable);
        };

        self.save_using(&stash).await
    }

    /// Save a message to the database.
    ///
    /// It's imperative that you use this method over [`Model::save_using()`] to
    /// ensure that existing conversations are updated.
    ///
    /// # Parameters
    ///
    /// * `interface` - The database interface, i.e. [`Stash`] or [`Tether`], to
    ///                 use for finding the records.
    ///
    /// # Errors
    ///
    /// Returns an error if the local conversation id is not set or the query
    /// failed.
    ///
    pub async fn save_using<A>(&mut self, interface: &A) -> Result<(), StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        if let Some(remote_id) = self.remote_id.clone() {
            if let Some(existing) = Self::find_by_id(remote_id, interface).await? {
                self.local_id = existing.local_id;
                self.row_id = existing.row_id;
                self.stash = existing.stash;
            }
        }

        <Self as Model>::save_using(self, interface).await
    }

    /// Label multiple conversations.
    ///
    /// # Parameters
    ///
    /// * `label_id`    - Id of the label to assign
    /// * `ids`         - The IDs of the conversations to label.
    /// * `interface`   - The interface to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the data could not be written to the database.
    ///
    pub async fn apply_label<A>(
        label_id: LocalId,
        ids: impl IntoIterator<Item = LocalId>,
        interface: &A,
    ) -> Result<(), StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        for id in ids {
            let message_ids = interface
                .query_values::<_, LocalId>(
                    indoc::formatdoc! {"
            WITH conv_msgs AS (
                SELECT local_id,? AS label_id FROM messages WHERE local_conversation_id=?
            )
            INSERT OR IGNORE INTO
                message_labels (local_message_id, local_label_id)
            SELECT * FROM conv_msgs RETURNING local_message_id AS value
"},
                    params![label_id, id],
                )
                .await?;

            if !message_ids.is_empty() {
                Conversation::label_impl(label_id, id, &message_ids, interface).await?
            } else {
                // Fallback without message metadata. We should grab the highest time values from
                // all the remaining labels assigned to this conversation. All conversations
                // messages will always have the All Mail label assigned.
                if ConversationLabel::find_first(
                    "WHERE local_conversation_id=? AND local_label_id=?",
                    params![id, label_id],
                    interface,
                )
                .await?
                .is_none()
                {
                    let Some(mut label) = Label::find_by_id(label_id, interface).await? else {
                        return Err(StashError::ExecutionError(SqliteError::QueryReturnedNoRows));
                    };

                    let mut new_label = ConversationLabel {
                        local_id: None,
                        local_conversation_id: Some(id),
                        local_label_id: Some(id),
                        remote_label_id: label.remote_id.clone(),
                        context_expiration_time: 0,
                        context_num_attachments: 0,
                        context_num_messages: 0,
                        context_num_unread: 0,
                        context_size: 0,
                        context_snooze_time: 0,
                        context_time: 0,
                        deleted: false,
                        row_id: None,
                        stash: None,
                    };
                    let conversation_labels = ConversationLabel::find(
                        "WHERE local_conversation_id=?",
                        params![id],
                        interface,
                        None,
                    )
                    .await?;
                    for conversation_label in conversation_labels {
                        new_label.context_expiration_time = conversation_label
                            .context_expiration_time
                            .max(new_label.context_expiration_time);
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

                    new_label.save_using(interface).await?;

                    label.total_conv += 1;
                    label.save_using(interface).await?;
                }
            }
        }

        Ok(())
    }

    /// Label multiple conversations.
    ///
    /// # Parameters
    ///
    /// * `label_id`    - The ID of the label to apply to the conversations.
    /// * `ids`         - The IDs of the conversations to unlabel.
    /// * `spam_action` - TODO: Document this parameter.
    /// * `api`         - The API instance to use.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed.
    ///
    pub async fn apply_label_to_multiple_remote<PM: ProtonMail>(
        label_id: LabelId,
        ids: Vec<RemoteId>,
        spam_action: Option<bool>,
        api: &PM,
    ) -> Result<Vec<OperationResult>, ApiServiceError> {
        let request = |ids: Vec<ApiRemoteId>| {
            let label_id = label_id.clone();
            async {
                api.put_conversations_label(ids, label_id.into(), spam_action)
                    .await
                    .map(|r| r.responses)
            }
        };
        Conversation::split_request(ids, request).await
    }

    /// TODO: Document this method.
    ///
    /// # Parameters
    ///
    /// * `conversations` - TODO: Document this parameter.
    /// * `interface`     - The interface to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the data could not be written to the database.
    ///
    pub async fn create_or_update_conversations<A>(
        conversations: Vec<Conversation>,
        interface: &A,
    ) -> Result<Vec<LocalId>, AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let mut ids = Vec::with_capacity(conversations.len());

        for mut conv in conversations {
            Self::save_using(&mut conv, interface).await?;
            ids.push(conv.local_id.unwrap());
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
    /// # Parameters
    ///
    /// * `label_id`  - Label ID where the action is performed
    /// * `ids`       - The IDs of the conversations to delete.
    /// * `interface` - The interface to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the data could not be written to the database.
    ///
    pub async fn mark_deleted<A>(
        label_id: LocalId,
        ids: impl IntoIterator<Item = LocalId>,
        interface: &A,
    ) -> Result<(), AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let all_mail_id = SystemLabel::AllMail.local_id(interface).await?;
        let is_all_mail = all_mail_id
            .filter(|all_mail_id| *all_mail_id == label_id)
            .is_some();

        if is_all_mail {
            Self::mark_deleted_all_mail(ids, interface).await?;
        } else {
            Self::mark_deleted_current_label(label_id, ids, interface).await?;
        }

        Ok(())
    }

    /// Mark conversations as deleted for `AllMail` label.
    /// More information can be found in [`Conversation::mark_deleted`].
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
    async fn mark_deleted_all_mail<A>(
        ids: impl IntoIterator<Item = LocalId>,
        interface: &A,
    ) -> Result<(), AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        for id in ids {
            let Some(mut conversation) = Conversation::find_by_id(id, interface).await? else {
                continue;
            };

            conversation.deleted = true;
            conversation.num_unread = 0;
            conversation.num_messages = 0;
            conversation.num_attachments = 0;
            conversation.size = 0;
            conversation.save_using(interface).await?;

            let mut messages = Message::find(
                formatdoc! {"
                WHERE local_conversation_id=? AND deleted = 0
               "},
                params![id],
                interface,
                None,
            )
            .await?;

            for message in &mut messages {
                message.deleted = true;
                message.save_using(interface).await?
            }

            if !messages.is_empty() {
                let stats = Message::update_message_counters_after_soft_delete(
                    messages.into_iter(),
                    interface,
                )
                .await?;
                conversation
                    .remove_conversation_from_all_labels(stats, interface)
                    .await?;
            }
        }

        Ok(())
    }

    /// Updates all labels counters after soft delete of conversation in active view `AllMail`.
    ///
    /// # Parameters
    ///
    /// * `all_stats`  - The stats of the messages that were deleted.
    /// * `interface`  - The interface to use for the database connection.
    ///
    /// # Errors
    ///
    /// Will return an error if the data could not be written to the database.
    ///
    async fn remove_conversation_from_all_labels<A>(
        &self,
        all_stats: HashMap<LocalId, MessageLabelStats>,
        interface: &A,
    ) -> Result<(), AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let conv_labels = ConversationLabel::find(
            "WHERE local_conversation_id=? AND deleted=0",
            params![self.local_id.unwrap()],
            interface,
            None,
        )
        .await?;

        for mut conv_label in conv_labels {
            let label_id = conv_label.local_label_id.unwrap();
            let mut label = Label::find_by_id(label_id, interface)
                .await?
                .ok_or_else(|| AppError::LabelNotFound(label_id))?;
            let stats = all_stats.get(&label_id);

            label.total_conv -= 1;

            if stats.filter(|s| s.unread_count > 0).is_some() {
                label.unread_conv -= 1;
            }

            label.save_using(interface).await?;

            conv_label.deleted = true;
            conv_label.save_using(interface).await?;
        }

        Ok(())
    }

    /// Mark conversations as deleted in active label.
    /// More information can be found in [`Conversation::mark_deleted`].
    ///
    /// # Parameters
    ///
    /// * `label_id`  - Label ID where the action is performed
    /// * `ids`       - The IDs of the conversations to delete.
    /// * `interface` - The interface to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the data could not be written to the database.
    ///
    async fn mark_deleted_current_label<A>(
        label_id: LocalId,
        ids: impl IntoIterator<Item = LocalId>,
        interface: &A,
    ) -> Result<(), AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        for id in ids {
            let Some(mut conversation) = Conversation::find_first(
                "WHERE local_id=? AND deleted=0 AND is_known=1",
                params![id],
                interface,
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
                interface,
                None,
            )
            .await?;

            for message in &mut messages {
                message.deleted = true;
                message.save_using(interface).await?
            }

            if !messages.is_empty() {
                let all_stats = Message::update_message_counters_after_soft_delete(
                    messages.into_iter(),
                    interface,
                )
                .await?;

                let stats = all_stats.get(&label_id);

                conversation
                    .mark_delete_update_stats(stats, interface)
                    .await?;

                conversation
                    .remove_conversation_from_label(label_id, stats, interface)
                    .await?;
            }
        }

        Ok(())
    }

    /// Updates active label counters after soft delete of conversation.
    ///
    /// # Parameters
    ///
    /// * `label_id`   - The ID of the label to update.
    /// * `all_stats`  - The stats of the messages that were deleted.
    /// * `interface`  - The interface to use for the database connection.
    ///
    /// # Errors
    ///
    /// Will return an error if the data could not be written to the database.
    ///
    pub async fn remove_conversation_from_label<A>(
        &mut self,
        label_id: LocalId,
        stats: Option<&MessageLabelStats>,
        interface: &A,
    ) -> Result<(), AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let conv_label = ConversationLabel::find_first(
            "WHERE local_conversation_id=? AND deleted=0 AND local_label_id=?",
            params![self.local_id.unwrap(), label_id],
            interface,
        )
        .await?;

        if let Some(mut conv_label) = conv_label {
            let mut label = Label::find_by_id(label_id, interface)
                .await?
                .ok_or_else(|| AppError::LabelNotFound(label_id))?;
            label.total_conv -= 1;

            if stats.filter(|s| s.unread_count > 0).is_some() {
                label.unread_conv -= 1;
            }

            label.save_using(interface).await?;

            conv_label.deleted = true;
            conv_label.save_using(interface).await?;
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
    /// # Parameters
    ///
    /// * `label_id`  - Label ID where the action is performed
    /// * `ids`       - The IDs of the conversations to delete.
    /// * `interface` - The interface to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the data could not be written to the database.
    ///
    pub async fn mark_undeleted<A>(
        label_id: LocalId,
        ids: impl IntoIterator<Item = LocalId>,
        interface: &A,
    ) -> Result<(), AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let all_mail_id = SystemLabel::AllMail.local_id(interface).await?;
        let is_all_mail = all_mail_id
            .filter(|all_mail_id| *all_mail_id == label_id)
            .is_some();

        if is_all_mail {
            Self::mark_undeleted_all_mail(ids, interface).await?;
        } else {
            Self::mark_undeleted_current_label(label_id, ids, interface).await?;
        }

        Ok(())
    }

    /// Mark conversations as undeleted for `AllMail` label.
    /// More information can be found in [`Conversation::mark_undeleted`].
    ///
    /// # Parameters
    ///
    /// * `ids`       - The IDs of the conversations to undelete.
    /// * `interface` - The interface to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the data could not be written to the database.
    ///
    async fn mark_undeleted_all_mail<A>(
        ids: impl IntoIterator<Item = LocalId>,
        interface: &A,
    ) -> Result<(), AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        for id in ids {
            let Some(mut conversation) = Conversation::find_by_id(id, interface).await? else {
                continue;
            };

            let mut messages = Message::find(
                formatdoc! {"
                WHERE local_conversation_id=? AND deleted = 1
               "},
                params![id],
                interface,
                None,
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

                message.save_using(interface).await?
            }

            conversation.deleted = false;
            conversation.num_messages += count;
            conversation.num_unread += unread_count;
            conversation.num_attachments += attachment_count;
            conversation.size += size;

            conversation.save_using(interface).await?;

            if !messages.is_empty() {
                let stats = Message::update_message_counters_after_soft_undelete(
                    messages.into_iter(),
                    interface,
                )
                .await?;
                conversation
                    .add_conversation_to_all_labels(stats, interface)
                    .await?;
            }
        }

        Ok(())
    }

    /// Updates all labels counters after undelete of conversation in active view `AllMail`.
    ///
    /// # Parameters
    ///
    /// * `all_stats`  - The stats of the messages that were undeleted.
    /// * `interface`  - The interface to use for the database connection.
    ///
    /// # Errors
    ///
    /// Will return an error if the data could not be written to the database.
    ///
    async fn add_conversation_to_all_labels<A>(
        &self,
        all_stats: HashMap<LocalId, MessageLabelStats>,
        interface: &A,
    ) -> Result<(), AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let conv_labels = ConversationLabel::find(
            "WHERE local_conversation_id=? AND deleted=1",
            params![self.local_id.unwrap()],
            interface,
            None,
        )
        .await?;

        for mut conv_label in conv_labels {
            let label_id = conv_label.local_label_id.unwrap();
            let mut label = Label::find_by_id(label_id, interface)
                .await?
                .ok_or_else(|| AppError::LabelNotFound(label_id))?;
            let stats = all_stats.get(&label_id);

            label.total_conv += 1;

            if stats.filter(|s| s.unread_count > 0).is_some() {
                label.unread_conv += 1;
            }

            label.save_using(interface).await?;

            conv_label.deleted = false;
            conv_label.save_using(interface).await?;
        }

        Ok(())
    }

    /// Mark conversations as undeleted in active label.
    /// More information can be found in [`Conversation::mark_undeleted`].
    ///
    /// # Parameters
    ///
    /// * `label_id`  - Label ID where the action is performed
    /// * `ids`       - The IDs of the conversations to undelete.
    /// * `interface` - The interface to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the data could not be written to the database.
    ///
    async fn mark_undeleted_current_label<A>(
        label_id: LocalId,
        ids: impl IntoIterator<Item = LocalId>,
        interface: &A,
    ) -> Result<(), AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        for id in ids {
            let Some(mut conversation) =
                Conversation::find_first("WHERE local_id=? AND is_known=1", params![id], interface)
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
                interface,
                None,
            )
            .await?;

            for message in &mut messages {
                message.deleted = false;
                message.save_using(interface).await?
            }

            if !messages.is_empty() {
                let all_stats = Message::update_message_counters_after_soft_undelete(
                    messages.into_iter(),
                    interface,
                )
                .await?;
                let stats = all_stats.get(&label_id);

                conversation
                    .add_conversation_to_label(label_id, stats, interface)
                    .await?;

                conversation
                    .mark_undelete_update_stats(stats, interface)
                    .await?;
            }
        }

        Ok(())
    }

    /// Updates active label counters after undelete of conversation.
    ///
    /// # Parameters
    ///
    /// * `label_id`   - The ID of the label to update.
    /// * `stats`      - The stats of the messages that were undeleted.
    /// * `interface`  - The interface to use for the database connection.
    ///
    /// # Errors
    ///
    /// Will return an error if the data could not be written to the database.
    ///
    pub async fn add_conversation_to_label<A>(
        &mut self,
        label_id: LocalId,
        stats: Option<&MessageLabelStats>,
        interface: &A,
    ) -> Result<(), AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let conv_label = ConversationLabel::find_first(
            "WHERE local_conversation_id=? AND deleted=1 AND local_label_id=?",
            params![self.local_id.unwrap(), label_id],
            interface,
        )
        .await?;

        if let Some(mut conv_label) = conv_label {
            let mut label = Label::find_by_id(label_id, interface)
                .await?
                .ok_or_else(|| AppError::LabelNotFound(label_id))?;
            label.total_conv += 1;

            if stats.filter(|s| s.unread_count > 0).is_some() {
                label.unread_conv += 1;
            }

            label.save_using(interface).await?;

            conv_label.deleted = false;
            conv_label.save_using(interface).await?;
        }

        Ok(())
    }
    /// Updates conversation counters after delete of conversation.
    ///
    /// # Parameters
    ///
    /// * `stats`      - The stats of the messages that were undeleted.
    /// * `interface`  - The interface to use for the database connection.
    ///
    /// # Errors
    ///
    /// Will return an error if the data could not be written to the database.
    ///
    pub async fn mark_delete_update_stats<A>(
        &mut self,
        stats: Option<&MessageLabelStats>,
        interface: &A,
    ) -> Result<(), AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let undeleted_messages = Message::count(
            "WHERE local_conversation_id=? AND deleted=0",
            params![self.local_id],
            interface,
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

        self.save_using(interface).await?;

        Ok(())
    }

    /// Updates conversation counters after undelete of conversation.
    ///
    /// # Parameters
    ///
    /// * `stats`      - The stats of the messages that were undeleted.
    /// * `interface`  - The interface to use for the database connection.
    ///
    /// # Errors
    ///
    /// Will return an error if the data could not be written to the database.
    ///
    pub async fn mark_undelete_update_stats<A>(
        &mut self,
        stats: Option<&MessageLabelStats>,
        interface: &A,
    ) -> Result<(), AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        if let Some(stats) = stats {
            self.num_messages += stats.count;
            self.num_unread += stats.unread_count;
            self.num_attachments += stats.attachment_count;
            self.size += stats.size;
            self.deleted = false;
            self.save_using(interface).await?;
        }

        Ok(())
    }

    /// Delete multiple conversations.
    ///
    /// # Parameters
    ///
    /// * `ids`      - The IDs of the conversations to delete.
    /// * `label_id` - TODO: Document this parameter.
    /// * `api`      - The API instance to use.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed.
    ///
    pub async fn delete_multiple_remote<PM: ProtonMail>(
        ids: Vec<RemoteId>,
        label_id: LabelId,
        api: &PM,
    ) -> Result<Vec<OperationResult>, ApiServiceError> {
        let request = |ids: Vec<ApiRemoteId>| {
            let label_id = label_id.clone();
            async {
                api.put_conversations_delete(ids, label_id.into())
                    .await
                    .map(|r| r.responses)
            }
        };
        Conversation::split_request(ids, request).await
    }

    /// Get the conversation counts.
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
    ) -> Result<Vec<ConversationCount>, ApiServiceError> {
        api.get_conversations_count()
            .await
            .map(|r| r.counts.into_iter().map(|c| c.into()).collect())
    }

    /// Retrieve in the first order the first unread message that should be displayed to the user
    /// from the conversation's `messages`. If none was found it will pick last message in the view.
    ///
    /// The returned message will depend on the `label` where the conversation
    /// is returned.
    ///
    /// # Parameters
    ///
    /// * `local_id` - local ID of the conversation.
    /// * `label`    - label model from where the conversation is being viewed.
    /// * `messages` - Array of message models for the conversation.
    ///
    /// # Errors
    ///
    /// When unable to pick the message for the conversation in the current view.
    ///
    pub fn message_id_to_open(
        local_id: LocalId,
        label: &Label,
        messages: &[Message],
    ) -> Result<LocalId, AppError> {
        if messages.is_empty() {
            return Err(AppError::ConversationHasNoMessages(local_id));
        }
        // If we fail to find any message, return the last message in the list.
        Ok(Self::first_unread_message(label, messages)
            .unwrap_or(messages.last().unwrap().local_id.unwrap()))
    }

    /// Retrieve in the first order the first unread message that should be displayed to the user
    /// from the conversation's `messages`. If none was found it will pick last message in the view.
    ///
    /// The returned message will depend on the `label` where the conversation
    /// is returned.
    ///
    /// # Parameters
    ///
    /// * `label`    - label model from where the conversation is being viewed.
    /// * `messages` - Array of message models for the conversation.
    ///
    pub fn first_unread_message(label: &Label, messages: &[Message]) -> Option<LocalId> {
        if messages.is_empty() {
            return None;
        }

        fn first_consecutive_unread_msg(
            label_id: &LabelId,
            messages: &[Message],
            filter: impl Fn(&Message) -> bool,
        ) -> Option<LocalId> {
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
                    .find(|m| filter(m) && m.label_ids.contains(label_id))
                    .and_then(|m| m.local_id)
            })
        }

        let view_is_starred_label_or_folder = label.label_type == LabelType::Label
            || label.label_type == LabelType::Folder
            || label.remote_id == Some(LabelId::starred());
        let label_id = label.remote_id.as_ref()?;

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
    pub async fn load_labels<A>(&self, interface: &A) -> Result<Vec<Label>, StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let ids = self
            .labels
            .iter()
            .filter_map(|label| label.local_label_id)
            .map(|id| Box::new(id) as Box<dyn ToSql + Send>)
            .collect_vec();

        let labels = Label::find(
            format!(
                "WHERE local_id IN ({}) ORDER BY display_order ASC",
                vec!["?"; ids.len()].join(",")
            ),
            ids,
            interface,
            None,
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
    async fn on_load(&mut self, interface: &AgnosticInterface) -> Result<(), StashError> {
        self.labels = ConversationLabel::find(
            "WHERE local_conversation_id = ?",
            params![self.local_id],
            interface,
            None,
        )
        .await?;
        let labels = self.load_labels(interface).await?;
        self.exclusive_location = ExclusiveLocation::from_labels(&labels);
        self.attachments_metadata =
            Attachment::load_conversation_attachment_metadata(self.local_id.unwrap(), interface)
                .await?;
        self.custom_labels = labels
            .into_iter()
            .filter(|l| l.label_type == LabelType::Label)
            .map(CustomLabel::from)
            .collect();

        // Example... not good to do this here, though, as the total number comes
        // from the API.
        // self.num_messages = stash.query::<_, QueryResultU64>(
        //     "SELECT COUNT(*) as value FROM messages WHERE local_conversation_id = ?",
        //     params![self.local_id],
        // ).await?.into_iter().next().unwrap().value;

        Ok(())
    }

    /// Extends [`Model::save()`] to set the contact id for children.
    ///
    /// # Errors
    ///
    /// See [`Model::save()`].
    ///
    pub async fn on_save(&mut self, interface: &AgnosticInterface) -> Result<(), StashError> {
        // Remove any labels that are no longer associated with this conversation.
        if !self.labels.is_empty() {
            #[allow(trivial_casts)]
            interface
                .execute(
                    formatdoc!(
                        "
                DELETE FROM
                    conversation_labels
                WHERE
                    local_conversation_id = ?
                    AND remote_label_id NOT IN ({})
                ",
                        vec!["?"; self.labels.len()].join(",")
                    ),
                    vec![Box::new(self.local_id) as Box<dyn ToSql + Send>]
                        .into_iter()
                        .chain(self.labels.iter().map(|label| {
                            Box::new(label.remote_label_id.clone()) as Box<dyn ToSql + Send>
                        }))
                        .collect(),
                )
                .await?;
        } else {
            interface
                .execute(
                    formatdoc!(
                        "
                DELETE FROM
                    conversation_labels
                WHERE
                    local_conversation_id = ?
                ",
                    ),
                    params![self.local_id],
                )
                .await?;
        }

        // Remove any attachments that are no longer associated with this conversation.
        if !self.attachments_metadata.is_empty() {
            let local_ids = {
                // Create attachment from partial metadata present in a conversation.
                // If attachment record already exists, the conversation ids are updated.
                // If no record exists we create a new one.
                let mut result = Vec::with_capacity(self.attachments_metadata.len());
                for metadata in &self.attachments_metadata {
                    let mut attachment = Attachment::find_first(
                        "WHERE remote_id = ?",
                        params![metadata.remote_id.clone()],
                        interface,
                    )
                    .await?
                    .unwrap_or(Attachment::from(metadata.clone()));

                    attachment.local_conversation_id = self.local_id;
                    attachment.remote_conversation_id = self.remote_id.clone();
                    attachment.save_using(interface).await?;

                    let local_id = attachment.local_id.expect("Should be set");

                    interface
                        .execute(
                            "INSERT OR IGNORE INTO conversation_attachments VALUES (?,?)",
                            params![self.local_id.unwrap(), local_id],
                        )
                        .await?;

                    result.push(local_id);
                }

                result
            };

            #[allow(trivial_casts)]
            interface
                .execute(
                    formatdoc!(
                        "
                DELETE FROM
                    conversation_attachments
                WHERE
                    local_conversation_id = ?
                    AND local_attachment_id NOT IN ({})
                ",
                        vec!["?"; local_ids.len()].join(",")
                    ),
                    vec![Box::new(self.local_id) as Box<dyn ToSql + Send>]
                        .into_iter()
                        .chain(
                            local_ids
                                .into_iter()
                                .map(|attachment| Box::new(attachment) as Box<dyn ToSql + Send>),
                        )
                        .collect(),
                )
                .await?;
        } else {
            interface
                .execute(
                    formatdoc!(
                        "
                DELETE FROM
                    conversation_attachments
                WHERE
                    local_conversation_id = ?
                ",
                    ),
                    params![self.local_id],
                )
                .await?;
        }

        for label in &mut self.labels {
            label.local_conversation_id = self.local_id;
            label.save_using(interface).await?
        }
        Ok(())
    }

    /// Mark multiple conversations as read.
    ///
    /// # Parameters
    ///
    /// * `ids`   - The IDs of the conversations to mark as read.
    /// * `tether` - The tether to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the data could not be written to the database.
    ///
    pub async fn mark_read<A>(
        conversation_ids: impl IntoIterator<Item = LocalId>,
        interface: &A,
    ) -> Result<(), StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        for conversation_id in conversation_ids {
            let mut conversation = Conversation::find_by_id(conversation_id, interface)
                .await?
                .ok_or(StashError::ExecutionError(SqliteError::QueryReturnedNoRows))?;
            // If conversation has no unread messages, there is nothing to do.
            if conversation.num_unread == 0 {
                continue;
            }

            // Update conversation unread count.
            conversation.num_unread = 0;
            conversation.save_using(interface).await?;

            // Update conversation labels unread stats.
            let conversation_labels = ConversationLabel::find(
                "WHERE local_conversation_id=? AND context_num_unread <> 0",
                params![conversation_id],
                interface,
                None,
            )
            .await?;

            let mut label_counts = HashMap::new();
            for mut conversation_label in conversation_labels {
                match label_counts.entry(conversation_label.local_label_id.unwrap()) {
                    HmEntry::Occupied(mut o) => {
                        *o.get_mut() += 1;
                    }
                    HmEntry::Vacant(v) => {
                        v.insert(1);
                    }
                }

                conversation_label.context_num_unread = 0;
                conversation_label.save_using(interface).await?
            }

            for (label_id, count) in &mut label_counts {
                if let Some(mut label) = Label::find_by_id(*label_id, interface).await? {
                    label.unread_conv -= *count;
                    label.save_using(interface).await?
                }

                // reset for messages.
                *count = 0;
            }

            // Update messages
            let messages = Message::find(
                "WHERE local_conversation_id=? AND unread<>0",
                params![conversation_id],
                interface,
                None,
            )
            .await?;

            for mut message in messages {
                let local_message_id = message.local_id.unwrap();
                message.unread = false;
                message.save_using(interface).await?;

                let label_ids = interface.query_values::<_, LocalId>("SELECT local_label_id AS value FROM message_labels WHERE local_message_id=?", params![local_message_id]).await?;
                for label_id in label_ids {
                    match label_counts.entry(label_id) {
                        HmEntry::Occupied(mut o) => {
                            *o.get_mut() += 1;
                        }
                        HmEntry::Vacant(v) => {
                            v.insert(1);
                        }
                    }
                }
            }

            // update message label counters
            for (label_id, count) in &mut label_counts {
                if let Some(mut label) = Label::find_by_id(*label_id, interface).await? {
                    label.unread_msg -= *count;
                    label.save_using(interface).await?
                }
            }
        }

        Ok(())
    }

    /// Mark multiple conversations as read.
    ///
    /// # Parameters
    ///
    /// * `ids` - The IDs of the conversations to mark as read.
    /// * `api` - The API instance to use.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed.
    ///
    pub async fn mark_multiple_as_read_remote<PM: ProtonMail>(
        ids: Vec<RemoteId>,
        api: &PM,
    ) -> Result<Vec<OperationResult>, ApiServiceError> {
        let request = |ids: Vec<ApiRemoteId>| async {
            api.put_conversations_read(ids).await.map(|r| r.responses)
        };
        Conversation::split_request(ids, request).await
    }

    /// Mark multiple conversations as unread.
    /// For each conversation only the last read message gets marked as unread.
    ///
    /// # Parameters
    ///
    /// * `local_label_id`  - Label id where the operation is being applied.
    /// * `ids`             - The IDs of the conversations to mark as unread.
    /// * `tether`          - The tether to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the data could not be written to the database.
    ///
    pub async fn mark_unread(
        local_label_id: LocalId,
        conversation_ids: impl IntoIterator<Item = LocalId>,
        tether: &Tether,
    ) -> Result<(), StashError> {
        for conversation_id in conversation_ids {
            let Some(mut conversation) = Conversation::find_by_id(conversation_id, tether).await?
            else {
                warn!("Conversation with id {conversation_id} does not exist!");
                continue;
            };
            // Find all messages that need to be marked as read.
            let message = Message::find_first(
                "WHERE local_conversation_id=?
                AND unread=0
                ORDER BY time",
                params![conversation_id],
                tether,
            )
            .await?;

            let total_conversation_message_count = tether
                .query_value::<_, u64>(
                    "SELECT COUNT(local_id) AS value FROM messages WHERE local_conversation_id=?",
                    params![conversation_id],
                )
                .await?;

            let Some(mut message) = message else {
                if total_conversation_message_count == 0 {
                    // These conversations where asked to be marked as read, but had
                    // no messages. Either the messages were already mark as read or
                    // there was no metadata. For these we need to set the unread
                    // count to 1 and update the current label count. We let the
                    // event loop take care of the rest.

                    let conv_labels = ConversationLabel::find(
                        "WHERE local_conversation_id=? AND local_label_id=?",
                        params![conversation_id, local_label_id],
                        tether,
                        None,
                    )
                    .await?;
                    for mut conv_label in conv_labels {
                        conv_label.context_num_unread += 1;
                        conv_label.save_using(tether).await?;
                    }

                    conversation.num_unread += 1;
                    conversation.save_using(tether).await?;

                    if let Some(mut label) = Label::find_by_id(local_label_id, tether).await? {
                        label.unread_conv += 1;
                        label.save_using(tether).await?;
                    }
                }
                continue;
            };

            // Update the message

            message.unread = true;
            message.save_using(tether).await?;

            // Update the label counts

            let label_ids = tether
                .query_values::<_, LocalId>(
                    "SELECT local_label_id AS value
                     FROM message_labels
                     WHERE local_message_id=?",
                    params![message.id_value()?],
                )
                .await?;

            for label_id in label_ids {
                if let Some(mut label) = Label::find_by_id(label_id, tether).await? {
                    // Always update the message count
                    label.unread_msg += 1;
                    // only update conversation unread count if we really marked
                    // all messages as unread. If we have mixture, this value
                    // should not be modified
                    if total_conversation_message_count == 1 {
                        label.unread_conv += 1;
                    }

                    label.save_using(tether).await?;
                }

                if let Some(mut conv_label) = ConversationLabel::find_first(
                    "WHERE local_label_id=?",
                    params![label_id],
                    tether,
                )
                .await?
                {
                    conv_label.context_num_unread += 1;
                    conv_label.save_using(tether).await?;
                }
            }

            // update conversations
            conversation.num_unread += 1;
            conversation.save_using(tether).await?;
        }
        Ok(())
    }

    /// Mark multiple conversations as unread.
    ///
    /// # Parameters
    ///
    /// * `ids` - The IDs of the conversations to mark as unread.
    /// * `api` - The API instance to use.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed.
    ///
    pub async fn mark_multiple_as_unread_remote<PM: ProtonMail>(
        ids: Vec<RemoteId>,
        api: &PM,
    ) -> Result<Vec<OperationResult>, ApiServiceError> {
        let request = |ids: Vec<ApiRemoteId>| async {
            api.put_conversations_unread(ids).await.map(|r| r.responses)
        };
        Conversation::split_request(ids, request).await
    }

    /// Unlabel multiple conversations.
    ///
    /// # Parameters
    ///
    /// * `label_id`    - Id of the label to remove.
    /// * `ids`         - The IDs of the conversations to unlabel.
    /// * `interface`   - The interface to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the data could not be written to the database.
    ///
    pub async fn remove_label<A>(
        label_id: LocalId,
        ids: impl IntoIterator<Item = LocalId>,
        interface: &A,
    ) -> Result<(), StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let mut label = Label::find_by_id(label_id, interface)
            .await?
            .ok_or(StashError::ExecutionError(SqliteError::QueryReturnedNoRows))?;

        for id in ids {
            // Remove label from messages
            let message_ids = interface
                .query_values::<_, LocalId>(
                    indoc! {"
                    DELETE FROM message_labels
                    WHERE local_message_id IN (
                        SELECT local_id FROM messages WHERE local_conversation_id=?1
                    ) AND message_labels.local_label_id=?2
                    RETURNING local_message_id AS value
                    "},
                    params![id, label_id],
                )
                .await?;

            // We can only do this part if we have conversation metadata.
            if !message_ids.is_empty() {
                let num_unread = Message::find(
                    format!(
                        "WHERE local_id IN ({})",
                        vec!["?"; message_ids.len()].join(",")
                    ),
                    message_ids
                        .iter()
                        .map(|&v| -> Box<dyn ToSql + Send> { Box::new(*v) })
                        .collect(),
                    interface,
                    None,
                )
                .await?
                .into_iter()
                .fold(0_u64, |mut value, message| {
                    if message.unread {
                        value += 1;
                    }
                    value
                });

                label.total_msg -= message_ids.len() as u64;
                label.unread_msg -= num_unread;
            }

            // Remove conversation label
            match interface
                .query_value::<_, u64>(
                    indoc! {"
                    DELETE FROM conversation_labels
                    WHERE local_conversation_id=? AND local_label_id=?
                    RETURNING context_num_unread AS value
                    "},
                    params![id, label_id],
                )
                .await
            {
                Ok(num_unread) => {
                    if num_unread > 0 {
                        label.unread_conv -= 1;
                    }
                    label.total_conv -= 1;
                }
                Err(e) => {
                    if !matches!(
                        e,
                        StashError::ExecutionError(SqliteError::QueryReturnedNoRows)
                    ) {
                        return Err(e);
                    }
                }
            }
        }

        label.save_using(interface).await?;
        Ok(())
    }

    /// Unlabel multiple conversations.
    ///
    /// # Parameters
    ///
    /// * `label_id` - The ID of the label to apply to the conversations.
    /// * `ids`      - The IDs of the conversations to unlabel.
    /// * `api`      - The API instance to use.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed.
    ///
    pub async fn remove_label_from_multiple_remote<PM: ProtonMail>(
        label_id: LabelId,
        ids: Vec<RemoteId>,
        api: &PM,
    ) -> Result<Vec<OperationResult>, ApiServiceError> {
        let request = |ids: Vec<ApiRemoteId>| {
            let label_id = label_id.clone();
            async {
                api.put_conversations_unlabel(ids, label_id.into())
                    .await
                    .map(|r| r.responses)
            }
        };
        Conversation::split_request(ids, request).await
    }

    /// Given a list of conversations check if there are any missing dependencies like undownloaded
    /// labels.
    ///
    ///
    /// # Parameters
    ///
    /// * `conversations` - The conversations to check.
    /// * `api`           - The API instance to use.
    /// * `stash`         - The stash to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed or the data could not be
    /// written to the database.
    ///
    async fn sync_dependencies<A>(
        conversations: &[ApiConversation],
        api: &Proton,
        interface: &A,
    ) -> Result<(), AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let mut missing_labels = vec![];
        for conv in conversations {
            for label in &conv.labels {
                let rid: RemoteId = label.id.clone().into();
                if (Label::find_by_id(rid, interface)).await?.is_none() {
                    missing_labels.push(label.id.clone());
                }
            }
        }

        if !missing_labels.is_empty() {
            info!(
                "{} label(s) were in a conversations but not locally, synchronizing...",
                missing_labels.len()
            );
            Label::sync_labels_by_ids(api, interface, missing_labels).await?;
        }
        Ok(())
    }
    /// Search for conversations.
    ///
    /// This function accepts search options and calls the API to find any
    /// conversations that fit the criteria. It operates globally and is not
    /// based on a particular mailbox; this restriction can be applied via the
    /// options.
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
    /// written to the database. Can also return an error if a found
    /// conversation cannot be loaded, although this would indicate a
    /// significant problem.
    ///
    pub async fn search(
        options: GetConversationsOptions,
        api: &Proton,
        stash: &Stash,
    ) -> Result<Vec<Conversation>, AppError> {
        // Fetch all the conversations from the API
        let conversations = api
            .get_conversations(options)
            .await
            .context("Error fetching the conversations from the API")?
            .conversations;

        Self::sync_dependencies(&conversations, api, stash).await?;

        let mut conversations = conversations
            .into_iter()
            .map(Conversation::from)
            .collect_vec();
        Self::create_or_update_conversations(conversations.clone(), stash).await?;
        conversations.sort_unstable_by(|x, y| x.display_order.cmp(&y.display_order).reverse());

        Ok(conversations)
    }

    /// Star multiple conversations.
    ///
    /// # Parameters
    ///
    /// * `ids`   - The IDs of the conversations to mark as starred.
    /// * `stash` - The stash to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the data could not be written to the database.
    ///
    pub async fn star_multiple(ids: Vec<LocalId>, stash: &Stash) -> Result<(), StashError> {
        let label_id = match Label::find_by_id(RemoteId::from(LabelId::starred()), stash).await? {
            Some(label) => label.local_id.unwrap(),
            None => {
                error!("Starred label not found");
                return Ok(());
            }
        };

        Self::apply_label(label_id, ids, &stash.connection()).await
    }

    /// Unstar multiple conversations.
    ///
    /// # Parameters
    ///
    /// * `ids`   - The IDs of the conversations to mark as starred.
    /// * `stash` - The stash to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the data could not be written to the database.
    ///
    pub async fn unstar_multiple(ids: Vec<LocalId>, stash: &Stash) -> Result<(), StashError> {
        let label_id = match Label::find_by_id(RemoteId::from(LabelId::starred()), stash).await? {
            Some(label) => label.local_id.unwrap(),
            None => {
                error!("Starred label not found");
                return Ok(());
            }
        };

        Self::remove_label(label_id, ids, &stash.connection()).await
    }

    /// Synchronize the conversations and message counts for each label.
    ///
    /// # Parameters
    ///
    /// * `api`   - The API instance to use.
    /// * `stash` - The stash to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed or the data could not be
    /// written to the database.
    ///
    pub async fn sync_conversation_and_message_counts<PM: ProtonMail>(
        api: &PM,
        stash: &Stash,
    ) -> Result<(), AppError> {
        let (conversation_counts, message_counts) =
            futures::join!(Conversation::fetch_counts(api), Message::fetch_counts(api));
        let (conversation_counts, message_counts) = (conversation_counts?, message_counts?);

        let tx = stash.transaction().await?;
        Label::create_or_update_conversation_counts(conversation_counts, &tx).await?;
        Label::create_or_update_message_counts(message_counts, &tx).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Synchronize the first `count` conversations of the label with `label_id`.
    ///
    /// # Parameters
    ///
    /// * `label_id` - The ID of the label to sync.
    /// * `count`    - TODO: Document this parameter.
    /// * `api`      - The API instance to use.
    /// * `stash`    - The stash to use for the database connection.
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
        stash: &Stash,
    ) -> Result<(), AppError> {
        let response = api
            .get_conversations(GetConversationsOptions {
                desc: Some(true),
                label_id: Some(label_id.into()),
                page: 0,
                page_size: count.min(MAX_PAGE_ELEMENT_COUNT) as u64,
                ..Default::default()
            })
            .await?;

        debug!(
            "Fetched {} conversations TOTAL={}",
            response.conversations.len(),
            response.total
        );
        let tx = stash.transaction().await?;
        Self::create_or_update_conversations(
            response
                .conversations
                .into_iter()
                .map(Conversation::from)
                .collect(),
            &tx,
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Undelete multiple conversations.
    ///
    /// # Parameters
    ///
    /// * `ids`      - The IDs of the conversations to undelete.
    /// * `label_id` - TODO: Document this parameter.
    /// * `tether`   - The tether to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the data could not be written to the database.
    ///
    pub async fn undelete_multiple(
        ids: Vec<LocalId>,
        label_id: LocalId,
        tether: &Tether,
    ) -> Result<usize, StashError> {
        // TODO: This used to do more, but the additional behaviour will be
        // TODO: covered when these operations are refactored.
        tether
            .execute(
                formatdoc!(
                    r"
            UPDATE
                messages
            SET
                deleted = 0
            WHERE
                local_conversation_id IN ({})
                AND deleted = 1
                AND local_id IN (
                    SELECT local_message_id FROM message_labels WHERE local_label_id = ?
                )
                ",
                    ids.iter()
                        .map(ToString::to_string)
                        .collect::<Vec<String>>()
                        .join(",")
                ),
                params![label_id],
            )
            .await
    }

    /// Undelete multiple conversations.
    ///
    /// # Parameters
    ///
    /// * `ids`      - The IDs of the conversations to undelete.
    /// * `label_id` - TODO: Document this parameter.
    /// * `api`      - The API instance to use.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed.
    ///
    pub async fn undelete_multiple_remote<PM: ProtonMail>(
        ids: Vec<RemoteId>,
        label_id: LabelId,
        api: &PM,
    ) -> Result<Vec<OperationResult>, ApiServiceError> {
        let request = |ids: Vec<ApiRemoteId>| {
            let label_id = label_id.clone();
            async {
                api.put_conversations_delete(ids, label_id.into())
                    .await
                    .map(|r| r.responses)
            }
        };
        Conversation::split_request(ids, request).await
    }

    /// Remove all removable labels from given conversations.
    ///
    /// N.B.: `all_mail` label is the only not removable label.
    async fn remove_all_labels<A>(
        conversation_ids: Vec<LocalId>,
        interface: &A,
    ) -> Result<(), StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let all_mail_id = LabelId::all_mail()
            .into_inner()
            .counterpart::<Label, _>(interface)
            .await?
            .expect("AllMail should be set");

        let (query, mut parameters) = find_in_query!(
            "DELETE FROM conversation_labels WHERE local_conversation_id in ({}) AND local_label_id != ?", 
            conversation_ids
        );
        parameters.push(Box::new(all_mail_id) as Box<dyn ToSql + Send>);

        interface.execute(query, parameters).await?;
        Ok(())
    }

    /// Move conversations between two labels.
    ///
    /// # Parameters
    /// * `source_id`        - Local label id where the conversations currently are.
    /// * `destination_id`   - Local label id where the conversations should be moved.
    /// * `conversation_ids` - The IDs of the conversations to move.
    /// * `interface`        - The tether to use for the database connection.
    ///
    /// This function returns a tuple containing the source and destination remote label ids,
    /// respectively.
    ///
    /// # Remarks
    ///
    /// This function can only be called with an active transaction.
    ///
    /// # Errors
    ///
    /// Returns errors if the operation failed.
    pub async fn move_conversations<A>(
        source_id: LocalId,
        destination_id: LocalId,
        conversation_ids: Vec<LocalId>,
        interface: &A,
    ) -> Result<(), AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let remote_source_id = Label::resolve_remote_label_id(source_id, interface).await?;
        let remote_destination_id =
            Label::resolve_remote_label_id(destination_id, interface).await?;

        // If moving to trash, mark conversations as read.
        if remote_destination_id == LabelId::trash() {
            Conversation::mark_read(conversation_ids.clone(), interface)
                .await
                .map_err(|e| {
                    error!("Failed to mark conversations as read when moving to trash: {e}");
                    e
                })?
        }

        // When moving in Trash or Spam, remove all labels (but AllMail)
        if remote_destination_id == LabelId::trash() || remote_destination_id == LabelId::spam() {
            Conversation::remove_all_labels(conversation_ids.clone(), interface)
                .await
                .inspect_err(|e| error!("Failed to remove labels: {e}"))?;
        } else if remote_source_id == LabelId::trash() || remote_source_id == LabelId::spam() {
            // When moving out of Trash or Spam, add AlmostAllMail label
            let almost_all_mail =
                Label::resolve_local_label_id(LabelId::almost_all_mail(), interface).await?;
            Conversation::apply_label(almost_all_mail, conversation_ids.clone(), interface)
                .await
                .inspect_err(|e| {
                    error!(
                        "Failed to apply almost all mail label when moving out of spam/trash:{e}"
                    )
                })?;
        }

        let Some(source) = Label::load(source_id, interface).await? else {
            return Err(AppError::LabelNotFound(source_id));
        };
        if source.is_movable_folder() {
            Conversation::remove_label(source_id, conversation_ids.clone(), interface).await?
        }

        Conversation::apply_label(destination_id, conversation_ids.clone(), interface).await?;

        Ok(())
    }

    /// Get the available actions for conversations depending on current view and stats of the given
    /// conversations.
    ///
    /// # Parameters
    ///
    /// * `view` - The label from which conversation is viewed.
    /// * `local_ids` - The IDs of the conversations to get the actions for.
    /// * `interface` - The interface to use for the database connection.
    ///
    /// # Errors
    ///
    /// Returns error if
    ///
    /// * the database request fail,
    /// * empty list of conversations is provided
    /// * conversation is not in the view
    ///
    pub async fn available_actions<A>(
        view: Label,
        conversation_ids: Vec<LocalId>,
        interface: &A,
    ) -> Result<ConversationAvailableActions, AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        if conversation_ids.is_empty() {
            return Err(AppError::EmptyListOfConversations);
        }

        let conversations = Conversation::find_by_ids(conversation_ids, interface).await?;

        let mut conversation_actions = Vec::new();
        if conversations.iter().any(|c| c.num_unread > 0) {
            conversation_actions.push(ConversationAction::MarkRead);
        }
        if conversations.iter().any(|c| c.num_unread == 0) {
            conversation_actions.push(ConversationAction::MarkUnread);
        }
        if conversations.iter().any(|c| c.is_starred()) {
            conversation_actions.push(ConversationAction::Unstar);
        }
        if conversations.iter().any(|c| !c.is_starred()) {
            conversation_actions.push(ConversationAction::Star);
        }
        conversation_actions.push(ConversationAction::LabelAs);

        let move_actions = MoveItemAction::from_view(view, interface).await?;

        let general_actions = vec![GeneralActions::SaveAsPdf, GeneralActions::Print];

        Ok(ConversationAvailableActions::builder()
            .conversation_actions(conversation_actions)
            .move_actions(move_actions)
            .general_actions(general_actions)
            .build())
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
    pub async fn available_label_as_actions<A>(
        local_ids: Vec<LocalId>,
        interface: &A,
    ) -> Result<Vec<LabelAsAction>, AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        if local_ids.is_empty() {
            return Err(AppError::EmptyListOfConversations);
        }

        let all_label_as = Label::find_by_kind(LabelType::Label, interface).await?;
        let conversations = Conversation::find(
            format!(
                "WHERE local_id IN ({})",
                local_ids.iter().map(ToString::to_string).join(",")
            ),
            vec![],
            interface,
            None,
        )
        .await?;
        let all_label_as_actions = conversations
            .iter()
            .flat_map(|conversation| {
                LabelAsAction::vec(all_label_as.iter(), |label| {
                    conversation
                        .custom_labels
                        .iter()
                        .map(|label| Some(label.local_id))
                        .contains(&label.local_id)
                })
            })
            .collect_vec();

        Ok(LabelAsAction::finalize(all_label_as_actions))
    }

    /// Watches `label as` actions for conversations
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
    pub async fn watch_available_label_as_actions<A>(
        local_ids: Vec<LocalId>,
        interface: &A,
        sender: flume::Sender<()>,
    ) -> Result<Vec<LabelAsAction>, AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        if local_ids.is_empty() {
            return Err(AppError::EmptyListOfConversations);
        }

        let all_label_as = Label::find_by_kind(LabelType::Label, interface).await?;
        let ids = local_ids.iter().map(ToString::to_string).join(",");

        let (cnv_tx, cnv_rx) = flume::unbounded();
        let (cnv_label_tx, cnv_label_rx) = flume::unbounded();

        let conversations = Conversation::find(
            "WHERE local_id IN (?)",
            params![ids.clone()],
            interface,
            Some(cnv_tx),
        )
        .await?;

        let _ = ConversationLabel::find(
            "WHERE local_conversation_id IN (?)",
            params![ids.clone()],
            interface,
            Some(cnv_label_tx),
        )
        .await?;

        tokio::spawn(async move {
            loop {
                if tokio::select! {
                    x = cnv_label_rx.recv_async() => x.map(|_| ()),
                    x = cnv_rx.recv_async() => x.map(|_| ()),
                }
                .is_err()
                {
                    error!("Bug in the watcher system: The watcher receiver was dropped");
                    return;
                };

                if sender.send_async(()).await.is_err() {
                    debug!("watch_available_label_as_actions stopped watching.");
                    return;
                }
            }
        });

        let all_label_as_actions = conversations
            .iter()
            .flat_map(|conversation| {
                LabelAsAction::vec(all_label_as.iter(), |label| {
                    conversation
                        .custom_labels
                        .iter()
                        .map(|label| Some(label.local_id))
                        .contains(&label.local_id)
                })
            })
            .collect_vec();

        Ok(LabelAsAction::finalize(all_label_as_actions))
    }

    /// Get the available move actions for conversations
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
    pub async fn available_move_to_actions<A>(
        view: Label,
        local_ids: Vec<LocalId>,
        interface: &A,
    ) -> Result<Vec<MoveAction>, AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        if local_ids.is_empty() {
            return Err(AppError::EmptyListOfConversations);
        }

        let all_system = Label::find_by_kind(LabelType::System, interface).await?;
        let all_system_excluding_view = all_system
            .iter()
            .filter(|label| label.local_id != view.local_id);
        let all_custom_folders = Label::find_by_kind(LabelType::Folder, interface).await?;
        let conversations = Conversation::find(
            format!(
                "WHERE local_id IN ({})",
                local_ids.iter().map(ToString::to_string).join(",")
            ),
            vec![],
            interface,
            None,
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
                    conversation.local_id.unwrap(),
                    view.name.clone(),
                ))
            }
        })?;

        let all_move_to_actions = MoveAction::vec(
            all_system_excluding_view
                .clone()
                .chain(all_custom_folders.iter()),
        );

        MoveAction::finalize(all_move_to_actions, interface).await
    }

    /// Finds all the messages from this conversation
    pub async fn load_messages<A>(&self, interface: &A) -> Result<Vec<Message>, StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        Message::find(
            "WHERE local_conversation_id == ? ORDER BY time ASC, display_order ASC",
            params![self.local_id.unwrap()],
            interface,
            None,
        )
        .await
    }

    /// Finds all the conversations that have expired and deletes them and all of its
    /// messages.
    pub async fn delete_expired<A>(interface: &A) -> Result<usize, AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let ids = Self::find_local_ids(
            r"
        WHERE
          expiration_time < STRFTIME('%s', 'NOW')
          AND expiration_time != 0
        ",
            vec![],
            interface,
        )
        .await?;

        let len = ids.len();

        if len != 0 {
            let label_id = SystemLabel::AllMail
                .local_id(interface)
                .await?
                .ok_or_else(|| StashError::IdNotSet)?;
            Self::mark_deleted(label_id, ids, interface).await?;
        }

        Ok(len)
    }

    #[cfg(test)]
    // TODO: Figure out how we want to do this in the future.
    ///
    /// Intended for testing only
    /// (local_attachment_id, local_message_id)
    /// Sets a conversation to be deleted in `expire_in` ms
    pub async fn set_expiration_time_in<A>(
        id: LocalId,
        expire_in: i64,
        db: &A,
    ) -> Result<(), StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let affected = db
            .execute(
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
            .await?;
        if affected != 1 {
            Err(StashError::Custom(String::from("No conversation found")))
        } else {
            Ok(())
        }
    }

    async fn check_has_label_and_is_unread<A>(
        local_label_id: LocalId,
        local_conversation_id: LocalId,
        interface: &A,
    ) -> Result<(bool, bool), StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        if let Some(label) = ConversationLabel::find_first(
            "WHERE local_conversation_id=? AND local_label_id=?",
            params![local_conversation_id, local_label_id],
            interface,
        )
        .await?
        {
            Ok((true, label.context_num_unread != 0))
        } else {
            Ok((false, false))
        }
    }

    /// Shared implementation to apply a label for messages and conversation.
    ///
    /// # Params
    ///
    /// * `local_label_id`         - Local label id of the [`Label`].
    /// * `local_conversation_id`  - Local conversation id to which the label
    ///                              should be applied.
    /// * `local_message_ids`      - Local ids of the messages which belong to
    ///                              `local_conversation_id` where the label
    ///                              should be applied.
    pub async fn label_impl<A>(
        local_label_id: LocalId,
        local_conversation_id: LocalId,
        local_message_ids: &[LocalId],
        interface: &A,
    ) -> Result<(), StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        if local_message_ids.is_empty() {
            return Ok(());
        }

        let (has_label, is_unread) = Conversation::check_has_label_and_is_unread(
            local_label_id,
            local_conversation_id,
            interface,
        )
        .await?;

        let stats = ConversationMessageLabelStats::with(
            local_conversation_id,
            local_label_id,
            local_message_ids,
            interface,
        )
        .await?;

        // Update conversation labels.
        let mut conversation_label = if let Some(mut label) = ConversationLabel::find_first(
            "WHERE local_conversation_id=? AND local_label_id=?",
            params![local_conversation_id, local_label_id],
            interface,
        )
        .await?
        {
            label.context_time = label.context_time.max(stats.time);
            label.context_snooze_time = label.context_snooze_time.max(stats.snooze_time);
            label.context_expiration_time =
                label.context_expiration_time.max(stats.expiration_time);
            label.context_size += stats.size;
            label.context_num_unread += stats.unread;
            label.context_num_attachments += stats.num_attachments as u64;
            label.context_num_messages += stats.count;
            label
        } else {
            let remote_label_id =
                if let Some(label) = Label::find_by_id(local_label_id, interface).await? {
                    label.remote_id
                } else {
                    None
                };
            ConversationLabel {
                local_id: None,
                local_conversation_id: Some(local_conversation_id),
                local_label_id: Some(local_label_id),
                remote_label_id,
                context_expiration_time: stats.expiration_time,
                context_num_attachments: stats.num_attachments as u64,
                context_num_messages: stats.count,
                context_num_unread: stats.unread,
                context_size: stats.size,
                context_snooze_time: stats.snooze_time,
                context_time: stats.time,
                deleted: false,
                row_id: None,
                stash: None,
            }
        };

        conversation_label.save_using(interface).await?;

        // Update message label counts.
        let Some(mut label) = Label::find_by_id(local_label_id, interface).await? else {
            error!("Could not find label");
            return Err(StashError::ExecutionError(SqliteError::QueryReturnedNoRows));
        };

        label.unread_msg += stats.unread;
        label.total_msg += stats.count;

        let should_increment_count = !has_label;
        let should_increment_unread = !is_unread && stats.unread != 0;

        label.total_conv += should_increment_count as u64;
        label.unread_conv += should_increment_unread as u64;

        label.save_using(interface).await?;

        Ok(())
    }

    /// Sync the conversation message for `local_conversation_id` from the server.
    ///
    /// The messages are only synced once if `has_messages` is not set to true.
    /// Future updates are expected to happen via the event loop.
    ///
    /// If `has_messages` is true, nothing is done.
    ///
    /// # Errors
    ///
    /// Returns error if the queries failed or if the server request failed.
    pub async fn sync_conversation_messages<A, PM>(
        local_conversation_id: LocalId,
        interface: &A,
        api: &PM,
    ) -> Result<(), AppError>
    where
        PM: ProtonMail,
        A: Into<AgnosticInterface> + Interface,
    {
        let Some(conversation) = Self::find_by_id(local_conversation_id, interface).await? else {
            return Err(AppError::ConversationNotFound(local_conversation_id));
        };

        if !conversation.has_messages {
            let Some(rid) = conversation.remote_id else {
                return Err(AppError::LabelDoesNotHaveRemoteId(local_conversation_id));
            };
            debug!("Syncing conversation messages");
            let conversation_response = api.get_conversation(rid.into()).await.map_err(|e| {
                error!("failed to download conversation messages: {e}");
                AppError::from(e)
            })?;

            let tx = interface.transaction().await?;

            let message_metadata: Vec<ApiMessageMetadata> = conversation_response
                .messages
                .into_iter()
                .map(Into::into)
                .collect();
            let mut new_conversation: Conversation = conversation_response.conversation.into();

            Message::create_or_update_messages_from_metadata(message_metadata, &tx)
                .await
                .map_err(|e| {
                    error!("Failed to write message metadata: {e}");
                    e
                })?;

            new_conversation.local_id = conversation.local_id;
            new_conversation.row_id = conversation.row_id;
            new_conversation.has_messages = true;

            new_conversation.save_using(&tx).await.map_err(|e| {
                error!("Failed to write conversation: {e}");
                e
            })?;

            tx.commit().await?;
        } else {
            debug!("Conversation messages already synced")
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
    pub async fn in_label<A>(
        local_label_id: LocalId,
        interface: &A,
        queue: Option<flume::Sender<ResultsetChange<Self, <Self as Model>::IdType>>>,
    ) -> Result<Vec<Self>, StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        Conversation::find(
            formatdoc!(
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
            interface,
            queue,
        )
        .await
    }

    /// Create a paginator for conversations in a given label.
    ///
    /// # Params
    ///
    /// * `context`        - Active user context.
    /// * `local_label_id` - Label to paginate in.
    /// * `page_count`     - Number of elements per page.
    /// * `filter`         - Filter options for pagination.
    /// * `local_first`    - Load local data immediately, to return to the
    ///                      caller without the delay of remote lookup. If set
    ///                      to `false`, no results will be returned until the
    ///                      remote API calls have completed. This only affects
    ///                      the first call to the paginator.
    /// * `queue`          - Optional subscriber for changes.
    ///
    /// # Errors
    ///
    /// Returns error if the query fails.
    ///
    pub async fn paginate_in_label(
        context: &MailUserContext,
        local_label_id: LocalId,
        page_count: u32,
        filter: PaginatorFilter,
        local_first: bool,
        queue: Option<flume::Sender<ResultsetChange<Self, <Self as Model>::IdType>>>,
    ) -> Result<PaginatorCompat<Self, ConversationDataSource>, AppError> {
        let remote_source =
            ConversationDataSource::new(context, local_label_id, filter.clone()).await?;

        let mut query = formatdoc!(
            "
            JOIN conversation_labels
                ON conversations.local_id = conversation_labels.local_conversation_id
            WHERE
                conversation_labels.local_label_id = ?
            AND
                conversation_labels.deleted = 0
            "
        );

        let params = vec![Param::Integer(
            i64::try_from(local_label_id.as_u64()).map_err(|err| {
                StashError::ExecutionError(SqliteError::ToSqlConversionFailure(Box::new(err)))
            })?,
        )];

        if let Some(unread) = filter.unread {
            query += &format!(
                "AND conversation_labels.context_num_unread {} 0 ",
                if unread { ">" } else { "=" }
            );
        }

        query += "ORDER BY
            conversation_labels.context_time DESC,
            conversations.display_order DESC
        ";

        Ok(PaginatorCompat::new(
            Paginator::new(
                query,
                params,
                context.user_stash(),
                NonZeroU32::new(page_count)
                    .ok_or(StashError::Custom("Invalid Page Count value".to_owned()))?,
                remote_source,
                local_first,
                queue,
            )
            .await?,
        ))
    }
    /// This fn should be called for conversation endpoints.
    /// Repeatedly calls `endpoint` in batches of 1 in parallel.
    async fn split_request<F, Fut>(
        ids: impl IntoIterator<Item = RemoteId>,
        endpoint: F,
    ) -> Result<Vec<OperationResult>, ApiServiceError>
    where
        F: Fn(Vec<ApiRemoteId>) -> Fut,
        Fut: Future<Output = Result<Vec<OperationResult>, ApiServiceError>>,
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
    pub async fn next_display_order<A>(interface: &A) -> Result<u64, StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        Ok(interface
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

    /// Only get Disposition::Inline attachments
    #[allow(dead_code)] // Will get used later on
    fn get_inline_attachment_metadata(&self) -> Vec<AttachmentMetadata> {
        self.attachments_metadata
            .iter()
            .filter(|mdata| matches!(mdata.disposition, Disposition::Inline))
            .cloned()
            .collect()
    }
}

impl From<ApiConversation> for Conversation {
    fn from(value: ApiConversation) -> Self {
        Self {
            local_id: None,
            remote_id: Some(value.id.into()),
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
            display_snooze_reminder: value.display_snooze_reminder,
            expiration_time: value.expiration_time,
            exclusive_location: None,
            labels: value.labels.into_iter().map(|v| v.into()).collect(),
            num_attachments: value.num_attachments,
            num_messages: value.num_messages,
            num_unread: value.num_unread,
            display_order: value.order,
            recipients: MessageAddresses {
                value: value.recipients.into_iter().map(|v| v.into()).collect(),
            },
            senders: MessageAddresses {
                value: value.senders.into_iter().map(|v| v.into()).collect(),
            },
            custom_labels: vec![],
            size: value.size,
            subject: value.subject,
            row_id: None,
            stash: None,
            is_known: true,
            has_messages: false,
        }
    }
}

/// Contextual label metadata associated with a Conversation.
///
/// When a conversation is opened in the context of label, the
/// [`ConversationLabel`] information is superimposed over the [`Conversation`]
/// for that context.
///
#[derive(Clone, Debug, Default, Eq, Model, PartialEq)]
#[TableName("conversation_labels")]
pub struct ConversationLabel {
    /// The local ID of the record, i.e. the ID assigned by the client
    /// application. This is a restricted-scope unique identifier for the record
    /// within the set of all records of this type, and is important for
    /// relating local records. It has no relationship to the centrally-stored
    /// API ID, and never leaves the local system.
    #[IdField(autoincrement)]
    pub local_id: Option<LocalId>,

    /// TODO: Document this field.
    #[DbField]
    pub local_conversation_id: Option<LocalId>,

    /// TODO: Document this field.
    #[DbField]
    pub local_label_id: Option<LocalId>,

    /// TODO: Document this field.
    #[DbField]
    pub remote_label_id: Option<LabelId>,

    /// TODO: Document this field.
    #[DbField]
    pub context_expiration_time: u64,

    /// TODO: Document this field.
    #[DbField]
    pub context_num_attachments: u64,

    /// TODO: Document this field.
    #[DbField]
    pub context_num_messages: u64,

    /// TODO: Document this field.
    #[DbField]
    pub context_num_unread: u64,

    /// TODO: Document this field.
    #[DbField]
    pub context_size: u64,

    /// TODO: Document this field.
    #[DbField]
    pub context_snooze_time: u64,

    /// TODO: Document this field.
    #[DbField]
    pub context_time: u64,

    #[DbField]
    pub deleted: bool,

    #[allow(clippy::doc_markdown)]
    /// The internal row ID of the record in the database. This is assigned by
    /// SQLite, and is used as a consistent identifier for records when
    /// listening for change notifications.
    #[RowIdField]
    pub row_id: Option<u64>,

    /// The database instance that the record is associated with. This is
    /// present for convenience.
    #[StashField]
    pub stash: Option<Stash>,
}

impl ConversationLabel {
    /// Get all local label ids for a given `conversation_id`.
    ///
    /// # Errors
    ///
    /// Returns error if the query failed.
    pub async fn labels_ids_for_conversation<A>(
        conversation_id: LocalId,
        interface: &A,
    ) -> Result<Vec<LocalId>, StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let query = format!(
            "SELECT local_label_id as value FROM {} WHERE local_conversation_id = ?",
            Self::table_name()
        );

        interface
            .query_values::<_, LocalId>(&query, params![conversation_id])
            .await
    }

    /// Get all local label with given IDs.
    ///
    /// # Parameters
    ///
    /// * `label_ids` - List of ids we want to find the corresponding `ConversationLabel`.
    /// * `interface` - The database interface.
    ///
    /// # Errors
    ///
    /// Returns an error if the query failed.
    ///
    pub async fn find_by_ids<A>(
        label_ids: impl IntoIterator<Item = LocalId>,
        interface: &A,
    ) -> Result<Vec<Self>, StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        ConversationLabel::find(
            format!("WHERE local_id IN ({})", label_ids.into_iter().join(", ")),
            vec![],
            interface,
            None,
        )
        .await
    }

    /// Save or update a Conversation Label.
    ///
    /// It's imperative that you use this method over [`Model::save()`] to
    /// ensure that the information is update correctly in the database.
    ///
    /// The current stash database does not allow us to resolve conflicts on
    /// other unique keys so we have to do this ourselves.
    /// If [`Model::save()`] is used directly it will bypass this check.
    ///
    /// # Errors
    ///
    /// Returns error if the local conversation id is not set, the remote
    /// label_id is not set, the local label can not be found or the query
    /// failed.
    pub async fn save(&mut self) -> Result<(), StashError> {
        let Some(stash) = self.stash.clone() else {
            return Err(StashError::NoStashAvailable);
        };

        self.save_using(&stash).await
    }

    /// Save or update a Conversation Label.
    ///
    /// It's imperative that you use this method over [`Model::save_using()`] to
    /// ensure that the information is update correctly in the database.
    ///
    /// The current stash database does not allow us to resolve conflicts on
    /// other unique keys so we have to do this ourselves.
    /// If [`Model::save_using()`] is used directly it will bypass this check.
    ///
    /// # Errors
    ///
    /// Returns error if the local conversation id is not set, the remote
    /// label_id is not set, the local label can not be found or the query
    /// failed.
    pub async fn save_using<A>(&mut self, interface: &A) -> Result<(), StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let Some(local_conversation_id) = self.local_conversation_id else {
            return Err(StashError::Custom(
                "Missing local conversation id".to_owned(),
            ));
        };

        let Some(remote_label_id) = self.remote_label_id.clone() else {
            return Err(StashError::Custom("Missing remote label id".to_owned()));
        };

        let Some(local_label) =
            Label::find_by_id(RemoteId::from(remote_label_id.clone()), interface).await?
        else {
            return Err(StashError::Custom(format!(
                "Can't find label with the remote id {remote_label_id}"
            )));
        };

        self.local_label_id = local_label.local_id;

        if let Some(label) = ConversationLabel::find_first(
            "WHERE local_label_id=? AND local_conversation_id=?",
            params![
                local_label.local_id.expect("Should be set"),
                local_conversation_id
            ],
            interface,
        )
        .await?
        {
            self.local_id = label.local_id;
            self.row_id = label.row_id;
        }

        <Self as Model>::save_using(self, interface).await
    }

    /// Adjust the stats of the conversation label when
    /// a message is marked as deleted.
    ///
    /// ## Parameters
    ///
    /// * `stats` - The stats of the message that was deleted.
    /// * `interface` - The interface to use for the database connection.
    ///
    /// ## Errors
    ///
    /// Returns error if the query fails.
    ///
    pub async fn mark_delete_update_stats<A>(
        &mut self,
        stats: Option<&MessageLabelStats>,
        interface: &A,
    ) -> Result<(), AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        if let Some(stats) = stats {
            self.context_num_messages = self.context_num_messages.saturating_sub(stats.count);
            self.context_num_unread = self.context_num_unread.saturating_sub(stats.unread_count);
            self.context_num_attachments = self
                .context_num_attachments
                .saturating_sub(stats.attachment_count);
            self.context_size = self.context_size.saturating_sub(stats.size);
            self.save_using(interface).await?;
        }

        Ok(())
    }

    /// Adjust the stats of the conversation label when
    /// a message is marked as undeleted.
    ///
    /// ## Parameters
    ///
    /// * `stats` - The stats of the message that was undeleted.
    /// * `interface` - The interface to use for the database connection.
    ///
    /// ## Errors
    ///
    /// Returns error if the query fails.
    ///
    pub async fn mark_undelete_update_stats<A>(
        &mut self,
        stats: Option<&MessageLabelStats>,
        interface: &A,
    ) -> Result<(), AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        if let Some(stats) = stats {
            self.context_num_messages += stats.count;
            self.context_num_unread += stats.unread_count;
            self.context_num_attachments += stats.attachment_count;
            self.context_size += stats.size;
            self.save_using(interface).await?;
        }

        Ok(())
    }
}

impl From<ApiConversationLabel> for ConversationLabel {
    fn from(value: ApiConversationLabel) -> Self {
        Self {
            local_id: None,
            local_conversation_id: None,
            local_label_id: None,
            remote_label_id: Some(value.id.into()),
            context_expiration_time: value.context_expiration_time,
            context_num_attachments: value.context_num_attachments,
            context_num_messages: value.context_num_messages,
            context_num_unread: value.context_num_unread,
            context_size: value.context_size,
            context_snooze_time: value.context_snooze_time,
            context_time: value.context_time,
            deleted: false,
            row_id: None,
            stash: None,
        }
    }
}

/// Calculates the combined information for a list of message that belong to a given
/// conversation and a given label.
pub struct ConversationMessageLabelStats {
    pub size: u64,
    pub time: u64,
    pub expiration_time: u64,
    pub count: u64,
    pub unread: u64,
    pub num_attachments: u32,
    pub snooze_time: u64,
}

impl ConversationMessageLabelStats {
    /// Get stats about for a conversation with `conversation_id` with the
    /// given `message_ids` for a label with `label_id`.
    async fn with<A>(
        conversation_id: LocalId,
        label_id: LocalId,
        message_ids: &[LocalId],
        interface: &A,
    ) -> Result<Self, StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let params = [label_id, conversation_id]
            .into_iter()
            .chain(message_ids.iter().cloned())
            .map(|v| -> Box<dyn ToSql + Send> { Box::new(v) })
            .collect();
        let messages = Message::find(format!(indoc! {"
                JOIN message_labels AS ML ON ML.local_message_id = messages.local_id AND ML.local_label_id = ?
                WHERE messages.local_conversation_id = ? AND messages.local_id IN ({})
            "}, vec!["?"; message_ids.len()].join(",")),
                                     params, interface, None).await?;

        if messages.is_empty() {
            return Err(StashError::ExecutionError(SqliteError::QueryReturnedNoRows));
        }

        Ok(Self::from_messages(&messages))
    }

    /// Get stats about for a conversation with `conversation_id` for all the
    /// message that do not match the given `message_ids` for a label with
    /// `label_id`.
    pub async fn without<A>(
        conversation_id: LocalId,
        label_id: LocalId,
        message_ids: &[LocalId],
        interface: &A,
    ) -> Result<Self, StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let params = [label_id, conversation_id]
            .into_iter()
            .chain(message_ids.iter().cloned())
            .map(|v| -> Box<dyn ToSql + Send> { Box::new(v) })
            .collect();
        let messages = Message::find(format!(indoc! {"
                JOIN message_labels AS ML ON ML.local_message_id = messages.local_id AND ML.local_label_id = ?
                WHERE messages.local_conversation_id = ? AND messages.local_id NOT IN ({})
            "}, vec!["?"; message_ids.len()].join(",")),
                                     params, interface, None).await?;

        if messages.is_empty() {
            return Err(StashError::ExecutionError(SqliteError::QueryReturnedNoRows));
        }

        Ok(Self::from_messages(&messages))
    }

    fn from_messages(messages: &[Message]) -> Self {
        let mut stats = Self {
            size: 0,
            time: 0,
            expiration_time: 0,
            count: 0,
            unread: 0,
            num_attachments: 0,
            snooze_time: 0,
        };

        for message in messages {
            stats.size += message.size;
            stats.time = stats.time.max(message.time);
            stats.expiration_time = stats.expiration_time.max(message.expiration_time);
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

/// A data source for a [`Paginator`] which syncs pages of [`Message`]s in
/// a [`Label`].
pub struct ConversationDataSource {
    /// Session for network request
    session: Session,

    /// Remote id of the label.
    remote_label_id: LabelId,

    /// Local id of the label.
    local_label_id: LocalId,

    /// Filter options for pagination.
    filter: PaginatorFilter,
}

impl ConversationDataSource {
    /// Create a new data source for the given `label_id`.
    ///
    /// # Parameters
    ///
    /// * `context`  - Active user context.
    /// * `label_id` - Local id of the label.
    /// * `filter`   - Filter options for pagination.
    ///
    /// # Errors
    ///
    /// Returns error if the remote id for the label can't be resolved.
    ///
    pub async fn new(
        context: &MailUserContext,
        label_id: LocalId,
        filter: PaginatorFilter,
    ) -> Result<Self, AppError> {
        let Some(remote_id) = label_id
            .counterpart::<Label, _>(context.user_stash())
            .await?
        else {
            return Err(AppError::LabelDoesNotHaveRemoteId(label_id));
        };

        Ok(Self {
            remote_label_id: remote_id.into(),
            session: context.session().clone(),
            local_label_id: label_id,
            filter,
        })
    }
}

impl DataSource for ConversationDataSource {
    type Item = Conversation;
    type Error = AppError;

    #[tracing::instrument(level=tracing::Level::DEBUG,skip(self, stash))]
    async fn total(&self, stash: &Stash) -> Result<usize, Self::Error> {
        let label = Label::find_by_id(self.local_label_id, stash)
            .await?
            .ok_or(AppError::LabelNotFound(self.local_label_id))?;
        debug!("Total conversations: {}", label.total_conv);
        Ok(label.total_conv.try_into().unwrap_or(0))
    }

    #[tracing::instrument(level=tracing::Level::DEBUG,skip(self))]
    async fn sync_first_page(
        &self,
        page_size: NonZeroU32,
        stash: &Stash,
    ) -> Result<Vec<Self::Item>, Self::Error> {
        let response = self
            .session
            .api()
            .get_conversations(GetConversationsOptions {
                desc: Some(true),
                label_id: Some(self.remote_label_id.clone().into()),
                page_size: page_size.get() as u64,
                unread: self.filter.unread,
                ..Default::default()
            })
            .await?;
        debug!(
            "Fetched {} conversations. Total={}",
            response.conversations.len(),
            response.total
        );
        Ok(self
            .save_to_database(
                response.conversations.into_iter().map_into().collect(),
                stash,
            )
            .await?)
    }

    #[tracing::instrument(level=tracing::Level::DEBUG,skip(self, elements))]
    async fn sync_page_after(
        &self,
        _: u32,
        page_size: NonZeroU32,
        elements: Vec<Self::Item>,
        stash: &Stash,
    ) -> Result<Vec<Self::Item>, Self::Error> {
        if elements.is_empty() {
            warn!("No element to sync");
            return Ok(vec![]);
        }

        // Find the first last element with a valid remote id.
        let Some(last_element) = elements
            .iter()
            .rev()
            .find(|element| element.remote_id.is_some())
        else {
            return Err(AppError::NoMessageWithValidRemoteIdFoundInPage);
        };
        // Safe to unwrap as we have validated this before.
        let last_element_id: proton_api_core::services::proton::common::RemoteId =
            last_element.remote_id.clone().unwrap().into();

        debug!("Last Element= {last_element_id}");

        let Some(last_element_time) = last_element
            .labels
            .iter()
            .find(|l| l.local_label_id.unwrap() == self.local_label_id)
            .map(|v| v.context_time)
        else {
            return Err(AppError::Other(anyhow!(
                "Conversation does not have active label"
            )));
        };

        let mut response = self
            .session
            .api()
            .get_conversations(GetConversationsOptions {
                desc: Some(true),
                end: Some(last_element_time),
                end_id: Some(last_element_id.clone()),
                label_id: Some(self.remote_label_id.clone().into()),
                page_size: page_size.get() as u64 + 1_u64,
                unread: self.filter.unread,
                ..Default::default()
            })
            .await?;
        debug!(
            "Fetched {} conversations. Total={}",
            response.conversations.len(),
            response.total
        );

        // `end_id` always returns the given conversation in the search results
        // if it exists.
        if response.conversations.is_empty() {
            return Ok(vec![]);
        }

        if response.conversations[0].id == last_element_id {
            response.conversations.remove(0);
        } else if response.conversations.len() > page_size.get() as usize {
            response.conversations.pop();
        }

        if response.conversations.is_empty() {
            return Ok(vec![]);
        }

        Ok(self
            .save_to_database(
                response.conversations.into_iter().map_into().collect(),
                stash,
            )
            .await?)
    }
}

impl ConversationDataSource {
    async fn save_to_database(
        &self,
        mut records: Vec<Conversation>,
        stash: &Stash,
    ) -> Result<Vec<Conversation>, StashError> {
        let tx = stash.transaction().await?;
        for record in &mut records {
            Conversation::save_using(record, &tx).await?;
        }
        tx.commit().await?;
        Ok(records)
    }
}
