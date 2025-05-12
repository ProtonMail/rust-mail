use crate::MailUserContext;
use crate::actions::{GenericLabelRelatedActionData, MailActionError, filter_responses};
use crate::datatypes::{LocalMessageId, RollbackItemType};
use crate::models::{Conversation, Message};
use proton_action_queue::action::{Action, DefaultVersionConverter, Type, WriterGuard};
use proton_action_queue::action::{ActionId, Handler as ActionHandler};
use proton_core_common::datatypes::LocalLabelId;
use proton_core_common::models::{ModelExtension, ModelIdExtension};
use proton_mail_api::services::proton::ProtonMail;
use proton_mail_ids::LocalConversationId;
use serde::{Deserialize, Serialize};
use stash::exports::SqliteError;
use stash::orm::Model;
use stash::params;
use stash::stash::{Bond, StashError};
use tracing::error;

/// Action which marks messages as deleted.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Delete(GenericLabelRelatedActionData<Message>);

impl Delete {
    /// Create a new instance which marks the messages as deleted.
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
    type Handler = Handler;
    type RemoteOutput = ();

    type LocalOutput = ();
    type Error = MailActionError;

    type Context = MailUserContext;
}

#[derive(Default)]
pub struct Handler;

impl ActionHandler for Handler {
    type Action = Delete;

    type Context = MailUserContext;
    async fn apply_local(
        &self,
        _: ActionId,
        _: &Self::Context,
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
        _: &Self::Context,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        Message::mark_undeleted(action.0.data.target_ids.clone(), tx).await?;
        action
            .0
            .mark_rollback(RollbackItemType::Message, tx)
            .await?;
        Ok(())
    }

    async fn apply_remote(
        &self,
        _: ActionId,
        ctx: &Self::Context,
        action: &mut Self::Action,
        mut guard: WriterGuard<'_>,
    ) -> Result<<Self::Action as Action>::RemoteOutput, <Self::Action as Action>::Error> {
        action.0.resolve_ids(guard.tether()).await?;
        let local_ids_without_remote_id = action
            .0
            .unsynced_item_ids(guard.tether())
            .await
            .inspect_err(|e| error!("Failed to load local only ids: {e:?}"))?;

        let failed_ids = if action.0.data.remote_target_ids.is_empty() {
            vec![]
        } else {
            let api = ctx.api();
            let message_ids = action.0.data.remote_target_ids.clone();
            let label_id = action.0.remote_label_id.clone();
            let response = api
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
                        let local_ids = Message::remote_ids_counterpart(failed_ids.clone(), tx).await?;

                        Message::mark_undeleted(local_ids, tx)
                            .await
                            .inspect_err(|e| error!("Failed to rollback delete on messages: {e:?}"))?;
                    }

                    for id in local_ids_without_remote_id {
                        if let Some(conv_id) = match tx.query_value::<_, LocalConversationId>(
                            format!(
                                "SELECT {} AS value FROM {} WHERE remote_id IS NULL AND {} IN (SELECT local_conversation_id FROM {} WHERE {} = ?)",
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
                                format!("SELECT COUNT(*) AS value FROM {} WHERE local_conversation_id=? AND deleted=0", Message::table_name()), params![conv_id]).await?;
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
}
