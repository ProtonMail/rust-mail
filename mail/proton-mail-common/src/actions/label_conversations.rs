use crate::db::{LocalConversationId, LocalLabelId, MailSqliteConnectionImpl};
use crate::exports::proton_sqlite3::rusqlite::Transaction;
use crate::{MailUserContext, WeakMailUserContext};
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
use std::any::Any;
use std::ops::Deref;

define_action_id!(
    LABEL_CONVERSATION_ACTION_ID,
    "49ee23de-089a-44a9-b581-8de05c21edc8"
);

#[derive(Serialize, Deserialize, Debug)]
#[serde(crate = "self::serde")]
pub struct LabelConversationsAction {
    label_id: LocalLabelId,
    ids: Vec<LocalConversationId>,
}

impl LabelConversationsAction {
    pub fn new(label_id: LocalLabelId, ids: impl IntoIterator<Item = LocalConversationId>) -> Self {
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

struct MarkConversationReadLocalHandler<'c, 't: 'c> {
    action: &'c LabelConversationsAction,
    tx: &'c mut Transaction<'t>,
}

impl<'c, 't: 'c> LocalActionHandler for MarkConversationReadLocalHandler<'c, 't> {
    fn apply_local(&mut self) -> ActionResult<()> {
        let conn = (*self.tx).deref();
        let mut tx = MailSqliteConnectionImpl::from(conn);
        let Some(label) = tx
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
        tx.label_conversations(self.action.label_id, self.action.ids.iter().cloned())
            .map_err(|e| ActionError::Local(anyhow!(e)))?;
        Ok(())
    }
}

struct MarkConversationReadRemoteHandler<'r, 't: 'r> {
    ctx: MailUserContext,
    action: LabelConversationsAction,
    session: MailSession,
    tx: &'r mut Transaction<'t>,
}

impl<'c, 't: 'c> RemoteActionHandler for MarkConversationReadRemoteHandler<'c, 't> {
    fn revert_local(&mut self) -> ActionResult<()> {
        let conn = (*self.tx).deref();
        let mut tx = MailSqliteConnectionImpl::from(conn);
        tx.unlabel_conversations(self.action.label_id, self.action.ids.iter().cloned())
            .map_err(|e| ActionError::Local(anyhow!(e)))?;
        Ok(())
    }

    fn validate_local(&mut self) -> ActionResult<ActionLocalValidationResult> {
        Ok(ActionLocalValidationResult::Valid)
    }

    fn apply_remote(&mut self) -> ActionResult<()> {
        let conn = (*self.tx).deref();
        let mut tx = MailSqliteConnectionImpl::from(conn);
        let Some(label) = tx
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

        let conv_ids = tx
            .local_to_remote_conversation_ids(self.action.ids.iter().cloned())
            .map_err(|e| {
                error!("Failed to resolve conversation ids: {e}");
                ActionError::Local(anyhow!(e))
            })?;
        let responses = self
            .ctx
            .mail_context()
            .async_runtime()
            .block_on(async {
                self.session
                    .label_conversations(label_rid, &conv_ids, None)
                    .await
            })
            .map_err(|e| {
                error!("Failed to mark conversations read on API: {e}");
                e
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
            let local_ids = tx
                .remote_to_local_conversation_ids(failed_messages.iter())
                .map_err(|e| ActionError::Local(anyhow!(e)))?;
            tx.unlabel_conversations(self.action.label_id, local_ids.into_iter())
                .map_err(|e| {
                    error!("Failed to rollback failed for conversations: {e}");
                    ActionError::Local(anyhow!(e))
                })?;
        }

        Ok(())
    }
}

#[derive(Debug)]
pub(super) struct LabelConversationsActionFactory {
    ctx: WeakMailUserContext,
}

impl LabelConversationsActionFactory {
    pub fn new(ctx: WeakMailUserContext) -> Self {
        Self { ctx }
    }
}

impl ActionFactoryInstance for LabelConversationsActionFactory {
    fn action_id(&self) -> &'static ActionId {
        &LABEL_CONVERSATION_ACTION_ID
    }

    fn local_handler<'r, 't: 'r>(
        &self,
        action: &'r dyn Any,
        tx: &'r mut Transaction<'t>,
    ) -> Result<Box<dyn LocalActionHandler + 'r>, ActionFactoryInstanceError> {
        let Some(action) = action.downcast_ref::<LabelConversationsAction>() else {
            return Err(ActionFactoryInstanceError::InvalidType(
                action.type_id(),
                std::any::TypeId::of::<LabelConversationsAction>(),
            ));
        };

        Ok(Box::new(MarkConversationReadLocalHandler { action, tx }))
    }

    fn remote_handler<'r, 't: 'r>(
        &'r self,
        action: &StoredAction,
        tx: &'r mut Transaction<'t>,
        session_provider: &dyn SessionProvider,
    ) -> Result<Box<dyn RemoteActionHandler + 'r>, ActionFactoryInstanceError> {
        let Some(ctx) = self.ctx.upgrade() else {
            return Err(ActionFactoryInstanceError::Unknown(anyhow!(
                "Could not upgrade context"
            )));
        };

        if action.version != LabelConversationsAction::VERSION {
            return Err(ActionFactoryInstanceError::InvalidVersion(action.version));
        }

        let action = action.deserialize::<LabelConversationsAction>()?;
        let session = session_provider.retrieve_session()?;

        Ok(Box::new(MarkConversationReadRemoteHandler {
            ctx,
            action,
            tx,
            session: MailSession::from(session),
        }))
    }
}
