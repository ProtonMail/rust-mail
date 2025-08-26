use crate::UserContext;
use crate::actions::event_poll::{self};
use proton_action_queue::action::{Metadata, Priority};
use proton_action_queue::queue::Error;
use proton_action_queue::{action::ActionId, queue::ActionError};
use std::time::Duration;

pub mod subscriber;

// Re-export common macros for easier access
pub use subscriber::macros::*;

use super::services::EventLoopService;

/// Defines how the event loop should be polled
#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub enum EventPollMode {
    /// On demand,
    Manual,
    /// Background task that queues a request to polls the event loop in the
    /// specified duration.
    Automatic(Duration),
}

#[derive(Debug, Default)]
pub struct EventLoopActionIds {
    pub last_event_loop_action_id: Option<ActionId>,
    pub last_rollback_action_id: Option<ActionId>,
}

impl UserContext {
    /// Queue an action to execute the event loop.
    ///
    /// If we are in automatic mode this is a noop.
    ///
    /// # Errors
    ///
    /// Returns error if the action failed to be queued.
    ///
    pub async fn poll_event_loop(&self) -> Result<(), ActionError<event_poll::EventPoll>> {
        let event_loop_service = self.event_loop_service();
        self.queue_poll_event_loop(event_loop_service).await
    }

    /// Queue an action to execute the event loop as soon as possible regardless of
    /// the selected polling mode.
    ///
    /// # Errors
    ///
    /// Returns error if the action failed to be queued.
    ///
    pub async fn force_event_loop_poll(&self) -> Result<(), ActionError<event_poll::EventPoll>> {
        let event_poll_action = event_poll::EventPoll {};
        let metadata = Metadata::builder()
            .with_priority_override(Priority::Highest)
            .build();
        self.queue()
            .queue_action_with_metadata(event_poll_action, metadata)
            .await?;
        Ok(())
    }

    async fn queue_poll_event_loop(
        &self,
        event_loop_service: &EventLoopService,
    ) -> Result<(), ActionError<event_poll::EventPoll>> {
        let mut last_action_ids = event_loop_service.last_event_loop_action_ids().lock().await;
        let event_poll_action = event_poll::EventPoll {};
        {
            let output = if let Some(last_action_id) = last_action_ids.last_event_loop_action_id {
                match self
                    .queue()
                    .replace_or_queue_action(last_action_id, event_poll_action)
                    .await
                {
                    Ok(output) => Ok(output),
                    Err(ActionError::Queue(Error::CyclicDependency)) => {
                        self.queue().queue_action(event_poll::EventPoll {}).await
                    }
                    Err(error) => Err(error),
                }?
            } else {
                self.queue().queue_action(event_poll_action).await?
            };
            last_action_ids.last_event_loop_action_id = Some(output.id);
        }
        Ok(())
    }
}
