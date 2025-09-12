use crate::UserContext;
use crate::actions::dependency_builder::ActionDependencyKeysBuilder;
use proton_action_queue::action::{
    ActionDependencyKey, ActionDependencyKeys, FactoryResult, VersionConverter,
    VersionConverterError, deserialize,
};
use proton_action_queue::{
    action::{self, Action, ActionId, Handler, Priority, Type, WriterGuard, WriterGuardError},
    queue::ActionRequeueReason,
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
#[derive(Default, Serialize, Deserialize)]
pub struct EventPoll {
    #[serde(default)]
    force: bool,
}

impl EventPoll {
    #[must_use]
    pub fn forced() -> Self {
        EventPoll { force: true }
    }
    #[must_use]
    pub fn dependency_key() -> ActionDependencyKey {
        ActionDependencyKey::from("event-poll")
    }
}

impl Action for EventPoll {
    const TYPE: Type = Type("event_poll");
    const VERSION: u32 = 2;
    const PRIORITY: Priority = Priority::Low;

    type VersionConverter = EventPollVersionConverter;
    type Handler = EventPollHandler;
    type RemoteOutput = ();
    type LocalOutput = ();
    type Error = ActionEventLoopError;

    fn dependency_keys(&self) -> ActionDependencyKeys {
        if self.force {
            ActionDependencyKeys::default()
        } else {
            ActionDependencyKeysBuilder::new()
                .record(Self::dependency_key())
                .with_optional(Self::dependency_key())
                .build()
        }
    }
}

pub struct EventPollVersionConverter;

impl VersionConverter for EventPollVersionConverter {
    type Output = EventPoll;

    fn convert(old_version: u32, current_version: u32, data: &[u8]) -> FactoryResult<Self::Output> {
        if !(old_version <= 2 && current_version == 2) {
            return Err(VersionConverterError::InvalidVersion(current_version).into());
        }

        Ok(deserialize::<EventPoll>(data)?)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ActionEventLoopError {
    #[error(transparent)]
    EventLoop(#[from] EventLoopError),
    #[error(transparent)]
    Subscriber(#[from] SubscriberError),
    #[error(transparent)]
    WriterGuard(#[from] WriterGuardError),
    #[error("Lost context")]
    LostContext,
}

impl action::Error for ActionEventLoopError {
    fn can_requeue(&self) -> Option<ActionRequeueReason> {
        match self {
            Self::EventLoop(
                EventLoopError::Provider(e)
                | EventLoopError::Subscriber(_, SubscriberError::Api(e))
                | EventLoopError::Refresh(_, SubscriberError::Api(e)),
            )
            | Self::Subscriber(SubscriberError::Api(e))
                if e.is_network_failure() =>
            {
                Some(ActionRequeueReason::NetworkFailed)
            }

            Self::WriterGuard(WriterGuardError::Expired) => Some(ActionRequeueReason::GuardExpired),
            Self::LostContext => Some(ActionRequeueReason::LostContext),

            _ => None,
        }
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
        Ok(())
    }

    async fn revert_local(
        &self,
        _: ActionId,
        _: &mut Self::Action,
        _: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
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
            .ok_or(ActionEventLoopError::LostContext)?
            .poll_event_loop_impl()
            .await
            .map_err(ActionEventLoopError::from)?;

        Ok(())
    }
}
