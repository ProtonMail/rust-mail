use crate::actions::{filter_responses, ActionError, GenericActionData};
use crate::datatypes::RollbackItemType;
use crate::models::Conversation;
use proton_action_queue::action::{Action, DefaultVersionConverter, Type};
use proton_api_core::services::proton::Proton;
use proton_api_core::session::{CoreSession, Session};
use proton_core_common::datatypes::{Id, LocalId, RemoteId};
use serde::{Deserialize, Serialize};
use stash::stash::{Interface, Stash, Tether};
use tracing::error;

/// Action to mark conversations as unread.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MarkUnread(GenericActionData<Conversation>, Option<LocalId>);

impl MarkUnread {
    /// Create a new action which marks the conversations with `ids` as read.
    pub fn new(label_id: LocalId, ids: impl IntoIterator<Item = LocalId>) -> Self {
        // TODO(db-tests): label_id was present in the original action, why was it used.
        Self(GenericActionData::new(label_id, ids), None)
    }
}

impl Action for MarkUnread {
    const TYPE: Type = Type("mark_conversations_unread");
    const VERSION: u32 = 1;
    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = Handler;
    type Output = ();
    type Error = ActionError;
}

#[derive(Default)]
pub struct Handler {}

impl proton_action_queue::action::Handler for Handler {
    type Action = MarkUnread;

    async fn apply_local(
        &self,
        action: &mut Self::Action,
        tx: &Tether,
    ) -> Result<(), <Self::Action as Action>::Error> {
        action.0.resolve_ids(tx).await?;

        Conversation::mark_unread(action.0.label_id, action.0.target_ids.clone(), tx).await?;
        Ok(())
    }

    async fn revert_local(
        &self,
        action: &mut Self::Action,
        tx: &Tether,
    ) -> Result<(), <Self::Action as Action>::Error> {
        Conversation::mark_read(action.0.target_ids.clone(), tx).await?;
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
        let response = Conversation::mark_multiple_as_unread_remote::<Proton>(
            action.0.remote_target_ids.clone(),
            session.api(),
        )
        .await?;

        let failed_ids = filter_responses(response);

        if !failed_ids.is_empty() {
            error!("Mark unread operation failed for: {:?}", failed_ids);

            let tx = stash.transaction().await?;
            let local_ids =
                RemoteId::counterparts::<Conversation, _>(failed_ids.clone(), &tx).await?;

            Conversation::mark_read(local_ids, &tx).await.map_err(|e| {
                error!("Failed to rollback failed conversations: {e}");
                e
            })?;

            tx.commit().await?;
        }
        Ok(())
    }
}
