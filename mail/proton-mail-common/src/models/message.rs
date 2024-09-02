use crate::actions::messages::delete::Delete;
use crate::actions::messages::label::Label as ActionLabel;
use crate::actions::messages::r#move::Move;
use crate::actions::messages::read::Read;
use crate::actions::messages::unlabel::Unlabel;
use crate::actions::messages::unread::Unread;
use crate::datatypes::SystemLabelId;
use crate::models::{Label, Message};
use crate::AppError;
use itertools::Itertools as _;
use proton_action_queue::queue::{ActionError, ActionStatus, Queue};
use proton_api_core::session::Session;
use proton_core_common::datatypes::{Id, LabelId, LocalId};
use stash::orm::Model;
use stash::params;
use stash::stash::{AgnosticInterface, Interface, StashError};
use tracing::error;

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
    ) -> Result<ActionStatus<()>, ActionError<ActionLabel>> {
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
    ) -> Result<ActionStatus<()>, ActionError<ActionLabel>> {
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
    ) -> Result<ActionStatus<()>, ActionError<Unlabel>> {
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
    ) -> Result<ActionStatus<()>, ActionError<Unlabel>> {
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
    ) -> Result<ActionStatus<()>, ActionError<Read>> {
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
    ) -> Result<ActionStatus<()>, ActionError<Unread>> {
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
    ) -> Result<ActionStatus<()>, ActionError<Delete>> {
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
    ) -> Result<ActionStatus<()>, ActionError<Move>> {
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
        for local_message_id in message_ids {
            interface
                .execute(
                    "DELETE FROM message_labels WHERE local_message_id = ? AND local_label_id != ?",
                    params![local_message_id, all_mail_id],
                )
                .await?;
        }
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
}
