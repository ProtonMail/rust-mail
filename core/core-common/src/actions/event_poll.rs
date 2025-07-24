use crate::UserContext;
use proton_action_queue::action::{
    Action, ActionId, DefaultVersionConverter, Handler, Priority, Type, WriterGuard,
    WriterGuardError,
};
use proton_event_loop::EventLoopError;
use proton_event_loop::subscriber::SubscriberError;
use serde::{Deserialize, Serialize};
use stash::stash::Bond;
use std::sync::Weak;

/// Action which polls the event loop.
///
/// Rather than control exclusive execution access between the queue and the event loop, run
/// the event loop as action in the queue.
#[derive(Serialize, Deserialize)]
pub struct EventPoll {}

impl Action for EventPoll {
    const TYPE: Type = Type("event_poll");
    const VERSION: u32 = 1;
    const PRIORITY: Priority = Priority::Low;

    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = EventPollHandler;
    type RemoteOutput = ();
    type LocalOutput = ();
    type Error = ActionEventLoopError;
}

/// Wrapper type for [`EventLoopError`].
#[derive(Debug, thiserror::Error)]
pub enum ActionEventLoopError {
    #[error(transparent)]
    EventLoop(#[from] EventLoopError),
    #[error(transparent)]
    Subscriber(#[from] SubscriberError),
    #[error(transparent)]
    WriterGuard(#[from] WriterGuardError),
}

impl proton_action_queue::action::Error for ActionEventLoopError {
    fn is_network_failure(&self) -> bool {
        if let ActionEventLoopError::EventLoop(
            EventLoopError::Provider(e)
            | EventLoopError::Subscriber(_, SubscriberError::Api(e))
            | EventLoopError::Refresh(_, SubscriberError::Api(e)),
        )
        | ActionEventLoopError::Subscriber(SubscriberError::Api(e)) = &self
        {
            return e.is_network_failure();
        }

        false
    }

    fn is_writer_guard_expired(&self) -> bool {
        matches!(self, Self::WriterGuard(WriterGuardError::Expired))
    }
}

pub struct EventPollHandler {
    pub ctx: Weak<UserContext>,
}

impl Handler for EventPollHandler {
    type Action = EventPoll;

    async fn apply_local(
        &self,
        _: ActionId,
        _: &mut Self::Action,
        _: &Bond<'_>,
    ) -> Result<<Self::Action as Action>::LocalOutput, <Self::Action as Action>::Error> {
        // Nothing to do.
        Ok(())
    }

    async fn revert_local(
        &self,
        _: ActionId,
        _: &mut Self::Action,
        _: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        // Nothing to do
        Ok(())
    }

    async fn apply_remote(
        &self,
        _: ActionId,
        _: &mut Self::Action,
        _: WriterGuard<'_>,
    ) -> Result<<Self::Action as Action>::RemoteOutput, <Self::Action as Action>::Error> {
        self.ctx
            .upgrade()
            .expect("context has died")
            .poll_event_loop_impl()
            .await
            .map_err(ActionEventLoopError::from)?;

        Ok(())
    }
}
