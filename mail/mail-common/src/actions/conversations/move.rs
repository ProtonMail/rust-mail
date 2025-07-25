use crate::actions::{ActionMoveData, MailActionError};
use crate::models::Conversation;
use crate::{AppError, MailUserContext};
use anyhow::Context;
use proton_action_queue::action::{
    Action, ActionId, DefaultVersionConverter, Handler as ActionHandler, Type, WriterGuard,
};
use proton_action_queue::queue::Queue;
use serde::{Deserialize, Serialize};
use stash::stash::{Bond, Tether};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Move(pub ActionMoveData<Conversation>);

impl Action for Move {
    const TYPE: Type = Type("move_conversations");
    const VERSION: u32 = 1;
    type VersionConverter = DefaultVersionConverter<Self>;
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

pub struct UndoMoveToConversations {
    pub action: Move,
    pub id: ActionId,
}

impl UndoMoveToConversations {
    pub async fn undo(self, queue: &Queue, _: &Tether) -> Result<(), AppError> {
        if queue.cancel(self.id).await.is_ok() {
            // The undoing is done by the revert_local of the action.
            return Ok(());
        };
        // The queue couldn't revert. This means that we're on our own to undo this.

        _ = queue
            .queue_actions(self.action.0.reverse().map(Move))
            .await
            .context("Error undoing")?;

        Ok(())
    }
}
