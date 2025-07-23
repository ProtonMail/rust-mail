use crate::MailUserContext;
use crate::actions::{ActionMoveData, MailActionError, filter_responses};
use crate::datatypes::LocalConversationId;
use crate::datatypes::RollbackItemType;
use crate::models::{Conversation, RollbackItem};
use itertools::Itertools;
use proton_action_queue::action::{Action, DefaultVersionConverter, Type, WriterGuard};
use proton_action_queue::action::{ActionId, Handler as ActionHandler};
use proton_core_common::datatypes::LocalLabelId;
use proton_core_common::models::ModelIdExtension;
use proton_mail_api::services::proton::ProtonMail;
use serde::{Deserialize, Serialize};
use stash::stash::Bond;
use tracing::error;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Move(ActionMoveData<Conversation>);

impl Move {
    pub fn new(
        source_label_id: LocalLabelId,
        destination_label_id: LocalLabelId,
        target_ids: impl IntoIterator<Item = LocalConversationId>,
    ) -> Self {
        Self(ActionMoveData::new(
            source_label_id,
            destination_label_id,
            target_ids,
        ))
    }
}

impl Action for Move {
    const TYPE: Type = Type("move_conversations");
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

impl ActionHandler for Handler {
    type Action = Move;

    type Context = MailUserContext;

    async fn apply_local(
        &self,
        _: ActionId,
        _: &Self::Context,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        action.0.resolve_ids(tx).await?;

        Conversation::move_conversations(
            action.0.source_label_id,
            action.0.destination_label_id,
            action.0.target_ids.clone(),
            tx,
        )
        .await?;

        Ok(())
    }

    async fn revert_local(
        &self,
        _: ActionId,
        _: &Self::Context,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        Conversation::move_conversations(
            action.0.destination_label_id,
            action.0.source_label_id,
            action.0.target_ids.clone(),
            tx,
        )
        .await?;

        for remote_id in &action.0.remote_target_ids {
            RollbackItem::new(remote_id.to_string(), RollbackItemType::Conversation)
                .save(tx)
                .await?;
        }

        Ok(())
    }

    async fn apply_remote(
        &self,
        _: ActionId,
        ctx: &Self::Context,
        action: &mut Self::Action,
        mut guard: WriterGuard<'_>,
    ) -> Result<<Self::Action as Action>::RemoteOutput, <Self::Action as Action>::Error> {
        let api = ctx.api();
        let conversation_ids = action
            .0
            .remote_target_ids
            .clone()
            .into_iter()
            .map_into()
            .collect();
        let label_id = action
            .0
            .remote_destination_label_id
            .clone()
            .expect("Should be set");
        let responses = api
            .put_conversations_label(conversation_ids, label_id, None)
            .await?
            .responses;

        let failed_ids = filter_responses(responses);

        if failed_ids.is_empty() {
            return Ok(());
        }

        error!("Move operation failed for: {:?}", failed_ids);

        guard
            .tx::<_, _, <Self::Action as Action>::Error>(async |tx: &Bond<'_>| {
                let local_ids =
                    Conversation::remote_ids_counterpart(failed_ids.clone(), tx).await?;

                Conversation::move_conversations(
                    action.0.destination_label_id,
                    action.0.source_label_id,
                    local_ids,
                    tx,
                )
                .await?;
                Ok(())
            })
            .await?;

        Ok(())
    }
}
