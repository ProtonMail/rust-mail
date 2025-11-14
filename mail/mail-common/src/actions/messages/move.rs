use crate::AppError;
use crate::actions::{ActionMoveData, MailActionError};
use crate::models::Message;
use proton_action_queue::action::{
    Action, ActionDependencyKeys, FactoryResult, Type, VersionConverter, WriterGuard,
};
use proton_action_queue::action::{ActionId, Handler};
use proton_action_queue::enqueue;
use proton_action_queue::queue::Queue;
use proton_action_queue::rebase::RebaseChangeSet;
use proton_core_api::session::Session;
use serde::{Deserialize, Serialize};
use stash::stash::{Bond, Tether};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Move(pub ActionMoveData<Message>);

impl VersionConverter for Move {
    type Output = Self;

    fn convert(old_version: u32, _: u32, data: &[u8]) -> FactoryResult<Self::Output> {
        ActionMoveData::convert(old_version, data).map(Move)
    }
}

impl Action for Move {
    const TYPE: Type = Type("move_messages");
    const VERSION: u32 = 3;
    type VersionConverter = Self;
    type Handler = MoveHandler;
    type RemoteOutput = ();
    type LocalOutput = Self;
    type Error = MailActionError;

    fn dependency_keys(&self) -> ActionDependencyKeys {
        self.0.action_dependency_keys()
    }
}

pub struct MoveHandler {
    pub api: Session,
}

impl Handler for MoveHandler {
    type Action = Move;

    async fn apply_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<Self::Action, <Self::Action as Action>::Error> {
        action.0.move_to_async(tx).await?;
        Ok(action.clone())
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
        action.0.apply_remote(&self.api, guard).await
    }

    async fn rebase_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        rebase_change_set: &RebaseChangeSet,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        action.0.rebase_local(rebase_change_set, tx).await?;
        Ok(())
    }
}

pub struct UndoMoveToMessages {
    pub action: Move,
    pub id: ActionId,
}

impl UndoMoveToMessages {
    pub async fn undo(self, queue: &Queue, _: &Tether) -> Result<(), AppError> {
        if queue.cancel(self.id).await.is_ok() {
            // The undoing is done by the revert_local of the action.
            return Ok(());
        };
        // The queue couldn't revert. This means that we're on our own to undo this.
        let (label_as, mark_unread) = self.action.0.build_undo_states();

        enqueue!(queue, [label_as, mark_unread,])?;

        Ok(())
    }
}
