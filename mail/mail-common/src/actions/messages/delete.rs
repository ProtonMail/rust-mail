use crate::actions::{
    GenericActionData, GenericLabelRelatedActionData, MailActionError, filter_responses,
};
use crate::datatypes::LocalConversationId;
use crate::datatypes::{LocalMessageId, RollbackItemType};
use crate::models::{Conversation, Message};
use proton_action_queue::action::{
    Action, ActionDependencyKeys, DefaultVersionConverter, Type, WriterGuard,
};
use proton_action_queue::action::{ActionId, Handler};
use proton_action_queue::rebase::RebaseChangeSet;
use proton_core_api::session::Session;
use proton_core_common::datatypes::LocalLabelId;
use proton_core_common::models::{ModelExtension, ModelIdExtension};
use proton_mail_api::services::proton::ProtonMail;
use serde::{Deserialize, Serialize};
use stash::exports::SqliteError;
use stash::orm::Model;
use stash::params;
use stash::stash::{Bond, StashError};
use tracing::{error, info};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Delete(GenericLabelRelatedActionData<Message>);

impl Delete {
    pub fn new(
        label_id: LocalLabelId,
        message_ids: impl IntoIterator<Item = LocalMessageId>,
    ) -> Self {
        Self(GenericLabelRelatedActionData::new(label_id, message_ids))
    }
}

impl Action for Delete {
    const TYPE: Type = Type("delete_messages");
    const VERSION: u32 = 1;

    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = DeleteHandler;
    type RemoteOutput = ();
    type LocalOutput = ();
    type Error = MailActionError;

    fn dependency_keys(&self) -> ActionDependencyKeys {
        self.0.action_dependency_keys_builder_optional().build()
    }
}

pub struct DeleteHandler {
    pub api: Session,
}

impl Handler for DeleteHandler {
    type Action = Delete;

    async fn apply_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        if action.0.data.target_ids.is_empty() {
            return Err(MailActionError::NoInput);
        }

        Message::mark_deleted(action.0.data.target_ids.clone(), tx).await?;
        Ok(())
    }

    async fn revert_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        Message::mark_undeleted(action.0.data.target_ids.clone(), tx).await?;
        Ok(())
    }

    async fn apply_remote(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        mut guard: WriterGuard<'_>,
    ) -> Result<<Self::Action as Action>::RemoteOutput, <Self::Action as Action>::Error> {
        let (label_id, remote_target_ids) = action.0.resolve_ids_legacy(guard.tether()).await?;

        let local_ids_without_remote_id = action
            .0
            .unsynced_item_ids(guard.tether())
            .await
            .inspect_err(|e| error!("Failed to load local only ids: {e:?}"))?;

        let failed_ids = if remote_target_ids.is_empty() {
            vec![]
        } else {
            let message_ids = remote_target_ids;

            info!("Deleting {message_ids:?}");

            let response = self
                .api
                .put_messages_delete(message_ids, label_id)
                .await?
                .responses;

            filter_responses(response)
        };

        if !failed_ids.is_empty() || !local_ids_without_remote_id.is_empty() {
            error!("Delete messages operation failed for: {failed_ids:?}");

            guard.tx::<_,_, <Self::Action as Action>::Error>(
                async |tx| {
                    if !failed_ids.is_empty() {
                        GenericActionData::<Message>::mark_rollback(&failed_ids, RollbackItemType::Message, tx).await?;
                        let local_ids = Message::remote_ids_counterpart(failed_ids.clone(), tx).await?;

                        Message::mark_undeleted(local_ids, tx)
                            .await
                            .inspect_err(|e| error!("Failed to rollback delete on messages: {e:?}"))?;
                    }

                    for id in local_ids_without_remote_id {
                        if let Some(conv_id) = match tx.query_value::<_, LocalConversationId>(
                            format!(
                                "SELECT {} FROM {} WHERE remote_id IS NULL AND {} IN (SELECT local_conversation_id FROM {} WHERE {} = ?)",
                                Conversation::id_field_name(),
                                Conversation::table_name(),
                                Conversation::id_field_name(),
                                Message::table_name(),
                                Message::id_field_name()
                            ),
                            params![id],
                        )
                            .await {
                            Ok(conv_id) => Some(conv_id),
                            Err(StashError::ExecutionError(SqliteError::QueryReturnedNoRows)) => None,
                            Err(e) => return {
                                error!("Failed to get conversation id: {e:?}");
                                Err(e.into())
                            },
                        } {
                            // We should only delete orphaned conversations.
                            let conversation_message_count = tx.query_value::<_, usize>(
                                format!("SELECT COUNT(*) FROM {} WHERE local_conversation_id=? AND deleted=0", Message::table_name()), params![conv_id]).await?;
                            if conversation_message_count == 0 {
                                Conversation::delete_by_id(conv_id, tx)
                                    .await
                                    .inspect_err(|e| {
                                        error!("Failed to delete orphaned conversation: {e:?}")
                                    })?;
                            }
                        }

                        Message::delete_by_id(id, tx)
                            .await
                            .inspect_err(|e| error!("Failed to delete message: {e:?}"))?;
                    }
                    Ok(())
                }
            ).await?;
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
