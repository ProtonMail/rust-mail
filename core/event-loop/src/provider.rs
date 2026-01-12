#![allow(clippy::module_name_repetitions)]

use async_trait::async_trait;
// avoid namespace conflicts
use crate::{EventId, RawEvent};

pub trait EventProviderError: std::error::Error + Send + Sync + 'static {
    fn is_network_failure(&self) -> bool;
    fn is_auth_failure(&self) -> bool;
}

pub type EventProviderResult<T> = Result<T, Box<dyn EventProviderError>>;

/// This trait allows abstraction over how to request the next event from the API.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait EventProvider: Send + Sync {
    async fn get_latest_event_id(&self) -> EventProviderResult<EventId>;
    async fn get_event(&self, event_id: &EventId) -> EventProviderResult<RawEvent>;
}
