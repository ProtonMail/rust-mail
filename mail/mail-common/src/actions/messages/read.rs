use crate::actions::{
    ConversationOrMessage, GenericActionData, MailActionError, filter_responses_by_codes,
};
use crate::datatypes::{LocalMessageId, RollbackItemType};
use crate::models::Message;
use proton_action_queue::action::{
    Action, ActionDependencyKeys, DefaultVersionConverter, Type, WriterGuard,
};
use proton_action_queue::action::{ActionId, Handler};
use proton_core_api::consts::General;
use proton_core_api::session::Session;
use proton_core_common::models::ModelIdExtension;
use proton_mail_api::services::proton::ProtonMail;
use serde::{Deserialize, Serialize};
use stash::stash::Bond;
use tracing::{error, info};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum ReadRemoteStrategy {
    #[default]
    Enabled,
    Disabled,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Read(
    GenericActionData<Message>,
    #[serde(default)] ReadRemoteStrategy,
);

impl Read {
    pub fn new(message_ids: impl IntoIterator<Item = LocalMessageId>) -> Self {
        Self(
            GenericActionData::new(message_ids),
            ReadRemoteStrategy::Enabled,
        )
    }

    pub fn for_push_notification(message_id: LocalMessageId) -> Self {
        Self(
            GenericActionData::new(std::iter::once(message_id)),
            ReadRemoteStrategy::Disabled,
        )
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

    fn dependency_keys(&self) -> ActionDependencyKeys {
        self.0.read_unread_action_dependency_keys().build()
    }
}

pub struct ReadHandler {
    pub api: Session,
}

impl Handler for ReadHandler {
    type Action = Read;

    async fn apply_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        // API call return an error 2501(Message does not exist) for message already
        // read, so we only pass to apply_remote the things that were unread.

        action.0.target_ids =
            Message::mark_read_async(action.0.target_ids.iter().copied(), tx).await?;

        if action.0.target_ids.is_empty() {
            tracing::warn!("mark read doesn't do anything.");
            return Ok(());
        }
        Ok(())
    }

    async fn revert_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        Message::mark_unread_async(action.0.target_ids.clone(), tx).await?;
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
        if action.0.target_ids.is_empty() {
            return Ok(());
        }
        if action.1 == ReadRemoteStrategy::Disabled {
            return Ok(());
        }

        let message_ids =
            Message::local_ids_counterpart(action.0.target_ids.clone(), guard.tether()).await?;
        info!("Marking {message_ids:?} as read");

        let response = self
            .api
            .put_messages_read(message_ids, None, None)
            .await?
            .responses;

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

                    Message::mark_unread_async(local_ids, tx)
                        .await
                        .inspect_err(|e| error!("Failed to rollback read on messages: {e:?}"))?;
                    Ok(())
                })
                .await?;
        }
        Ok(())
    }
}
