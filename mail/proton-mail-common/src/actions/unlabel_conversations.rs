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
    UNLABEL_CONVERSATION_ACTION_ID,
    "a20802e4-99f6-4632-88ef-429115a6bb1f"
);

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(crate = "self::serde")]
pub struct UnlabelConversationsAction {
    label_id: u64,
    ids: Vec<u64>,
}

impl UnlabelConversationsAction {
    pub fn new(label_id: u64, ids: impl IntoIterator<Item = u64>) -> Self {
        Self {
            label_id,
            ids: Vec::from_iter(ids),
        }
    }
}

impl Action for UnlabelConversationsAction {
    const ID: ActionId = UNLABEL_CONVERSATION_ACTION_ID;
    const VERSION: u32 = 1;
}

struct MarkConversationReadLocalHandler {
    action: UnlabelConversationsAction,
    tx: Tether,
}

impl LocalActionHandler for MarkConversationReadLocalHandler {
    fn apply_local(&mut self) -> ActionResult<()> {
        if self.action.ids.is_empty() {
            return Err(ActionError::Local(anyhow!(
                "No conversations in this action"
            )));
        }

        let Some(label) = self
            .tx
            .label_with_id(self.action.label_id)
            .map_err(|e| ActionError::Local(anyhow!(e)))?
        else {
            let err = anyhow!("Failed to find label with id {}", self.action.label_id);
            error!("{err}");
            return Err(ActionError::Local(err));
        };

        if !label.is_applicable_label() {
            let err = anyhow!("Invalid label type");
            error!("{err}");
            return Err(ActionError::Local(err));
        }
        self.tx
            .unlabel_conversations(self.action.label_id, self.action.ids.iter().cloned())
            .map_err(|e| ActionError::Local(anyhow!(e)))?;
        Ok(())
    }
}

struct MarkConversationReadRemoteHandler {
    action: UnlabelConversationsAction,
    session: MailSession,
    tx: Tether,
}

impl RemoteActionHandler for MarkConversationReadRemoteHandler {
    fn revert_local(&mut self) -> ActionResult<()> {
        self.tx
            .label_conversations(self.action.label_id, self.action.ids.iter().cloned())
            .map_err(|e| ActionError::Local(anyhow!(e)))?;
        Ok(())
    }

    fn validate_local(&mut self) -> ActionResult<ActionLocalValidationResult> {
        Ok(ActionLocalValidationResult::Valid)
    }

    fn apply_remote(&mut self) -> ActionResult<()> {
        let Some(label) = self
            .tx
            .label_with_id(self.action.label_id)
            .map_err(|e| ActionError::Local(anyhow!(e)))?
        else {
            let err = anyhow!("Failed to find label with id {}", self.action.label_id);
            error!("{err}");
            return Err(ActionError::Local(err));
        };

        let Some(label_rid) = &label.rid else {
            let err = anyhow!("Label {} does not have a remote id", self.action.label_id);
            error!("{err}");
            return Err(ActionError::Local(err));
        };

        let conv_ids = self
            .tx
            .local_to_remote_conversation_ids(self.action.ids.iter().cloned())
            .map_err(|e| {
                error!("Failed to resolve conversation ids: {e}");
                ActionError::Local(anyhow!(e))
            })?;
        let responses = block_on(async {
            self.session
                .unlabel_conversations(label_rid, &conv_ids)
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
                "Label conversation operation failed for: {:?}",
                failed_messages
            );
            let local_ids = self
                .tx
                .remote_to_local_conversation_ids(failed_messages.iter())
                .map_err(|e| ActionError::Local(anyhow!(e)))?;
            self.tx
                .label_conversations(self.action.label_id, local_ids.into_iter())
                .map_err(|e| {
                    error!("Failed to rollback failed for conversations: {e}");
                    ActionError::Local(anyhow!(e))
                })?;
        }

        Ok(())
    }
}

#[derive(Debug)]
pub(super) struct UnlabelConversationsActionFactory {}

impl UnlabelConversationsActionFactory {
    pub fn new() -> Self {
        Self {}
    }
}

impl ActionFactoryInstance for UnlabelConversationsActionFactory {
    fn action_id(&self) -> &'static ActionId {
        &UNLABEL_CONVERSATION_ACTION_ID
    }

    fn local_handler(
        &self,
        action: Box<dyn Any>,
        tx: Tether,
    ) -> Result<Box<dyn LocalActionHandler>, ActionFactoryInstanceError> {
        let type_id = action.type_id().clone();
        let Ok(action) = action.downcast::<UnlabelConversationsAction>() else {
            return Err(ActionFactoryInstanceError::InvalidType(
                type_id,
                std::any::TypeId::of::<UnlabelConversationsAction>(),
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
        if action.version != UnlabelConversationsAction::VERSION {
            return Err(ActionFactoryInstanceError::InvalidVersion(action.version));
        }

        let action = action.deserialize::<UnlabelConversationsAction>()?;
        let session = session_provider.retrieve_session()?;

        Ok(Box::new(MarkConversationReadRemoteHandler {
            action,
            tx,
            session: MailSession::from(session),
        }))
    }
}
