use crate::actions::{
    ConversationOrMessage, GenericActionData, MailActionError, filter_responses_by_codes,
};
use crate::datatypes::{LocalMessageId, RollbackItemType};
use crate::models::Message;
use itertools::Itertools;
use proton_action_queue::action::{
    Action, ActionId, DefaultVersionConverter, Handler, Type, WriterGuard,
};
use proton_core_api::consts::General;
use proton_core_api::services::proton::Proton;
use proton_core_common::models::ModelIdExtension;
use proton_mail_api::services::proton::ProtonMail;
use serde::{Deserialize, Serialize};
use stash::stash::Bond;
use tracing::{error, info};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Unread(GenericActionData<Message>);

impl Unread {
    pub fn new(message_ids: impl IntoIterator<Item = LocalMessageId>) -> Self {
        Self(GenericActionData::new(message_ids))
    }
}

impl Action for Unread {
    const TYPE: Type = Type("mark_messages_unread");
    const VERSION: u32 = 1;

    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = UnreadHandler;
    type RemoteOutput = ();
    type LocalOutput = ();
    type Error = MailActionError;
}

pub struct UnreadHandler {
    pub api: Proton,
}

impl Handler for UnreadHandler {
    type Action = Unread;

    async fn apply_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        // API call return an error 2501(Message does not exist) for message already unread
        let messages = Message::find_by_ids(action.0.target_ids.clone(), tx).await?;

        action.0.target_ids = messages
            .into_iter()
            .filter(|m| !m.unread)
            .filter_map(|m| m.local_id)
            .collect();

        action.0.resolve_ids(tx).await?;
        Message::mark_unread(action.0.target_ids.iter().copied(), tx).await?;

        Ok(())
    }

    async fn revert_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        tx: &Bond<'_>,
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
        _: ActionId,
        action: &mut Self::Action,
        mut guard: WriterGuard<'_>,
    ) -> Result<<Self::Action as Action>::RemoteOutput, <Self::Action as Action>::Error> {
        let message_ids = action
            .0
            .remote_target_ids
            .iter()
            .cloned()
            .map_into()
            .collect();

        info!("Marking {message_ids:?} as unread");

        let responses = self.api.put_messages_unread(message_ids).await?.responses;

        // In this case General::NotExists is returned also for messages already marked as unread
        let failed_ids = filter_responses_by_codes(
            responses,
            &[General::NoError as u32, General::NotExists as u32],
        );

        if !failed_ids.is_empty() {
            error!("Unread messages failed for: {failed_ids:?} ");

            guard
                .tx::<_, _, <Self::Action as Action>::Error>(async |tx| {
                    let local_ids = Message::remote_ids_counterpart(failed_ids.clone(), tx).await?;

                    Message::mark_read(local_ids, tx)
                        .await
                        .inspect_err(|e| error!("Failed to rollback unread on messages: {e:?}"))?;

                    Ok(())
                })
                .await?;
        }

        Ok(())
    }
}
