use crate::actions::rollback::RollbackAction;
use crate::user_context::events::subscriber::MailEventSubscriber;
use crate::{MailContextError, MailUserContext};
use proton_action_queue::action::ActionId;
use proton_action_queue::queue::ActionError;
use proton_core_common::event_loop::EventPollMode;
use proton_event_loop::EventLoopError;
use std::sync::Weak;
use std::time::Duration;
use tracing::{Instrument, error, warn};

#[derive(Debug, Default)]
pub(crate) struct EventLoopActionIds {
    last_event_loop_action_id: Option<ActionId>,
    last_rollback_action_id: Option<ActionId>,
}

impl MailUserContext {
    /// Setup a background task that queues the event loop action.
    pub(crate) async fn init_event_loop_poll(
        &self,
        duration: Duration,
    ) -> Result<(), MailContextError> {
        tracing::info!(
            "Initializing event loop poll with {} second interval",
            duration.as_secs()
        );
        let ctx = self.this.clone();
        let mut interval = tokio::time::interval(duration);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let watcher = self.user_context.initialization_watcher.clone();
        self.spawn(
            async move {
                // Wait until `MailUserContext` is initialized.
                tracing::info!("Starting event poll init loop");
                loop {
                    let Some(ctx) = ctx.upgrade() else {
                        return;
                    };

                    tracing::debug!("Waiting on context to be initialized.");
                    if let Err(e) = ctx.wait_on_initialized(watcher.as_ref()).await {
                        error!("Mail User Context failed to initialize: {e:?}, trying again...");
                        continue;
                    }

                    break;
                }
                tracing::info!("Starting event poll loop");
                // `MailUserContext` is now initialized, we can proceed with the event poll.
                loop {
                    interval.tick().await;
                    let Some(ctx) = ctx.upgrade() else {
                        return;
                    };

                    if let Err(e) = ctx.queue_poll_event_loop().await {
                        error!("Failed to queue poll event loop poll: {e:?}");
                    }

                    if let Err(e) = ctx.queue_item_rollback().await {
                        error!("Failed to queue item rollback action: {e:?}")
                    }
                }
            }
            .instrument(tracing::debug_span!("event_loop")),
        );
        Ok(())
    }

    /// Queue an action to execute the event loop.
    ///
    /// If we are in automatic mode this is a noop.
    ///
    /// # Errors
    ///
    /// Returns error if the action failed to be queued.
    pub async fn poll_event_loop(
        &self,
    ) -> Result<(), ActionError<crate::actions::event_poll::EventPoll>> {
        if self.user_context().event_poll_mode() != EventPollMode::Manual {
            warn!("Event poll mode is not configured as manual");
            return Ok(());
        }
        self.queue_poll_event_loop().await
    }

    /// Queue an action to execute the event loop as soon as possible regardless of
    /// the selected polling mode.
    ///
    /// # Errors
    ///
    /// Returns error if the action failed to be queued.
    pub async fn force_event_loop_poll(
        &self,
    ) -> Result<(), ActionError<crate::actions::event_poll::EventPoll>> {
        let event_poll_action = crate::actions::event_poll::EventPoll {};
        self.action_queue().queue_action(event_poll_action).await?;
        Ok(())
    }

    async fn queue_poll_event_loop(
        &self,
    ) -> Result<(), ActionError<crate::actions::event_poll::EventPoll>> {
        let mut last_action_ids = self.last_event_loop_action_ids.lock().await;
        let event_poll_action = crate::actions::event_poll::EventPoll {};
        {
            let output = if let Some(last_action_id) = last_action_ids.last_event_loop_action_id {
                self.action_queue()
                    .replace_or_queue_action(last_action_id, event_poll_action)
                    .await?
            } else {
                self.action_queue().queue_action(event_poll_action).await?
            };
            last_action_ids.last_event_loop_action_id = Some(output.id);
        }
        Ok(())
    }

    async fn queue_item_rollback(&self) -> Result<(), ActionError<RollbackAction>> {
        let mut last_action_ids = self.last_event_loop_action_ids.lock().await;
        {
            let item_rollback_action = RollbackAction {};
            let output = if let Some(last_action_id) = last_action_ids.last_rollback_action_id {
                self.action_queue()
                    .replace_or_queue_action(last_action_id, item_rollback_action)
                    .await?
            } else {
                self.action_queue()
                    .queue_action(item_rollback_action)
                    .await?
            };
            last_action_ids.last_event_loop_action_id = Some(output.id);
        }
        Ok(())
    }

    /// Register subscribers
    ///
    /// It can be done now at any point, but since they were already in one place,
    /// it does not hurt to leave them here as for now.
    ///
    pub(crate) async fn register_subscribers(&self) -> Result<(), EventLoopError> {
        let mail_subscriber = MailEventSubscriber::new(Weak::clone(&self.this));

        self.event_loop().register(mail_subscriber.boxed()).await?;

        Ok(())
    }
}
