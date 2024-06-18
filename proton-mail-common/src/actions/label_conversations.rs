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
use std::any::Any;
use stash::stash::Tether;

define_action_id!(
    LABEL_CONVERSATION_ACTION_ID,
    "49ee23de-089a-44a9-b581-8de05c21edc8"
);

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(crate = "self::serde")]
pub struct LabelConversationsAction {
    label_id: u64,
    ids: Vec<u64>,
}

impl LabelConversationsAction {
    pub fn new(label_id: u64, ids: impl IntoIterator<Item = u64>) -> Self {
        Self {
            label_id,
            ids: Vec::from_iter(ids),
        }
    }
}

impl Action for LabelConversationsAction {
    const ID: ActionId = LABEL_CONVERSATION_ACTION_ID;
    const VERSION: u32 = 1;
}

struct MarkConversationReadLocalHandler {
    action: LabelConversationsAction,
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
            .label_conversations(self.action.label_id, self.action.ids.iter().cloned())
            .map_err(|e| ActionError::Local(anyhow!(e)))?;
        Ok(())
    }
}

struct MarkConversationReadRemoteHandler {
    action: LabelConversationsAction,
    session: MailSession,
    tx: Tether,
}

impl RemoteActionHandler for MarkConversationReadRemoteHandler {
    fn revert_local(&mut self) -> ActionResult<()> {
        self.tx
            .unlabel_conversations(self.action.label_id, self.action.ids.iter().cloned())
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
                .label_conversations(label_rid, &conv_ids, None)
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
                .unlabel_conversations(self.action.label_id, local_ids.into_iter())
                .map_err(|e| {
                    error!("Failed to rollback failed for conversations: {e}");
                    ActionError::Local(anyhow!(e))
                })?;
        }

        Ok(())
    }
}

#[derive(Debug)]
pub(super) struct LabelConversationsActionFactory {}

impl LabelConversationsActionFactory {
    pub fn new() -> Self {
        Self {}
    }
}

impl ActionFactoryInstance for LabelConversationsActionFactory {
    fn action_id(&self) -> &'static ActionId {
        &LABEL_CONVERSATION_ACTION_ID
    }

    fn local_handler(
        &self,
        action: Box<dyn Any>,
        tx: Tether,
    ) -> Result<Box<dyn LocalActionHandler>, ActionFactoryInstanceError> {
        let type_id = action.type_id().clone();
        let Ok(action) = action.downcast::<LabelConversationsAction>() else {
            return Err(ActionFactoryInstanceError::InvalidType(
                type_id,
                std::any::TypeId::of::<LabelConversationsAction>(),
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
        if action.version != LabelConversationsAction::VERSION {
            return Err(ActionFactoryInstanceError::InvalidVersion(action.version));
        }

        let action = action.deserialize::<LabelConversationsAction>()?;
        let session = session_provider.retrieve_session()?;

        Ok(Box::new(MarkConversationReadRemoteHandler {
            action,
            tx,
            session: MailSession::from(session),
        }))
    }
}
