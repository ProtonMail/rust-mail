use crate::actions::{
    ConversationOrMessage, GenericActionData, GenericLabelRelatedActionData, MailActionError,
    filter_responses_by_codes,
};
use crate::datatypes::LocalConversationId;
use crate::datatypes::{ContextualConversation, RollbackItemType};
use crate::models::Conversation;
use proton_action_queue::action::{
    Action, ActionDependencyKeys, ActionId, DefaultVersionConverter, Handler, Type, WriterGuard,
};
use proton_action_queue::rebase::RebaseChangeSet;
use proton_core_api::consts::General;
use proton_core_api::session::Session;
use proton_core_common::datatypes::LocalLabelId;
use proton_core_common::models::ModelIdExtension;
use serde::{Deserialize, Serialize};
use stash::stash::Bond;
use tracing::error;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MarkRead {
    data: GenericLabelRelatedActionData<Conversation>,
    snooze_remind_ids: Vec<LocalConversationId>,
}

impl MarkRead {
    pub fn new(label_id: LocalLabelId, ids: impl IntoIterator<Item = LocalConversationId>) -> Self {
        Self {
            data: GenericLabelRelatedActionData::new(label_id, ids),
            snooze_remind_ids: Vec::new(),
        }
    }
}

impl Action for MarkRead {
    const TYPE: Type = Type("mark_conversations_read");
    const VERSION: u32 = 1;

    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = MarkReadHandler;
    type RemoteOutput = ();
    type LocalOutput = ();
    type Error = MailActionError;

    fn dependency_keys(&self) -> ActionDependencyKeys {
        self.data.read_unread_action_dependency_keys().build()
    }
}

pub struct MarkReadHandler {
    pub api: Session,
}

impl Handler for MarkReadHandler {
    type Action = MarkRead;

    async fn apply_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        // API call return an error 2501(Conversation was not updated) for conversation already read
        let conversations =
            Conversation::find_by_ids(action.data.data.target_ids.clone(), tx).await?;
        action.snooze_remind_ids = conversations
            .iter()
            .filter(|c| c.display_snooze_reminder)
            .filter_map(|c| c.local_id)
            .collect();
        action.data.data.target_ids = conversations
            .into_iter()
            .filter_map(|c| ContextualConversation::new(c, action.data.label_id))
            .filter(|c| c.num_unread > 0 || c.display_snooze_reminder)
            .map(|c| c.local_id)
            .collect();

        let ids = action.data.data.target_ids.clone();
        if ids.is_empty() {
            return Err(MailActionError::NoInput);
        }

        Conversation::mark_read_async(ids, tx).await?;
        Ok(())
    }

    async fn revert_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        if !action.snooze_remind_ids.is_empty() {
            Conversation::set_display_snooze_reminder(&action.snooze_remind_ids, tx).await?;
        }

        Conversation::mark_unread_async(
            action.data.label_id,
            action.data.data.target_ids.clone(),
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
        let (_, remote_target_ids) = action.data.resolve_ids_legacy(guard.tether()).await?;
        let responses =
            Conversation::mark_multiple_as_read_remote(remote_target_ids, &self.api).await?;

        // In this case General::NotExists is returned also for conversations already marked as read
        let failed_ids = filter_responses_by_codes(
            responses,
            &[General::NoError as u32, General::NotExists as u32],
        );

        if !failed_ids.is_empty() {
            error!("Mark read operation failed for: {:?}", failed_ids);
            guard
                .tx::<_, _, <Self::Action as Action>::Error>(async |tx| {
                    GenericActionData::<Conversation>::mark_rollback(
                        &failed_ids,
                        RollbackItemType::Conversation,
                        tx,
                    )
                    .await?;
                    let local_ids =
                        Conversation::remote_ids_counterpart(failed_ids.clone(), tx).await?;

                    Conversation::mark_unread_async(action.data.label_id, local_ids, tx)
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
