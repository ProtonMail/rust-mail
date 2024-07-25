use crate::actions::conversations::ActionData;
use crate::actions::ActionError;
use crate::models::Conversation;
use proton_action_queue::action::{Action, DefaultVersionConverter, Type};
use proton_api_core::services::proton::Proton;
use proton_api_core::session::{CoreSession, Session};
use serde::{Deserialize, Serialize};
use stash::stash::Tether;
use tracing::error;

/// Action which removes a label from conversations.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Unlabel(ActionData);

impl Unlabel {
    /// Create a new instance which removes `label_id` from the conversations with `ids`.
    pub fn new(label_id: u64, ids: impl IntoIterator<Item = u64>) -> Self {
        Self(ActionData::new(label_id, ids))
    }
}

impl Action for Unlabel {
    const TYPE: Type = Type("unlabel_conversation");
    const VERSION: u32 = 1;
    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = Handler;
    type Output = ();
    type Error = ActionError;
}

#[derive(Default)]
pub struct Handler {}

impl proton_action_queue::action::Handler for Handler {
    type Action = Unlabel;

    async fn apply_local(
        &self,
        action: &mut Self::Action,
        tx: &Tether,
    ) -> Result<(), <Self::Action as Action>::Error> {
        action.0.resolve_ids(tx).await?;
        Conversation::remove_label_from_multiple(action.0.label_id, action.0.ids.clone(), tx)
            .await?;
        Ok(())
    }

    async fn revert_local(
        &self,
        action: &mut Self::Action,
        tx: &Tether,
    ) -> Result<(), <Self::Action as Action>::Error> {
        Conversation::apply_label_to_multiple(action.0.label_id, action.0.ids.clone(), tx).await?;
        Ok(())
    }

    async fn apply_remote(
        &self,
        action: &mut Self::Action,
        session: &Session,
    ) -> Result<(), <Self::Action as Action>::Error> {
        let response = Conversation::remove_label_from_multiple_remote::<Proton>(
            action.0.remote_label_id.clone().expect("Should be set"),
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
            error!("Unlabel operation failed for: {:?}", action.0.failed_ids);
            let local_ids = Conversation::find_local_ids(action.0.failed_ids.clone(), tx).await?;
            Conversation::apply_label_to_multiple(action.0.label_id, local_ids, tx)
                .await
                .map_err(|e| {
                    error!("Failed to rollback failed conversations: {e}");
                    e
                })?;
        }
        Ok(())
    }
}
