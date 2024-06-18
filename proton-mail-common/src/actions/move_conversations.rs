use futures::executor::block_on;
use proton_action_queue::{
    define_action_id, Action, ActionError, ActionFactoryInstance, ActionFactoryInstanceError,
    ActionId, ActionLocalValidationResult, ActionResult, LocalActionHandler, RemoteActionHandler,
    SessionProvider, StoredAction,
};
use proton_api_mail::domain::LabelId;
use proton_api_mail::exports::anyhow::anyhow;
use proton_api_mail::exports::serde::{self, Deserialize, Serialize};
use proton_api_mail::exports::tracing::error;
use proton_api_mail::MailSession;
use stash::stash::Tether;
use std::any::Any;

define_action_id!(
    MOVE_CONVERSATIONS_ACTION_ID,
    "e9ccc85a-23fe-40e5-9e53-106ab0c35fe9"
);

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(crate = "self::serde")]
pub struct MoveConversationsAction {
    active_label_id: u64,
    destination_label_id: u64,
    ids: Vec<u64>,
}

impl MoveConversationsAction {
    pub fn new(
        active_label_id: u64,
        destination_label_id: u64,
        ids: impl IntoIterator<Item = u64>,
    ) -> Self {
        Self {
            active_label_id,
            destination_label_id,
            ids: Vec::from_iter(ids),
        }
    }
}

impl Action for MoveConversationsAction {
    const ID: ActionId = MOVE_CONVERSATIONS_ACTION_ID;
    const VERSION: u32 = 1;
}

struct MoveConversationsLocalHandler {
    action: MoveConversationsAction,
    tx: Tether,
}

impl LocalActionHandler for MoveConversationsLocalHandler {
    fn apply_local(&mut self) -> ActionResult<()> {
        if self.action.ids.is_empty() {
            return Err(ActionError::Local(anyhow!(
                "No conversations in this action"
            )));
        }

        let src_label = self
            .tx
            .label_with_id_or_err(self.action.active_label_id)
            .map_err(|e| ActionError::Local(anyhow!(e)))?;
        let dst_label = self
            .tx
            .label_with_id_or_err(self.action.destination_label_id)
            .map_err(|e| ActionError::Local(anyhow!(e)))?;
        // If moving to trash, mark conversations as read.
        if dst_label.rid.as_ref() == Some(LabelId::trash()) {
            self.tx
                .mark_conversations_read(self.action.ids.iter().cloned())
                .map_err(|e| ActionError::Local(anyhow!(e)))?;
        }

        // When moving in Trash or Spam, remove all labels (but AllMail)
        if dst_label.rid.as_ref() == Some(LabelId::trash())
            || dst_label.rid.as_ref() == Some(LabelId::spam())
        {
            let all_mail_id = self
                .tx
                .resolve_remote_label_id(LabelId::all_mail())
                .map_err(|e| ActionError::Local(anyhow!(e)))?;
            if let Some(all_mail_id) = all_mail_id {
                for &local_conversation_id in &self.action.ids {
                    if let Some(local_label_ids) = self
                        .tx
                        .conversation_label_ids(local_conversation_id)
                        .map_err(|e| ActionError::Local(anyhow!(e)))?
                    {
                        local_label_ids
                            .iter()
                            .filter(|&&id| id != all_mail_id)
                            .try_for_each(|&local_label_id| {
                                self.tx
                                    .unlabel_conversation(local_label_id, local_conversation_id)
                                    .map_err(|e| ActionError::Local(anyhow!(e)))
                            })?;
                    }
                }
            }
            // When moving out of Trash or Spam, add AlmostAllMail label
        } else if src_label.rid.as_ref() == Some(LabelId::trash())
            || src_label.rid.as_ref() == Some(LabelId::spam())
        {
            let almost_all_mail_id = self
                .tx
                .resolve_remote_label_id(LabelId::almost_all_mail())
                .map_err(|e| ActionError::Local(anyhow!(e)))?;
            if let Some(almost_all_mail_id) = almost_all_mail_id {
                self.tx
                    .label_conversations(almost_all_mail_id, self.action.ids.iter().cloned())
                    .map_err(|e| ActionError::Local(anyhow!(e)))?;
            }
        }

        if src_label.is_movable_folder() {
            self.tx
                .unlabel_conversations(self.action.active_label_id, self.action.ids.iter().cloned())
                .map_err(|e| ActionError::Local(anyhow!(e)))?;
        }
        self.tx
            .label_conversations(
                self.action.destination_label_id,
                self.action.ids.iter().cloned(),
            )
            .map_err(|e| ActionError::Local(anyhow!(e)))?;
        Ok(())
    }
}

struct MoveConversationsRemoteHandler {
    action: MoveConversationsAction,
    session: MailSession,
    tx: Tether,
}

impl RemoteActionHandler for MoveConversationsRemoteHandler {
    fn revert_local(&mut self) -> ActionResult<()> {
        let src_label = self
            .tx
            .label_with_id_or_err(self.action.active_label_id)
            .map_err(|e| ActionError::Local(anyhow!(e)))?;
        if src_label.is_movable_folder() {
            self.tx
                .label_conversations(self.action.active_label_id, self.action.ids.iter().cloned())
                .map_err(|e| ActionError::Local(anyhow!(e)))?;
        }
        self.tx
            .unlabel_conversations(
                self.action.destination_label_id,
                self.action.ids.iter().cloned(),
            )
            .map_err(|e| ActionError::Local(anyhow!(e)))?;
        Ok(())
    }

    fn validate_local(&mut self) -> ActionResult<ActionLocalValidationResult> {
        Ok(ActionLocalValidationResult::Valid)
    }

    fn apply_remote(&mut self) -> ActionResult<()> {
        let src_label = self
            .tx
            .label_with_id_or_err(self.action.active_label_id)
            .map_err(|e| ActionError::Local(anyhow!(e)))?;
        let dst_label = self
            .tx
            .label_with_id_or_err(self.action.destination_label_id)
            .map_err(|e| ActionError::Local(anyhow!(e)))?;

        let Some(dst_rid) = dst_label.rid.as_ref() else {
            return Err(ActionError::Local(anyhow!(
                "Label {} does not have a remote id",
                self.action.destination_label_id
            )));
        };

        let src_is_folder = src_label.is_movable_folder();

        let conv_ids = self
            .tx
            .local_to_remote_conversation_ids(self.action.ids.iter().cloned())
            .map_err(|e| {
                error!("Failed to resolve conversation ids: {e}");
                ActionError::Local(anyhow!(e))
            })?;
        let responses = block_on(async {
            {
                self.session
                    .label_conversations(dst_rid, &conv_ids, None)
                    .await
            }
            .map_err(|e| {
                error!("Failed to move conversations on API: {e}");
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
                "Move conversations operation failed for: {:?}",
                failed_messages
            );
            let local_ids = self
                .tx
                .remote_to_local_conversation_ids(failed_messages.iter())
                .map_err(|e| ActionError::Local(anyhow!(e)))?;

            if src_is_folder {
                self.tx
                    .label_conversations(self.action.active_label_id, local_ids.iter().cloned())
                    .map_err(|e| {
                        error!("Failed to rollback failed for conversations: {e}");
                        ActionError::Local(anyhow!(e))
                    })?;
            }

            self.tx
                .unlabel_conversations(self.action.destination_label_id, local_ids.into_iter())
                .map_err(|e| {
                    error!("Failed to rollback failed for conversations: {e}");
                    ActionError::Local(anyhow!(e))
                })?;
        }

        Ok(())
    }
}

#[derive(Debug)]
pub(super) struct MoveConversationsActionFactory {}

impl MoveConversationsActionFactory {
    pub fn new() -> Self {
        Self {}
    }
}

impl ActionFactoryInstance for MoveConversationsActionFactory {
    fn action_id(&self) -> &'static ActionId {
        &MOVE_CONVERSATIONS_ACTION_ID
    }

    fn local_handler(
        &self,
        action: Box<dyn Any>,
        tx: Tether,
    ) -> Result<Box<dyn LocalActionHandler>, ActionFactoryInstanceError> {
        let type_id = action.type_id().clone();
        let Ok(action) = action.downcast::<MoveConversationsAction>() else {
            return Err(ActionFactoryInstanceError::InvalidType(
                type_id,
                std::any::TypeId::of::<MoveConversationsAction>(),
            ));
        };

        Ok(Box::new(MoveConversationsLocalHandler {
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
        if action.version != MoveConversationsAction::VERSION {
            return Err(ActionFactoryInstanceError::InvalidVersion(action.version));
        }

        let action = action.deserialize::<MoveConversationsAction>()?;
        let session = session_provider.retrieve_session()?;

        Ok(Box::new(MoveConversationsRemoteHandler {
            action,
            tx,
            session: MailSession::from(session),
        }))
    }
}
