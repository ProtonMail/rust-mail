use crate::MailUserContext;
use crate::actions::rollback::RollbackAction;
use crate::models::RollbackItem;
use proton_action_queue::queue::ActionError;
use proton_core_common::actions::event_poll::EventPoll;
use proton_core_common::services::EventLoopService;
use proton_core_common::services::InitializationService;
use proton_event_loop::EventLoopError;
use stash::orm::Model;
use std::time::Duration;
use tokio::time;
use tracing::{Instrument, error};

impl MailUserContext {
    pub(crate) fn init_event_loop_poll(&self, duration: Duration) {
        tracing::info!(
            "Initializing event loop poll with {} second interval",
            duration.as_secs()
        );

        let ctx = self.this.clone();

        let mut interval = {
            let mut interval = time::interval(duration);

            interval.set_missed_tick_behavior(time::MissedTickBehavior::Skip);
            interval
        };

        let watcher = self
            .user_context
            .get_service::<InitializationService>()
            .initialization_watcher()
            .clone();
        self.spawn(async move {
            async {
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

                    if let Err(e) = ctx.poll_event_loop().await {
                        error!("Failed to queue poll event loop poll: {e:?}");
                    }

                    if let Err(e) = ctx.queue_item_rollback().await {
                        error!("Failed to queue item rollback action: {e:?}")
                    }
                }
            }
            .instrument(tracing::debug_span!("event_loop"))
            .await;
        });
    }

    /// Queue an action to execute the event loop.
    ///
    /// If we are in automatic mode this is a noop.
    ///
    /// # Errors
    ///
    /// Returns error if the action failed to be queued.
    ///
    pub async fn poll_event_loop(&self) -> Result<(), ActionError<EventPoll>> {
        // Delegate to UserContext
        self.user_context().poll_event_loop().await
    }

    /// Queue an action to execute the event loop as soon as possible regardless of
    /// the selected polling mode.
    ///
    /// # Errors
    ///
    /// Returns error if the action failed to be queued.
    ///
    pub async fn force_event_loop_poll(&self) -> Result<(), ActionError<EventPoll>> {
        // Delegate to UserContext
        self.user_context().force_event_loop_poll().await
    }

    async fn queue_item_rollback(&self) -> Result<(), ActionError<RollbackAction>> {
        // Only queue rollback if there is actually anything to rollback.
        let tether = self.user_stash().connection();
        if RollbackItem::count("", vec![], &tether).await? == 0 {
            return Ok(());
        }
        drop(tether);
        let mut last_action_ids = self
            .user_context()
            .event_loop_service()
            .last_event_loop_action_ids()
            .lock()
            .await;
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
            last_action_ids.last_rollback_action_id = Some(output.id);
        }
        Ok(())
    }

    /// Register subscribers
    ///
    /// It can be done now at any point, but since they were already in one place,
    /// it does not hurt to leave them here as for now.
    ///
    pub(crate) async fn register_subscribers(&self) -> Result<(), EventLoopError> {
        let mail_subscriber = self.event_subscriber();

        self.user_context()
            .get_service::<EventLoopService>()
            .event_poll()
            .register(Box::new(mail_subscriber))
            .await?;

        Ok(())
    }
}
