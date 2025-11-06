use crate::actions::{
    ConversationOrMessage, GenericLabelRelatedActionData, MailActionError,
    filter_responses_by_codes,
};
use crate::datatypes::LocalConversationId;
use crate::datatypes::{ContextualConversation, RollbackItemType};
use crate::models::Conversation;
use anyhow::Context;
use proton_action_queue::action::{
    Action, ActionDependencyKeys, ActionId, DefaultVersionConverter, Handler, Type, WriterGuard,
};
use proton_action_queue::rebase::RebaseChangeSet;
use proton_core_api::consts::General;
use proton_core_api::session::Session;
use proton_core_common::datatypes::LocalLabelId;
use proton_core_common::models::ModelIdExtension;
use serde::{Deserialize, Serialize};
use stash::exports::Transaction;
use stash::stash::{Bond, RunTransaction};
use tracing::error;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MarkUnread(GenericLabelRelatedActionData<Conversation>);

impl MarkUnread {
    pub fn new(label_id: LocalLabelId, ids: impl IntoIterator<Item = LocalConversationId>) -> Self {
        // TODO(db-tests): label_id was present in the original action, why was it used.
        Self(GenericLabelRelatedActionData::new(label_id, ids))
    }
}

impl Action for MarkUnread {
    const TYPE: Type = Type("mark_conversations_unread");
    const VERSION: u32 = 1;

    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = MarkUnreadHandler;
    type RemoteOutput = ();
    type LocalOutput = ();
    type Error = MailActionError;

    fn dependency_keys(&self) -> ActionDependencyKeys {
        self.0.read_unread_action_dependency_keys().build()
    }
}

pub struct MarkUnreadHandler {
    pub api: Session,
}

impl Handler for MarkUnreadHandler {
    type Action = MarkUnread;

    async fn apply_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        // API call return an error 2501(Conversation was not updated) for conversation already unread
        let conversations = Conversation::find_by_ids(action.0.data.target_ids.clone(), tx).await?;
        action.0.data.target_ids = conversations
            .into_iter()
            .filter_map(|c| ContextualConversation::new(c, action.0.label_id))
            .filter(|c| c.num_unread < c.num_messages)
            .map(|c| c.local_id)
            .collect();

        action.0.resolve_ids(tx).await?;

        Conversation::mark_unread_async(action.0.label_id, action.0.data.target_ids.clone(), tx)
            .await?;
        Ok(())
    }

    async fn revert_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        Conversation::mark_read_async(action.0.data.target_ids.clone(), tx).await?;
        action
            .0
            .mark_rollback(RollbackItemType::Conversation, tx)
            .await?;

        Ok(())
    }

    async fn apply_remote(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        mut guard: WriterGuard<'_>,
    ) -> Result<<Self::Action as Action>::RemoteOutput, <Self::Action as Action>::Error> {
        let responses = Conversation::mark_multiple_as_unread_remote(
            action.0.data.remote_target_ids.clone(),
            action.0.remote_label_id.clone().expect("Should be set"),
            &self.api,
        )
        .await?;

        // In this case General::NotExists is returned also for conversations already marked as unread
        let failed_ids = filter_responses_by_codes(
            responses,
            &[General::NoError as u32, General::NotExists as u32],
        );

        if !failed_ids.is_empty() {
            error!("Mark unread operation failed for: {:?}", failed_ids);

            guard
                .run_tx_sync(move |tx: &Transaction<'_>| {
                    let local_ids = Conversation::remote_ids_counterpart_sync(&failed_ids, tx)?;

                    Conversation::mark_read(local_ids, tx)
                        .context("Failed to rollback failed conversations")?;
                    Ok(())
                })
                .await?;
        }
        Ok(())
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
