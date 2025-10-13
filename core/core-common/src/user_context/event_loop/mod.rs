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
use crate::app_events::OnForceEventPollEvent;
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
    pub last_event_loop_action_id_forced: Option<ActionId>,
    pub last_event_loop_action_id_normal: Option<ActionId>,
    pub last_rollback_action_id: Option<ActionId>,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum EventPollIntent {
    Forced,
    Normal,
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
    pub async fn poll_event_loop(&self) -> Result<(), ActionError<EventPoll>> {
        tracing::debug!("Polling event loop (normal)");
        let event_loop_service = self.event_loop_service();
        self.queue_poll_event_loop(event_loop_service, EventPollIntent::Normal)
            .await
    }

    pub async fn cancel_event_poll(&self) -> Result<(), QueuedError> {
        // Note: this only cancels normal periodically queued event polls,
        // forced event poll remain unaffected.
        let event_loop_service = self.event_loop_service();
        let last_action_ids = event_loop_service.last_event_loop_action_ids().lock().await;
        if let Some(last_action_id) = last_action_ids.last_event_loop_action_id_normal {
            if let Err(e) = self.queue().cancel(last_action_id).await {
                match e {
                    QueuedError::ActionNotFound(_) | QueuedError::ActionInExecution(_) => {
                        // nothing to do
                    }
                    e => return Err(e),
                }
            }
        }
        Ok(())
    }

    /// Queue an action to execute the event loop as soon as possible regardless of
    /// the selected polling mode.
    ///
    /// # Errors
    ///
    /// Returns error if the action failed to be queued.
    ///
    pub async fn force_event_loop_poll(&self) -> Result<(), ActionError<EventPoll>> {
        tracing::debug!("Polling event loop (forced)");
        let event_loop_service = self.event_loop_service();
        self.queue_poll_event_loop(event_loop_service, EventPollIntent::Forced)
            .await?;
        let event_service = self.context.event_service();
        event_service.publish(OnForceEventPollEvent);
        Ok(())
    }

    async fn queue_poll_event_loop(
        &self,
        event_loop_service: &EventLoopService,
        intent: EventPollIntent,
    ) -> Result<(), ActionError<EventPoll>> {
        let last_action_ids = event_loop_service.last_event_loop_action_ids().lock().await;
        let (action, priority) = if intent == EventPollIntent::Forced {
            (EventPoll::forced(), Priority::Highest)
        } else {
            (EventPoll::default(), EventPoll::PRIORITY)
        };
        let metadata = Metadata::builder().with_priority_override(priority).build();
        {
            let last_action_id = &mut if intent == EventPollIntent::Forced {
                last_action_ids.last_event_loop_action_id_forced
            } else {
                last_action_ids.last_event_loop_action_id_normal
            };
            if let Some(last_action_id) = *last_action_id {
                if let Err(e) = self.queue().cancel(last_action_id).await {
                    match e {
                        QueuedError::ActionNotFound(_) => {
                            // do nothing
                        }
                        QueuedError::ActionInExecution(_) => {
                            // Don't want to re-queue if event poll is already running
                            return Ok(());
                        }
                        e => {
                            error!("Failed to cancel previous event loop: {e}");
                        }
                    }
                }
            }
            let output = self
                .queue()
                .queue_action_with_metadata(action, metadata)
                .await?;
            *last_action_id = Some(output.id);
        }
        Ok(())
    }
}
