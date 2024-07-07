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
    MARK_CONVERSATION_READ_ACTION_ID,
    "f6ed74cd-0ac7-4002-b3de-6de05fb1f155"
);

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MarkConversationsReadAction {
    active_label_id: u64,
    ids: Vec<u64>,
}

impl MarkConversationsReadAction {
    pub fn new(active_label_id: u64, ids: impl IntoIterator<Item = u64>) -> Self {
        Self {
            active_label_id,
            ids: Vec::from_iter(ids),
        }
    }
}

impl Action for MarkConversationsReadAction {
    const ID: ActionId = MARK_CONVERSATION_READ_ACTION_ID;
    const VERSION: u32 = 1;
}

struct MarkConversationReadLocalHandler {
    action: MarkConversationsReadAction,
    tx: Tether,
}

impl LocalActionHandler for MarkConversationReadLocalHandler {
    fn apply_local(&mut self) -> ActionResult<()> {
        block_on(async {
            Conversation::mark_multiple_as_read(self.action.ids.clone(), self.tx.stash()).await
        })
        .map_err(|e| ActionError::Local(anyhow!(e)))
    }
}

struct MarkConversationReadRemoteHandler {
    action: MarkConversationsReadAction,
    session: Session,
    tx: Tether,
}

impl RemoteActionHandler for MarkConversationReadRemoteHandler {
    fn revert_local(&mut self) -> ActionResult<()> {
        if self.action.ids.is_empty() {
            return Err(ActionError::Local(anyhow!(
                "No conversations in this action"
            )));
        }

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
            Conversation::mark_multiple_as_read_remote::<Proton>(conv_ids, self.session.api()).await
        })
        .map_err(|e| {
            error!("Failed to mark conversations read on API: {e}");
            e
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
                Conversation::mark_multiple_as_unread(local_ids, self.tx.stash()).await
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
pub(super) struct MarkConversationsReadActionFactory {}

impl MarkConversationsReadActionFactory {
    pub fn new() -> Self {
        Self {}
    }
}

impl ActionFactoryInstance for MarkConversationsReadActionFactory {
    fn action_id(&self) -> &'static ActionId {
        &MARK_CONVERSATION_READ_ACTION_ID
    }

    fn local_handler(
        &self,
        action: Box<dyn Any>,
        tx: Tether,
    ) -> Result<Box<dyn LocalActionHandler>, ActionFactoryInstanceError> {
        let type_id = TypeId::of::<Box<dyn Any>>();
        let Ok(action) = action.downcast::<MarkConversationsReadAction>() else {
            return Err(ActionFactoryInstanceError::InvalidType(
                type_id,
                TypeId::of::<MarkConversationsReadAction>(),
            ));
        };

        Ok(Box::new(MarkConversationReadLocalHandler {
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
        if action.version != MarkConversationsReadAction::VERSION {
            return Err(ActionFactoryInstanceError::InvalidVersion(action.version));
        }

        let action = action.deserialize::<MarkConversationsReadAction>()?;
        let session = session_provider.retrieve_session()?;

        Ok(Box::new(MarkConversationReadRemoteHandler {
            action,
            tx,
            session,
        }))
    }
}
