use crate::actions::conversations::label_as::Handler as LabelAsHandler;
use crate::actions::conversations::LabelAs;
use crate::actions::conversations::{Label as ActionLabel, MarkRead, MarkUnread, Move, Unlabel};
use crate::actions::{
    filter_responses, AllBottomBarMessageActions, BottomBarActions, MovableSystemFolderAction,
};
use crate::datatypes::{ExclusiveLocation, LabelType, MobileActions, SystemLabelId};
use crate::find_in_query;
use crate::models::Label;
use crate::{actions::conversations::Delete, models::Conversation, AppError};
use anyhow::anyhow;
use itertools::Itertools;
use proton_action_queue::queue::{ActionError, ActionOutput, Queue};
use proton_api_core::session::{CoreSession, Session};
use proton_api_mail::services::proton::ProtonMail;
use proton_core_common::datatypes::{Id, LabelId, LocalId, RemoteId};
use stash::orm::Model;
use stash::stash::{AgnosticInterface, Interface, StashError};
use std::collections::{HashMap, HashSet};
use tracing::{error, warn};

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
        session: &Session,
        queue: &Queue,
        label_id: LocalId,
        conversation_ids: Vec<LocalId>,
    ) -> Result<ActionOutput<ActionLabel>, ActionError<ActionLabel>> {
        let action = ActionLabel::new(label_id, conversation_ids);
        queue.apply_action(session, action).await
    }

    /// Star multiple conversations.
    ///
    /// # Parameters
    ///
    /// * `session`          - The session.
    /// * `queue`            - The action queue.
    /// * `conversation_ids` - The IDs of the conversations to star.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed.
    ///
    pub async fn action_star(
        session: &Session,
        queue: &Queue,
        conversation_ids: Vec<LocalId>,
    ) -> Result<ActionOutput<ActionLabel>, ActionError<ActionLabel>> {
        let label_id = LabelId::starred()
            .counterpart::<crate::models::Label, _>(queue.stash())
            .await
            .map_err(|e| ActionError::Queue(e.into()))?
            .expect("Star system label not found");
        let action = ActionLabel::new(label_id, conversation_ids.into_iter().map_into());
        queue.apply_action(session, action).await
    }

    /// Unstar multiple conversations.
    ///
    /// # Parameters
    ///
    /// * `session`          - The session.
    /// * `queue`            - The action queue.
    /// * `conversation_ids` - The IDs of the conversations to unstar.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed.
    ///
    pub async fn action_unstar(
        session: &Session,
        queue: &Queue,
        conversation_ids: Vec<LocalId>,
    ) -> Result<ActionOutput<Unlabel>, ActionError<Unlabel>> {
        let label_id = LabelId::starred()
            .counterpart::<crate::models::Label, _>(queue.stash())
            .await?
            .expect("Star system label not found");
        let action = Unlabel::new(label_id, conversation_ids.into_iter().map_into());
        queue.apply_action(session, action).await
    }

    /// Unlabel multiple conversations.
    ///
    /// # Parameters
    ///
    /// * `session`          - The session.
    /// * `queue`            - The action queue.
    /// * `label_id`         - The ID of the label to apply to the conversations.
    /// * `conversation_ids` - The IDs of the conversations to unlabel.
    ///
    /// # Errors
    ///
    /// Returns an error if the action failed.
    ///
    pub async fn action_remove_label(
        session: &Session,
        queue: &Queue,
        label_id: LocalId,
        conversation_ids: Vec<LocalId>,
    ) -> Result<ActionOutput<Unlabel>, ActionError<Unlabel>> {
        let action = Unlabel::new(label_id, conversation_ids.into_iter().map_into());
        queue.apply_action(session, action).await
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
        session: &Session,
        queue: &Queue,
        label_id: LocalId,
        conversation_ids: Vec<LocalId>,
    ) -> Result<ActionOutput<MarkRead>, ActionError<MarkRead>> {
        let action = MarkRead::new(label_id, conversation_ids);
        queue.apply_action(session, action).await
    }

    /// Mark multiple conversations as unread.
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
    pub async fn action_mark_unread(
        session: &Session,
        queue: &Queue,
        label_id: LocalId,
        conversation_ids: Vec<LocalId>,
    ) -> Result<ActionOutput<MarkUnread>, ActionError<MarkUnread>> {
        let action = MarkUnread::new(label_id, conversation_ids);
        queue.apply_action(session, action).await
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
    pub async fn action_delete(
        session: &Session,
        queue: &Queue,
        label_id: LocalId,
        conversation_ids: Vec<LocalId>,
    ) -> Result<ActionOutput<Delete>, ActionError<Delete>> {
        let action = Delete::new(label_id, conversation_ids);
        queue.apply_action(session, action).await
    }

    /// Move multiple conversations.
    ///
    /// # Parameters
    ///
    /// * `session`        - The session.
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
        session: &Session,
        queue: &Queue,
        source_id: LocalId,
        destination_id: LocalId,
        target_ids: Vec<LocalId>,
    ) -> Result<ActionOutput<Move>, ActionError<Move>> {
        let action = Move::new(source_id, destination_id, target_ids);
        queue.apply_action(session, action).await
    }

    /// Soft delete multiple conversations.
    ///
    /// # Parameters
    ///
    /// * `session`          - The session.
    /// * `queue`            - The action queue.
    /// * `label_id`         - The ID of the current view.
    /// * `conversation_ids` - The IDs of the converstations to delete.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed.
    ///
    pub async fn action_mark_deleted(
        session: &Session,
        queue: &Queue,
        label_id: LocalId,
        conversation_ids: impl IntoIterator<Item = LocalId>,
    ) -> Result<ActionOutput<Delete>, ActionError<Delete>> {
        let action = Delete::new(label_id, conversation_ids);
        queue.apply_action(session, action).await
    }

    /// Action to change labels on a batch of conversations.
    ///
    /// All given conversations will get the selected labels.
    /// All given conversations will keep the partially selected labels.
    /// All given conversations will lose any other labels.
    ///
    /// # Parameters
    ///
    /// * `session`                      - The session.
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
        session: &Session,
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
            .apply_action(session, action)
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
        let messages = Self::find_by_ids(conversation_ids.to_vec(), interface).await?;
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
}
