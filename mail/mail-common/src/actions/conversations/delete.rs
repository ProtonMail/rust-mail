use crate::actions::{filter_responses, ActionError, GenericActionData};
use crate::datatypes::RollbackItemType;
use crate::models::Conversation;
use crate::MailUserContext;
use proton_action_queue::action::{Action, DefaultVersionConverter, Type};
use proton_api_core::session::CoreSession;
use proton_core_common::datatypes::LocalLabelId;
use proton_core_common::models::{ModelExtension, ModelIdExtension};
use proton_mail_ids::LocalConversationId;
use serde::{self, Deserialize, Serialize};
use stash::stash::{Bond, Stash};
use tracing::error;

/// Delete conversations action.
///
/// This action permanently deletes the given conversations.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Delete(GenericActionData<Conversation>);

impl Delete {
    /// Create new instance.
    pub fn new(label_id: LocalLabelId, ids: impl IntoIterator<Item = LocalConversationId>) -> Self {
        Self(GenericActionData::new(label_id, ids))
    }
}

impl Action for Delete {
    const TYPE: Type = Type("delete_conversations");
    const VERSION: u32 = 1;
    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = Handler;
    type RemoteOutput = ();

    type LocalOutput = ();
    type Error = ActionError;
    type Context = MailUserContext;
}

#[derive(Default)]
pub struct Handler {}

impl proton_action_queue::action::Handler for Handler {
    type Action = Delete;

    type Context = MailUserContext;

    async fn apply_local(
        &self,
        _: &Self::Context,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<<Self::Action as Action>::LocalOutput, <Self::Action as Action>::Error> {
        if action.0.target_ids.is_empty() {
            return Err(ActionError::NoInput);
        }

        Conversation::mark_deleted(action.0.label_id, action.0.target_ids.clone(), tx).await?;

        Ok(())
    }

    async fn revert_local(
        &self,
        _: &Self::Context,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        Conversation::mark_undeleted(action.0.label_id, action.0.target_ids.clone(), tx).await?;
        action
            .0
            .mark_rollback(RollbackItemType::Conversation, tx)
            .await?;

        Ok(())
    }

    async fn apply_remote(
        &self,
        ctx: &Self::Context,
        action: &mut Self::Action,
        stash: &Stash,
    ) -> Result<<Self::Action as Action>::RemoteOutput, <Self::Action as Action>::Error> {
        let remote_label_id = action
            .0
            .remote_label_id
            .clone()
            .expect("Should not be none");

        let mut conn = stash.connection();

        action.0.resolve_ids(&conn).await?;

        let local_ids_without_remote_id = action
            .0
            .local_only_ids(&conn)
            .await
            .inspect_err(|e| error!("Failed to load local only ids: {e})"))?;

        let failed_ids = if action.0.remote_target_ids.is_empty() {
            vec![]
        } else {
            let responses = Conversation::delete_multiple_remote(
                action.0.remote_target_ids.clone(),
                remote_label_id,
                ctx.session().api(),
            )
            .await
            .map_err(|e| {
                error!("Failed to delete conversations on API: {e}");
                e
            })?;

            filter_responses(responses)
        };

        if !failed_ids.is_empty() || !local_ids_without_remote_id.is_empty() {
            let tx = conn.transaction().await?;
            if !failed_ids.is_empty() {
                error!("Delete operation failed for: {:?}", failed_ids);
                let local_ids =
                    Conversation::remote_ids_counterpart(failed_ids.clone(), &tx).await?;

                Conversation::remove_label(action.0.label_id, local_ids, &tx)
                    .await
                    .map_err(|e| {
                        error!("Failed to rollback failed conversations: {e}");
                        e
                    })?;
            }

            for id in local_ids_without_remote_id {
                // All messages associated with this conversation are also purged.
                Conversation::delete_by_id(id, &tx)
                    .await
                    .inspect_err(|e| error!("Failed to delete local conversation: {e}"))?;
            }

            tx.commit().await?;
        }
        Ok(())
    }
}
