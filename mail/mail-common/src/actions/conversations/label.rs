use crate::actions::{GenericLabelRelatedActionData, MailActionError, filter_responses};
use crate::datatypes::LocalConversationId;
use crate::datatypes::RollbackItemType;
use crate::models::Conversation;
use proton_action_queue::action::{
    Action, ActionId, DefaultVersionConverter, Handler, Type, WriterGuard,
};
use proton_core_api::services::proton::Proton;
use proton_core_common::datatypes::LocalLabelId;
use proton_core_common::models::ModelIdExtension;
use serde::{Deserialize, Serialize};
use stash::stash::Bond;
use tracing::error;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Label(GenericLabelRelatedActionData<Conversation>);

impl Label {
    pub fn new(label_id: LocalLabelId, ids: impl IntoIterator<Item = LocalConversationId>) -> Self {
        Self(GenericLabelRelatedActionData::new(label_id, ids))
    }
}

impl Action for Label {
    const TYPE: Type = Type("label_conversations");
    const VERSION: u32 = 1;

    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = LabelHandler;
    type RemoteOutput = ();
    type LocalOutput = ();
    type Error = MailActionError;
}

pub struct LabelHandler {
    pub api: Proton,
}

impl Handler for LabelHandler {
    type Action = Label;

    async fn apply_local(
        &self,
        _: ActionId,
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
            &self.api,
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
