#![allow(clippy::module_name_repetitions)]

use async_trait::async_trait;
use proton_core_api::services::proton::GetEventOptions;
use proton_core_api::services::proton::ProtonCore;
// avoid namespace conflicts
use crate::RawEvent;
use proton_core_api::service::ApiServiceError;
use proton_core_api::services::proton::EventId;
use proton_core_api::session::{CoreSession, Session};

/// This trait allows abstraction over how to request the next event from the API.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait Provider: Send + Sync {
    async fn get_latest_event_id(&self) -> Result<EventId, ApiServiceError>;

    async fn get_event(&self, event_id: &EventId) -> Result<RawEvent, ApiServiceError>;
}

pub struct ProtonProvider {
    session: Session,
}

impl ProtonProvider {
    #[must_use]
    pub fn new(session: Session) -> Self {
        Self { session }
    }
}

#[async_trait]
impl Provider for ProtonProvider {
    async fn get_latest_event_id(&self) -> Result<EventId, ApiServiceError> {
        Ok(self.session.api().get_events_latest().await?.event_id)
    }

    async fn get_event(&self, event_id: &EventId) -> Result<RawEvent, ApiServiceError> {
        let bytes = self
            .session
            .api()
            .get_event(event_id.clone(), GetEventOptions::default())
            .await?;

        Ok(RawEvent::from_json_bytes(bytes)?)
    }
}
