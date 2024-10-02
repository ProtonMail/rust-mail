use super::Message;
use crate::actions::messages::label::Label;
use crate::actions::messages::read::Read;
use crate::actions::messages::unlabel::Unlabel;
use crate::actions::messages::unread::Unread;
use crate::{actions::messages::delete::Delete, datatypes::SystemLabelId};
use itertools::Itertools as _;
use proton_action_queue::queue::{ActionError, ActionStatus, Queue};
use proton_api_core::session::Session;
use proton_core_common::datatypes::{Id, LabelId, LocalId};

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
    /// Returns an error if the API request failed.
    ///
    pub async fn action_apply_label(
        session: &Session,
        queue: &Queue,
        label_id: LocalId,
        message_ids: Vec<LocalId>,
    ) -> Result<ActionStatus<()>, ActionError<Label>> {
        let action = Label::new(label_id, message_ids.into_iter().map_into());
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
    ) -> Result<ActionStatus<()>, ActionError<Label>> {
        let label_id = LabelId::starred()
            .counterpart::<crate::models::Label, _>(queue.stash())
            .await?
            .expect("Star system label not found");
        let action = Label::new(label_id, message_ids.into_iter().map_into());
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
    /// Returns an error if the API request failed.
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
}
