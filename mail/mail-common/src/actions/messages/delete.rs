use crate::actions::{filter_responses, ActionError, GenericActionData};
use crate::datatypes::RollbackItemType;
use crate::models::Message;
use crate::MailUserContext;
use proton_action_queue::action::Handler as ActionHandler;
use proton_action_queue::action::{Action, DefaultVersionConverter, Type};
use proton_api_core::session::{CoreSession, Session};
use proton_api_mail::services::proton::ProtonMail;
use proton_core_common::datatypes::{Id, LocalId, RemoteId};
use serde::{Deserialize, Serialize};
use stash::stash::{Interface, Stash, Tether};
use tracing::error;

/// Action which marks messages as deleted.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Delete(GenericActionData<Message>);

impl Delete {
    /// Create a new instance which marks the messages as deleted.
    pub fn new(label_id: LocalId, message_ids: impl IntoIterator<Item = LocalId>) -> Self {
        Self(GenericActionData::new(label_id, message_ids))
    }
}

impl Action for Delete {
    const TYPE: Type = Type("delete_messages");
    const VERSION: u32 = 1;
    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = Handler;
    type RemoteOutput = ();

    type LocalOutput = ();
    type Error = ActionError;

    type Context = MailUserContext;
}

#[derive(Default)]
pub struct Handler;

impl ActionHandler for Handler {
    type Action = Delete;

    type Context = MailUserContext;
    async fn apply_local(
        &self,
        _: &Self::Context,
        action: &mut Self::Action,
        tx: &Tether,
    ) -> Result<(), <Self::Action as Action>::Error> {
        action.0.resolve_ids(tx).await?;
        Message::mark_deleted(action.0.target_ids.clone(), tx).await?;
        Ok(())
    }

    async fn revert_local(
        &self,
        _: &Self::Context,
        action: &mut Self::Action,
        tx: &Tether,
    ) -> Result<(), <Self::Action as Action>::Error> {
        Message::mark_undeleted(action.0.target_ids.clone(), tx).await?;
        action
            .0
            .mark_rollback(RollbackItemType::Message, tx)
            .await?;
        Ok(())
    }

    async fn apply_remote(
        &self,
        _: &Self::Context,
        action: &mut Self::Action,
        session: &Session,
        stash: &Stash,
    ) -> Result<<Self::Action as Action>::RemoteOutput, <Self::Action as Action>::Error> {
        let api = session.api();
        let message_ids = action
            .0
            .remote_target_ids
            .clone()
            .into_iter()
            .map(Into::into)
            .collect();
        let label_id = action
            .0
            .remote_label_id
            .clone()
            .map(|x| x.into_inner().into());
        let response = api
            .put_messages_delete(message_ids, label_id)
            .await?
            .responses;

        let failed_ids = filter_responses(response);

        if !failed_ids.is_empty() {
            error!("Delete messages operation failed for: {failed_ids:?}");

            let tx = stash.transaction().await?;
            let local_ids = RemoteId::counterparts::<Message, _>(failed_ids.clone(), &tx).await?;

            Message::mark_undeleted(local_ids, &tx)
                .await
                .inspect_err(|e| error!("Failed to rollback delete on messages: {e}"))?;
            tx.commit().await?;
        }
        Ok(())
    }
}
