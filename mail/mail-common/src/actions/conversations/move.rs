use crate::AppError;
use crate::actions::conversations::LabelAs;
use crate::actions::messages::Unread;
use crate::actions::{ActionMoveData, LabelAsData, MailActionError};
use crate::models::Conversation;
use anyhow::Context;
use itertools::Itertools;
use proton_action_queue::action::{
    Action, ActionDependencyKeys, ActionId, FactoryResult, Handler, Type, VersionConverter,
    WriterGuard,
};
use proton_action_queue::enqueue;
use proton_action_queue::queue::Queue;
use proton_core_api::session::Session;
use serde::{Deserialize, Serialize};
use stash::stash::{Bond, Tether};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Move(pub ActionMoveData<Conversation>);

impl VersionConverter for Move {
    type Output = Self;

    fn convert(old_version: u32, _: u32, data: &[u8]) -> FactoryResult<Self::Output> {
        ActionMoveData::convert(old_version, data).map(Move)
    }
}

impl Action for Move {
    const TYPE: Type = Type("move_conversations");
    const VERSION: u32 = 2;
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
        let mut action_2 = action.clone();
        let action_2 = tx
            .sync_bridge(|tx| {
                action_2.0.move_to(tx)?;
                Ok(action_2)
            })
            .await?;
        *action = action_2;
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

        let move_actions = self.action.0.reverse().map(Move).collect_vec();

        let label_as_data = LabelAsData {
            source_label_id: 0.into(), // This is fine because it's unused (no archiving, no undoing)
            add: self.action.0.removed_labels,
            remove: vec![],
        };

        let id = enqueue!(
            queue,
            [
                LabelAs(label_as_data),
                Unread::new(self.action.0.marked_read),
            ]
        )?;

        queue
            .queue_actions(move_actions, Some(id))
            .await
            .context("Error undoing")?
            .last()
            .map(|a| a.id);

        Ok(())
    }
}
