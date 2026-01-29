use crate::actions::{
    GenericActionData, GenericLabelRelatedActionData, MailActionError, filter_responses,
};
use crate::datatypes::LocalConversationId;
use crate::datatypes::{LocalMessageId, RollbackItemType};
use crate::models::{Conversation, Message};
#[cfg(feature = "foundation_search")]
use crate::search::MailSearchService;
use proton_action_queue::action::{
    Action, ActionDependencyKeys, ActionId, DefaultVersionConverter, Handler, Type, WriterGuard,
};
use proton_action_queue::rebase::{RebaseChangeSet, RebaseKey};
use proton_core_api::session::Session;
use proton_core_common::datatypes::LocalLabelId;
use proton_core_common::models::{ModelExtension, ModelIdExtension};
use proton_mail_api::services::proton::ProtonMail;
use serde::{Deserialize, Serialize};
use stash::exports::SqliteError;
use stash::orm::Model;
use stash::stash::{Bond, StashError};
use stash::{UserDb, params};
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

impl Action<UserDb> for Delete {
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

impl Handler<UserDb> for DeleteHandler {
    type Action = Delete;

    async fn apply_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action<UserDb>>::Error> {
        if action.0.data.target_ids.is_empty() {
            return Err(MailActionError::NoInput);
        }

        Message::mark_deleted(action.0.data.target_ids.clone(), tx).await?;

        // We only remove from the index in apply_remote after permanent deletion succeeds.
        // This handler is only used for PERMANENT deletion (e.g., "Empty Trash", "Delete Permanently").
        // When users swipe to trash/archive, that uses MoveHandler, not DeleteHandler, so messages
        // in trash/archive remain searchable.

        Ok(())
    }

    async fn revert_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action<UserDb>>::Error> {
        let message_ids = action.0.data.target_ids.clone();
        Message::mark_undeleted(message_ids, tx).await?;

        // Note: No need to re-index on undelete. Message content hasn't changed.
        // If a pending "remove" intent exists, it will be a no-op (document already gone
        // or still present). The message will be re-indexed if its body is accessed again.

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
        let (label_id, remote_target_ids) = action.0.resolve_ids_legacy(guard.tether()).await?;

        let local_ids_without_remote_id = action
            .0
            .unsynced_item_ids(guard.tether())
            .await
            .inspect_err(|e| error!("Failed to load local only ids: {e:?}"))?;

        let failed_ids = if remote_target_ids.is_empty() {
            vec![]
        } else {
            let message_ids = remote_target_ids.clone();

            info!("Deleting {message_ids:?}");

            let response = self
                .api
                .put_messages_delete(message_ids, label_id)
                .await?
                .responses;

            filter_responses(response)
        };

        // Track which local-only messages were permanently deleted (for search removal)
        // Note: Messages with remote_id will be handled by the event subscriber when
        // the delete event is processed, so we don't need to queue search removal for them here.
        #[cfg(feature = "foundation_search")]
        let mut permanently_deleted_ids = Vec::new();

        if !failed_ids.is_empty() || !local_ids_without_remote_id.is_empty() {
            error!("Delete messages operation failed for: {failed_ids:?}");

            guard.tx::<_,_, <Self::Action as Action<UserDb>>::Error>(
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

                        // Track for search removal (feature-gated)
                        #[cfg(feature = "foundation_search")]
                        {
                        permanently_deleted_ids.push(id);
                        }
                    }

                    Ok(())
                }
            ).await?;
        }

        // Queue search removal intents for all permanently deleted messages
        #[cfg(feature = "foundation_search")]
        if !permanently_deleted_ids.is_empty() {
            guard
                .tx::<_, _, <Self::Action as Action<UserDb>>::Error>(async |tx| {
                    for id in &permanently_deleted_ids {
                        if let Err(e) = MailSearchService::queue_remove(id.as_u64(), tx).await {
                            error!("Failed to queue search removal for message {}: {:?}", id, e);
                            // Continue with other messages even if one fails
                        }
                    }
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
        for id in &action.0.data.target_ids {
            let rebase_key: RebaseKey = (*id).into();
            if changeset.contains(&rebase_key) {
                Message::mark_deleted(vec![*id], tx).await?;
            }
        }
        Ok(())
    }
}
