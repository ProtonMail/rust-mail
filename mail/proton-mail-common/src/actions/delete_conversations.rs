use crate::models::{Conversation, Label};
use anyhow::anyhow;
use async_trait::async_trait;
use futures::executor::block_on;
use proton_action_queue::{
    define_action_id, Action, ActionError, ActionFactoryInstance, ActionFactoryInstanceError,
    ActionId, ActionLocalValidationResult, ActionResult, LocalActionHandler, RemoteActionHandler,
    SessionProvider, StoredAction,
};
use proton_api_core::session::{CoreSession, Session};
use proton_core_common::datatypes::RemoteId;
use serde::{Deserialize, Serialize};
use stash::orm::Model;
use stash::stash::Tether;
use std::any::{Any, TypeId};
use tracing::error;

define_action_id!(
    DELETE_CONVERSATION_ACTION_ID,
    "5cb14a1d-2b1f-48b3-8ea3-c8cc880cf8bd"
);

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DeleteConversationsAction {
    label_id: u64,
    ids: Vec<u64>,
}

impl DeleteConversationsAction {
    pub fn new(label_id: u64, ids: impl IntoIterator<Item = u64>) -> Self {
        Self {
            label_id,
            ids: Vec::from_iter(ids),
        }
    }
}

impl Action for DeleteConversationsAction {
    const ID: ActionId = DELETE_CONVERSATION_ACTION_ID;
    const VERSION: u32 = 1;
}

struct DeleteConversationLocalHandler {
    action: DeleteConversationsAction,
    tx: Tether,
}

impl LocalActionHandler for DeleteConversationLocalHandler {
    fn apply_local(&mut self) -> ActionResult<()> {
        if self.action.ids.is_empty() {
            return Err(ActionError::Local(anyhow!(
                "No conversations in this action"
            )));
        }
        block_on(async {
            Conversation::delete_multiple(
                self.action.ids.clone(),
                self.action.label_id,
                self.tx.stash(),
            )
            .await
            .map_err(|e| ActionError::Local(anyhow!(e)))
        })?;
        Ok(())
    }
}

struct DeleteConversationRemoteHandler {
    action: DeleteConversationsAction,
    session: Session,
    tx: Tether,
}

#[async_trait]
impl RemoteActionHandler for DeleteConversationRemoteHandler {
    fn revert_local(&mut self) -> ActionResult<()> {
        block_on(async {
            Conversation::undelete_multiple(
                self.action.ids.clone(),
                self.action.label_id,
                self.tx.stash(),
            )
            .await
            .map_err(|e| ActionError::Local(anyhow!(e)))
        })?;
        Ok(())
    }

    fn validate_local(&mut self) -> ActionResult<ActionLocalValidationResult> {
        Ok(ActionLocalValidationResult::Valid)
    }

    fn apply_remote(&mut self) -> ActionResult<()> {
        let Some(label) =
            block_on(async { Label::load_using(self.action.label_id, &self.tx).await })
                .map_err(|e| ActionError::Local(anyhow!(e)))?
        else {
            return Err(ActionError::Local(anyhow!(
                "Could not resolve label with id {}",
                self.action.label_id
            )));
        };

        let Some(label_id) = label.remote_id else {
            return Err(ActionError::Local(anyhow!(
                "Label {} has no remote id",
                self.action.label_id
            )));
        };

        let conv_ids = block_on(async {
            Conversation::find_remote_ids(self.action.ids.clone(), self.tx.stash()).await
        })
        .map_err(|e| {
            error!("Failed to resolve conversation ids: {e}");
            ActionError::Local(anyhow!(e))
        })?;
        let responses = block_on(async {
            Conversation::delete_multiple_remote(conv_ids, label_id, self.session.api())
                .await
                .map_err(|e| {
                    error!("Failed to delete conversations on API: {e}");
                    e
                })
        })?;

        let failed_messages = responses
            .into_iter()
            .filter(|r| r.response.code != 1000)
            .map(|r| RemoteId::from(r.id))
            .collect::<Vec<_>>();
        if !failed_messages.is_empty() {
            error!("Delete operation failed for: {:?}", failed_messages);
            let local_ids = block_on(async {
                Conversation::find_local_ids(failed_messages, self.tx.stash()).await
            })
            .map_err(|e| ActionError::Local(anyhow!(e)))?;
            block_on(async {
                Conversation::undelete_multiple(
                    local_ids.clone(),
                    self.action.label_id,
                    self.tx.stash(),
                )
                .await
                .map_err(|e| ActionError::Local(anyhow!(e)))
            })
            .map_err(|e| {
                error!("Failed to rollback failed conversations: {e}");
                ActionError::Local(anyhow!(e))
            })?;
        }

        Ok(())
    }
}

#[derive(Debug)]
pub(super) struct DeleteConversationsActionFactory {}

impl DeleteConversationsActionFactory {
    pub fn new() -> Self {
        Self {}
    }
}

impl ActionFactoryInstance for DeleteConversationsActionFactory {
    fn action_id(&self) -> &'static ActionId {
        &DELETE_CONVERSATION_ACTION_ID
    }

    fn local_handler(
        &self,
        action: Box<dyn Any>,
        tx: Tether,
    ) -> Result<Box<dyn LocalActionHandler>, ActionFactoryInstanceError> {
        let type_id = TypeId::of::<Box<dyn Any>>();
        let Ok(action) = action.downcast::<DeleteConversationsAction>() else {
            return Err(ActionFactoryInstanceError::InvalidType(
                type_id,
                TypeId::of::<DeleteConversationsAction>(),
            ));
        };

        Ok(Box::new(DeleteConversationLocalHandler {
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
        if action.version != DeleteConversationsAction::VERSION {
            return Err(ActionFactoryInstanceError::InvalidVersion(action.version));
        }

        let action = action.deserialize::<DeleteConversationsAction>()?;
        let session = session_provider.retrieve_session()?;

        Ok(Box::new(DeleteConversationRemoteHandler {
            action,
            tx,
            session,
        }))
    }
}
