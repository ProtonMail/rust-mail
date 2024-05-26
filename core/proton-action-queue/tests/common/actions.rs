use crate::common::{FolderId, MessageId, RemoteSource, TestLocalSourceTransaction};
use proton_action_queue::{
    define_action_id, Action, ActionError, ActionFactoryInstance, ActionFactoryInstanceError,
    ActionId, ActionLocalValidationResult, ActionResult, LocalActionHandler, RemoteActionHandler,
    SessionProvider, StoredAction,
};
use proton_api_core::exports::serde::Serialize;
use serde::Deserialize;
use std::any::Any;
use std::fmt::{Debug, Formatter};
use std::marker::PhantomData;
use std::sync::Arc;
use futures::executor::block_on;
use stash::stash::Tether;

#[derive(Debug, Serialize, Deserialize, Eq, PartialEq, Clone)]
pub struct MoveMessageAction {
    pub ids: Vec<MessageId>,
    pub from: FolderId,
    pub to: FolderId,
}

impl MoveMessageAction {
    pub fn new(from: FolderId, to: FolderId, m: impl IntoIterator<Item = MessageId>) -> Self {
        Self {
            from,
            to,
            ids: Vec::from_iter(m),
        }
    }
}

define_action_id!(pub MOVE_MESSAGE_ACTION_ID, "ca651620-8cc2-4394-926f-ab34e5ca3aba");

pub const MOVE_MESSAGE_ACTION_VERSION: u32 = 1;

impl Action for MoveMessageAction {
    const ID: ActionId = MOVE_MESSAGE_ACTION_ID;
    const VERSION: u32 = MOVE_MESSAGE_ACTION_VERSION;
}

#[derive(Debug, Default)]
pub struct MoveMessageLocalActionHandler {}

pub struct TestLocalActionHandler<T: Action> {
    action: T,
    tx: TestLocalSourceTransaction,
}
impl LocalActionHandler for TestLocalActionHandler<MoveMessageAction> {
    fn apply_local(&mut self) -> ActionResult<()> {
        block_on(async {
        self.tx
            .move_message_to_folder(&self.action.ids, self.action.to).await
            .map_err(ActionError::Local)
        })?;
        Ok(())
    }
}

pub struct TestRemoteActionHandler<T: Action> {
    action: T,
    tx: TestLocalSourceTransaction,
    remote: Arc<dyn RemoteSource>,
}

impl RemoteActionHandler for TestRemoteActionHandler<MoveMessageAction> {
    fn revert_local(&mut self) -> ActionResult<()> {
        block_on(async {
        let cur_message_state = self
            .tx
            .get_move_message_state(&self.action.ids).await
            .map_err(ActionError::Local)?;
        //TODO: Improve result here;

        for (m, f) in cur_message_state {
            self.tx
                .move_message_to_folder(&[m], f).await
                .map_err(ActionError::Local)?;
        }
            Ok(())
        })
    }
    
    fn validate_local(&mut self) -> ActionResult<ActionLocalValidationResult> {
        let local_messages = block_on(async {self
            .tx
            .get_messages(&self.action.ids).await
            .map_err(ActionError::Local)})?;

        for msg in &local_messages {
            // Check if message is still in the state we put it in.
            if msg.folder != Some(self.action.to) {
                self.action.ids.retain(|id| *id != msg.id);
            }
        }

        // check for deleted messages.
        self.action
            .ids
            .retain(|id| local_messages.iter().find(|&m| m.id == *id).is_some());

        if self.action.ids.is_empty() {
            return Ok(ActionLocalValidationResult::Invalid);
        }

        Ok(ActionLocalValidationResult::Valid)
    }

    fn apply_remote(&mut self) -> ActionResult<()> {
        self.remote
            .move_messages(self.action.to, &self.action.ids)
            .map_err(ActionError::Remote)?;
        
        block_on(async {
        self.tx
            .update_move_message_dependency(self.action.to, &self.action.ids).await
            .map_err(ActionError::Local)
        })?;
        
        Ok(())
    }
}

pub struct TestActionFactoryInstance<T: Action> {
    remote: Arc<dyn RemoteSource>,
    p: PhantomData<T>,
}

impl<T: Action + Any> Debug for TestActionFactoryInstance<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "TestActionFactoryInstance<{}>",
            std::any::type_name::<T>()
        )
    }
}

impl<T: Action> TestActionFactoryInstance<T> {
    pub fn new(remote: Arc<dyn RemoteSource>) -> Self {
        Self {
            remote,
            p: PhantomData,
        }
    }
}

impl<T: Action + 'static + Send + Sync> ActionFactoryInstance for TestActionFactoryInstance<T>
where
    TestRemoteActionHandler<T>: RemoteActionHandler,
    TestLocalActionHandler<T>: LocalActionHandler,
{
    fn action_id(&self) -> &'static ActionId {
        &T::ID
    }

    fn local_handler(
        &self,
        action: Box<dyn Any>,
        tx: Tether,
    ) -> Result<Box<dyn LocalActionHandler>, ActionFactoryInstanceError> {
        let action = *action.downcast::<T>().map_err(|action| {
            ActionFactoryInstanceError::InvalidType(
                action.type_id(),
                std::any::TypeId::of::<T>(),
            )
        })?;

        Ok(Box::new(TestLocalActionHandler {
            action,
            tx: TestLocalSourceTransaction::new(tx.clone()),
        }))
    }

    fn remote_handler(
        &self,
        action: StoredAction,
        tx: Tether,
        _: &dyn SessionProvider,
    ) -> Result<Box<dyn RemoteActionHandler>, ActionFactoryInstanceError> {
        if action.version != MOVE_MESSAGE_ACTION_VERSION {
            return Err(ActionFactoryInstanceError::InvalidVersion(action.version));
        }

        let action = action.deserialize::<T>()?;

        Ok(Box::new(TestRemoteActionHandler {
            action,
            tx: TestLocalSourceTransaction::new(tx.clone()),
            remote: self.remote.clone(),
        }))
    }
}

#[derive(Debug, Serialize, Deserialize, Eq, PartialEq, Clone)]
pub struct DeleteMessageAction {
    ids: Vec<MessageId>,
}

impl DeleteMessageAction {
    pub fn new(m: impl IntoIterator<Item = MessageId>) -> Self {
        Self {
            ids: Vec::from_iter(m),
        }
    }
}

define_action_id!(pub DELETE_MESSAGE_ACTION_ID, "24214397-6bf8-459b-af36-4595fd52bc86");

pub const DELETE_MESSAGE_ACTION_VERSION: u32 = 1;

impl Action for DeleteMessageAction {
    const ID: ActionId = DELETE_MESSAGE_ACTION_ID;
    const VERSION: u32 = DELETE_MESSAGE_ACTION_VERSION;
}

impl LocalActionHandler for TestLocalActionHandler<DeleteMessageAction> {
    fn apply_local(&mut self) -> ActionResult<()> {
        block_on(async {
        self.tx
            .mark_messages_deleted(true, &self.action.ids).await
            .map_err(ActionError::Local)
        })?;
        Ok(())
    }
}

impl RemoteActionHandler for TestRemoteActionHandler<DeleteMessageAction> {
    fn revert_local(&mut self) -> ActionResult<()> {
        block_on(async {
        self.tx
            .mark_messages_deleted(false, &self.action.ids).await
            .map_err(ActionError::Local)
        })?;
        Ok(())
    }
    
    fn validate_local(&mut self) -> ActionResult<ActionLocalValidationResult> {
        let messages = block_on(async {self
            .tx
            .get_messages_with_deleted(&self.action.ids).await
            .map_err(ActionError::Local)})?;
        self.action
            .ids
            .retain(|id| messages.iter().find(|m| m.id == *id).is_some());

        if self.action.ids.is_empty() {
            return Ok(ActionLocalValidationResult::Invalid);
        }

        Ok(ActionLocalValidationResult::Valid)
    }

    fn apply_remote(&mut self) -> ActionResult<()> {
        self.remote
            .delete_messages(&self.action.ids)
            .map_err(ActionError::Remote)
    }
}
