use crate::actions::{filter_responses, ActionError, GenericActionData};
use crate::datatypes::RollbackItemType;
use crate::models::Conversation;
use proton_action_queue::action::{Action, DefaultVersionConverter, Type};
use proton_api_core::session::{CoreSession, Session};
use proton_core_common::datatypes::{Id, LocalId, RemoteId};
use serde::{self, Deserialize, Serialize};
use stash::stash::{Interface, Stash, Tether};
use tracing::error;

/// Delete conversations action.
///
/// This action permanently deletes the given conversations.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Delete(GenericActionData<Conversation>);

impl Delete {
    /// Create new instance.
    pub fn new(label_id: LocalId, ids: impl IntoIterator<Item = LocalId>) -> Self {
        Self(GenericActionData::new(label_id, ids))
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

        Conversation::mark_deleted(action.0.label_id, action.0.target_ids.clone(), tx).await?;

        Ok(())
    }

    async fn revert_local(
        &self,
        action: &mut Self::Action,
        tx: &Tether,
    ) -> Result<(), <Self::Action as Action>::Error> {
        Conversation::mark_undeleted(action.0.label_id, action.0.target_ids.clone(), tx).await?;
        action
            .0
            .mark_rollback(RollbackItemType::Conversation, tx)
            .await?;

        Ok(())
    }

    async fn apply_remote(
        &self,
        action: &mut Self::Action,
        session: &Session,
        stash: &Stash,
    ) -> Result<<Self::Action as Action>::Output, <Self::Action as Action>::Error> {
        let remote_label_id = action
            .0
            .remote_label_id
            .clone()
            .expect("Should not be none");
        let responses = Conversation::delete_multiple_remote(
            action.0.remote_target_ids.clone(),
            remote_label_id,
            session.api(),
        )
        .await
        .map_err(|e| {
            error!("Failed to delete conversations on API: {e}");
            e
        })?;

        let failed_ids = filter_responses(responses);

        if !failed_ids.is_empty() {
            error!("Delete operation failed for: {:?}", failed_ids);
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
