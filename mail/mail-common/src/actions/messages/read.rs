use crate::actions::{GenericActionData, MailActionError, filter_responses_by_codes};
use crate::datatypes::{LocalMessageId, RollbackItemType};
use crate::models::Message;
use proton_action_queue::action::{
    Action, ActionDependencyKeys, DefaultVersionConverter, Type, WriterGuard,
};
use proton_action_queue::action::{ActionId, Handler};
use proton_action_queue::rebase::RebaseChangeSet;
use proton_core_api::consts::General;
use proton_core_api::session::Session;
use proton_core_common::models::ModelIdExtension;
use proton_mail_api::services::proton::ProtonMail;
use serde::{Deserialize, Serialize};
use stash::UserDb;
use stash::stash::Bond;
use tracing::{error, info};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Read(GenericActionData<Message>);

impl Read {
    pub fn new(message_ids: impl IntoIterator<Item = LocalMessageId>) -> Self {
        Self(GenericActionData::new(message_ids))
    }

    pub fn single(message_id: LocalMessageId) -> Self {
        Self(GenericActionData::new(std::iter::once(message_id)))
    }
}

impl Action<UserDb> for Read {
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

impl Handler<UserDb> for ReadHandler {
    type Action = Read;

    async fn apply_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action<UserDb>>::Error> {
        action
            .0
            .apply_changes_sync(tx, |id, tx| Message::mark_read_or_unread(true, &[id], tx))
            .await?;
        Ok(())
    }

    async fn revert_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action<UserDb>>::Error> {
        Message::mark_unread_async(action.0.target_ids_with_modifications(), tx).await?;
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
        // API call return an error 2501(Message does not exist) for message already
        // read, so we only pass to apply_remote the things that were unread.
        let message_ids = action.0.resolve_ids(guard.tether()).await?;
        if message_ids.is_empty() {
            return Ok(());
        }
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
                .tx::<_, _, <Self::Action as Action<UserDb>>::Error>(async |tx| {
                    GenericActionData::<Message>::mark_rollback(
                        &failed_ids,
                        RollbackItemType::Message,
                        tx,
                    )
                    .await?;

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

    async fn rebase_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        changeset: &RebaseChangeSet,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action<UserDb>>::Error> {
        action
            .0
            .rebase_changes_sync(changeset, tx, |id, _, tx| {
                Message::mark_read_or_unread(true, &[id], tx)
            })
            .await?;
        Ok(())
    }
}
