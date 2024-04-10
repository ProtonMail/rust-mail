#![allow(clippy::module_name_repetitions)] // avoid namespace conflicts
use proton_api_core::domain::{Event, EventId};
use proton_api_core::Session;
use proton_async::async_trait::async_trait;

#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait Provider<T: Event>: Send + Sync {
    async fn get_latest_event_id(&self) -> proton_api_core::http::Result<EventId>;

    async fn get_event(&self, event_id: &EventId) -> proton_api_core::http::Result<T>;
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
impl<T: Event> Provider<T> for ProtonProvider {
    async fn get_latest_event_id(&self) -> proton_api_core::http::Result<EventId> {
        self.session.get_latest_event().await
    }

    async fn get_event(&self, event_id: &EventId) -> proton_api_core::http::Result<T> {
        self.session.get_event::<T>(event_id).await
    }
}
