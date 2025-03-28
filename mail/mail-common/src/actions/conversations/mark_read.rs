use crate::MailUserContext;
use crate::actions::{GenericActionData, MailActionError, filter_responses_by_codes};
use crate::datatypes::{ContextualConversation, RollbackItemType};
use crate::models::Conversation;
use proton_action_queue::action::{Action, ActionId, DefaultVersionConverter, Type, WriterGuard};
use proton_api_core::consts::General;
use proton_api_core::services::proton::Proton;
use proton_core_common::datatypes::LocalLabelId;
use proton_core_common::models::ModelIdExtension;
use proton_mail_ids::LocalConversationId;
use serde::{Deserialize, Serialize};
use stash::stash::Bond;
use tracing::error;

/// Action to mark conversations read.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MarkRead(GenericActionData<Conversation>);

impl MarkRead {
    /// Create a new action which marks the conversations with `ids` as read.
    pub fn new(label_id: LocalLabelId, ids: impl IntoIterator<Item = LocalConversationId>) -> Self {
        // TODO(db-tests): label_id was present in the original action, why was it used.
        Self(GenericActionData::new(label_id, ids))
    }
}

impl Action for MarkRead {
    const TYPE: Type = Type("mark_conversations_read");
    const VERSION: u32 = 1;
    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = Handler;
    type RemoteOutput = ();

    type LocalOutput = ();
    type Error = MailActionError;
    type Context = MailUserContext;
}

#[derive(Default)]
pub struct Handler {}

impl proton_action_queue::action::Handler for Handler {
    type Action = MarkRead;
    type Context = MailUserContext;

    async fn apply_local(
        &self,
        _: ActionId,
        _: &Self::Context,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        // API call return an error 2501(Conversation was not updated) for conversation already read
        let conversations = Conversation::find_by_ids(action.0.target_ids.clone(), tx).await?;
        action.0.target_ids = conversations
            .into_iter()
            .filter_map(|c| ContextualConversation::new(c, action.0.label_id))
            .filter(|c| c.num_unread > 0)
            .map(|c| c.local_id)
            .collect();

        action.0.resolve_ids(tx).await?;

        Conversation::mark_read(action.0.target_ids.clone(), tx).await?;
        Ok(())
    }

    async fn revert_local(
        &self,
        _: ActionId,
        _: &Self::Context,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        Conversation::mark_unread(action.0.label_id, action.0.target_ids.clone(), tx).await?;
        action
            .0
            .mark_rollback(RollbackItemType::Conversation, tx)
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
        let responses = Conversation::mark_multiple_as_read_remote::<Proton>(
            action.0.remote_target_ids.clone(),
            ctx.api(),
        )
        .await?;

        // In this case General::NotExists is returned also for conversations already marked as read
        let failed_ids = filter_responses_by_codes(
            responses,
            &[General::NoError as u32, General::NotExists as u32],
        );

        if !failed_ids.is_empty() {
            error!("Mark read operation failed for: {:?}", failed_ids);
            guard
                .tx::<_, _, <Self::Action as Action>::Error>(async |tx| {
                    let local_ids =
                        Conversation::remote_ids_counterpart(failed_ids.clone(), tx).await?;

                    Conversation::mark_unread(action.0.label_id, local_ids, tx)
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
}
