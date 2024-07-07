use crate::datatypes::SystemLabelId;
use crate::models::{Conversation, ConversationLabel, Label};
use anyhow::anyhow;
use futures::executor::block_on;
use proton_action_queue::{
    define_action_id, Action, ActionError, ActionFactoryInstance, ActionFactoryInstanceError,
    ActionId, ActionLocalValidationResult, ActionResult, LocalActionHandler, RemoteActionHandler,
    SessionProvider, StoredAction,
};
use proton_api_core::services::proton::Proton;
use proton_api_core::session::{CoreSession, Session};
use proton_core_common::datatypes::{LabelId, RemoteId};
use serde::{Deserialize, Serialize};
use stash::orm::Model;
use stash::params;
use stash::stash::Tether;
use std::any::{Any, TypeId};
use tracing::error;

define_action_id!(
    MOVE_CONVERSATIONS_ACTION_ID,
    "e9ccc85a-23fe-40e5-9e53-106ab0c35fe9"
);

#[derive(Clone, Debug, Deserialize, Serialize)]
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

        let src_label =
            block_on(async { Label::load_using(self.action.active_label_id, &self.tx).await })
                .map_err(|e| ActionError::Local(anyhow!(e)))
                .and_then(|opt| opt.ok_or(ActionError::Local(anyhow!("Not found"))))?;
        let dst_label =
            block_on(async { Label::load_using(self.action.destination_label_id, &self.tx).await })
                .map_err(|e| ActionError::Local(anyhow!(e)))
                .and_then(|opt| opt.ok_or(ActionError::Local(anyhow!("Not found"))))?;
        // If moving to trash, mark conversations as read.
        if dst_label.remote_id == Some(LabelId::trash()) {
            block_on(async {
                Conversation::mark_multiple_as_read(self.action.ids.clone(), self.tx.stash()).await
            })
            .map_err(|e| ActionError::Local(anyhow!(e)))?;
        }

        // When moving in Trash or Spam, remove all labels (but AllMail)
        if dst_label.remote_id == Some(LabelId::trash())
            || dst_label.remote_id == Some(LabelId::spam())
        {
            let all_mail_id = block_on(async {
                Label::find_first(
                    "WHERE remote_id = ?",
                    params![LabelId::all_mail()],
                    self.tx.stash(),
                )
                .await
            })
            .map_err(|e| ActionError::Local(anyhow!(e)))?
            .and_then(|l| l.local_id);
            if let Some(all_mail_id) = all_mail_id {
                for &local_conversation_id in &self.action.ids {
                    block_on(async {
                        ConversationLabel::find(
                            "WHERE conversation_id = ?",
                            params![local_conversation_id],
                            self.tx.stash(),
                            None,
                        )
                        .await
                    })
                    .map_err(|e| ActionError::Local(anyhow!(e)))?
                    .iter()
                    .filter(|&cl| cl.local_id != Some(all_mail_id))
                    .try_for_each(|local_label_id| {
                        block_on(async {
                            Conversation::remove_label_from_multiple(
                                local_label_id.local_id.unwrap(),
                                vec![local_conversation_id],
                                self.tx.stash(),
                            )
                            .await
                        })
                        .map_err(|e| ActionError::Local(anyhow!(e)))
                    })?;
                }
            }
            // When moving out of Trash or Spam, add AlmostAllMail label
        } else if src_label.remote_id == Some(LabelId::trash())
            || src_label.remote_id == Some(LabelId::spam())
        {
            let almost_all_mail_id = block_on(async {
                Label::find_first(
                    "WHERE remote_id = ?",
                    params![LabelId::almost_all_mail()],
                    self.tx.stash(),
                )
                .await
            })
            .map_err(|e| ActionError::Local(anyhow!(e)))?
            .and_then(|l| l.local_id);
            if let Some(almost_all_mail_id) = almost_all_mail_id {
                block_on(async {
                    Conversation::apply_label_to_multiple(
                        almost_all_mail_id,
                        self.action.ids.clone(),
                        self.tx.stash(),
                    )
                    .await
                    .map_err(|e| ActionError::Local(anyhow!(e)))
                })?;
            }
        }

        if src_label.is_movable_folder() {
            block_on(async {
                Conversation::remove_label_from_multiple(
                    self.action.active_label_id,
                    self.action.ids.clone(),
                    self.tx.stash(),
                )
                .await
                .map_err(|e| ActionError::Local(anyhow!(e)))
            })?;
        }
        block_on(async {
            Conversation::apply_label_to_multiple(
                self.action.destination_label_id,
                self.action.ids.clone(),
                self.tx.stash(),
            )
            .await
            .map_err(|e| ActionError::Local(anyhow!(e)))
        })?;
        Ok(())
    }
}

struct MoveConversationsRemoteHandler {
    action: MoveConversationsAction,
    session: Session,
    tx: Tether,
}

impl RemoteActionHandler for MoveConversationsRemoteHandler {
    fn revert_local(&mut self) -> ActionResult<()> {
        let src_label =
            block_on(async { Label::load_using(self.action.active_label_id, &self.tx).await })
                .map_err(|e| ActionError::Local(anyhow!(e)))
                .and_then(|opt| opt.ok_or(ActionError::Local(anyhow!("Not found"))))?;
        if src_label.is_movable_folder() {
            block_on(async {
                Conversation::apply_label_to_multiple(
                    self.action.active_label_id,
                    self.action.ids.clone(),
                    self.tx.stash(),
                )
                .await
                .map_err(|e| ActionError::Local(anyhow!(e)))
            })?;
        }
        block_on(async {
            Conversation::remove_label_from_multiple(
                self.action.destination_label_id,
                self.action.ids.clone(),
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
        let src_label =
            block_on(async { Label::load_using(self.action.active_label_id, &self.tx).await })
                .map_err(|e| ActionError::Local(anyhow!(e)))
                .and_then(|opt| opt.ok_or(ActionError::Local(anyhow!("Not found"))))?;
        let dst_label =
            block_on(async { Label::load_using(self.action.destination_label_id, &self.tx).await })
                .map_err(|e| ActionError::Local(anyhow!(e)))
                .and_then(|opt| opt.ok_or(ActionError::Local(anyhow!("Not found"))))?;

        let Some(dst_rid) = dst_label.remote_id else {
            return Err(ActionError::Local(anyhow!(
                "Label {} does not have a remote id",
                self.action.destination_label_id
            )));
        };

        let src_is_folder = src_label.is_movable_folder();

        let conv_ids = block_on(async {
            Conversation::find_remote_ids(self.action.ids.clone(), self.tx.stash()).await
        })
        .map_err(|e| {
            error!("Failed to resolve conversation ids: {e}");
            ActionError::Local(anyhow!(e))
        })?;
        let responses = block_on(async {
            Conversation::apply_label_to_multiple_remote::<Proton>(
                dst_rid,
                conv_ids,
                None,
                self.session.api(),
            )
            .await
        })
        .map_err(|e| {
            error!("Failed to move conversations on API: {e}");
            e
        })?;

        let failed_messages = responses
            .into_iter()
            .filter(|r| r.response.code != 1000)
            .map(|r| RemoteId::from(r.id))
            .collect::<Vec<_>>();
        if !failed_messages.is_empty() {
            error!(
                "Move conversations operation failed for: {:?}",
                failed_messages
            );
            let local_ids = block_on(async {
                Conversation::find_local_ids(failed_messages, self.tx.stash()).await
            })
            .map_err(|e| ActionError::Local(anyhow!(e)))?;

            if src_is_folder {
                block_on(async {
                    Conversation::apply_label_to_multiple(
                        self.action.active_label_id,
                        local_ids.clone(),
                        self.tx.stash(),
                    )
                    .await
                })
                .map_err(|e| {
                    error!("Failed to rollback failed for conversations: {e}");
                    ActionError::Local(anyhow!(e))
                })?;
            }

            block_on(async {
                Conversation::remove_label_from_multiple(
                    self.action.destination_label_id,
                    local_ids.clone(),
                    self.tx.stash(),
                )
                .await
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
        let type_id = TypeId::of::<Box<dyn Any>>();
        let Ok(action) = action.downcast::<MoveConversationsAction>() else {
            return Err(ActionFactoryInstanceError::InvalidType(
                type_id,
                TypeId::of::<MoveConversationsAction>(),
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
            session,
        }))
    }
}
