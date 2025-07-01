use crate::MailUserContext;
use crate::actions::{GenericLabelRelatedActionData, MailActionError, filter_responses};
use crate::datatypes::{LocalMessageId, RollbackItemType};
use crate::models::Message;
use proton_action_queue::action::{
    Action, ActionId, DefaultVersionConverter, Handler as ActionHandler, Type, WriterGuard,
};
use proton_core_common::datatypes::LocalLabelId;
use proton_core_common::models::ModelIdExtension;
use proton_mail_api::services::proton::ProtonMail;
use serde::{Deserialize, Serialize};
use stash::stash::Bond;
use tracing::{error, info};

/// Action which remove a label from messages.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Unlabel(GenericLabelRelatedActionData<Message>);

impl Unlabel {
    /// Create a new instance which remove `label_id` from the messages with `message_ids`
    pub fn new(
        label_id: LocalLabelId,
        message_ids: impl IntoIterator<Item = LocalMessageId>,
    ) -> Self {
        Self(GenericLabelRelatedActionData::new(label_id, message_ids))
    }
}

impl Action for Unlabel {
    const TYPE: Type = Type("unlabel_messages");
    const VERSION: u32 = 1;
    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = Handler;
    type RemoteOutput = ();

    type LocalOutput = ();
    type Error = MailActionError;
    type Context = MailUserContext;
}

#[derive(Default)]
pub struct Handler;

impl ActionHandler for Handler {
    type Action = Unlabel;
    type Context = MailUserContext;

    async fn apply_local(
        &self,
        _: ActionId,
        _: &Self::Context,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        action.0.resolve_ids(tx).await?;
        Message::remove_label(action.0.label_id, action.0.data.target_ids.clone(), tx).await?;
        Ok(())
    }

    async fn revert_local(
        &self,
        _: ActionId,
        _: &Self::Context,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        Message::apply_label(action.0.label_id, action.0.data.target_ids.clone(), tx).await?;
        action
            .0
            .mark_rollback(RollbackItemType::Message, tx)
            .await?;
        Ok(())
    }

    async fn apply_remote(
        &self,
        _: ActionId,
        ctx: &Self::Context,
        action: &mut Self::Action,
        mut guard: WriterGuard<'_>,
    ) -> Result<<Self::Action as Action>::RemoteOutput, <Self::Action as Action>::Error> {
        let api = ctx.api();
        let message_ids = action.0.data.remote_target_ids.clone();
        let label_id = action.0.remote_label_id.clone().expect("Should be set");
        info!("Removing {label_id:?} from {message_ids:?}");
        let response = api
            .put_messages_unlabel(message_ids, label_id)
            .await?
            .responses;

        let failed_ids = filter_responses(response);

        if !failed_ids.is_empty() {
            error!("Unlabel messages failed for: {failed_ids:?} ");

            guard
                .tx::<_, _, <Self::Action as Action>::Error>(async |tx| {
                    let local_ids = Message::remote_ids_counterpart(failed_ids.clone(), tx).await?;

                    Message::apply_label(action.0.label_id, local_ids, tx)
                        .await
                        .inspect_err(|e| error!("Failed to rollback unlabel on messages: {e:?}"))?;
                    Ok(())
                })
                .await?;
        }
        Ok(())
    }
}
