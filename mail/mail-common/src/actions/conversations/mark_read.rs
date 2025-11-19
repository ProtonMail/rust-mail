use crate::actions::{
    ConversationOrMessage, GenericActionData, GenericLabelRelatedActionData, MailActionError,
    filter_responses_by_codes,
};
use crate::datatypes::LocalConversationId;
use crate::datatypes::RollbackItemType;
use crate::models::{Conversation, Message};
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
        let conversations =
            Conversation::find_by_ids(action.data.data.target_ids.clone(), tx).await?;
        action.snooze_remind_ids = conversations
            .iter()
            .filter(|c| c.display_snooze_reminder)
            .filter_map(|c| c.local_id)
            .collect();

        action
            .data
            .apply_changes_sync(tx, |id, tx| Conversation::mark_read([id], tx))
            .await?;

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

        let modified_message_ids = action.data.modified_message_ids();
        Message::mark_unread_async(modified_message_ids, tx).await?;
        Ok(())
    }

    async fn apply_remote(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        mut guard: WriterGuard<'_>,
    ) -> Result<<Self::Action as Action>::RemoteOutput, <Self::Action as Action>::Error> {
        let (_, remote_target_ids) = action.data.resolve_ids(guard.tether()).await?;

        // API call return an error 2501(Conversation was not updated) for conversation already read
        if remote_target_ids.is_empty() {
            return Ok(());
        }
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
        _: ActionId,
        action: &mut Self::Action,
        changeset: &RebaseChangeSet,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        action
            .data
            .rebase_changes_sync(changeset, tx, |id, modified, tx| {
                if !modified.is_empty() {
                    Message::mark_read_or_unread(
                        false,
                        &modified.iter().copied().collect::<Vec<_>>(),
                        tx,
                    )?;
                }
                Conversation::mark_read([id], tx)
            })
            .await?;
        Ok(())
    }
}
