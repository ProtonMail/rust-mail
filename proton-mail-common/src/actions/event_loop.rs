use crate::MailUserContext;
use anyhow::anyhow;
use futures::executor::block_on;
use proton_action_queue::{
    define_action_id, Action, ActionError, ActionFactoryInstance, ActionFactoryInstanceError,
    ActionId, ActionLocalValidationResult, ActionResult, LocalActionHandler, RemoteActionHandler,
    SessionProvider, StoredAction,
};
use proton_event_loop::EventLoopError;
use serde::{Deserialize, Serialize};
use stash::stash::Tether;
use std::any::{Any, TypeId};
use std::sync::{Arc, Weak};

define_action_id!(EVENT_LOOP_ACTION_ID, "cccb153b-4cee-4634-90ae-6d7424e5f4d1");
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct EventLoopAction {}

impl Action for EventLoopAction {
    const ID: ActionId = EVENT_LOOP_ACTION_ID;
    const VERSION: u32 = 1;
}

struct EventLoopLocalActionHandler {}
struct EventLoopRemoteActionHandler {
    ctx: Arc<MailUserContext>,
}

impl LocalActionHandler for EventLoopLocalActionHandler {
    fn apply_local(&mut self) -> ActionResult<()> {
        Ok(())
    }
}

impl RemoteActionHandler for EventLoopRemoteActionHandler {
    fn revert_local(&mut self) -> ActionResult<()> {
        // Nothing to do.
        Ok(())
    }

    fn validate_local(&mut self) -> ActionResult<ActionLocalValidationResult> {
        // Nothing to do.
        Ok(ActionLocalValidationResult::Valid)
    }

    fn apply_remote(&mut self) -> ActionResult<()> {
        block_on(async {
            self.ctx.poll_event_loop().await.map_err(|e| match e {
                EventLoopError::StoreRead(e) => ActionError::Local(e),
                EventLoopError::StoreWrite(e) => ActionError::Local(e),
                EventLoopError::Provider(err) => ActionError::Remote(err),
                EventLoopError::Subscriber(s, e) => {
                    ActionError::Local(anyhow!("Failed to apply subscriber error ({s}): {e}"))
                }
                EventLoopError::Other(e) => ActionError::Unknown(anyhow!(e)),
            })
        })
    }
}

#[derive(Debug)]
pub(super) struct EventLoopActionFactory {
    ctx: Weak<MailUserContext>,
}

impl EventLoopActionFactory {
    pub fn new(ctx: Weak<MailUserContext>) -> Self {
        Self { ctx }
    }
}

impl ActionFactoryInstance for EventLoopActionFactory {
    fn action_id(&self) -> &'static ActionId {
        &EVENT_LOOP_ACTION_ID
    }

    fn local_handler(
        &self,
        action: Box<dyn Any>,
        _: Tether,
    ) -> Result<Box<dyn LocalActionHandler>, ActionFactoryInstanceError> {
        let Some(_) = action.downcast_ref::<EventLoopAction>() else {
            return Err(ActionFactoryInstanceError::InvalidType(
                TypeId::of::<Box<dyn Any>>(),
                TypeId::of::<EventLoopAction>(),
            ));
        };
        Ok(Box::new(EventLoopLocalActionHandler {}))
    }

    fn remote_handler(
        &self,
        action: StoredAction,
        _: Tether,
        _: &dyn SessionProvider,
    ) -> Result<Box<dyn RemoteActionHandler>, ActionFactoryInstanceError> {
        let Some(ctx) = self.ctx.upgrade() else {
            return Err(ActionFactoryInstanceError::Unknown(anyhow!(
                "Could not upgrade context"
            )));
        };

        if action.version != EventLoopAction::VERSION {
            return Err(ActionFactoryInstanceError::InvalidVersion(action.version));
        }

        Ok(Box::new(EventLoopRemoteActionHandler { ctx }))
    }
}
