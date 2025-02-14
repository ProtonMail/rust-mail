use crate::MailUserContext;
use proton_action_queue::action::{Action, ActionId, DefaultVersionConverter, Priority, Type};
use proton_event_loop::subscriber::SubscriberError;
use proton_event_loop::EventLoopError;
use serde::{Deserialize, Serialize};
use stash::stash::{Bond, Stash};

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
    type Context = MailUserContext;
}

/// Wrapper type for [`EventLoopError`].
#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub struct ActionEventLoopError(pub EventLoopError);

impl proton_action_queue::action::Error for ActionEventLoopError {
    fn is_network_failure(&self) -> bool {
        // The event loop action is meant to be queued periodically, even
        // if it fails once due to lack of network it will be run again
        // and there is no need to keep it in the queue until network
        // communication is restored.
        false
    }
}

#[derive(Default)]
pub struct EventPollHandler;

impl proton_action_queue::action::Handler for EventPollHandler {
    type Action = EventPoll;
    type Context = MailUserContext;

    async fn apply_local(
        &self,
        _: ActionId,
        _: &Self::Context,
        _: &mut Self::Action,
        _: &Bond<'_>,
    ) -> Result<<Self::Action as Action>::LocalOutput, <Self::Action as Action>::Error> {
        // Nothing to do.
        Ok(())
    }

    async fn revert_local(
        &self,
        _: ActionId,
        _: &Self::Context,
        _: &mut Self::Action,
        _: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        // Nothing to do
        Ok(())
    }

    async fn apply_remote(
        &self,
        _: ActionId,
        context: &Self::Context,
        _: &mut Self::Action,
        _: &Stash,
    ) -> Result<<Self::Action as Action>::RemoteOutput, <Self::Action as Action>::Error> {
        if let Err(e) = context.poll_event_loop_impl().await {
            if let EventLoopError::Provider(e)
            | EventLoopError::Subscriber(_, SubscriberError::Api(e)) = &e
            {
                // We do not want to report network failure errors to the user so
                // we pretend that this actually worked.
                if e.is_network_failure() {
                    return Ok(());
                }
            }

            return Err(ActionEventLoopError(e));
        }
        Ok(())
    }
}
