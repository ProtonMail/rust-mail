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
    MARK_CONVERSATION_UNREAD_ACTION_ID,
    "a4a3e11b-3464-4781-844b-8e716bc72ffa"
);

#[derive(Serialize, Deserialize, Debug)]
#[serde(crate = "self::serde")]
pub struct MarkConversationsUnreadAction {
    active_label_id: LocalLabelId,
    ids: Vec<LocalConversationId>,
}

impl MarkConversationsUnreadAction {
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

impl Action for MarkConversationsUnreadAction {
    const ID: ActionId = MARK_CONVERSATION_UNREAD_ACTION_ID;
    const VERSION: u32 = 1;
}

struct MarkConversationUnreadLocalHandler<'c, 't: 'c> {
    action: &'c MarkConversationsUnreadAction,
    tx: &'c mut Transaction<'t>,
}

impl<'c, 't: 'c> LocalActionHandler for MarkConversationUnreadLocalHandler<'c, 't> {
    fn apply_local(&mut self) -> ActionResult<()> {
        let conn = (*self.tx).deref();
        let mut tx = MailSqliteConnectionImpl::from(conn);
        tx.mark_conversations_unread(self.action.active_label_id, self.action.ids.iter().cloned())
            .map_err(|e| ActionError::Local(anyhow!(e)))?;
        Ok(())
    }
}

struct MarkConversationUnreadRemoteHandler<'r, 't: 'r> {
    ctx: MailUserContext,
    action: MarkConversationsUnreadAction,
    session: MailSession,
    tx: &'r mut Transaction<'t>,
}

impl<'c, 't: 'c> RemoteActionHandler for MarkConversationUnreadRemoteHandler<'c, 't> {
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
            .block_on(async { self.session.mark_conversations_unread(&conv_ids).await })
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
            tx.mark_conversations_read(local_ids.into_iter())
                .map_err(|e| {
                    error!("Failed to rollback failed for conversations: {e}");
                    ActionError::Local(anyhow!(e))
                })?;
        }

        Ok(())
    }
}

#[derive(Debug)]
pub(super) struct MarkConversationsUnreadActionFactory {
    ctx: WeakMailUserContext,
}

impl MarkConversationsUnreadActionFactory {
    pub fn new(ctx: WeakMailUserContext) -> Self {
        Self { ctx }
    }
}

impl ActionFactoryInstance for MarkConversationsUnreadActionFactory {
    fn action_id(&self) -> &'static ActionId {
        &MARK_CONVERSATION_UNREAD_ACTION_ID
    }

    fn local_handler<'r, 't: 'r>(
        &self,
        action: &'r dyn Any,
        tx: &'r mut Transaction<'t>,
    ) -> Result<Box<dyn LocalActionHandler + 'r>, ActionFactoryInstanceError> {
        let Some(action) = action.downcast_ref::<MarkConversationsUnreadAction>() else {
            return Err(ActionFactoryInstanceError::InvalidType(
                action.type_id(),
                std::any::TypeId::of::<MarkConversationsUnreadAction>(),
            ));
        };

        Ok(Box::new(MarkConversationUnreadLocalHandler { action, tx }))
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

        if action.version != MarkConversationsUnreadAction::VERSION {
            return Err(ActionFactoryInstanceError::InvalidVersion(action.version));
        }

        let action = action.deserialize::<MarkConversationsUnreadAction>()?;
        let session = session_provider.retrieve_session()?;

        Ok(Box::new(MarkConversationUnreadRemoteHandler {
            ctx,
            action,
            tx,
            session: MailSession::from(session),
        }))
    }
}
