use crate::actions::{filter_responses, ActionError, ActionMoveData};
use crate::datatypes::RollbackItemType;
use crate::models::{Message, RollbackItem};
use crate::MailUserContext;
use itertools::Itertools;
use proton_action_queue::action::Handler as ActionHandler;
use proton_action_queue::action::{Action, DefaultVersionConverter, Type};
use proton_api_core::session::CoreSession;
use proton_api_mail::services::proton::ProtonMail;
use proton_core_common::datatypes::{Id, LocalId, RemoteId};
use serde::{Deserialize, Serialize};
use stash::stash::{Interface, Stash, Tether};
use tracing::error;

/// Action which applies a label to messages.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Move(ActionMoveData<Message>);

impl Move {
    /// Create a new action which moves messages with `target_ids` from `source_label_id` to
    /// `destination_label_id`.
    pub fn new(
        source_label_id: LocalId,
        destination_label_id: LocalId,
        target_ids: impl IntoIterator<Item = LocalId>,
    ) -> Self {
        Self(ActionMoveData::new(
            source_label_id,
            destination_label_id,
            target_ids,
        ))
    }
}

impl Action for Move {
    const TYPE: Type = Type("move_messages");
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
    type Action = Move;
    type Context = MailUserContext;

    async fn apply_local(
        &self,
        _: &Self::Context,
        action: &mut Self::Action,
        tx: &Tether,
    ) -> Result<(), <Self::Action as Action>::Error> {
        action.0.resolve_ids(tx).await?;

        Message::move_messages(
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
        _: &Self::Context,
        action: &mut Self::Action,
        tx: &Tether,
    ) -> Result<(), <Self::Action as Action>::Error> {
        Message::move_messages(
            action.0.destination_label_id,
            action.0.source_label_id,
            action.0.target_ids.clone(),
            tx,
        )
        .await?;

        for remote_id in &action.0.remote_target_ids {
            RollbackItem::new(remote_id.clone(), RollbackItemType::Message)
                .save_using(tx)
                .await?;
        }

        Ok(())
    }

    async fn apply_remote(
        &self,
        ctx: &Self::Context,
        action: &mut Self::Action,
        stash: &Stash,
    ) -> Result<<Self::Action as Action>::RemoteOutput, <Self::Action as Action>::Error> {
        let api = ctx.session().api();
        let message_ids = action
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
            .expect("Should be set")
            .into();
        let response = api
            .put_messages_label(message_ids, label_id, None)
            .await?
            .responses;

        let failed_ids = filter_responses(response);

        if !failed_ids.is_empty() {
            error!("Move messages operation failed for: {failed_ids:?}");

            let tx = stash.transaction().await?;
            let local_ids = RemoteId::counterparts::<Message, _>(failed_ids.clone(), &tx).await?;
            Message::move_messages(
                action.0.destination_label_id,
                action.0.source_label_id,
                local_ids,
                &tx,
            )
            .await?;
            tx.commit().await?;
        }
        Ok(())
    }
}
