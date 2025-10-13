use crate::CoreContextError;
use crate::app_events::{OnEnterForegroundEvent, OnExitForegroundEvent, OnForceEventPollEvent};
use crate::services::Service;
use async_trait::async_trait;
use proton_event_service::EventService;
use std::ops::Deref;

#[derive(Default)]
pub struct ContextEventService {
    event_service: EventService,
}

impl ContextEventService {
    #[must_use]
    pub fn new() -> Self {
        Self {
            event_service: EventService::new(),
        }
    }
}

impl Deref for ContextEventService {
    type Target = EventService;
    fn deref(&self) -> &Self::Target {
        &self.event_service
    }
}

#[async_trait]
impl Service for ContextEventService {
    type Error = CoreContextError;

    async fn init(&self) -> Result<(), Self::Error> {
        self.event_service.register::<OnEnterForegroundEvent>();
        self.event_service.register::<OnExitForegroundEvent>();
        self.event_service.register::<OnForceEventPollEvent>();
        Ok(())
    }
}
