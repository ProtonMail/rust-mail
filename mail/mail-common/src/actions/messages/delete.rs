use crate::actions::{filter_responses, ActionError, GenericActionData};
use crate::datatypes::{LocalMessageId, RollbackItemType};
use crate::models::{Conversation, Message};
use crate::MailUserContext;
use proton_action_queue::action::Handler as ActionHandler;
use proton_action_queue::action::{Action, DefaultVersionConverter, Type};
use proton_api_core::session::CoreSession;
use proton_api_mail::services::proton::ProtonMail;
use proton_core_common::datatypes::LocalLabelId;
use proton_core_common::models::{ModelExtension, ModelIdExtension};
use proton_mail_ids::LocalConversationId;
use serde::{Deserialize, Serialize};
use stash::exports::SqliteError;
use stash::orm::Model;
use stash::params;
use stash::stash::{Bond, Stash, StashError};
use tracing::error;

/// Action which marks messages as deleted.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Delete(GenericActionData<Message>);

impl Delete {
    /// Create a new instance which marks the messages as deleted.
    pub fn new(
        label_id: LocalLabelId,
        message_ids: impl IntoIterator<Item = LocalMessageId>,
    ) -> Self {
        Self(GenericActionData::new(label_id, message_ids))
    }
}

impl Action for Delete {
    const TYPE: Type = Type("delete_messages");
    const VERSION: u32 = 1;
    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = Handler;
    type RemoteOutput = ();

    type LocalOutput = ();
    type Error = ActionError;

    type Context = MailUserContext;
}

#[derive(Default)]
pub struct Handler;

impl ActionHandler for Handler {
    type Action = Delete;

    type Context = MailUserContext;
    async fn apply_local(
        &self,
        _: &Self::Context,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        if action.0.target_ids.is_empty() {
            return Err(ActionError::NoInput);
        }

        Message::mark_deleted(action.0.target_ids.clone(), tx).await?;
        Ok(())
    }

    async fn revert_local(
        &self,
        _: &Self::Context,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        Message::mark_undeleted(action.0.target_ids.clone(), tx).await?;
        action
            .0
            .mark_rollback(RollbackItemType::Message, tx)
            .await?;
        Ok(())
    }

    async fn apply_remote(
        &self,
        ctx: &Self::Context,
        action: &mut Self::Action,
        stash: &Stash,
    ) -> Result<<Self::Action as Action>::RemoteOutput, <Self::Action as Action>::Error> {
        let mut conn = stash.connection();

        action.0.resolve_ids(&conn).await?;

        let local_ids_without_remote_id = action
            .0
            .unsynced_item_ids(&conn)
            .await
            .inspect_err(|e| error!("Failed to load local only ids: {e}"))?;

        let failed_ids = if action.0.remote_target_ids.is_empty() {
            vec![]
        } else {
            let api = ctx.session().api();
            let message_ids = action
                .0
                .remote_target_ids
                .clone()
                .into_iter()
                .map(Into::into)
                .collect();
            let label_id = action.0.remote_label_id.clone();
            let response = api
                .put_messages_delete(message_ids, label_id)
                .await?
                .responses;

            filter_responses(response)
        };

        if !failed_ids.is_empty() || !local_ids_without_remote_id.is_empty() {
            error!("Delete messages operation failed for: {failed_ids:?}");

            let tx = conn.transaction().await?;

            if !failed_ids.is_empty() {
                let local_ids = Message::remote_ids_counterpart(failed_ids.clone(), &tx).await?;

                Message::mark_undeleted(local_ids, &tx)
                    .await
                    .inspect_err(|e| error!("Failed to rollback delete on messages: {e}"))?;
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
                        error!("Failed to get conversation id: {e}");
                        Err(e.into())
                    },
                } {
                    // We should only delete orphaned conversations.
                    let conversation_message_count = tx.query_value::<_, usize>(
                        format!("SELECT COUNT(*) AS value FROM {} WHERE local_conversation_id=? AND deleted=0", Message::table_name()), params![conv_id]).await?;
                    if conversation_message_count == 0 {
                        Conversation::delete_by_id(conv_id, &tx)
                            .await
                            .inspect_err(|e| {
                                error!("Failed to delete orphaned conversation: {e}")
                            })?;
                    }
                }

                Message::delete_by_id(id, &tx)
                    .await
                    .inspect_err(|e| error!("Failed to delete message: {e}"))?;
            }

            tx.commit().await?;
        }
        Ok(())
    }
}
