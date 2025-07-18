use std::collections::HashSet;
use std::mem;

use crate::actions::conversations::Move as MoveAction;
use crate::actions::{ActionMoveData, LabelAsData, MailActionError};
use crate::models::{Conversation, ConversationCounters};
use anyhow::Context;
use proton_action_queue::action::{
    self, Action, ActionId, FactoryError, Handler as ActionHandler, MetadataBuilder,
    SingleVersionConverter, Type, VersionConverter, VersionConverterError, WriterGuard,
};
use proton_action_queue::queue::Queue;
use serde::{Deserialize, Serialize};
use stash::orm::Model;
use stash::stash::{Bond, Tether};
use std::collections::HashSet;
use std::mem;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LabelAs(pub LabelAsData<Conversation>);

impl Action for LabelAs {
    const TYPE: Type = Type("label_conversation_as");
    const VERSION: u32 = 1;
    type VersionConverter = SingleVersionConverter<Self>;
    type Handler = Handler;
    type RemoteOutput = ();
    type LocalOutput = bool;
    type Error = MailActionError;
}

pub struct LabelAsHandler {
    pub api: Proton,
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

        let total = ConversationCounters::load(action.0.source_label_id, tx)
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
    ) -> Result<(), <Self::Action as Action>::Error> {
        action.0.apply_remote(&self.api, guard).await
    }
}

pub struct UndoLabelAsConversations {
    pub action: LabelAs,
    pub id: ActionId,
    pub must_archive: bool,
}

impl UndoLabelAsConversations {
    pub async fn undo(self, queue: &Queue, tether: &Tether) -> Result<(), AppError> {
        let mut action = self.action;
        let Err(e) = queue.cancel(self.id).await else {
            // The undoing is done by the revert_local of the action.
            return Ok(());
        };

        tracing::error!("{e:?}");
        // The queue couldn't revert. This means that we're on our own to undo this.
        // Let's create the opposite action: Swap add and remove.
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
                ActionMoveData::new(&tether, action.0.source_label_id, all).await?
            {
                let queued_move = queue
                    .queue_action(action)
                    .await
                    .context("Error queuing move to archive")?;

                let meta = MetadataBuilder::new()
                    .with_dependency(queued_move.id)
                    .build();

                queue
                    .queue_action_with_metadata(MoveAction(move_action_data), meta)
                    .await
                    .context("Error queuing with move to archive dependency")?;
            }

            return Ok(());
        };
        queue
            .queue_action(action)
            .await
            .context("Error queuing action")?;
        Ok(())
    }
}
