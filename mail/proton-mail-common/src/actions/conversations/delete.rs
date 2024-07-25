use crate::actions::conversations::ActionData;
use crate::actions::ActionError;
use crate::models::Conversation;
use proton_action_queue::action::{Action, DefaultVersionConverter, Type};
use proton_api_core::session::{CoreSession, Session};
use serde::{self, Deserialize, Serialize};
use stash::stash::Tether;
use tracing::error;

/// Delete conversations action.
///
/// This action permanently deletes the given conversations.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Delete(ActionData);

impl Delete {
    /// Create new instance.
    pub fn new(label_id: u64, ids: impl IntoIterator<Item = u64>) -> Self {
        Self(ActionData::new(label_id, ids))
    }
}

impl Action for Delete {
    const TYPE: Type = Type("delete_conversations");
    const VERSION: u32 = 1;
    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = Handler;
    type Output = ();
    type Error = ActionError;
}

#[derive(Default)]
pub struct Handler {}

impl proton_action_queue::action::Handler for Handler {
    type Action = Delete;

    async fn apply_local(
        &self,
        action: &mut Self::Action,
        tx: &Tether,
    ) -> Result<(), <Self::Action as Action>::Error> {
        action.0.resolve_ids(tx).await?;

        Conversation::delete_multiple(action.0.ids.clone(), action.0.label_id, tx).await?;

        Ok(())
    }

    async fn revert_local(
        &self,
        action: &mut Self::Action,
        tx: &Tether,
    ) -> Result<(), <Self::Action as Action>::Error> {
        Conversation::undelete_multiple(action.0.ids.clone(), action.0.label_id, tx).await?;
        Ok(())
    }

    async fn apply_remote(
        &self,
        action: &mut Self::Action,
        session: &Session,
    ) -> Result<(), <Self::Action as Action>::Error> {
        let remote_label_id = action
            .0
            .remote_label_id
            .clone()
            .expect("Should not be none");
        let responses = Conversation::delete_multiple_remote(
            action.0.remote_ids.clone(),
            remote_label_id,
            session.api(),
        )
        .await
        .map_err(|e| {
            error!("Failed to delete conversations on API: {e}");
            e
        })?;

        action.0.filter_responses(responses);

        Ok(())
    }

    async fn apply_local_post_remote(
        &self,
        action: &mut Self::Action,
        tx: &Tether,
    ) -> Result<<Self::Action as Action>::Output, <Self::Action as Action>::Error> {
        if !action.0.failed_ids.is_empty() {
            error!("Delete operation failed for: {:?}", action.0.failed_ids);
            let local_ids = Conversation::find_local_ids(action.0.failed_ids.clone(), tx).await?;
            Conversation::remove_label_from_multiple(action.0.label_id, local_ids, tx)
                .await
                .map_err(|e| {
                    error!("Failed to rollback failed conversations: {e}");
                    e
                })?;
        }
        Ok(())
    }
}
