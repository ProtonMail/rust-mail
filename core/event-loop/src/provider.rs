#![allow(clippy::module_name_repetitions)]

use async_trait::async_trait;
// avoid namespace conflicts
use crate::RawEvent;
use proton_core_api::services::proton::EventId;

pub trait ProviderError: std::error::Error + Send + Sync + 'static {
    fn is_network_failure(&self) -> bool;
}

pub type ProviderResult<T> = Result<T, Box<dyn ProviderError>>;

/// This trait allows abstraction over how to request the next event from the API.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait Provider: Send + Sync {
    async fn get_latest_event_id(&self) -> ProviderResult<EventId>;
    async fn get_event(&self, event_id: &EventId) -> ProviderResult<RawEvent>;
}
