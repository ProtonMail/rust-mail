use crate::AppError;
use crate::actions::{GenericLabelRelatedActionData, MailActionError, filter_responses};
use crate::datatypes::LocalConversationId;
use crate::datatypes::RollbackItemType;
use crate::models::Conversation;
use proton_action_queue::action::{
    Action, ActionDependencyKeys, ActionId, DefaultVersionConverter, Handler, Type, WriterGuard,
};
use proton_core_api::session::Session;
use proton_core_common::datatypes::LocalLabelId;
use proton_core_common::models::{ModelExtension, ModelIdExtension};
use serde::{self, Deserialize, Serialize};
use stash::stash::Bond;
use tracing::error;

/// Delete conversations action.
///
/// This action permanently deletes the given conversations.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Delete(GenericLabelRelatedActionData<Conversation>);

impl Delete {
    pub fn new(label_id: LocalLabelId, ids: impl IntoIterator<Item = LocalConversationId>) -> Self {
        Self(GenericLabelRelatedActionData::new(label_id, ids))
    }
}

impl Action for Delete {
    const TYPE: Type = Type("delete_conversations");
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
    ) -> Result<<Self::Action as Action>::LocalOutput, <Self::Action as Action>::Error> {
        if action.0.data.target_ids.is_empty() {
            return Err(MailActionError::NoInput);
        }

        Conversation::mark_deleted(action.0.label_id, action.0.data.target_ids.clone(), tx).await?;

        Ok(())
    }

    async fn revert_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        Conversation::mark_undeleted(action.0.label_id, action.0.data.target_ids.clone(), tx)
            .await?;
        action
            .0
            .mark_rollback(RollbackItemType::Conversation, tx)
            .await?;

        Ok(())
    }

    async fn apply_remote(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        mut guard: WriterGuard<'_>,
    ) -> Result<<Self::Action as Action>::RemoteOutput, <Self::Action as Action>::Error> {
        action.0.resolve_ids(guard.tether()).await?;
        let remote_label_id = action
            .0
            .remote_label_id
            .clone()
            .ok_or_else(|| AppError::LabelDoesNotHaveRemoteId(action.0.label_id))?;

        let local_ids_without_remote_id = action
            .0
            .unsynced_item_ids(guard.tether())
            .await
            .inspect_err(|e| error!("Failed to load local only ids: {e:?})"))?;

        let failed_ids = if action.0.data.remote_target_ids.is_empty() {
            vec![]
        } else {
            let responses = Conversation::delete_multiple_remote(
                action.0.data.remote_target_ids.clone(),
                remote_label_id,
                &self.api,
            )
            .await
            .map_err(|e| {
                error!("Failed to delete conversations on API: {e:?}");
                e
            })?;

            filter_responses(responses)
        };

        if !failed_ids.is_empty() || !local_ids_without_remote_id.is_empty() {
            guard
                .tx::<_, _, <Self::Action as Action>::Error>(async |tx| {
                    if !failed_ids.is_empty() {
                        error!("Delete operation failed for: {:?}", failed_ids);
                        let local_ids =
                            Conversation::remote_ids_counterpart(failed_ids.clone(), tx).await?;

                        Conversation::remove_label(action.0.label_id, local_ids, tx)
                            .await
                            .map_err(|e| {
                                error!("Failed to rollback failed conversations: {e:?}");
                                e
                            })?;
                    }

                    for id in local_ids_without_remote_id {
                        // All messages associated with this conversation are also purged.
                        Conversation::delete_by_id(id, tx).await.inspect_err(|e| {
                            error!("Failed to delete local conversation: {e:?}")
                        })?;
                    }

                    Ok(())
                })
                .await?;
        }
        Ok(())
    }
}
