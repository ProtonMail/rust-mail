use crate::actions::conversations::Move;
use crate::actions::{LabelAsData, MailActionError};
use crate::datatypes::SystemLabelId;
use crate::models::{Conversation, ConversationCounters};
use crate::{AppError, MailUserContext};
use anyhow::Context;
use proton_action_queue::action::{
    self, Action, ActionId, FactoryError, Handler as ActionHandler, MetadataBuilder, Type,
    VersionConverter, VersionConverterError, WriterGuard,
};
use proton_action_queue::queue::Queue;
use proton_core_api::services::proton::LabelId;
use proton_core_common::models::Label;
use serde::{Deserialize, Serialize};
use stash::orm::Model;
use stash::stash::{Bond, Tether};
use std::collections::HashSet;
use std::mem;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LabelAs(pub LabelAsData<Conversation>);

pub struct Converter;

impl VersionConverter for Converter {
    type Output = LabelAs;

    fn convert(
        old_version: u32,
        current_version: u32,
        data: &[u8],
    ) -> Result<Self::Output, FactoryError> {
        if current_version != LabelAs::VERSION && old_version != LabelAs::VERSION {
            return Err(FactoryError::VersionConverter(
                VersionConverterError::InvalidVersion(current_version),
            ));
        }

        Ok(action::deserialize::<LabelAs>(data)?)
    }
}

impl Action for LabelAs {
    const TYPE: Type = Type("label_conversation_as");
    const VERSION: u32 = 1;

    type VersionConverter = Converter;
    type Handler = Handler;
    type RemoteOutput = ();
    type LocalOutput = bool;
    type Error = MailActionError;
    type Context = MailUserContext;
}

#[derive(Default)]
pub struct Handler;

impl ActionHandler for Handler {
    type Action = LabelAs;
    type Context = MailUserContext;

    async fn apply_local(
        &self,
        _: ActionId,
        _: &Self::Context,
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
        _: &Self::Context,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        action.0.revert_local(tx).await?;
        Ok(())
    }

    async fn apply_remote(
        &self,
        _: ActionId,
        ctx: &Self::Context,
        action: &mut Self::Action,
        guard: WriterGuard<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        action.0.apply_remote(ctx, guard).await
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
            // We have to undo the archiving
            let archive = Label::resolve_local_label_id(LabelId::archive(), tether).await?;

            let mut all = HashSet::new();
            for &i in &action.0.add {
                all.insert(i.id);
            }

            for &i in &action.0.remove {
                all.insert(i.id);
            }

            let move_action = Move::new(archive, action.0.source_label_id, all);
            let queued_move = queue
                .queue_action(action)
                .await
                .context("Error queuing move to archive")?;

            let meta = MetadataBuilder::new()
                .with_dependency(queued_move.id)
                .build();

            queue
                .queue_action_with_metadata(move_action, meta)
                .await
                .context("Error queuing with move to archive dependency")?;
        } else {
            queue
                .queue_action(action)
                .await
                .context("Error queuing action")?;
        };

        Ok(())
    }
}
