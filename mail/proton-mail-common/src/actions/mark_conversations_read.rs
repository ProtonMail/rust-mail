use crate::exports::proton_sqlite3::rusqlite::Transaction;
use crate::{MailUserContext, WeakMailUserContext};
use proton_action_queue::{
    define_action_id, Action, ActionError, ActionFactoryInstance, ActionFactoryInstanceError,
    ActionId, ActionLocalValidationResult, ActionResult, LocalActionHandler, RemoteActionHandler,
    SessionProvider, StoredAction,
};
use proton_api_mail::exports::anyhow::anyhow;
use proton_api_mail::exports::serde::{self, Deserialize, Serialize};
use proton_api_mail::exports::tracing::error;
use proton_api_mail::MailSession;
use proton_mail_db::{LocalConversationId, LocalLabelId, MailSqliteConnectionImpl};
use std::any::Any;
use std::ops::Deref;

define_action_id!(
    MARK_CONVERSATION_READ_ACTION_ID,
    "f6ed74cd-0ac7-4002-b3de-6de05fb1f155"
);

#[derive(Serialize, Deserialize, Debug)]
#[serde(crate = "self::serde")]
pub struct MarkConversationsReadAction {
    active_label_id: LocalLabelId,
    ids: Vec<LocalConversationId>,
}

impl MarkConversationsReadAction {
    pub fn new(
        active_label_id: LocalLabelId,
        ids: impl IntoIterator<Item = LocalConversationId>,
    ) -> Self {
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

struct MarkConversationReadLocalHandler<'c, 't: 'c> {
    action: &'c MarkConversationsReadAction,
    tx: &'c mut Transaction<'t>,
}

impl<'c, 't: 'c> LocalActionHandler for MarkConversationReadLocalHandler<'c, 't> {
    fn apply_local(&mut self) -> ActionResult<()> {
        let conn = (*self.tx).deref();
        let mut tx = MailSqliteConnectionImpl::from(conn);
        tx.mark_conversations_read(self.action.ids.iter().cloned())
            .map_err(|e| ActionError::Local(anyhow!(e)))?;
        Ok(())
    }
}

struct MarkConversationReadRemoteHandler<'r, 't: 'r> {
    ctx: MailUserContext,
    action: MarkConversationsReadAction,
    session: MailSession,
    tx: &'r mut Transaction<'t>,
}

impl<'c, 't: 'c> RemoteActionHandler for MarkConversationReadRemoteHandler<'c, 't> {
    fn revert_local(&mut self) -> ActionResult<()> {
        let conn = (*self.tx).deref();
        let mut tx = MailSqliteConnectionImpl::from(conn);
        tx.mark_conversations_read(self.action.ids.iter().cloned())
            .map_err(|e| ActionError::Local(anyhow!(e)))?;
        Ok(())
    }

    fn validate_local(&mut self) -> ActionResult<ActionLocalValidationResult> {
        Ok(ActionLocalValidationResult::Valid)
    }

    fn apply_remote(&mut self) -> ActionResult<()> {
        let conn = (*self.tx).deref();
        let mut tx = MailSqliteConnectionImpl::from(conn);
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
            .block_on(async { self.session.mark_conversations_read(&conv_ids).await })
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
                "Mark conversations read operation failed for: {:?}",
                failed_messages
            );
            let local_ids = tx
                .remote_to_local_conversation_ids(failed_messages.iter())
                .map_err(|e| ActionError::Local(anyhow!(e)))?;
            tx.mark_conversations_unread(self.action.active_label_id, local_ids.into_iter())
                .map_err(|e| {
                    error!("Failed to rollback failed for conversations: {e}");
                    ActionError::Local(anyhow!(e))
                })?;
        }

        Ok(())
    }
}

#[derive(Debug)]
pub(super) struct MarkConversationsReadActionFactory {
    ctx: WeakMailUserContext,
}

impl MarkConversationsReadActionFactory {
    pub fn new(ctx: WeakMailUserContext) -> Self {
        Self { ctx }
    }
}

impl ActionFactoryInstance for MarkConversationsReadActionFactory {
    fn action_id(&self) -> &'static ActionId {
        &MARK_CONVERSATION_READ_ACTION_ID
    }

    fn local_handler<'r, 't: 'r>(
        &self,
        action: &'r dyn Any,
        tx: &'r mut Transaction<'t>,
    ) -> Result<Box<dyn LocalActionHandler + 'r>, ActionFactoryInstanceError> {
        let Some(action) = action.downcast_ref::<MarkConversationsReadAction>() else {
            return Err(ActionFactoryInstanceError::InvalidType(
                action.type_id(),
                std::any::TypeId::of::<MarkConversationsReadAction>(),
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

        if action.version != MarkConversationsReadAction::VERSION {
            return Err(ActionFactoryInstanceError::InvalidVersion(action.version));
        }

        let action = action.deserialize::<MarkConversationsReadAction>()?;
        let session = session_provider.retrieve_session()?;

        Ok(Box::new(MarkConversationReadRemoteHandler {
            ctx,
            action,
            tx,
            session: MailSession::from(session),
        }))
    }
}
