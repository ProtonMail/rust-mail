use crate::exports::anyhow::anyhow;
use crate::exports::serde::{self, Deserialize, Serialize};
use crate::{MailUserContext, WeakMailUserContext};
use proton_action_queue::{
    define_action_id, Action, ActionError, ActionFactoryInstance, ActionFactoryInstanceError,
    ActionId, ActionLocalValidationResult, ActionResult, LocalActionHandler, RemoteActionHandler,
    SessionProvider, StoredAction,
};
use proton_event_loop::EventLoopError;
use proton_sqlite3::SqliteTransaction;
use std::any::Any;

define_action_id!(EVENT_LOOP_ACTION_ID, "cccb153b-4cee-4634-90ae-6d7424e5f4d1");
#[derive(Serialize, Deserialize, Debug)]
#[serde(crate = "self::serde")]
pub struct EventLoopAction {}

impl Action for EventLoopAction {
    const ID: ActionId = EVENT_LOOP_ACTION_ID;
    const VERSION: u32 = 1;
}

struct EventLoopLocalActionHandler {}
struct EventLoopRemoteActionHandler {
    ctx: MailUserContext,
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
        let ctx = self.ctx.clone();
        ctx.mail_context()
            .async_runtime()
            .block_on(async { self.ctx.poll_event_loop().await })
            .map_err(|e| match e {
                EventLoopError::StoreRead(e) => ActionError::Local(e),
                EventLoopError::StoreWrite(e) => ActionError::Local(e),
                EventLoopError::Provider(err) => ActionError::Remote(err),
                EventLoopError::Subscriber(s, e) => {
                    ActionError::Local(anyhow!("Failed to apply subscriber error ({s}): {e}"))
                }
                EventLoopError::Other(e) => ActionError::Unknown(anyhow!(e)),
            })
    }
}

#[derive(Debug)]
pub(super) struct EventLoopActionFactory {
    ctx: WeakMailUserContext,
}

impl EventLoopActionFactory {
    pub fn new(ctx: WeakMailUserContext) -> Self {
        Self { ctx }
    }
}

impl ActionFactoryInstance for EventLoopActionFactory {
    fn action_id(&self) -> &'static ActionId {
        &EVENT_LOOP_ACTION_ID
    }

    fn local_handler<'r, 't: 'r>(
        &self,
        action: &'r dyn Any,
        _: &'r mut SqliteTransaction<'t>,
    ) -> Result<Box<dyn LocalActionHandler + 'r>, ActionFactoryInstanceError> {
        let Some(_) = action.downcast_ref::<EventLoopAction>() else {
            return Err(ActionFactoryInstanceError::InvalidType(
                action.type_id(),
                std::any::TypeId::of::<EventLoopAction>(),
            ));
        };
        Ok(Box::new(EventLoopLocalActionHandler {}))
    }

    fn remote_handler<'r, 't: 'r>(
        &'r self,
        action: &StoredAction,
        _: &'r mut SqliteTransaction<'t>,
        _: &dyn SessionProvider,
    ) -> Result<Box<dyn RemoteActionHandler + 'r>, ActionFactoryInstanceError> {
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
