use crate::app_events::{UserSessionCreatedEvent, UserSessionDeletedEvent};
use crate::db::account::{CoreSessionObserver, CoreSessionObserverNotification};
use crate::{Context, CoreContextError};
use proton_event_service::{Event, EventService, EventStream};
use std::sync::Weak;

pub struct SessionObserverService {
    event_service: EventService,
    context: Weak<Context>,
}

impl SessionObserverService {
    pub fn new(event_service: EventService, context: Weak<Context>) -> Self {
        Self {
            event_service,
            context,
        }
    }

    pub fn subscribe_to_session_deleted(&self) -> Option<EventStream<UserSessionDeletedEvent>> {
        self.event_service.subscribe::<UserSessionDeletedEvent>()
    }

    pub fn register_session_events(&self, capacity: usize) {
        self.event_service
            .register_with_capacity::<UserSessionDeletedEvent>(capacity);
        self.event_service
            .register_with_capacity::<UserSessionCreatedEvent>(capacity);
    }

    pub fn publish(&self, event: impl Event) {
        self.event_service.publish(event);
    }

    pub fn publish_session_created(&self, event: UserSessionCreatedEvent) {
        self.event_service.publish(event);
    }

    pub fn publish_session_deleted(&self, event: UserSessionDeletedEvent) {
        self.event_service.publish(event);
    }

    #[tracing::instrument(skip_all)]
    async fn on_session_notification(mut observer: CoreSessionObserver, ctx: Weak<Context>) {
        tracing::debug!("Starting task");
        while let Ok(notifications) = observer.next().await {
            let Some(ctx) = ctx.upgrade() else {
                tracing::debug!("Context no longer alive, terminating");
                return;
            };
            tracing::debug!("Task received: {:?}", notifications);
            let Ok(event_service) = ctx.session_observer_service() else {
                tracing::error!("Session observer service disappeared in the middle of processing");
                return;
            };
            for notification in notifications {
                match notification {
                    CoreSessionObserverNotification::Created(session_id, user_id) => {
                        event_service.publish(UserSessionCreatedEvent {
                            session_id,
                            user_id,
                        });
                    }
                    CoreSessionObserverNotification::Deleted(session_id, user_id) => {
                        tracing::info!("User {user_id}'s session {session_id} has been deleted");
                        event_service.publish(UserSessionDeletedEvent {
                            session_id,
                            user_id,
                        });
                    }
                }
            }
        }
        tracing::debug!("Stopping task");
    }

    pub async fn start(&self, capacity: usize) -> Result<(), CoreContextError> {
        let Some(ctx) = self.context.upgrade() else {
            return Err(CoreContextError::Other(anyhow::anyhow!(
                "Context not available"
            )));
        };

        let session_observer = CoreSessionObserver::new(ctx.account_stash().clone())
            .await
            .inspect_err(|e| tracing::error!("Failed to create session observer: {e:?}"))?;

        self.register_session_events(capacity);

        let ctx_weak = self.context.clone();
        ctx.task_service()
            .spawn(async move { Self::on_session_notification(session_observer, ctx_weak).await });

        Ok(())
    }

    pub fn on_session_deleted(&self, hook: impl crate::OnSessionDeleted) {
        let Some(mut receiver) = self.subscribe_to_session_deleted() else {
            tracing::error!("User session deleted event not registered");
            return;
        };

        if let Some(ctx) = self.context.upgrade() {
            ctx.task_service().spawn(async move {
                while let Ok(event) = receiver.next().await {
                    if hook
                        .on_session_deleted(event.session_id, event.user_id)
                        .await
                        == crate::OnSessionDeletedResponse::Terminate
                    {
                        break;
                    }
                }
            });
        }
    }
}
