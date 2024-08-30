use super::Message;
use crate::actions::messages::label::Label;
use crate::actions::messages::unlabel::Unlabel;
use proton_action_queue::queue::{ActionError, ActionStatus, Queue};
use proton_api_core::session::Session;
use proton_core_common::datatypes::LocalId;

impl Message {
    /// Label multiple messages.
    ///
    /// # Parameters
    ///
    /// * `session`     - The session.
    /// * `queue`       - The action queue.
    /// * `label_id`    - The ID of the label to apply to the messages.
    /// * `message_ids` - The IDs of the messages to label.
    /// * `spam_action` - TODO: Document this parameter.
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
        let action = Label::new(label_id, message_ids.into_iter().map(Into::into));
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
        let action = Unlabel::new(label_id, message_ids.into_iter().map(Into::into));
        queue.apply_action(session, action).await
    }
}
