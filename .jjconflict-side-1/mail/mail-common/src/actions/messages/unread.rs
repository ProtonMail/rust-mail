use crate::actions::{
    ConversationOrMessage, GenericActionData, MailActionError, filter_responses_by_codes,
};
use crate::datatypes::{LocalMessageId, RollbackItemType};
use crate::models::Message;
use proton_action_queue::action::{
    Action, ActionDependencyKeys, ActionId, DefaultVersionConverter, Handler, Type, WriterGuard,
};
use proton_action_queue::rebase::RebaseChangeSet;
use proton_core_api::consts::General;
use proton_core_api::session::Session;
use proton_core_common::models::ModelIdExtension;
use proton_mail_api::services::proton::ProtonMail;
use serde::{Deserialize, Serialize};
use stash::UserDb;
use stash::stash::{Bond, RunTransaction};
use tracing::{error, info};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Unread(GenericActionData<Message>);

impl Unread {
    pub fn new(message_ids: impl IntoIterator<Item = LocalMessageId>) -> Self {
        Self(GenericActionData::new(message_ids))
    }
}

impl Action<UserDb> for Unread {
    const TYPE: Type = Type("mark_messages_unread");
    const VERSION: u32 = 1;

    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = UnreadHandler;
    type RemoteOutput = ();
    type LocalOutput = ();
    type Error = MailActionError;

    fn dependency_keys(&self) -> ActionDependencyKeys {
        self.0.read_unread_action_dependency_keys().build()
    }
}

pub struct UnreadHandler {
    pub api: Session,
}

impl Handler<UserDb> for UnreadHandler {
    type Action = Unread;

    async fn apply_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action<UserDb>>::Error> {
        action
            .0
            .apply_changes_sync(tx, |id, tx| Message::mark_read_or_unread(false, &[id], tx))
            .await?;

        Ok(())
    }

    async fn revert_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action<UserDb>>::Error> {
        Message::mark_read_async(action.0.target_ids_with_modifications(), tx).await?;
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
        // unread, so we only pass to apply_remote the things that were read.
        let message_ids = action.0.resolve_ids(guard.tether()).await?;
        if message_ids.is_empty() {
            return Ok(());
        }

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
                .run_tx_sync(move |tx| {
                    GenericActionData::<Message>::mark_rollback_sync(
                        &failed_ids,
                        RollbackItemType::Message,
                        tx,
                    )?;

                    let local_ids = Message::remote_ids_counterpart_sync(&failed_ids, tx)?;

                    Message::mark_read(local_ids, tx)
                        .inspect_err(|e| error!("Failed to rollback unread on messages: {e:?}"))?;

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
                Message::mark_read_or_unread(false, &[id], tx)
            })
            .await?;
        Ok(())
    }
}
