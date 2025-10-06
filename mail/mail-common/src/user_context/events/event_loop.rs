use crate::actions::rollback::RollbackAction;
use crate::models::RollbackItem;
use crate::{MailContextError, MailUserContext};
use anyhow::anyhow;
use proton_action_queue::action::{Action, ActionGroup};
use proton_action_queue::queue::{ActionError, BroadcastMessage};
use proton_core_common::actions::event_poll::EventPoll;
use proton_core_common::app_events::OnEnterForegroundEvent;
use proton_core_common::services::EventLoopService;
use proton_core_common::services::InitializationService;
use proton_event_loop::EventLoopError;
use stash::orm::Model;
use std::time::Duration;
use tokio::sync::broadcast::error::RecvError;
use tokio::time;
use tracing::{Instrument, error};

impl MailUserContext {
    pub(crate) fn init_event_loop_poll(&self, duration: Duration) -> Result<(), MailContextError> {
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

        let mut on_enter_foreground_subscriber = self
            .mail_context
            .core_context()
            .event_service()
            .subscribe::<OnEnterForegroundEvent>()
            .ok_or(MailContextError::Other(anyhow!(
                "Missing on foreground event"
            )))?;
        let mut queue_observer = self.user_context().queue().new_broadcast_receiver();
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
                    let Some(ctx) = ctx.upgrade() else {
                        return;
                    };
                    tokio::select! {
                        _ = interval.tick() => {}
                        event = on_enter_foreground_subscriber.next() => {
                            if event.is_ok() {
                               tracing::info!("Queuing event poll from enter foreground");
                                interval.reset();
                            }
                        }
                        r = queue_observer.recv() => {
                            let msg = match r {
                                Ok(msg) => msg,
                                Err(e) => {
                                    match e {
                                        RecvError::Closed => {
                                            return;
                                        }
                                        RecvError::Lagged(_) => {
                                            continue;
                                        }
                                    }
                                }
                            };

                            // if we have queued an action in this process in the default group
                            // that is not the event loop, reset the timer and replace
                            // the event poll so it runs after this action. We also want to
                            // reset the timer and cancel the event poll after some action
                            // has executed on the server to allow for more time to for the data
                            // to be processed and not accidentally bring back old state.
                            match msg {
                                BroadcastMessage::Queued(_, metadata) | BroadcastMessage::Success(_, metadata) => {
                                    if metadata.action_group == ActionGroup::default().as_ref() {
                                        interval.reset();
                                        if metadata.action_type == EventPoll::TYPE.as_ref() {
                                            continue;
                                        }
                                        if let Err(e) = ctx.user_context().cancel_event_poll().await {
                                            tracing::error!("Failed to cancel queued event poll: {e}");
                                        }
                                        continue;
                                    } else {
                                        continue;
                                    }
                                },
                                _ => {
                                    continue
                                },
                            }
                        }
                    };

                    if let Err(e) = ctx.poll_event_loop(None).await {
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

        Ok(())
    }

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
        // Delegate to UserContext
        self.user_context().poll_event_loop(with_delay).await
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
        let tether = self.user_stash().connection().await?;
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
