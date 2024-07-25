use crate::actions::conversations::ActionData;
use crate::actions::ActionError;
use crate::models::Conversation;
use proton_action_queue::action::{Action, DefaultVersionConverter, Type};
use proton_api_core::services::proton::Proton;
use proton_api_core::session::{CoreSession, Session};
use serde::{Deserialize, Serialize};
use stash::stash::Tether;
use tracing::error;

/// Action to mark conversations read.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MarkRead(ActionData);

impl MarkRead {
    /// Create a new action which marks the conversations with `ids` as read.
    pub fn new(label_id: u64, ids: impl IntoIterator<Item = u64>) -> Self {
        // TODO(db-tests): label_id was present in the original action, why was it used.
        Self(ActionData::new(label_id, ids))
    }
}

impl Action for MarkRead {
    const TYPE: Type = Type("mark_conversations_read");
    const VERSION: u32 = 1;
    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = Handler;
    type Output = ();
    type Error = ActionError;
}

#[derive(Default)]
pub struct Handler {}

impl proton_action_queue::action::Handler for Handler {
    type Action = MarkRead;

    async fn apply_local(
        &self,
        action: &mut Self::Action,
        tx: &Tether,
    ) -> Result<(), <Self::Action as Action>::Error> {
        action.0.resolve_ids(tx).await?;

        Conversation::mark_multiple_as_read(action.0.ids.clone(), tx).await?;
        Ok(())
    }

    async fn revert_local(
        &self,
        action: &mut Self::Action,
        tx: &Tether,
    ) -> Result<(), <Self::Action as Action>::Error> {
        Conversation::mark_multiple_as_unread(action.0.ids.clone(), tx).await?;
        Ok(())
    }

    async fn apply_remote(
        &self,
        action: &mut Self::Action,
        session: &Session,
    ) -> Result<(), <Self::Action as Action>::Error> {
        let response = Conversation::mark_multiple_as_read_remote::<Proton>(
            action.0.remote_ids.clone(),
            session.api(),
        )
        .await?;

        action.0.filter_responses(response);

        Ok(())
    }

    async fn apply_local_post_remote(
        &self,
        action: &mut Self::Action,
        tx: &Tether,
    ) -> Result<<Self::Action as Action>::Output, <Self::Action as Action>::Error> {
        if !action.0.failed_ids.is_empty() {
            error!("Mark read operation failed for: {:?}", action.0.failed_ids);
            let local_ids = Conversation::find_local_ids(action.0.failed_ids.clone(), tx).await?;
            Conversation::mark_multiple_as_unread(local_ids, tx)
                .await
                .map_err(|e| {
                    error!("Failed to rollback failed conversations: {e}");
                    e
                })?;
        }
        Ok(())
    }
}
