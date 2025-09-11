use crate::app_events::{UserSessionCreatedEvent, UserSessionDeletedEvent};
use crate::db::account::{CoreSessionObserver, CoreSessionObserverNotification};
use crate::{Context, CoreContextError, OnSessionDeletedResponse};
use proton_event_service::{EventService, EventStream};
use std::sync::Weak;

pub struct SessionObserverService {
    context: Weak<Context>,
    capacity: usize,
}

impl SessionObserverService {
    #[must_use]
    pub fn new(context: Weak<Context>, capacity: usize) -> Self {
        Self { context, capacity }
    }

    pub fn subscribe_to_session_deleted(
        &self,
        event_service: &EventService,
    ) -> Option<EventStream<UserSessionDeletedEvent>> {
        event_service.subscribe::<UserSessionDeletedEvent>()
    }

    pub fn register_session_events(&self, event_service: &EventService, capacity: usize) {
        event_service.register_with_capacity::<UserSessionDeletedEvent>(capacity);
        event_service.register_with_capacity::<UserSessionCreatedEvent>(capacity);
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
            let event_service = ctx.event_service();
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

    pub async fn start(
        &self,
        event_service: &EventService,
        capacity: usize,
    ) -> Result<(), CoreContextError> {
        let Some(ctx) = self.context.upgrade() else {
            return Err(CoreContextError::Other(anyhow::anyhow!(
                "Context not available"
            )));
        };

        let session_observer = CoreSessionObserver::new(ctx.account_stash().clone())
            .await
            .inspect_err(|e| tracing::error!("Failed to create session observer: {e:?}"))?;

        self.register_session_events(event_service, capacity);

        let ctx_weak = self.context.clone();
        ctx.task_service()
            .spawn(async move { Self::on_session_notification(session_observer, ctx_weak).await });

        Ok(())
    }

    pub fn on_session_deleted(
        &self,
        event_service: &EventService,
        hook: impl crate::OnSessionDeleted,
    ) {
        let Some(mut receiver) = self.subscribe_to_session_deleted(event_service) else {
            tracing::error!("User session deleted event not registered");
            return;
        };

        if let Some(ctx) = self.context.upgrade() {
            ctx.task_service().spawn(async move {
                while let Ok(event) = receiver.next().await {
                    if hook
                        .on_session_deleted(event.session_id, event.user_id)
                        .await
                        == OnSessionDeletedResponse::Terminate
                    {
                        break;
                    }
                }
            });
        }
    }
}

use super::Service;
use async_trait::async_trait;

#[async_trait]
impl Service for SessionObserverService {
    type Error = CoreContextError;

    async fn init(&self) -> Result<(), Self::Error> {
        let ctx = self
            .context
            .upgrade()
            .ok_or(CoreContextError::Other(anyhow::anyhow!("Context is dead")))?;
        let event_service = ctx.event_service();
        self.start(event_service, self.capacity).await?;

        let ctx_weak = self.context.clone();
        self.on_session_deleted(event_service, move |_, user_id| {
            let ctx_weak = ctx_weak.clone();

            async move {
                let Some(ctx) = ctx_weak.upgrade() else {
                    return OnSessionDeletedResponse::Terminate;
                };
                ctx.active_user_contexts.lock().await.remove(&user_id);
                OnSessionDeletedResponse::Continue
            }
        });
        Ok(())
    }
}
