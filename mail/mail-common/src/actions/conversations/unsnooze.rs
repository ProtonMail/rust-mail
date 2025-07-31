use crate::actions::{GenericLabelRelatedActionData, MailActionError, filter_responses};
use crate::datatypes::{LocalConversationId, RollbackItemType};
use crate::models::{Conversation, RollbackItem};
use proton_action_queue::action::{Action, ActionId, DefaultVersionConverter, Type, WriterGuard};
use proton_core_api::services::proton::Proton;
use proton_core_common::datatypes::LocalLabelId;
use proton_mail_api::services::proton::ProtonMail;
use serde::{self, Deserialize, Serialize};
use stash::stash::Bond;
use tracing::error;

/// Unsnooze conversations action.
///
/// This action unsnoozes the given conversations.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Unsnooze {
    action_data: GenericLabelRelatedActionData<Conversation>,
}

impl Unsnooze {
    pub fn new(label_id: LocalLabelId, ids: impl IntoIterator<Item = LocalConversationId>) -> Self {
        Self {
            action_data: GenericLabelRelatedActionData::new(label_id, ids),
        }
    }
}

impl Action for Unsnooze {
    const TYPE: Type = Type("unsnooze_conversations");
    const VERSION: u32 = 1;
    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = UnsnoozeHandler;
    type RemoteOutput = ();

    type LocalOutput = ();
    type Error = MailActionError;
}

pub struct UnsnoozeHandler {
    pub api: Proton,
}

impl proton_action_queue::action::Handler for UnsnoozeHandler {
    type Action = Unsnooze;

    async fn apply_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<<Self::Action as Action>::LocalOutput, <Self::Action as Action>::Error> {
        if action.action_data.data.target_ids.is_empty() {
            return Err(MailActionError::NoInput);
        }

        Conversation::unsnooze(
            action.action_data.label_id,
            action.action_data.data.target_ids.clone(),
            tx,
        )
        .await?;

        Ok(())
    }

    async fn revert_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        action
            .action_data
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
        action.action_data.resolve_ids(guard.tether()).await?;

        if action.action_data.data.remote_target_ids.is_empty() {
            tracing::warn!(
                "No remote target ids to unsnooze, local only ids: {:?}",
                action.action_data.data.target_ids
            );
            return Ok(());
        }

        let response = self
            .api
            .put_conversations_unsnooze(action.action_data.data.remote_target_ids.clone())
            .await?;

        let responses = filter_responses(response.responses);

        if !responses.is_empty() {
            guard
                .tx::<_, _, <Self::Action as Action>::Error>(async |tx| {
                    error!("Unsnooze operation failed for: {:?}", responses);

                    for remote_id in responses {
                        RollbackItem::new(remote_id.to_string(), RollbackItemType::Conversation)
                            .save(tx)
                            .await?;
                    }

                    Ok(())
                })
                .await?;
        }

        Ok(())
    }
}
