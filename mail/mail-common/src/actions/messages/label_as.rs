use crate::AppError;
use crate::actions::messages::r#move::Move as MoveAction;
use crate::actions::{ActionMoveData, LabelAsData, MailActionError};
use crate::models::{Message, MessageCounters};
use anyhow::Context;
use proton_action_queue::action::{
    Action, ActionDependencyKeys, ActionId, FactoryResult, Handler, Type, VersionConverter,
    WriterGuard,
};
use proton_action_queue::enqueue;
use proton_action_queue::queue::Queue;
use proton_action_queue::rebase::RebaseChangeSet;
use proton_core_api::session::Session;
use serde::{Deserialize, Serialize};
use stash::orm::Model;
use stash::stash::{Bond, Tether};
use std::collections::HashSet;
use std::mem;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LabelAs(pub LabelAsData<Message>);

impl VersionConverter for LabelAs {
    type Output = Self;

    fn convert(old_version: u32, _: u32, data: &[u8]) -> FactoryResult<Self::Output> {
        LabelAsData::convert(old_version, data).map(LabelAs)
    }
}

impl Action for LabelAs {
    const TYPE: Type = Type("label_messages_as");
    const VERSION: u32 = 2;
    type VersionConverter = Self;
    type Handler = LabelAsHandler;
    type RemoteOutput = ();
    type LocalOutput = bool;
    type Error = MailActionError;

    fn dependency_keys(&self) -> ActionDependencyKeys {
        self.0.action_dependency_keys()
    }
}

pub struct LabelAsHandler {
    pub api: Session,
}

impl Handler for LabelAsHandler {
    type Action = LabelAs;

    async fn apply_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<bool, <Self::Action as Action>::Error> {
        action.0.apply_local_common(tx).await?;

        let total = MessageCounters::load(action.0.source_label_id, tx)
            .await?
            .map_or(0, |x| x.total);

        Ok(total == 0)
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
        this_id: ActionId,
        action: &mut Self::Action,
        _: &RebaseChangeSet,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        //TODO(ET-5183): Test me!
        self.apply_local(this_id, action, tx).await?;
        Ok(())
    }
}

pub struct UndoLabelAsMessages {
    pub action: LabelAs,
    pub id: ActionId,
    pub must_archive: bool,
}

impl UndoLabelAsMessages {
    pub async fn undo(self, queue: &Queue, tether: &Tether) -> Result<(), AppError> {
        if queue.cancel(self.id).await.is_ok() {
            // The undoing is done by the revert_local of the action.
            return Ok(());
        };

        // The queue couldn't revert. This means that we're on our own to undo this.
        // Let's create the opposite action: Swap add and remove.
        let mut action = self.action;
        mem::swap(&mut action.0.add, &mut action.0.remove);

        if self.must_archive {
            let mut all = HashSet::new();

            for &i in &action.0.add {
                all.insert(i.id);
            }
            for &i in &action.0.remove {
                all.insert(i.id);
            }

            if let Some(move_action_data) =
                ActionMoveData::new(tether, action.0.source_label_id, all).await?
            {
                let _id = enqueue!(queue, [action, MoveAction(move_action_data)])?;
                return Ok(());
            }
        };
        queue
            .queue_action(action)
            .await
            .context("Error queuing action")?;

        Ok(())
    }
}
