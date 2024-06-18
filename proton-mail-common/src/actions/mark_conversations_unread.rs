use futures::executor::block_on;
use proton_action_queue::{
    define_action_id, Action, ActionError, ActionFactoryInstance, ActionFactoryInstanceError,
    ActionId, ActionLocalValidationResult, ActionResult, LocalActionHandler, RemoteActionHandler,
    SessionProvider, StoredAction,
};
use proton_api_mail::exports::anyhow::anyhow;
use proton_api_mail::exports::serde::{self, Deserialize, Serialize};
use proton_api_mail::exports::tracing::error;
use proton_api_mail::MailSession;
use stash::stash::Tether;
use std::any::Any;

define_action_id!(
    MARK_CONVERSATION_UNREAD_ACTION_ID,
    "a4a3e11b-3464-4781-844b-8e716bc72ffa"
);

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(crate = "self::serde")]
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

        self.tx
            .mark_conversations_unread(self.action.active_label_id, self.action.ids.iter().cloned())
            .map_err(|e| ActionError::Local(anyhow!(e)))?;
        Ok(())
    }
}

struct MarkConversationUnreadRemoteHandler {
    action: MarkConversationsUnreadAction,
    session: MailSession,
    tx: Tether,
}

impl RemoteActionHandler for MarkConversationUnreadRemoteHandler {
    fn revert_local(&mut self) -> ActionResult<()> {
        self.tx
            .mark_conversations_read(self.action.ids.iter().cloned())
            .map_err(|e| ActionError::Local(anyhow!(e)))?;
        Ok(())
    }

    fn validate_local(&mut self) -> ActionResult<ActionLocalValidationResult> {
        Ok(ActionLocalValidationResult::Valid)
    }

    fn apply_remote(&mut self) -> ActionResult<()> {
        let conv_ids = self
            .tx
            .local_to_remote_conversation_ids(self.action.ids.iter().cloned())
            .map_err(|e| {
                error!("Failed to resolve conversation ids: {e}");
                ActionError::Local(anyhow!(e))
            })?;
        let responses = block_on(async {
            self.session
                .mark_conversations_unread(&conv_ids)
                .await
                .map_err(|e| {
                    error!("Failed to mark conversations read on API: {e}");
                    e
                })
        })?;

        let failed_messages = responses
            .into_iter()
            .filter(|r| r.response.code != 1000)
            .map(|r| r.id)
            .collect::<Vec<_>>();
        if !failed_messages.is_empty() {
            error!(
                "Mark conversations read operation failed for: {:?}",
                failed_messages
            );
            let local_ids = self
                .tx
                .remote_to_local_conversation_ids(failed_messages.iter())
                .map_err(|e| ActionError::Local(anyhow!(e)))?;
            self.tx
                .mark_conversations_read(local_ids.into_iter())
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
        let type_id = action.type_id().clone();
        let Ok(action) = action.downcast::<MarkConversationsUnreadAction>() else {
            return Err(ActionFactoryInstanceError::InvalidType(
                type_id,
                std::any::TypeId::of::<MarkConversationsUnreadAction>(),
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
            session: MailSession::from(session),
        }))
    }
}
