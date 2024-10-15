use itertools::Itertools;
use proton_action_queue::queue::{ActionError, ActionStatus, Queue};
use proton_api_core::session::Session;
use proton_core_common::datatypes::{Id, LabelId, LocalId};

use crate::actions::conversations::{Label as ActionLabel, MarkRead, MarkUnread, Move, Unlabel};
use crate::datatypes::SystemLabelId;
use crate::{actions::conversations::Delete, models::Conversation};

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
    ) -> Result<ActionStatus<()>, ActionError<ActionLabel>> {
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
    ) -> Result<ActionStatus<()>, ActionError<ActionLabel>> {
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
    ) -> Result<ActionStatus<()>, ActionError<Unlabel>> {
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
    ) -> Result<ActionStatus<()>, ActionError<Unlabel>> {
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
    ) -> Result<ActionStatus<()>, ActionError<MarkRead>> {
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
    ) -> Result<ActionStatus<()>, ActionError<MarkUnread>> {
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
    ) -> Result<ActionStatus<()>, ActionError<Delete>> {
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
    ) -> Result<ActionStatus<()>, ActionError<Move>> {
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
    ) -> Result<ActionStatus<()>, ActionError<Delete>> {
        let action = Delete::new(label_id, conversation_ids);
        queue.apply_action(session, action).await
    }
}
