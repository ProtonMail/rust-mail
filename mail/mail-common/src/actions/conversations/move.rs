use crate::MailUserContext;
use crate::actions::{ActionMoveData, MailActionError};
use crate::models::Conversation;
use proton_action_queue::action::{
    Action, ActionId, Handler as ActionHandler, SingleVersionConverter, Type, WriterGuard,
};
use serde::{Deserialize, Serialize};
use stash::stash::Bond;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Move(pub ActionMoveData<Conversation>);

impl Action for Move {
    const TYPE: Type = Type("move_conversations");
    const VERSION: u32 = 1;
    type VersionConverter = SingleVersionConverter<Self>;
    type Handler = Handler;
    type RemoteOutput = ();

    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = MoveHandler;
    type RemoteOutput = ();
    type LocalOutput = ();
    type Error = MailActionError;
}

pub struct MoveHandler {
    pub api: Proton,
}

impl Handler for MoveHandler {
    type Action = Move;

    async fn apply_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        action.0.move_to(tx).await?;
        Ok(())
    }

    async fn revert_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        action.0.revert_local(tx).await?;
        Ok(())
    }

    async fn apply_remote(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        guard: WriterGuard<'_>,
    ) -> Result<<Self::Action as Action>::RemoteOutput, <Self::Action as Action>::Error> {
        action.0.apply_remote(ctx, guard).await
    }
}
