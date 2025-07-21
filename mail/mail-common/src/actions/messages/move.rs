use crate::actions::{ActionMoveData, MailActionError};
use crate::models::Message;
use crate::{AppError, MailUserContext};
use anyhow::Context;
use proton_action_queue::action::{Action, SingleVersionConverter, Type, WriterGuard};
use proton_action_queue::action::{ActionId, Handler as ActionHandler};
use proton_action_queue::queue::Queue;
use serde::{Deserialize, Serialize};
use stash::stash::{Bond, Tether};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Move(pub ActionMoveData<Message>);

impl Action for Move {
    const TYPE: Type = Type("move_messages");
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
        for reverse in action.0.reverse() {
            reverse.move_to(tx).await?;
        }
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

pub struct UndoMoveToMessages {
    pub action: Move,
    pub id: ActionId,
}

impl UndoMoveToMessages {
    pub async fn undo(self, queue: &Queue, tether: &mut Tether) -> Result<(), AppError> {
        let Err(e) = queue.cancel(self.id).await else {
            // The undoing is done by the revert_local of the action.
            return Ok(());
        };

        tracing::error!("{e:?}");
        // The queue couldn't revert. This means that we're on our own to undo this.

        _ = queue
            .queue_actions(self.action.0.reverse().map(Move))
            .await
            .context("Error undoing")?;

        tether
            .tx(async |tx| self.action.0.queue_rollback_items(tx).await)
            .await?;

        Ok(())
    }
}
