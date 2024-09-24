use proton_action_queue::queue::{ActionError, ActionStatus, Queue};
use proton_api_core::session::Session;
use proton_core_common::datatypes::LocalId;

use crate::{actions::conversations::Delete, models::Conversation};

impl Conversation {
    /// Soft delete multiple conversations.
    ///
    /// # Parameters
    ///
    /// * `session`     - The session.
    /// * `queue`       - The action queue.
    /// * `label_id`    - The ID of the current view.
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
