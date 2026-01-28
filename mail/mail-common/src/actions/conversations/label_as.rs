use proton_core_api::session::Session;
use stash::UserDb;

use crate::AppError;
use crate::actions::conversations::Move;
use crate::actions::{LabelAsData, MailActionError};
use crate::models::{Conversation, ConversationCounter};
use proton_action_queue::action::{
    Action, ActionDependencyKeys, ActionId, FactoryResult, Handler, Metadata, Type,
    VersionConverter, WriterGuard,
};
use proton_action_queue::queue::Queue;
use proton_action_queue::rebase::RebaseChangeSet;
use serde::{Deserialize, Serialize};
use stash::orm::Model;
use stash::stash::{Bond, Tether};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LabelAs(pub LabelAsData<Conversation>);

impl VersionConverter<UserDb> for LabelAs {
    type Output = Self;

    fn convert(old_version: u32, _: u32, data: &[u8]) -> FactoryResult<Self::Output> {
        LabelAsData::convert(old_version, data).map(LabelAs)
    }
}
impl Action<UserDb> for LabelAs {
    const TYPE: Type = Type("label_conversation_as");
    const VERSION: u32 = 3;
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

impl Handler<UserDb> for LabelAsHandler {
    type Action = LabelAs;

    async fn apply_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<bool, <Self::Action as Action<UserDb>>::Error> {
        action.0.apply_local_common(tx).await?;

        let total = if let Some(label_id) = action.0.source_label_id {
            ConversationCounter::load(label_id, tx)
                .await?
                .map_or(0, |x| x.total)
        } else {
            0
        };
        Ok(total == 0)
    }

    async fn revert_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action<UserDb>>::Error> {
        action.0.revert_local(tx).await?;
        Ok(())
    }

    async fn apply_remote(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        guard: WriterGuard<'_, UserDb>,
    ) -> Result<(), <Self::Action as Action<UserDb>>::Error> {
        action.0.apply_remote(&self.api, guard).await
    }

    async fn rebase_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        changeset: &RebaseChangeSet,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action<UserDb>>::Error> {
        action.0.rebase_local(changeset, tx).await?;
        Ok(())
    }
}

pub struct UndoLabelAsConversations {
    pub action: LabelAs,
    pub id: ActionId,
    pub must_archive: Option<UndoLabelAsArchiveConversations>,
}

pub struct UndoLabelAsArchiveConversations {
    pub action: Move,
    pub id: ActionId,
}

impl UndoLabelAsConversations {
    pub async fn undo(self, queue: &Queue<UserDb>, _: &Tether) -> Result<(), AppError> {
        let cancelled_id = match queue.cancel(self.id).await {
            Ok(ids) => ids,
            Err(_) => {
                // The queue couldn't revert. This means that we're on our own to undo this.
                // Let's create the opposite action: Swap add and remove.
                let action = LabelAs(self.action.0.reversed());
                queue
                    .queue_action_with_metadata(
                        action,
                        Metadata::builder().with_dependency(self.id).build(),
                    )
                    .await?;
                vec![]
            }
        };

        if let Some(move_action) = self.must_archive
            && !cancelled_id.contains(&move_action.id)
            && queue.cancel(move_action.id).await.is_err()
        {
            let (label, unread) = move_action.action.0.build_undo_states();
            queue
                .tether()
                .await?
                .tx::<_, _, AppError>(async |tx| {
                    let metadata = Metadata::builder().with_dependency(move_action.id).build();
                    queue
                        .queue_action_with_metadata_in_tx(label, metadata.clone(), tx)
                        .await?;
                    if let Some(unread) = unread {
                        queue
                            .queue_action_with_metadata_in_tx(unread, metadata.clone(), tx)
                            .await?;
                    }
                    Ok(())
                })
                .await?;
        };
        Ok(())
    }
}
