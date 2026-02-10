use crate::actions::{GenericLabelRelatedActionData, MailActionError, filter_responses};
use crate::datatypes::{LocalMessageId, RollbackItemType};
use crate::models::Message;
use proton_action_queue::action::{
    Action, ActionDependencyKeys, ActionId, DefaultVersionConverter, Handler, Type, WriterGuard,
};
use proton_core_api::services::proton::Proton;
use proton_core_common::datatypes::LocalLabelId;
use proton_core_common::models::ModelIdExtension;
use proton_mail_api::services::proton::ProtonMail;
use serde::{Deserialize, Serialize};
use stash::stash::Bond;
use tracing::{error, info};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Unlabel(GenericLabelRelatedActionData<Message>);

impl Unlabel {
    pub fn new(
        label_id: LocalLabelId,
        message_ids: impl IntoIterator<Item = LocalMessageId>,
    ) -> Self {
        Self(GenericLabelRelatedActionData::new(label_id, message_ids))
    }
}

impl Action<UserDb> for Unlabel {
    const TYPE: Type = Type("unlabel_messages");
    const VERSION: u32 = 1;

    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = UnlabelHandler;
    type RemoteOutput = ();
    type LocalOutput = ();
    type Error = MailActionError;

    fn dependency_keys(&self) -> ActionDependencyKeys {
        self.0.label_unlabel_action_dependency_keys().build()
    }
}

pub struct UnlabelHandler {
    pub api: Proton,
}

impl Handler<UserDb> for UnlabelHandler {
    type Action = Unlabel;

    async fn apply_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action<UserDb>>::Error> {
        action.0.resolve_ids(tx).await?;
        Message::remove_label(action.0.label_id, action.0.data.target_ids.clone(), tx).await?;
        Ok(())
    }

    async fn revert_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action<UserDb>>::Error> {
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
        action: &mut Self::Action,
        mut guard: WriterGuard<'_, UserDb>,
    ) -> Result<
        <Self::Action as Action<UserDb>>::RemoteOutput,
        <Self::Action as Action<UserDb>>::Error,
    > {
        let message_ids = action.0.data.remote_target_ids.clone();
        let label_id = action.0.remote_label_id.clone().expect("Should be set");

        info!("Removing {label_id:?} from {message_ids:?}");

        let response = self
            .api
            .put_messages_unlabel(message_ids, label_id)
            .await?
            .responses;

        let failed_ids = filter_responses(response);

        if !failed_ids.is_empty() {
            error!("Unlabel messages failed for: {failed_ids:?} ");

            guard
                .tx::<_, _, <Self::Action as Action<UserDb>>::Error>(async |tx| {
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
