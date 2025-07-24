use crate::actions::{GenericActionData, MailActionError, filter_responses_by_codes};
use crate::datatypes::{LocalMessageId, RollbackItemType};
use crate::models::Message;
use proton_action_queue::action::{Action, DefaultVersionConverter, Type, WriterGuard};
use proton_action_queue::action::{ActionId, Handler};
use proton_core_api::consts::General;
use proton_core_api::services::proton::Proton;
use proton_core_common::models::ModelIdExtension;
use proton_mail_api::services::proton::ProtonMail;
use serde::{Deserialize, Serialize};
use stash::stash::Bond;
use tracing::{error, info};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Read(GenericActionData<Message>);

impl Read {
    pub fn new(message_ids: impl IntoIterator<Item = LocalMessageId>) -> Self {
        Self(GenericActionData::new(message_ids))
    }
}

impl Action for Read {
    const TYPE: Type = Type("mark_messages_read");
    const VERSION: u32 = 1;

    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = ReadHandler;
    type RemoteOutput = ();
    type LocalOutput = ();
    type Error = MailActionError;
}

pub struct ReadHandler {
    pub api: Proton,
}

impl Handler for ReadHandler {
    type Action = Read;

    async fn apply_local(
        &self,
        _: ActionId,
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
        _: ActionId,
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
        _: ActionId,
        action: &mut Self::Action,
        mut guard: WriterGuard<'_>,
    ) -> Result<<Self::Action as Action>::RemoteOutput, <Self::Action as Action>::Error> {
        let message_ids = action.0.remote_target_ids.clone();

        info!("Marking {message_ids:?} as read");

        let response = self.api.put_messages_read(message_ids).await?.responses;

        // In this case General::NotExists is returned also for messages already marked as read
        let failed_ids = filter_responses_by_codes(
            response,
            &[General::NoError as u32, General::NotExists as u32],
        );

        if !failed_ids.is_empty() {
            error!("Read messages operation failed for: {failed_ids:?}");

            guard
                .tx::<_, _, <Self::Action as Action>::Error>(async |tx| {
                    let local_ids = Message::remote_ids_counterpart(failed_ids.clone(), tx).await?;

                    Message::mark_unread(local_ids, tx)
                        .await
                        .inspect_err(|e| error!("Failed to rollback read on messages: {e:?}"))?;
                    Ok(())
                })
                .await?;
        }
        Ok(())
    }
}
