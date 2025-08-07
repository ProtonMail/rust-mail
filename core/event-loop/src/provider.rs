#![allow(clippy::module_name_repetitions)]

use async_trait::async_trait;
// avoid namespace conflicts
use crate::RawEvent;
use proton_core_api::service::ApiServiceError;
use proton_core_api::services::proton::EventId;

/// This trait allows abstraction over how to request the next event from the API.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait Provider: Send + Sync {
    async fn get_latest_event_id(&self) -> Result<EventId, ApiServiceError>;
    async fn get_event(&self, event_id: &EventId) -> Result<RawEvent, ApiServiceError>;
}
