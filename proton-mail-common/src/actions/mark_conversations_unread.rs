use crate::models::Conversation;
use anyhow::anyhow;
use futures::executor::block_on;
use proton_action_queue::{
    define_action_id, Action, ActionError, ActionFactoryInstance, ActionFactoryInstanceError,
    ActionId, ActionLocalValidationResult, ActionResult, LocalActionHandler, RemoteActionHandler,
    SessionProvider, StoredAction,
};
use proton_api_core::services::proton::Proton;
use proton_api_core::session::{CoreSession, Session};
use proton_core_common::datatypes::RemoteId;
use serde::{Deserialize, Serialize};
use stash::stash::Tether;
use std::any::{Any, TypeId};
use tracing::error;

define_action_id!(
    MARK_CONVERSATION_UNREAD_ACTION_ID,
    "a4a3e11b-3464-4781-844b-8e716bc72ffa"
);

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MarkConversationsUnreadAction {
    active_label_id: u64,
    ids: Vec<u64>,
}

impl MarkConversationsUnreadAction {
    pub fn new(active_label_id: u64, ids: impl IntoIterator<Item = u64>) -> Self {
        Self {
            active_label_id,
            ids: Vec::from_iter(ids),
        }
    }
}

impl Action for MarkConversationsUnreadAction {
    const ID: ActionId = MARK_CONVERSATION_UNREAD_ACTION_ID;
    const VERSION: u32 = 1;
}

struct MarkConversationUnreadLocalHandler {
    action: MarkConversationsUnreadAction,
    tx: Tether,
}

impl<'c, 't: 'c> LocalActionHandler for MarkConversationUnreadLocalHandler {
    fn apply_local(&mut self) -> ActionResult<()> {
        if self.action.ids.is_empty() {
            return Err(ActionError::Local(anyhow!(
                "No conversations in this action"
            )));
        }

        // TODO: This is simplified, and will be updated when these operations are
        // TODO: refactored
        block_on(async {
            Conversation::mark_multiple_as_read(self.action.ids.clone(), self.tx.stash()).await
        })
        .map_err(|e| ActionError::Local(anyhow!(e)))?;
        Ok(())
    }
}

struct MarkConversationUnreadRemoteHandler {
    action: MarkConversationsUnreadAction,
    session: Session,
    tx: Tether,
}

impl RemoteActionHandler for MarkConversationUnreadRemoteHandler {
    fn revert_local(&mut self) -> ActionResult<()> {
        block_on(async {
            Conversation::mark_multiple_as_read(self.action.ids.clone(), self.tx.stash()).await
        })
        .map_err(|e| ActionError::Local(anyhow!(e)))
    }

    fn validate_local(&mut self) -> ActionResult<ActionLocalValidationResult> {
        Ok(ActionLocalValidationResult::Valid)
    }

    fn apply_remote(&mut self) -> ActionResult<()> {
        let conv_ids = block_on(async {
            Conversation::find_remote_ids(self.action.ids.clone(), self.tx.stash()).await
        })
        .map_err(|e| {
            error!("Failed to resolve conversation ids: {e}");
            ActionError::Local(anyhow!(e))
        })?;
        let responses = block_on(async {
            Conversation::mark_multiple_as_unread_remote::<Proton>(conv_ids, self.session.api())
                .await
                .map_err(|e| {
                    error!("Failed to mark conversations read on API: {e}");
                    e
                })
        })?;

        let failed_messages = responses
            .into_iter()
            .filter(|r| r.response.code != 1000)
            .map(|r| RemoteId::from(r.id))
            .collect::<Vec<_>>();
        if !failed_messages.is_empty() {
            error!(
                "Mark conversations read operation failed for: {:?}",
                failed_messages
            );
            let local_ids = block_on(async {
                Conversation::find_local_ids(failed_messages, self.tx.stash()).await
            })
            .map_err(|e| ActionError::Local(anyhow!(e)))?;
            block_on(async {
                Conversation::mark_multiple_as_read(local_ids, self.tx.stash()).await
            })
            .map_err(|e| {
                error!("Failed to rollback failed for conversations: {e}");
                ActionError::Local(anyhow!(e))
            })?;
        }

        Ok(())
    }
}

#[derive(Debug)]
pub(super) struct MarkConversationsUnreadActionFactory {}

impl MarkConversationsUnreadActionFactory {
    pub fn new() -> Self {
        Self {}
    }
}

impl ActionFactoryInstance for MarkConversationsUnreadActionFactory {
    fn action_id(&self) -> &'static ActionId {
        &MARK_CONVERSATION_UNREAD_ACTION_ID
    }

    fn local_handler(
        &self,
        action: Box<dyn Any>,
        tx: Tether,
    ) -> Result<Box<dyn LocalActionHandler>, ActionFactoryInstanceError> {
        let type_id = TypeId::of::<Box<dyn Any>>();
        let Ok(action) = action.downcast::<MarkConversationsUnreadAction>() else {
            return Err(ActionFactoryInstanceError::InvalidType(
                type_id,
                TypeId::of::<MarkConversationsUnreadAction>(),
            ));
        };

        Ok(Box::new(MarkConversationUnreadLocalHandler {
            action: *action,
            tx,
        }))
    }

    fn remote_handler(
        &self,
        action: StoredAction,
        tx: Tether,
        session_provider: &dyn SessionProvider,
    ) -> Result<Box<dyn RemoteActionHandler>, ActionFactoryInstanceError> {
        if action.version != MarkConversationsUnreadAction::VERSION {
            return Err(ActionFactoryInstanceError::InvalidVersion(action.version));
        }

        let action = action.deserialize::<MarkConversationsUnreadAction>()?;
        let session = session_provider.retrieve_session()?;

        Ok(Box::new(MarkConversationUnreadRemoteHandler {
            action,
            tx,
            session,
        }))
    }
}
