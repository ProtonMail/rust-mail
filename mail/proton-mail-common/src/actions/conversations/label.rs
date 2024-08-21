use crate::actions::conversations::ActionData;
use crate::actions::ActionError;
use crate::models::Conversation;
use proton_action_queue::action::{Action, DefaultVersionConverter, Type};
use proton_api_core::services::proton::Proton;
use proton_api_core::session::{CoreSession, Session};
use proton_core_common::datatypes::{Id, LocalId, RemoteId};
use serde::{Deserialize, Serialize};
use stash::stash::{Interface, Stash, Tether};
use tracing::error;

use super::filter_conversation_responses;

/// Action which applies a label to conversations.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Label(ActionData);

impl Label {
    /// Create a new instance which applies `label_id` to the conversations with `ids`.
    pub fn new(label_id: LocalId, ids: impl IntoIterator<Item = LocalId>) -> Self {
        Self(ActionData::new(label_id, ids))
    }
}

impl Action for Label {
    const TYPE: Type = Type("label_conversations");
    const VERSION: u32 = 1;
    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = Handler;
    type Output = ();
    type Error = ActionError;
}

#[derive(Default)]
pub struct Handler {}

impl proton_action_queue::action::Handler for Handler {
    type Action = Label;

    async fn apply_local(
        &self,
        action: &mut Self::Action,
        tx: &Tether,
    ) -> Result<(), <Self::Action as Action>::Error> {
        action.0.resolve_ids(tx).await?;

        Conversation::apply_label(action.0.label_id, action.0.ids.clone(), tx).await?;
        Ok(())
    }

    async fn revert_local(
        &self,
        action: &mut Self::Action,
        tx: &Tether,
    ) -> Result<(), <Self::Action as Action>::Error> {
        Conversation::remove_label(action.0.label_id, action.0.ids.clone(), tx).await?;

        action.0.mark_rollback_conversations(tx).await?;

        Ok(())
    }

    async fn apply_remote(
        &self,
        action: &mut Self::Action,
        session: &Session,
        stash: &Stash,
    ) -> Result<<Self::Action as Action>::Output, <Self::Action as Action>::Error> {
        let response = Conversation::apply_label_to_multiple_remote::<Proton>(
            action
                .0
                .remote_label_id
                .clone()
                .expect("Should be set")
                .clone(),
            action.0.remote_ids.clone(),
            None,
            session.api(),
        )
        .await?;

        let failed_ids = filter_conversation_responses(response);

        if !failed_ids.is_empty() {
            error!("Label operation failed for: {:?}", failed_ids);

            let tx = stash.transaction().await?;
            let local_ids =
                RemoteId::counterparts::<Conversation, _>(failed_ids.clone(), &tx).await?;

            Conversation::remove_label(action.0.label_id, local_ids, &tx)
                .await
                .map_err(|e| {
                    error!("Failed to rollback failed conversations: {e}");
                    e
                })?;

            tx.commit().await?;
        }
        Ok(())
    }
}
