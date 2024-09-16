use crate::actions::{filter_responses, ActionError, GenericActionData};
use crate::datatypes::RollbackItemType;
use crate::models::Message;
use itertools::Itertools;
use proton_action_queue::action::{
    Action, DefaultVersionConverter, Handler as ActionHandler, Type,
};
use proton_api_core::session::{CoreSession, Session};
use proton_api_mail::services::proton::ProtonMail;
use proton_core_common::datatypes::{Id, LocalId, RemoteId};
use serde::{Deserialize, Serialize};
use stash::stash::{Interface, Stash, Tether};
use tracing::error;

/// Action which marks messages as unread.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Unread(GenericActionData<Message>);

impl Unread {
    /// Create a new instance which marks the messages as unread.
    pub fn new(label_id: LocalId, message_ids: impl IntoIterator<Item = LocalId>) -> Self {
        Self(GenericActionData::new(label_id, message_ids))
    }
}

impl Action for Unread {
    const TYPE: Type = Type("mark_messages_unread");
    const VERSION: u32 = 1;
    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = Handler;
    type Output = ();
    type Error = ActionError;
}

#[derive(Default)]
pub struct Handler;

impl ActionHandler for Handler {
    type Action = Unread;

    async fn apply_local(
        &self,
        action: &mut Self::Action,
        tx: &Tether,
    ) -> Result<(), <Self::Action as Action>::Error> {
        action.0.resolve_ids(tx).await?;
        Message::mark_unread(action.0.target_ids.iter().copied(), tx).await?;
        Ok(())
    }

    async fn revert_local(
        &self,
        action: &mut Self::Action,
        tx: &Tether,
    ) -> Result<(), <Self::Action as Action>::Error> {
        Message::mark_read(action.0.target_ids.iter().copied(), tx).await?;
        action
            .0
            .mark_rollback(RollbackItemType::Message, tx)
            .await?;
        Ok(())
    }

    async fn apply_remote(
        &self,
        action: &mut Self::Action,
        session: &Session,
        stash: &Stash,
    ) -> Result<<Self::Action as Action>::Output, <Self::Action as Action>::Error> {
        let api = session.api();
        let message_ids = action
            .0
            .remote_target_ids
            .iter()
            .cloned()
            .map_into()
            .collect();
        let response = api.put_messages_unread(message_ids).await?.responses;

        let failed_ids = filter_responses(response);

        if !failed_ids.is_empty() {
            error!("Unread messages failed for: {failed_ids:?} ");

            let tx = stash.transaction().await?;
            let local_ids = RemoteId::counterparts::<Message, _>(failed_ids.clone(), &tx).await?;

            Message::mark_read(local_ids, &tx)
                .await
                .inspect_err(|e| error!("Failed to rollback unread on messages: {e}"))?;
            tx.commit().await?;
        }
        Ok(())
    }
}
