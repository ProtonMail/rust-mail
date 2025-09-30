use crate::UserContext;
use proton_action_queue::action::{Action, Metadata, Priority};
use proton_action_queue::queue::QueuedError;
use proton_action_queue::{action::ActionId, queue::ActionError};
use std::time::Duration;
use tracing::error;

pub mod subscriber;

// Re-export common macros for easier access
use super::services::EventLoopService;
use crate::actions::event_poll::EventPoll;
pub use subscriber::macros::*;

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
    pub async fn poll_event_loop(
        &self,
        with_delay: Option<Duration>,
    ) -> Result<(), ActionError<EventPoll>> {
        let event_loop_service = self.event_loop_service();
        self.queue_poll_event_loop(event_loop_service, with_delay, None)
            .await
    }

    /// Queue an action to execute the event loop as soon as possible regardless of
    /// the selected polling mode.
    ///
    /// # Errors
    ///
    /// Returns error if the action failed to be queued.
    ///
    pub async fn force_event_loop_poll(&self) -> Result<(), ActionError<EventPoll>> {
        let event_loop_service = self.event_loop_service();
        self.queue_poll_event_loop(event_loop_service, None, Some(Priority::Highest))
            .await?;
        Ok(())
    }

    async fn queue_poll_event_loop(
        &self,
        event_loop_service: &EventLoopService,
        with_delay: Option<Duration>,
        priority: Option<Priority>,
    ) -> Result<(), ActionError<EventPoll>> {
        let mut last_action_ids = event_loop_service.last_event_loop_action_ids().lock().await;
        let metadata = Metadata::builder()
            .with_delay(with_delay.unwrap_or(Duration::from_secs(0)))
            .with_priority_override(priority.unwrap_or(EventPoll::PRIORITY))
            .build();
        let event_poll_action = EventPoll::default();
        {
            if let Some(last_action_id) = last_action_ids.last_event_loop_action_id {
                if let Err(e) = self.queue().cancel(last_action_id).await {
                    match e {
                        QueuedError::ActionNotFound(_) | QueuedError::ActionInExecution(_) => {
                            // nothing to do
                        }
                        e => {
                            error!("Failed to cancel previous event loop: {e}");
                        }
                    }
                }
            }
            let output = self
                .queue()
                .queue_action_with_metadata(event_poll_action, metadata)
                .await?;
            last_action_ids.last_event_loop_action_id = Some(output.id);
        }
        Ok(())
    }
}
