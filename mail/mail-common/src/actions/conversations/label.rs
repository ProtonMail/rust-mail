use crate::MailUserContext;
use crate::actions::{GenericLabelRelatedActionData, MailActionError, filter_responses};
use crate::datatypes::RollbackItemType;
use crate::models::Conversation;
use proton_action_queue::action::{Action, ActionId, DefaultVersionConverter, Type, WriterGuard};
use proton_core_api::services::proton::Proton;
use proton_core_common::datatypes::LocalLabelId;
use proton_core_common::models::ModelIdExtension;
use proton_mail_ids::LocalConversationId;
use serde::{Deserialize, Serialize};
use stash::stash::Bond;
use tracing::error;

/// Action which applies a label to conversations.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Label(GenericLabelRelatedActionData<Conversation>);

impl Label {
    /// Create a new instance which applies `label_id` to the conversations with `ids`.
    pub fn new(label_id: LocalLabelId, ids: impl IntoIterator<Item = LocalConversationId>) -> Self {
        Self(GenericLabelRelatedActionData::new(label_id, ids))
    }
}

impl Action for Label {
    const TYPE: Type = Type("label_conversations");
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
    type Action = Label;
    type Context = MailUserContext;

    async fn apply_local(
        &self,
        _: ActionId,
        _: &Self::Context,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        action.0.resolve_ids(tx).await?;

        Conversation::apply_label(action.0.label_id, action.0.data.target_ids.clone(), tx).await?;
        Ok(())
    }

    async fn revert_local(
        &self,
        _: ActionId,
        _: &Self::Context,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        Conversation::remove_label(action.0.label_id, action.0.data.target_ids.clone(), tx).await?;
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
        let response = Conversation::apply_label_to_multiple_remote::<Proton>(
            action
                .0
                .remote_label_id
                .clone()
                .expect("Should be set")
                .clone(),
            action.0.data.remote_target_ids.clone(),
            None,
            ctx.api(),
        )
        .await?;

        let failed_ids = filter_responses(response);

        if !failed_ids.is_empty() {
            error!("Label operation failed for: {:?}", failed_ids);
            guard
                .tx::<_, _, <Self::Action as Action>::Error>(async |tx| {
                    let local_ids =
                        Conversation::remote_ids_counterpart(failed_ids.clone(), tx).await?;

                    Conversation::remove_label(action.0.label_id, local_ids, tx)
                        .await
                        .map_err(|e| {
                            error!("Failed to rollback failed conversations: {e:?}");
                            e
                        })?;
                    Ok(())
                })
                .await?
        }
        Ok(())
    }
}
