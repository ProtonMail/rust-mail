use crate::actions::{filter_responses_by_codes, ActionError, GenericActionData};
use crate::datatypes::{LocalMessageId, RollbackItemType};
use crate::models::Message;
use crate::MailUserContext;
use proton_action_queue::action::{Action, DefaultVersionConverter, Type};
use proton_action_queue::action::{Handler as ActionHandler, Id};
use proton_api_core::consts::General;
use proton_api_core::session::CoreSession;
use proton_api_mail::services::proton::ProtonMail;
use proton_core_common::datatypes::LocalLabelId;
use proton_core_common::models::ModelIdExtension;
use serde::{Deserialize, Serialize};
use stash::stash::{Bond, Stash};
use tracing::error;

/// Action which marks messages as read.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Read(GenericActionData<Message>);

impl Read {
    /// Create a new instance which marks the messages as read.
    pub fn new(
        label_id: LocalLabelId,
        message_ids: impl IntoIterator<Item = LocalMessageId>,
    ) -> Self {
        Self(GenericActionData::new(label_id, message_ids))
    }
}

impl Action for Read {
    const TYPE: Type = Type("mark_messages_read");
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
    type Action = Read;

    type Context = MailUserContext;

    async fn apply_local(
        &self,
        _: Id,
        _: &Self::Context,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        // API call return an error 2501(Message does not exist) for message already read
        let messages = Message::find_by_ids(action.0.target_ids.clone(), tx).await?;
        action.0.target_ids = messages
            .into_iter()
            .filter(|m| m.unread)
            .filter_map(|m| m.local_id)
            .collect();

        action.0.resolve_ids(tx).await?;
        Message::mark_read(action.0.target_ids.clone(), tx).await?;
        Ok(())
    }

    async fn revert_local(
        &self,
        _: Id,
        _: &Self::Context,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        Message::mark_unread(action.0.target_ids.clone(), tx).await?;
        action
            .0
            .mark_rollback(RollbackItemType::Message, tx)
            .await?;
        Ok(())
    }

    async fn apply_remote(
        &self,
        _: Id,
        ctx: &Self::Context,
        action: &mut Self::Action,
        stash: &Stash,
    ) -> Result<<Self::Action as Action>::RemoteOutput, <Self::Action as Action>::Error> {
        let api = ctx.session().api();
        let message_ids = action
            .0
            .remote_target_ids
            .clone()
            .into_iter()
            .map(Into::into)
            .collect();
        let response = api.put_messages_read(message_ids).await?.responses;

        // In this case General::NotExists is returned also for messages already marked as read
        let failed_ids = filter_responses_by_codes(
            response,
            &[General::NoError as u32, General::NotExists as u32],
        );

        if !failed_ids.is_empty() {
            error!("Read messages operation failed for: {failed_ids:?}");

            let mut conn = stash.connection();
            let tx = conn.transaction().await?;
            let local_ids = Message::remote_ids_counterpart(failed_ids.clone(), &tx).await?;

            Message::mark_unread(local_ids, &tx)
                .await
                .inspect_err(|e| error!("Failed to rollback read on messages: {e:?}"))?;
            tx.commit().await?;
        }
        Ok(())
    }
}
