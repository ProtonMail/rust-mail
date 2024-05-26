use crate::db::{LocalConversationId, LocalLabelId, MailSqliteConnectionMut};
use futures::executor::block_on;
use proton_action_queue::{
    define_action_id, Action, ActionError, ActionFactoryInstance, ActionFactoryInstanceError,
    ActionId, ActionLocalValidationResult, ActionResult, LocalActionHandler, RemoteActionHandler,
    SessionProvider, StoredAction,
};
use proton_api_mail::domain::LabelType;
use proton_api_mail::exports::anyhow::anyhow;
use proton_api_mail::exports::serde::{self, Deserialize, Serialize};
use proton_api_mail::exports::tracing::error;
use proton_api_mail::MailSession;
use proton_sqlite3::SqliteTransaction;
use std::any::Any;

define_action_id!(
    UNLABEL_CONVERSATION_ACTION_ID,
    "a20802e4-99f6-4632-88ef-429115a6bb1f"
);

#[derive(Serialize, Deserialize, Debug)]
#[serde(crate = "self::serde")]
pub struct UnlabelConversationsAction {
    label_id: LocalLabelId,
    ids: Vec<LocalConversationId>,
}

impl UnlabelConversationsAction {
    pub fn new(label_id: LocalLabelId, ids: impl IntoIterator<Item = LocalConversationId>) -> Self {
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

struct MarkConversationReadLocalHandler<'c, 't: 'c> {
    action: &'c UnlabelConversationsAction,
    tx: MailSqliteConnectionMut<'t>,
}

impl<'c, 't: 'c> LocalActionHandler for MarkConversationReadLocalHandler<'c, 't> {
    fn apply_local(&mut self) -> ActionResult<()> {
        let Some(label) = self
            .tx
            .label_with_id(self.action.label_id)
            .map_err(|e| ActionError::Local(anyhow!(e)))?
        else {
            let err = anyhow!("Failed to find label with id {}", self.action.label_id);
            error!("{err}");
            return Err(ActionError::Local(err));
        };

        if label.label_type != LabelType::Label {
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

struct MarkConversationReadRemoteHandler<'t> {
    action: UnlabelConversationsAction,
    session: MailSession,
    tx: MailSqliteConnectionMut<'t>,
}

impl<'t> RemoteActionHandler for MarkConversationReadRemoteHandler<'t> {
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

    fn local_handler<'r, 't: 'r>(
        &self,
        action: &'r dyn Any,
        tx: &'r mut SqliteTransaction<'t>,
    ) -> Result<Box<dyn LocalActionHandler + 'r>, ActionFactoryInstanceError> {
        let Some(action) = action.downcast_ref::<UnlabelConversationsAction>() else {
            return Err(ActionFactoryInstanceError::InvalidType(
                action.type_id(),
                std::any::TypeId::of::<UnlabelConversationsAction>(),
            ));
        };

        Ok(Box::new(MarkConversationReadLocalHandler {
            action,
            tx: MailSqliteConnectionMut::new(tx),
        }))
    }

    fn remote_handler<'r, 't: 'r>(
        &'r self,
        action: &StoredAction,
        tx: &'r mut SqliteTransaction<'t>,
        session_provider: &dyn SessionProvider,
    ) -> Result<Box<dyn RemoteActionHandler + 'r>, ActionFactoryInstanceError> {
        if action.version != UnlabelConversationsAction::VERSION {
            return Err(ActionFactoryInstanceError::InvalidVersion(action.version));
        }

        let action = action.deserialize::<UnlabelConversationsAction>()?;
        let session = session_provider.retrieve_session()?;

        Ok(Box::new(MarkConversationReadRemoteHandler {
            action,
            tx: MailSqliteConnectionMut::new(tx),
            session: MailSession::from(session),
        }))
    }
}
