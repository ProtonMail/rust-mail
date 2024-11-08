use super::MessageBodyMetadata;
use crate::actions::messages::delete::Delete;
use crate::actions::messages::label::Label as ActionLabel;
use crate::actions::messages::label_as::LabelAs;
use crate::actions::messages::r#move::Move;
use crate::actions::messages::read::Read;
use crate::actions::messages::unlabel::Unlabel;
use crate::actions::messages::unread::Unread;
use crate::actions::{AllBottomBarMessageActions, BottomBarActions, MovableSystemFolderAction};
use crate::datatypes::{Disposition, ExclusiveLocation, LabelType, MobileActions, SystemLabelId};
use crate::models::{Label, Message};
use crate::{find_in_query, Mailbox};
use crate::{AppError, MailboxError};
use anyhow::anyhow;
use itertools::Itertools as _;
use proton_action_queue::queue::{ActionError, ActionOutput, ActionRemoteOutput, Queue};
use proton_api_core::session::{CoreSession, Session};
use proton_api_mail::services::proton::ProtonMail;
use proton_core_common::datatypes::{Id, LabelId, LocalId, RemoteId};
use proton_sqlite3::rusqlite::ToSql;
use stash::orm::Model;
use stash::stash::{AgnosticInterface, Interface, StashError};
use std::collections::{HashMap, HashSet};
use tracing::{error, warn};

impl Message {
    /// Label multiple messages.
    ///
    /// # Parameters
    ///
    /// * `session`     - The session.
    /// * `queue`       - The action queue.
    /// * `label_id`    - The ID of the label to apply to the messages.
    /// * `message_ids` - The IDs of the messages to label.
    ///
    /// # Errors
    ///
    /// Returns an error if the action failed.
    ///
    pub async fn action_apply_label(
        session: &Session,
        queue: &Queue,
        label_id: LocalId,
        message_ids: Vec<LocalId>,
    ) -> Result<ActionOutput<ActionLabel>, ActionError<ActionLabel>> {
        let action = ActionLabel::new(label_id, message_ids.into_iter().map_into());
        queue.apply_action(session, action).await
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
        session: &Session,
        queue: &Queue,
        message_ids: Vec<LocalId>,
    ) -> Result<ActionOutput<ActionLabel>, ActionError<ActionLabel>> {
        let label_id = LabelId::starred()
            .counterpart::<crate::models::Label, _>(queue.stash())
            .await
            .map_err(|e| ActionError::Queue(e.into()))?
            .expect("Star system label not found");
        let action = ActionLabel::new(label_id, message_ids.into_iter().map_into());
        queue.apply_action(session, action).await
    }

    /// Unstar multiple messages.
    ///
    /// # Parameters
    ///
    /// * `session`     - The session.
    /// * `queue`       - The action queue.
    /// * `message_ids` - The IDs of the messages to unstar.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed.
    ///
    pub async fn action_unstar(
        session: &Session,
        queue: &Queue,
        message_ids: Vec<LocalId>,
    ) -> Result<ActionOutput<Unlabel>, ActionError<Unlabel>> {
        let label_id = LabelId::starred()
            .counterpart::<crate::models::Label, _>(queue.stash())
            .await?
            .expect("Star system label not found");
        let action = Unlabel::new(label_id, message_ids.into_iter().map_into());
        queue.apply_action(session, action).await
    }

    /// Unlabel multiple messages.
    ///
    /// # Parameters
    ///
    /// * `session`     - The session.
    /// * `queue`       - The action queue.
    /// * `label_id`    - The ID of the label to apply to the messages.
    /// * `message_ids` - The IDs of the messages to unlabel.
    ///
    /// # Errors
    ///
    /// Returns an error if the action failed.
    ///
    pub async fn action_remove_label(
        session: &Session,
        queue: &Queue,
        label_id: LocalId,
        message_ids: Vec<LocalId>,
    ) -> Result<ActionOutput<Unlabel>, ActionError<Unlabel>> {
        let action = Unlabel::new(label_id, message_ids.into_iter().map_into());
        queue.apply_action(session, action).await
    }

    /// Mark multiple messages as read.
    ///
    /// # Parameters
    ///
    /// * `session`     - The session.
    /// * `queue`       - The action queue.
    /// * `label_id`    - The ID of the label to apply to the messages.
    /// * `message_ids` - The IDs of the target messages.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed.
    ///
    pub async fn action_mark_read(
        session: &Session,
        queue: &Queue,
        label_id: LocalId,
        message_ids: Vec<LocalId>,
    ) -> Result<ActionOutput<Read>, ActionError<Read>> {
        let action = Read::new(label_id, message_ids);
        queue.apply_action(session, action).await
    }

    /// Mark multiple messages as unread.
    ///
    /// # Parameters
    ///
    /// * `session`     - The session.
    /// * `queue`       - The action queue.
    /// * `label_id`    - The ID of the label to apply to the messages.
    /// * `message_ids` - The IDs of the target messages.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed.
    ///
    pub async fn action_mark_unread(
        session: &Session,
        queue: &Queue,
        label_id: LocalId,
        message_ids: Vec<LocalId>,
    ) -> Result<ActionOutput<Unread>, ActionError<Unread>> {
        let action = Unread::new(label_id, message_ids);
        queue.apply_action(session, action).await
    }

    /// Mark multiple messages as read.
    ///
    /// # Parameters
    ///
    /// * `session`     - The session.
    /// * `queue`       - The action queue.
    /// * `label_id`    - The ID of the label to apply to the messages.
    /// * `message_ids` - The IDs of the target messages.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request failed.
    ///
    pub async fn action_delete(
        session: &Session,
        queue: &Queue,
        label_id: LocalId,
        message_ids: Vec<LocalId>,
    ) -> Result<ActionOutput<Delete>, ActionError<Delete>> {
        let action = Delete::new(label_id, message_ids);
        queue.apply_action(session, action).await
    }

    /// Move multiple messages.
    ///
    /// # Parameters
    ///
    /// * `session`        - The session.
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
        session: &Session,
        queue: &Queue,
        source_id: LocalId,
        destination_id: LocalId,
        target_ids: Vec<LocalId>,
    ) -> Result<ActionOutput<Move>, ActionError<Move>> {
        let action = Move::new(source_id, destination_id, target_ids);
        queue.apply_action(session, action).await
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
    pub async fn mark_multiple_as_read<A>(
        ids: Vec<LocalId>,
        interface: &A,
    ) -> Result<(), StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        for id in ids {
            if let Some(mut message) = Message::load(id, interface).await? {
                message.unread = false;
                message.save_using(interface).await?;
            }
        }
        Ok(())
    }

    /// Remove all removable labels from given messages.
    ///
    /// N.B.: `all_mail` label is the only not removable label.
    async fn remove_all_labels<A>(
        message_ids: Vec<LocalId>,
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
            "DELETE FROM message_labels WHERE local_message_id in ({}) AND local_label_id != ?",
            message_ids
        );
        parameters.push(Box::new(all_mail_id) as Box<dyn ToSql + Send>);

        interface.execute(query, parameters).await?;
        Ok(())
    }

    /// Move messages between two labels.
    ///
    /// # Parameters
    /// * `source_id`      - Local label id where the messages currently are.
    /// * `destination_id` - Local label id where the messages should be moved.
    /// * `message_ids`    - The IDs of the conversations to move.
    /// * `interface`      - A tether or a stash to use for the database connection.
    ///
    /// # Remarks
    ///
    /// This function can only be called with an active transaction.
    ///
    /// # Errors
    ///
    /// Returns errors if the operation failed.
    pub async fn move_messages<A>(
        source_id: LocalId,
        destination_id: LocalId,
        message_ids: Vec<LocalId>,
        interface: &A,
    ) -> Result<(), AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let remote_source_id = Label::resolve_remote_label_id(source_id, interface).await?;
        let remote_destination_id =
            Label::resolve_remote_label_id(destination_id, interface).await?;

        // If moving to trash, mark targets as read.
        if remote_destination_id == LabelId::trash() {
            Message::mark_multiple_as_read(message_ids.to_vec(), interface)
                .await
                .inspect_err(|e| {
                    error!("Failed to mark messages as read when moving to trash: {e}")
                })?;
        }

        // When moving in Trash or Spam, remove all labels (but AllMail)
        if remote_destination_id == LabelId::trash() || remote_destination_id == LabelId::spam() {
            Message::remove_all_labels(message_ids.to_vec(), interface)
                .await
                .inspect_err(|e| error!("Failed to remove labels: {e}"))?;
        } else if remote_source_id == LabelId::trash() || remote_source_id == LabelId::spam() {
            // When moving out of Trash or Spam, add AlmostAllMail label
            let almost_all_mail =
                Label::resolve_local_label_id(LabelId::almost_all_mail(), interface).await?;
            Message::apply_label(almost_all_mail, message_ids.to_vec(), interface)
                .await
                .inspect_err(|e| error!("Failed to add messages to almost_all_mail when moving out of spam/trash: {e}"))?;
        }

        let Some(source) = Label::load(source_id, interface).await? else {
            return Err(AppError::LabelNotFound(source_id));
        };
        if source.is_movable_folder() {
            Message::remove_label(source_id, message_ids.to_vec(), interface)
                .await
                .inspect_err(|e| error!("Failed to remove source label from messages: {e}"))?;
        }

        Message::apply_label(destination_id, message_ids.to_vec(), interface)
            .await
            .inspect_err(|e| error!("Failed to apply destination label to messages: {e}"))?;

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
    pub async fn label_as<A>(
        source_label_id: LocalId,
        message_ids: Vec<LocalId>,
        selected_label_ids: &[LocalId],
        partially_selected_label_ids: &[LocalId],
        all_label_ids: &[LocalId],
        must_archive: bool,
        interface: &A,
    ) -> Result<(), AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        for label_id in all_label_ids {
            if selected_label_ids.contains(label_id) {
                Self::apply_label(*label_id, message_ids.clone(), interface).await?
            } else if !partially_selected_label_ids.contains(label_id) {
                Self::remove_label(*label_id, message_ids.clone(), interface).await?
            }
            // else keep label as is
        }

        if must_archive {
            let archive_id =
                RemoteId::counterpart::<Label, _>(&LabelId::archive().into_inner(), interface)
                    .await?
                    .expect("Archive label must have a RemoteId");
            Self::move_messages(source_label_id, archive_id, message_ids, interface).await?;
        }
        Ok(())
    }

    /// Compute which labels must be set
    ///
    /// # Parameters
    /// * `current_labels`               - Labels currently set.
    /// * `selected_labels_ids`          - Ids of the wanted label.
    /// * `partially_selected_label_ids` - Ids of the label that should be kept as his.
    ///
    fn compute_expected_labels(
        current_labels: &[Label],
        selected_label_ids: &[LocalId],
        partially_selected_label_ids: &[LocalId],
    ) -> Vec<LocalId> {
        let current_labels: HashSet<LocalId> = HashSet::from_iter(
            current_labels
                .iter()
                .map(|l| l.local_id.expect("Should be set")),
        );
        let selected_label_ids = HashSet::from_iter(selected_label_ids.iter().cloned());
        let partially_selected_label_ids =
            HashSet::from_iter(partially_selected_label_ids.iter().cloned());
        let labels_to_keep: HashSet<_> = current_labels
            .intersection(&partially_selected_label_ids)
            .cloned()
            .collect();
        labels_to_keep.union(&selected_label_ids).cloned().collect()
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
        session: &Session,
        queue: &Queue,
        source_label_id: LocalId,
        message_ids: Vec<LocalId>,
        selected_label_ids: Vec<LocalId>,
        partially_selected_label_ids: Vec<LocalId>,
        must_archive: bool,
    ) -> Result<bool, AppError> {
        let action = LabelAs::new(
            source_label_id,
            message_ids,
            selected_label_ids,
            partially_selected_label_ids,
            must_archive,
        );
        match queue
            .apply_action(session, action)
            .await
            .map_err(|e| AppError::Other(anyhow!(e)))?
            .remote
        {
            ActionRemoteOutput::Executed(result) => Ok(result),
            ActionRemoteOutput::Queued(id) => Err(AppError::ActionStillQueued(id)),
        }
    }

    pub async fn relabel_message<A>(
        &self,
        session: &Session,
        selected_label_ids: &[LocalId],
        partially_selected_label_ids: &[LocalId],
        interface: &A,
    ) -> Result<(), AppError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let api = session.api();

        let current_labels = self.all_message_labels(interface).await?;
        let current_labels: Vec<_> = current_labels
            .into_iter()
            .filter(|l| l.label_type == LabelType::Label)
            .collect();

        let labels_to_set = Message::compute_expected_labels(
            &current_labels,
            selected_label_ids,
            partially_selected_label_ids,
        );
        let labels_to_set = LocalId::counterparts::<Label, _>(labels_to_set, interface).await?;
        let labels_to_set = labels_to_set.into_iter().map_into().collect();

        if let Some(remote_message_id) = &self.remote_id {
            // TODO: api.relabel_message return a MessageMetadata. Should we use it to update current message?
            api.relabel_message(remote_message_id.clone().into(), labels_to_set)
                .await?;
        } else {
            warn!(
                "While labeling messages, message without remote_id: {:?}",
                self.local_id
            );
        };
        Ok(())
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
    pub(crate) async fn find_by_ids<A>(
        message_ids: impl IntoIterator<Item = LocalId>,
        interface: &A,
    ) -> Result<Vec<Self>, StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let (query, params) = find_in_query!("WHERE deleted = 0 AND local_id IN ({})", message_ids);
        Message::find(query, params, interface, None).await
    }

    /// Gets the embedded attachment by cid for a message.
    /// Returns None if it does not exist
    ///
    /// # Parameters
    ///
    /// * `mailbox`  - The current Mailbox.
    /// * `id`       - The id of the message
    /// * `cid`      - The cid of the attachment
    ///
    pub async fn get_embedded_attachment(
        mailbox: &Mailbox,
        id: LocalId,
        cid: &str,
    ) -> Result<Option<EmbeddedAttachmentInfo>, MailboxError> {
        let mdata = MessageBodyMetadata::for_message(id, mailbox.stash())
            .await?
            .ok_or(AppError::MessageBodyMetadataMissing(id))?;

        let Some(att) = mdata
            .attachments
            .into_iter()
            .filter(|at| matches!(at.disposition, Disposition::Inline))
            .find(|at| at.content_id.as_deref() == Some(cid))
        else {
            return Ok(None);
        };

        // PERF: Optimize this part
        let path = mailbox.get_attachment_content(&att).await?;
        let data = tokio::fs::read(path).await?;
        Ok(Some(EmbeddedAttachmentInfo {
            data,
            mime: att.mime_type.to_string(),
            height: att.image_height.clone(),
            width: att.image_width.clone(),
        }))
    }

    /// Get the available actions from bottom bar for given messages
    ///
    /// # Parameters
    ///
    /// * `current_label_id`  - Id of the current mailbox.
    /// * `message_ids` - List of the messages IDs.
    /// * `interface`   - The database interface.
    ///
    pub async fn all_available_bottom_bar_actions_for_messages<A>(
        current_label_id: LocalId,
        message_ids: Vec<LocalId>,
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
        let messages = Self::find_by_ids(message_ids.to_vec(), interface).await?;
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

    /// Revert locally the LabelAs action for conversation.
    pub(crate) async fn undo_label_as<A>(
        local_ids: Vec<LocalId>,
        source_label_id: LocalId,
        mut added_labels: HashMap<LocalId, HashSet<LocalId>>,
        mut removed_labels: HashMap<LocalId, HashSet<LocalId>>,
        original_location: HashMap<LocalId, Option<ExclusiveLocation>>,
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

        for message_id in &local_ids {
            let Some(mut message) = Message::load(*message_id, interface).await? else {
                warn!("While reverting locally, could not find message with id: {message_id:?}");
                continue;
            };

            let added_labels = added_labels.remove(message_id).unwrap_or_default();
            let removed_labels = removed_labels.remove(message_id).unwrap_or_default();
            let current_labels = RemoteId::counterparts::<Label, _>(
                message
                    .label_ids
                    .iter()
                    .map(|l| l.clone().into_inner())
                    .collect(),
                interface,
            )
            .await?;
            let current_labels = HashSet::from_iter(current_labels.into_iter());
            let new_labels = &(&current_labels - &removed_labels) | &added_labels;
            let new_labels =
                LocalId::counterparts::<Label, _>(Vec::from_iter(new_labels), interface).await?;
            message.label_ids = new_labels.into_iter().map_into().collect();

            if let Some(location) = original_location.get(message_id) {
                message.exclusive_location = location.clone();
            }
            if must_archive {
                Message::move_messages(archive_id, source_label_id, local_ids.clone(), interface)
                    .await?;
            }
            message.save_using(interface).await?
        }
        Ok(())
    }
}

pub struct EmbeddedAttachmentInfo {
    pub data: Vec<u8>,
    pub mime: String,
    pub height: Option<String>,
    pub width: Option<String>,
}
