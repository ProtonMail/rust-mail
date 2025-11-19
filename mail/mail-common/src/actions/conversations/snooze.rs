use crate::AppError;
use crate::actions::{GenericLabelRelatedActionData, MailActionError, filter_responses};
use crate::datatypes::LocalConversationId;
use crate::models::Conversation;
use proton_action_queue::action::{
    Action, ActionDependencyKeys, ActionId, DefaultVersionConverter, Handler, Type, WriterGuard,
};
use proton_action_queue::rebase::RebaseChangeSet;
use proton_core_api::session::Session;
use proton_core_common::datatypes::{LocalLabelId, UnixTimestamp};
use proton_core_common::models::ModelIdExtension;
use proton_mail_api::services::proton::ProtonMail;
use serde::{self, Deserialize, Serialize};
use stash::stash::Bond;
use tracing::error;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Snooze {
    action_data: GenericLabelRelatedActionData<Conversation>,
    snooze_until: UnixTimestamp,
}

impl Snooze {
    pub fn new(
        label_id: LocalLabelId,
        ids: impl IntoIterator<Item = LocalConversationId>,
        snooze_until: UnixTimestamp,
    ) -> Self {
        Self {
            action_data: GenericLabelRelatedActionData::new(label_id, ids),
            snooze_until,
        }
    }
}

impl Action for Snooze {
    const TYPE: Type = Type("snooze_conversations");
    const VERSION: u32 = 1;

    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = SnoozeHandler;
    type RemoteOutput = ();
    type LocalOutput = ();
    type Error = MailActionError;

    fn dependency_keys(&self) -> ActionDependencyKeys {
        self.action_data
            .snooze_unsnooze_action_dependency_keys()
            .build()
    }
}

pub struct SnoozeHandler {
    pub api: Session,
}

impl Handler for SnoozeHandler {
    type Action = Snooze;

    async fn apply_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<<Self::Action as Action>::LocalOutput, <Self::Action as Action>::Error> {
        if action.action_data.data.target_ids.is_empty() {
            return Err(MailActionError::NoInput);
        }

        Conversation::snooze(
            action.action_data.label_id,
            &action.action_data.data.target_ids,
            action.snooze_until,
            tx,
        )
        .await?;

        Ok(())
    }

    async fn revert_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        Conversation::unsnooze(
            action.action_data.label_id,
            &action.action_data.data.target_ids,
            tx,
        )
        .await?;

        Ok(())
    }

    async fn apply_remote(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        mut guard: WriterGuard<'_>,
    ) -> Result<<Self::Action as Action>::RemoteOutput, <Self::Action as Action>::Error> {
        let (_, remote_target_ids) = action
            .action_data
            .resolve_ids_legacy(guard.tether())
            .await?;

        if remote_target_ids.is_empty() {
            tracing::warn!(
                "No remote target ids to snooze, local only ids: {:?}",
                action.action_data.data.target_ids
            );
            return Ok(());
        }

        let now = UnixTimestamp::now();
        if action.snooze_until <= now {
            return Err(MailActionError::App(AppError::SnoozeTimeInThePast));
        }

        let response = self
            .api
            .put_conversations_snooze(remote_target_ids, action.snooze_until.as_u64())
            .await?;

        let responses = filter_responses(response.responses);

        if !responses.is_empty() {
            guard
                .tx::<_, _, <Self::Action as Action>::Error>(async |tx| {
                    error!("Snooze operation failed for: {:?}", responses);

                    let local_ids =
                        Conversation::remote_ids_counterpart(responses.clone(), tx).await?;

                    Conversation::unsnooze(action.action_data.label_id, &local_ids, tx)
                        .await
                        .map_err(|e| {
                            error!("Failed to rollback failed conversations: {e:?}");
                            e
                        })?;

                    Ok(())
                })
                .await?;
        }

        Ok(())
    }
    async fn rebase_local(
        &self,
        this_id: ActionId,
        action: &mut Self::Action,
        _: &RebaseChangeSet,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        //TODO(ET-5183): Test me!
        self.apply_local(this_id, action, tx).await?;
        Ok(())
    }
}
